//! Egress proxy: the only network exit of a sandboxed run (design §6.1, research 1a §3).
//!
//! The sandbox lets a run reach exactly one loopback port, where this proxy listens. It accepts
//! `CONNECT host:port` only, passes a host only if it exactly matches (ASCII case-insensitive)
//! an entry of the allowlist, requires port 443, and then relays bytes both ways without looking
//! inside them (no TLS interception). Every request ends in one [`EgressDecision`], allowed or
//! not, which [`EgressProxy::stop`] returns for the audit trail.
//!
//! Responses: `200 Connection established`, `400` (unparsable request), `403` (host or port not
//! allowed), `405` (not `CONNECT`), `408` (header not complete within 10 s), `431` (header over
//! 8 KiB), `502` (allowed, but the upstream connection failed).

use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const MAX_HEADER_BYTES: usize = 8 * 1024;
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);
const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// After an error response, unread request bytes are drained for at most this long so closing
/// the socket does not reset the connection before the client reads the response.
const LINGER: Duration = Duration::from_secs(1);

/// One proxy decision. `allowed` is the policy verdict for a parsed `CONNECT`; requests that
/// never reach a verdict are recorded as not allowed with `port` 0 and `host` set to the raw
/// request target, or empty when there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressDecision {
    pub host: String,
    pub port: u16,
    pub allowed: bool,
    pub at_us: i64,
}

/// A running proxy on `127.0.0.1:<port>`. Dropping it without [`stop`](Self::stop) also shuts it
/// down, but loses the decision log.
pub struct EgressProxy {
    port: u16,
    cancel: CancellationToken,
    task: JoinHandle<()>,
    log: Arc<Mutex<Vec<EgressDecision>>>,
}

struct Policy {
    allow_hosts: Vec<String>,
    allow_local_ports_for_tests: bool,
}

impl Policy {
    fn allows(&self, host: &str, port: u16) -> bool {
        let host = host.to_ascii_lowercase();
        let listed = self.allow_hosts.contains(&host);
        let local = host == "127.0.0.1" || host == "localhost";
        listed && (port == 443 || (self.allow_local_ports_for_tests && local))
    }
}

impl EgressProxy {
    /// Starts a proxy passing `CONNECT <host>:443` for the listed hosts only. Entries are exact
    /// host names; an empty entry or one containing `*`, `:`, `/` or whitespace is refused with
    /// `InvalidInput`.
    pub async fn start(allow_hosts: Vec<String>) -> std::io::Result<EgressProxy> {
        Self::start_with(allow_hosts, false).await
    }

    /// As [`start`](Self::start), but `127.0.0.1` and `localhost` (if listed) are passed on any
    /// port, so tests can tunnel to servers they start. Never available in production builds.
    #[cfg(feature = "test-support")]
    pub async fn start_allowing_local_ports_for_tests(
        allow_hosts: Vec<String>,
    ) -> std::io::Result<EgressProxy> {
        Self::start_with(allow_hosts, true).await
    }

    async fn start_with(
        allow_hosts: Vec<String>,
        allow_local_ports_for_tests: bool,
    ) -> std::io::Result<EgressProxy> {
        for h in &allow_hosts {
            let bad = h.is_empty()
                || h.chars()
                    .any(|c| c == '*' || c == ':' || c == '/' || c.is_whitespace());
            if bad {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("egress allowlist entry {h:?} is not an exact host name"),
                ));
            }
        }
        let policy = Arc::new(Policy {
            allow_hosts: allow_hosts.iter().map(|h| h.to_ascii_lowercase()).collect(),
            allow_local_ports_for_tests,
        });

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let port = listener.local_addr()?.port();
        let cancel = CancellationToken::new();
        let log = Arc::new(Mutex::new(Vec::new()));

        let task = tokio::spawn({
            let cancel = cancel.clone();
            let log = log.clone();
            async move {
                // Dropping the set when the loop ends aborts every open tunnel.
                let mut conns = JoinSet::new();
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        accepted = listener.accept() => match accepted {
                            Ok((sock, _)) => {
                                conns.spawn(handle(sock, policy.clone(), log.clone()));
                            }
                            // Typically out of descriptors; back off instead of spinning.
                            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                        },
                        Some(_) = conns.join_next(), if !conns.is_empty() => {}
                    }
                }
            }
        });

        Ok(EgressProxy {
            port,
            cancel,
            task,
            log,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Stops listening, closes every tunnel, and returns all decisions in the order they were
    /// made.
    pub async fn stop(mut self) -> Vec<EgressDecision> {
        self.cancel.cancel();
        let _ = (&mut self.task).await;
        std::mem::take(&mut *self.log.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Drop for EgressProxy {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn record(log: &Mutex<Vec<EgressDecision>>, host: &str, port: u16, allowed: bool) {
    let at_us = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX));
    log.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(EgressDecision {
            host: host.to_string(),
            port,
            allowed,
            at_us,
        });
}

enum HeadError {
    TooLarge,
    Closed,
}

/// Reads up to and including the blank line ending the request header. Returns the header and
/// any bytes the client sent after it.
async fn read_head(sock: &mut TcpStream) -> Result<(Vec<u8>, Vec<u8>), HeadError> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = sock.read(&mut chunk).await.map_err(|_| HeadError::Closed)?;
        if n == 0 {
            return Err(HeadError::Closed);
        }
        let searched_from = buf.len().saturating_sub(3);
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf[searched_from..]
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
        {
            let end = searched_from + i + 4;
            if end > MAX_HEADER_BYTES {
                return Err(HeadError::TooLarge);
            }
            let rest = buf.split_off(end);
            return Ok((buf, rest));
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Err(HeadError::TooLarge);
        }
    }
}

/// Splits `host:port` or `[v6]:port`.
fn parse_authority(target: &str) -> Option<(&str, u16)> {
    let (host, port) = match target.strip_prefix('[') {
        Some(rest) => rest.split_once("]:")?,
        None => target.rsplit_once(':')?,
    };
    if host.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((host, port.parse().ok()?))
}

async fn respond_and_close(mut sock: TcpStream, status: &str) {
    let msg = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    if sock.write_all(msg.as_bytes()).await.is_err() {
        return;
    }
    let _ = sock.shutdown().await;
    let _ = timeout(LINGER, async {
        let mut sink = [0u8; 4096];
        while matches!(sock.read(&mut sink).await, Ok(n) if n > 0) {}
    })
    .await;
}

async fn handle(mut client: TcpStream, policy: Arc<Policy>, log: Arc<Mutex<Vec<EgressDecision>>>) {
    let (head, rest) = match timeout(HEADER_TIMEOUT, read_head(&mut client)).await {
        Ok(Ok(parts)) => parts,
        Ok(Err(HeadError::TooLarge)) => {
            record(&log, "", 0, false);
            return respond_and_close(client, "431 Request Header Fields Too Large").await;
        }
        Ok(Err(HeadError::Closed)) => return record(&log, "", 0, false),
        Err(_) => {
            record(&log, "", 0, false);
            return respond_and_close(client, "408 Request Timeout").await;
        }
    };

    let line = head.split(|&b| b == b'\n').next().unwrap_or_default();
    let line = String::from_utf8_lossy(line);
    let mut parts = line.trim_end_matches('\r').split(' ');
    let (method, target, version) = (parts.next(), parts.next(), parts.next());
    let target = target.unwrap_or_default();
    if parts.next().is_some() || version.is_none_or(|v| !v.starts_with("HTTP/")) {
        record(&log, target, 0, false);
        return respond_and_close(client, "400 Bad Request").await;
    }
    if method != Some("CONNECT") {
        record(&log, target, 0, false);
        return respond_and_close(client, "405 Method Not Allowed").await;
    }
    let Some((host, port)) = parse_authority(target) else {
        record(&log, target, 0, false);
        return respond_and_close(client, "400 Bad Request").await;
    };

    let allowed = policy.allows(host, port);
    record(&log, host, port, allowed);
    if !allowed {
        return respond_and_close(client, "403 Forbidden").await;
    }

    let mut upstream =
        match timeout(UPSTREAM_CONNECT_TIMEOUT, TcpStream::connect((host, port))).await {
            Ok(Ok(s)) => s,
            _ => return respond_and_close(client, "502 Bad Gateway").await,
        };
    if client
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .await
        .is_err()
        || upstream.write_all(&rest).await.is_err()
    {
        return;
    }
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
}
