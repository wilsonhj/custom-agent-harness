//! The egress CONNECT proxy, against TLS-less targets this test starts on localhost.

// The plan denies these lints in non-test code only (Global Constraints).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;
use std::time::{Duration, Instant};

use harness_exec::LaunchPlan;
use harness_exec::egress::{EgressDecision, EgressProxy};
use harness_exec::sandbox_macos::{Network, SandboxProfile, wrap};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Echoes every byte back. Returns its port.
async fn echo_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut r, mut w) = sock.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    port
}

/// Answers any request with a fixed HTTP/1.0 body. Returns its port.
async fn http_server(body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    port
}

/// Sends `request` to the proxy and returns the stream plus the response head.
async fn send(proxy_port: u16, request: &[u8]) -> (TcpStream, String) {
    let mut sock = TcpStream::connect(("127.0.0.1", proxy_port)).await.unwrap();
    sock.write_all(request).await.unwrap();
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match sock.read(&mut byte).await.unwrap() {
            0 => break,
            _ => head.push(byte[0]),
        }
    }
    (sock, String::from_utf8_lossy(&head).into_owned())
}

async fn connect(proxy_port: u16, authority: &str) -> (TcpStream, String) {
    send(
        proxy_port,
        format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n\r\n").as_bytes(),
    )
    .await
}

fn status(head: &str) -> &str {
    head.split(' ').nth(1).unwrap_or("")
}

#[tokio::test]
async fn allowed_host_connects_and_other_host_gets_403() {
    let target = echo_server().await;
    let proxy = EgressProxy::start_allowing_local_ports_for_tests(vec!["LocalHost".into()])
        .await
        .unwrap();

    let (mut tunnel, head) = connect(proxy.port(), &format!("localhost:{target}")).await;
    assert!(
        head.starts_with("HTTP/1.1 200 Connection established"),
        "{head}"
    );
    tunnel.write_all(b"ping").await.unwrap();
    let mut buf = [0u8; 4];
    tunnel.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ping");

    // Exact match only, case-insensitive: a different spelling of the same address is denied.
    let (_, head) = connect(proxy.port(), &format!("LOCALHOST:{target}")).await;
    assert_eq!(status(&head), "200");
    let (_, head) = connect(proxy.port(), &format!("127.0.0.1:{target}")).await;
    assert_eq!(status(&head), "403", "{head}");
    let (_, head) = connect(proxy.port(), &format!("sub.localhost:{target}")).await;
    assert_eq!(status(&head), "403", "{head}");

    proxy.stop().await;
}

#[tokio::test]
async fn production_proxy_allows_port_443_only() {
    let target = echo_server().await;
    let proxy = EgressProxy::start(vec!["localhost".into()]).await.unwrap();
    let (_, head) = connect(proxy.port(), &format!("localhost:{target}")).await;
    assert_eq!(status(&head), "403", "{head}");
    let log = proxy.stop().await;
    assert_eq!(log.len(), 1);
    assert!(!log[0].allowed);
    assert_eq!(log[0].port, target);
}

#[tokio::test]
async fn wildcard_allow_entries_are_refused() {
    assert!(
        EgressProxy::start(vec!["*.example.com".into()])
            .await
            .is_err()
    );
    assert!(EgressProxy::start(vec!["".into()]).await.is_err());
}

#[tokio::test]
async fn non_connect_methods_get_405() {
    let proxy = EgressProxy::start(vec!["example.com".into()])
        .await
        .unwrap();
    let (_, head) = send(
        proxy.port(),
        b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_eq!(status(&head), "405", "{head}");
    proxy.stop().await;
}

#[tokio::test]
async fn oversized_header_is_rejected() {
    let proxy = EgressProxy::start(vec!["example.com".into()])
        .await
        .unwrap();
    let mut req = b"CONNECT example.com:443 HTTP/1.1\r\nX-Pad: ".to_vec();
    req.extend(std::iter::repeat_n(b'a', 9 * 1024));
    req.extend_from_slice(b"\r\n\r\n");
    let (_, head) = send(proxy.port(), &req).await;
    assert_eq!(status(&head), "431", "{head}");
    let log = proxy.stop().await;
    assert_eq!(log.len(), 1);
    assert!(!log[0].allowed);
}

#[tokio::test]
async fn slow_header_times_out() {
    let proxy = EgressProxy::start(vec!["example.com".into()])
        .await
        .unwrap();
    let started = Instant::now();
    let (_, head) = send(proxy.port(), b"CONNECT example.com:443 HTTP/1.1\r\n").await;
    let waited = started.elapsed();
    assert_eq!(status(&head), "408", "{head}");
    assert!(waited >= Duration::from_secs(9), "{waited:?}");
    assert!(waited < Duration::from_secs(20), "{waited:?}");
    proxy.stop().await;
}

#[tokio::test]
async fn direct_connection_from_sandbox_is_blocked_proxy_works() {
    let target = http_server("hello").await;
    let proxy = EgressProxy::start_allowing_local_ports_for_tests(vec!["localhost".into()])
        .await
        .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let wt = tmp.path().join("wt");
    let home = tmp.path().join("home");
    std::fs::create_dir(&wt).unwrap();
    std::fs::create_dir(&home).unwrap();
    let profile = SandboxProfile {
        worktree: wt.clone(),
        worktree_writable: true,
        task_home: home,
        read_deny: Vec::new(),
        read_allow_extra: Vec::new(),
        network: Network::ProxyOnly { port: proxy.port() },
    };
    // The blocking process runs off the runtime thread so the proxy keeps serving.
    let run = |argv: &[&str]| {
        let w = wrap(
            &profile,
            LaunchPlan {
                argv: argv.iter().map(|s| s.to_string()).collect(),
                env: Vec::new(),
                cwd: wt.clone(),
                stdin: Vec::new(),
            },
        )
        .unwrap();
        tokio::task::spawn_blocking(move || {
            Command::new(&w.argv[0])
                .args(&w.argv[1..])
                .envs(w.env.iter().map(|(k, v)| (k, v)))
                .current_dir(&w.cwd)
                .output()
                .unwrap()
        })
    };

    let target_s = target.to_string();
    let direct = run(&["/usr/bin/nc", "-vz", "-G", "2", "127.0.0.1", &target_s])
        .await
        .unwrap();
    assert!(!direct.status.success());
    assert!(
        String::from_utf8_lossy(&direct.stderr).contains("Operation not permitted"),
        "{direct:?}"
    );

    // curl -p tunnels plain HTTP through CONNECT; the proxy URL comes from the injected env.
    let url = format!("http://localhost:{target}/");
    let via_proxy = run(&[
        "/bin/sh",
        "-c",
        &format!("/usr/bin/curl -sS -m 5 -p -x \"$HTTPS_PROXY\" {url}"),
    ])
    .await
    .unwrap();
    assert!(via_proxy.status.success(), "{via_proxy:?}");
    assert_eq!(via_proxy.stdout, b"hello");

    let log = proxy.stop().await;
    assert_eq!(log.len(), 1);
    assert!(log[0].allowed);
    assert_eq!(log[0].host, "localhost");
}

#[tokio::test]
async fn every_decision_is_logged() {
    let target = echo_server().await;
    let proxy = EgressProxy::start_allowing_local_ports_for_tests(vec!["localhost".into()])
        .await
        .unwrap();
    let before = now_us();

    let (_t, head) = connect(proxy.port(), &format!("localhost:{target}")).await;
    assert_eq!(status(&head), "200");
    let (_, head) = connect(proxy.port(), "denied.example:443").await;
    assert_eq!(status(&head), "403");
    let (_, head) = connect(proxy.port(), "localhost:notaport").await;
    assert_eq!(status(&head), "400");
    let (_, head) = send(proxy.port(), b"POST / HTTP/1.1\r\n\r\n").await;
    assert_eq!(status(&head), "405");

    let log: Vec<EgressDecision> = proxy.stop().await;
    let summary: Vec<(&str, u16, bool)> = log
        .iter()
        .map(|d| (d.host.as_str(), d.port, d.allowed))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("localhost", target, true),
            ("denied.example", 443, false),
            ("localhost:notaport", 0, false),
            ("/", 0, false),
        ]
    );
    assert!(log.iter().all(|d| d.at_us >= before && d.at_us <= now_us()));
}

fn now_us() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0)
}
