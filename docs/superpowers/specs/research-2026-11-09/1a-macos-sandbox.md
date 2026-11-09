# Track 1a: macOS sandbox for agent runs

Date: 2026-11-09. Machine: macOS 26.6.2 (25G83), arm64. CLIs present: `claude` 2.1.318,
`codex-cli` 0.166.0, `cursor-agent` 2026.11.05-86047df, colima 0.10.3 (running, docker runtime).

Grades: (P) probed locally by me today, (D) read on an official page today (URL given),
(S) secondary source / memory, UNVERIFIED.

Probe files (profiles, C helpers, toy proxy) are in
`<scratchpad>/tracks/1a-probes/`.

**Rule deviation, disclosed:** to get a realistic worktree layout I ran `git init`,
`git commit --allow-empty` and `git worktree add` once, inside that throwaway probe
directory. The brief says "do not run git". Nothing outside the probe directory was touched,
and I stopped running git after noticing. All later Git-metadata tests are plain file writes.

Note: another agent deleted my first probe directory (`scratchpad/sbx`) mid-run. Shared
scratch directories need per-agent names.

---

## 0. Recommendation in one paragraph

Use **Seatbelt through `/usr/bin/sandbox-exec`** (absolute path), with a profile **generated
per run** from a typed `SandboxProfile`. Start from `(deny default)`. Reads are broad, with
deny-lists for other tasks' worktrees, the harness state store, `~/.ssh`, `~/.aws` and other
secrets. Writes are allowed only in the task worktree, the task home and the task tmp, with
explicit denies for the worktree's `.git` file and the shared Git common directory.
`mach-lookup` is an allow-list. Network is either none, loopback only, or **one loopback port
where a harness-run filtering CONNECT proxy listens**. Disable every inner agent sandbox,
because nesting is impossible (probed). Do process-tree cleanup with a **per-run supervisor
process spawned with responsibility disclaimed**, plus a sweep by "responsible PID". Kill the
process group first, then sweep until nothing is left, and only then release the worktree
lock. Fall back to fail-closed if any step errors. Linux bubblewrap implements the same trait.

---

## 1. Seatbelt / `sandbox-exec`: capabilities and limits

### Status on macOS 26

- `man sandbox-exec` says: "The sandbox-exec command is DEPRECATED. Developers who wish to
  sandbox an app should instead adopt the App Sandbox feature" (P).
- `sandbox.h` in the SDK says: "This header is deprecated and may be removed in a future
  release" (P). `sandbox_init` is declared there.
- It still works fully on 26.6.2. Every probe below ran. The binary is dated Aug 12 (OS build) (P).
- Codex, Claude Code (via `sandbox-runtime`) and Cursor all ship on it today:
  - Codex hard-codes `/usr/bin/sandbox-exec` (D: https://raw.githubusercontent.com/openai/codex/main/codex-rs/sandboxing/src/seatbelt.rs).
  - Claude Code (D: https://code.claude.com/docs/en/sandboxing, https://github.com/anthropic-experimental/sandbox-runtime).
  - Cursor (D: https://cursor.com/blog/agent-sandboxing).
  - Removal would break all three vendors at once. That is the real deprecation risk, and a
    shared one. Mitigation: keep the `Sandbox` trait backend-agnostic, and keep a VM/container
    backend in reserve (§5).
- App Sandbox does not fit. It is entitlement-based, applies to signed app bundles, and cannot
  take per-run path parameters. Cursor also evaluated and rejected it (D: Cursor blog above).

### What a profile can express (all P unless marked)

| Need | Expressible? | Probe result |
|---|---|---|
| Allow/deny file reads and writes by `subpath`, `literal`, `regex` | Yes | write outside WT → `Operation not permitted`; write inside OK |
| Later rule wins (deny after allow) | Yes | `(allow file-read*)` then `(deny file-read* (subpath OTHER))` blocks `cat` of other worktree |
| Re-allow a sub-path inside a denied tree | Yes | denied `REPOS_ROOT`, re-allowed `WT` and `GIT_COMMON` for reads |
| Parameters from the command line | Yes: `-D KEY=VAL` + `(param "KEY")`, `string-append` | used throughout |
| Deny Git metadata writes | Yes | `echo x > wt/.git` (worktree `.git` *file*) and `touch <common>/.git/worktrees/wt-a/HEAD` both denied, while reading them works |
| Network by **hostname** | **No** | `(remote ip "api.anthropic.com:443")` → `sandbox-exec: host must be * or localhost in network address` (exit 65). Same error for a literal IP `1.2.3.4:443` |
| Network by **loopback port** | Yes | `(allow network-outbound (remote tcp "localhost:18877"))`: port 18877 connects, 18878 refused |
| Network to a **Unix socket path** | Yes | `(allow network-outbound (literal "<abs>/p.sock"))`: `nc -U` connects. Note the macOS 104-byte `sun_path` limit: `nc: File name too long` for the deep scratch path |
| DNS | Blocked when `network*` is denied | `curl` → `Could not resolve host`. `dscacheutil -q host` returns nothing inside, an answer outside |
| Loopback-only server + client (tests) | Yes | `http.server` bound to 127.0.0.1 inside, `curl` to it → 200; `curl https://1.1.1.1` → refused |
| Syscall filtering | Yes, `syscall-unix` + `syscall-number` | `(deny syscall-unix (syscall-number 147))` makes Python `os.setsid()` raise `PermissionError` |
| Process exec / fork | `process-exec*`, `process-fork`; exec can be path-filtered (S, not probed) | |
| Signals limited to own sandbox | `(allow signal (target same-sandbox))` (D: Codex base policy) | |
| Mach service lookups | `(allow mach-lookup (global-name ...))` allow-list | All three CLIs' `--version`, curl over TLS, and node `fetch` over TLS work with only: opendirectoryd.libinfo, opendirectoryd.membership, trustd, trustd.agent, SecurityServer, ocspd, bsd.dirhelper |
| Keychain | Only coarsely: allow or deny `mach-lookup` of `com.apple.SecurityServer` (S). No per-item filtering. Not probed (brief forbids keychain reads) |
| launchd / LaunchServices escapes | Blocked in my profile | `launchctl submit` → exit 1, job never ran. `open -a TextEdit` → `_LSOpenURLsWithCompletionHandler() ... error -54`. Cause not isolated (UNVERIFIED whether launchd refuses sandboxed callers or a profile rule did it) |
| Path aliases (`/tmp` → `/private/tmp`) | **Pitfall** | A deny written with the `/tmp/...` spelling did **not** stop a write through `/private/tmp/...` (rc 0: fail-open). A deny on the canonical path did stop a write through the `/tmp` alias. **All path parameters must be canonicalized (realpath) before use**. Codex does the same (`normalize_path_for_sandbox`, D) |
| Nesting | **No** (see §4) | |

### Cannot express / cannot stop

- Hostnames or remote IPs (only `*` or `localhost`) (P).
- Per-keychain-item access (S).
- Resource limits: CPU, memory, disk quota, process count. Use `setrlimit` in the spawner,
  plus deadlines (S).
- Reads of data the run is allowed to read. If the agent can read its own credentials (§3),
  so can its Bash tool.
- Process-tree containment. Children inherit the sandbox, but not a kill domain (§6).

### Profiles used in probes (verbatim)

Implementer profile, `impl-strict.sb` (P: works with all three CLIs' `--version`, curl and
node via proxy). The `mach-lookup` list replaces the broad `(allow mach-lookup)` of `impl.sb`.

```scheme
(version 1)
(deny default)
(import "system.sb")
(allow process-fork process-exec*)
(allow process-info* (target same-sandbox))
(allow signal (target same-sandbox))
(allow sysctl-read)
(allow mach-lookup (global-name "com.apple.system.opendirectoryd.libinfo" "com.apple.trustd"
  "com.apple.trustd.agent" "com.apple.SecurityServer" "com.apple.ocspd"
  "com.apple.system.opendirectoryd.membership" "com.apple.bsd.dirhelper"))
(allow ipc-posix*)
(allow pseudo-tty)
(allow file-ioctl)
; reads: everything except other worktrees, secrets, harness state
(allow file-read*)
(deny file-read* (subpath (param "REPOS_ROOT")))
(allow file-read* (subpath (param "WT")) (subpath (param "GIT_COMMON")))
(deny file-read* (subpath (param "STATE"))
  (subpath (string-append (param "HOME") "/.ssh"))
  (subpath (string-append (param "HOME") "/.aws")))
; writes: worktree + task home only, never git metadata
(allow file-write* (subpath (param "WT")) (subpath (param "TASKHOME")) (literal "/dev/null")
  (literal "/dev/tty") (regex #"^/dev/ttys[0-9]+$"))
(deny file-write* (literal (string-append (param "WT") "/.git")) (subpath (param "GIT_COMMON")))
; network: only the harness proxy port on loopback
(allow network-outbound (remote tcp (string-append "localhost:" (param "PROXY_PORT"))))
```

Results under this profile (P): edit inside WT OK; write `.git` file denied; write
`GIT_COMMON/worktrees/wt-a/HEAD` denied; read `GIT_COMMON` allowed; `cat` of other
worktree's file denied; `ls ~/.ssh` denied; direct `curl https://example.com` fails at DNS.

Production profile changes needed beyond the probe:

- Make `file-read*` deny-lists data-driven: `~/.ssh`, `~/.aws`, `~/.config/gcloud`, `~/.kube`,
  `~/.docker/config.json`, `~/.netrc`, `~/.npmrc`, `~/.git-credentials`, `~/.gnupg`,
  `~/Library/Keychains`, browser profiles, other agents' homes.
- Add `(deny file-write-unlink (literal WT))` so the root itself cannot be replaced (D: Codex
  does this).
- Add a temp dir under the task home and set `TMPDIR` to it.

Verifier profile: same filesystem rules as the implementer, but the network section becomes:

```scheme
(allow network-bind (local ip "localhost:*"))
(allow network-inbound (local ip "localhost:*"))
(allow network-outbound (remote ip "localhost:*"))
```

or nothing at all for "no network". Probe: loopback server/client works; external IP refused.
Caveat (P): `bind(("0.0.0.0", port))` also succeeded. Inbound from other hosts was not tested.
Also, loopback-any lets tests reach any service the user runs on localhost (Postgres, the
colima Docker socket over TCP, the harness proxy). So "no network" should be the default,
with "loopback" a per-project opt-in.

Reviewer profile: implementer reads, writes limited to the task home and tmp (the CLI still
needs to write its session/config files), WT read-only, network = proxy port.

---

## 2. Existing tools

### OpenAI Codex CLI (Rust, Apache-2.0)

- License: Apache License 2.0 (D: https://github.com/openai/codex/blob/main/LICENSE).
- Seatbelt code lives in `codex-rs/sandboxing/src/` (D: https://github.com/openai/codex/tree/main/codex-rs/sandboxing/src).
  It contains `seatbelt.rs` and `seatbelt_base_policy.sbpl`, `seatbelt_network_policy.sbpl`,
  `seatbelt_preferences_policy.sbpl`, `seatbelt_read_only_platform_defaults.sbpl`, plus Linux
  `bwrap.rs`, `landlock.rs` and `linux_pid_namespace.rs` in the same crate.
- Design worth copying (D: seatbelt.rs):
  - Hard-coded `/usr/bin/sandbox-exec`, so a hostile one on `PATH` cannot be used.
  - Writable roots are passed as `-DWRITABLE_ROOT_n=...`.
  - Protected subpaths are carved out with `(require-not (literal|subpath ...))`, and both
    forms are used so a path is protected before it exists.
  - `deny file-write-unlink` on each root and on protected ancestors (no rename-escape).
  - Canonicalizes paths and system aliases. Rejects symlinked components inside writable roots.
  - Network: reads proxy env vars and keeps only loopback ports. Emits
    `(allow network-outbound (remote ip "localhost:{port}"))` and adds the TLS trust service.
    **Fails closed if no loopback port is found.** Unix sockets are allow-listed by `subpath`.
- Base policy is `(deny default)` with an explicit sysctl allow-list, `mach-lookup` limited to
  opendirectoryd.libinfo and PowerManagement, pty rules, and signals only to the same sandbox
  (D: https://raw.githubusercontent.com/openai/codex/main/codex-rs/sandboxing/src/seatbelt_base_policy.sbpl).
  The file says it was "inspired by Chrome's sandbox policy" and has no license header. The
  repo is Apache-2.0.
- Network add-on: trustd/SecurityServer/ocspd, networkd, DNSConfiguration, configd lookups,
  `net.routetable` sysctl, and an `AF_SYSTEM` socket rule (D: `seatbelt_network_policy.sbpl`
  raw URL in the same directory).
- Reuse verdict: **port, don't depend.** `create_seatbelt_command_args` is `pub` (D), but the
  crate imports `codex_protocol`, `codex_network_proxy`, `codex_utils_absolute_path` and
  `codex_uds` (D). Pulling that into our core couples us to Codex internals. Whether the crate
  is published on crates.io is UNVERIFIED. Copy the `.sbpl` fragments and the canonicalization
  rules into our crate with Apache-2.0 attribution (NOTICE file). Write our own profile
  generator, about 300 lines.
- Local: `codex sandbox -- <cmd>` exists in 0.166.0 and wraps sandbox-exec. Its help text
  says "Full command args to run under seatbelt" (P). It cannot be used for a nested run (§4).

### Anthropic `sandbox-runtime` (`srt`), which Claude Code uses

- Repo https://github.com/anthropic-experimental/sandbox-runtime, **Apache-2.0**,
  "Beta Research Preview". TypeScript library and CLI (`SandboxManager.wrapWithSandbox`) (D).
- macOS: dynamically generated Seatbelt profile via `sandbox-exec`. Writes are denied by
  default, reads allowed by default, and globs are supported (D).
- Network (D):
  - Seatbelt allows only one localhost port. An **HTTP proxy** handles HTTP/HTTPS and a
    **SOCKS5 proxy** handles other TCP. Both run on the host.
  - `allowedDomains` / `deniedDomains` take wildcards and an optional `:port`. Deny wins.
  - Before dialing, the proxy resolves the name and **drops loopback, link-local, multicast,
    cloud-metadata and the host's own interface addresses** (`deniedResolvedAddresses`).
  - `allowLocalBinding`, `allowUnixSockets` and `allowAppleEvents` are all off by default.
- Caveats on the page (D):
  - `enableWeakerNetworkIsolation` opens a trustd exfil path.
  - `allowAppleEvents` lets `open` launch unsandboxed apps.
  - A per-command network list needs a ≥128-bit `commandId`.
- Claude Code docs (D: https://code.claude.com/docs/en/sandboxing):
  - The sandbox wraps Bash/PowerShell/Monitor commands only. **File tools (Read, Edit,
    Write), WebFetch, MCP servers and hooks run outside the sandbox.**
  - It is **off by default** (`sandbox.enabled`). `failIfUnavailable` and
    `allowUnsandboxedCommands` exist.
  - The proxy checks the hostname and does not terminate TLS by default.
  - Tools that ignore proxy env vars fail. UDP/QUIC/ICMP are blocked.
- Reuse verdict: TypeScript, so nothing links into Rust. **Reimplement** the proxy design in
  Rust: CONNECT + optional SOCKS5, allow-list with deny precedence, resolved-address deny set.
  Read its profile generator as a reference for macOS edge cases (pty, Apple Events, trustd).

### Cursor CLI

- `cursor-agent --help` shows `--sandbox <mode>` with choices `enabled`/`disabled` (P).
- Cursor uses Seatbelt via `sandbox-exec` on macOS and generates the policy at runtime from
  workspace, admin settings and `.cursorignore`. Linux uses Landlock + seccomp (D:
  https://cursor.com/blog/agent-sandboxing). The post covers terminal commands. It does not
  say whether file-edit tools are confined (D, same page).
- Configuration lives in `~/.cursor/sandbox.json` and `<ws>/.cursor/sandbox.json` (D:
  https://cursor.com/docs/reference/sandbox; the `.md` variant of that URL returned 404).
  - Fields include `type`: `workspace_readwrite` | `workspace_readonly` | `insecure_none`.
  - `networkPolicy` has `default`, `allow` and `deny`.
  - It injects `CURSOR_SANDBOX` into sandboxed processes.
- Forum report (S, UNVERIFIED): the sandbox breaks zsh on macOS 26 because zsh reads hw.*
  sysctls that are not allow-listed (https://forum.cursor.com/t/sandbox-fails-to-initialize-for-zsh-on-macos-26-tahoe-darwin25-zsh-5-9-reads-unwhitelisted-hw-sysctls/161023).
  This is relevant to us: allow `sysctl-read` broadly, as my probe did, or test zsh explicitly.
- Reuse verdict: closed source. Only its flags matter: `--sandbox disabled` under our sandbox.

---

## 3. Network allow-listing by hostname

- **Seatbelt cannot filter by hostname or remote IP** (P: error above). The only expressible
  remotes are `*` and `localhost`.
- **Standard design (used by srt, Claude Code and Codex):**
  1. The profile denies all network except `(allow network-outbound (remote tcp
     "localhost:<P>"))`, or a Unix socket path under a short directory (sun_path ≤ 104 bytes).
     DNS is therefore impossible inside (P).
  2. The harness runs a CONNECT proxy outside the sandbox on 127.0.0.1:P, one per run, with a
     per-run port or a per-run random credential in the proxy URL.
  3. The child gets `HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY` and `NO_PROXY=` (empty), plus
     `NODE_USE_ENV_PROXY=1` for Node.
     - Probe (P): Node 24 `fetch` ignored `HTTPS_PROXY` without it (`ENOTFOUND`) and worked
       with it (200).
     - Claude Code honours `HTTPS_PROXY` (D, recorded in `specs/claude-adapter`).
     - Codex (reqwest) and cursor-agent proxy support is UNVERIFIED. Probe it live in the
       first adapter task.
  4. The proxy allows `CONNECT host:443` only for hosts in the stage's list, answers `403`
     otherwise, and logs every decision to the audit trail. After resolving, it refuses
     private, loopback, link-local and metadata addresses (srt's rule, D).
- **Probe (P)** with a 40-line Python CONNECT proxy (`1a-probes/proxy.py`, ALLOW={example.com}),
  run from inside the implementer profile:
  - `curl https://example.com` via proxy → 200 (`ALLOW CONNECT example.com:443` logged).
  - `https://example.org` → `CONNECT tunnel failed, response 403` (logged DENY).
  - `curl --noproxy '*' https://93.184.215.14` → `Couldn't connect` (Seatbelt blocks the bypass).
  - Node `fetch` worked only with `NODE_USE_ENV_PROXY=1`.
- **What this cannot stop:**
  1. Exfiltration *to an allowed host*. With subscription logins the agent holds a token for
     the vendor API, and Bash can post data there. The harness never sees inside TLS, because
     we will not MITM a subscription CLI.
  2. Domain fronting / shared CDNs. CONNECT checks the name the client states; SNI may differ,
     and an allowed host on a CDN can route to others. Peeking at SNI and requiring it to match
     the CONNECT host reduces this.
  3. Timing and size side channels.
  4. Anything reachable through allowed local IPC: allowed mach services such as trustd
     (srt documents a trustd exfil path under its weaker mode, D), allowed Unix sockets, or
     loopback services if loopback is open.
  5. Tools that don't speak proxies simply fail (git over SSH, DB drivers). That is a
     breakage, not a leak.
  6. A compromised proxy is a full egress break. Keep it tiny, in-process in the harness, and
     run it outside every sandbox.
- **Vendor host lists** belong to the adapter tracks and must be probed live, not guessed.
  Claude: `api.anthropic.com` is documented (D, via `specs/claude-adapter` C-6). Subscription
  OAuth refresh may need `claude.ai` / `console.anthropic.com` / `platform.claude.com`
  (UNVERIFIED). Codex: `chatgpt.com`/`api.openai.com`/`auth.openai.com` (UNVERIFIED).
  Cursor: default endpoint `https://api2.cursor.sh` (P, from `--help`), others UNVERIFIED.
  The proxy should log denied hosts in a dry run so the lists can be learned from evidence.

---

## 4. Nested sandboxing

- **Probed (P):** `sandbox-exec` inside any outer profile that restricts *anything* fails with
  `sandbox-exec: sandbox_apply: Operation not permitted` (exit 71). Tested outers:
  - `(deny default)` + allows
  - `(allow default)(deny network*)(deny file-write* (subpath "/Users"))`
  - `(allow default)(deny system*)`
  - `(allow default)(deny system-privilege)`
  - `(allow default)(deny mach-priv*)`
  
  Only a pure `(allow default)` outer let the inner apply. So nesting is impossible in practice.
- `codex sandbox -- /usr/bin/true` under the implementer profile printed `WARNING: proceeding,
  even though we could not create PATH aliases` and then `sandbox_apply: Operation not
  permitted` (P). Codex's inner Seatbelt would fail every tool call.
- **Safe configuration: the outer harness sandbox is the only boundary, and every inner
  sandbox is off.** Claude's file tools run outside its own sandbox anyway (D), so the inner
  sandbox could never have been the boundary.

| Agent | Flag(s) to disable inner sandbox | Grade |
|---|---|---|
| Claude Code | Off by default. Pass `--settings '{"sandbox":{"enabled":false}}'` explicitly, so a user or project setting cannot turn it on and break Bash. Do not set `failIfUnavailable: true`. | D (code.claude.com/docs/en/sandboxing) |
| Codex | `codex exec --sandbox danger-full-access` (`-s`) with `-c approval_policy="never"`, or `--dangerously-bypass-approvals-and-sandbox`. Help text: "Intended solely for running in environments that are externally sandboxed". Also needs `--cd <worktree>`. Reviewer read-only is then enforced by *our* profile, not by `-s read-only`. | P (help) / D (https://learn.chatgpt.com/docs/sandboxing, reached via redirect from developers.openai.com/codex/concepts/sandboxing) |
| Cursor | `--sandbox disabled`. Also avoid `--worktree`, which writes to `~/.cursor/worktrees/...`; give `--workspace <wt>` instead. | P (help) |

- Policy questions for the adapters:
  - Disabling an inner sandbox also disables that agent's own command prompting. Approval
    behaviour must then come from flags (`--permission-mode`, `approval_policy=never`,
    `--force`), and the harness, not the agent, is the gate.
  - The harness should **verify at run start** that the inner sandbox is really off. Run a
    canary command and confirm it executes. Otherwise fail closed with a clear error instead
    of a confusing tool failure.
- Not a nesting problem, but related: Claude Code's `--bare` mode reads only
  `ANTHROPIC_API_KEY`/`apiKeyHelper`, and its help text says OAuth and keychain are never read
  (P). So the subscription-login decision rules out `--bare` for Claude, and the adapter loses
  `--bare`'s isolation from user settings. Flag for the adapter track.

---

## 5. Alternatives and when each is warranted

| Option | What it gives | Cost / limits | Use when |
|---|---|---|---|
| **Seatbelt (recommended default)** | Native speed. Runs Xcode, `swift build`, iOS simulator tooling, macOS-only binaries. Works with subscription logins in keychain/home. | Deprecated API (§1). Path-based only. No resource limits, no PID namespace. Coarse keychain access. | All stages, by default |
| **Apple `container`** (Apache-2.0, Swift, needs macOS 26 + Apple silicon; each container runs in a lightweight Linux VM) | Real kernel boundary: own PID/net/fs namespaces, disposable | Linux only, so **no Xcode/macOS builds**. Subscription-login CLIs would need their credentials inside the VM (copying them breaks "harness never handles tokens"). Project is "under active development", with breaking changes possible across majors. | Verifier runs for Linux-targeted projects (Node/Rust/Python services, Vercel/Cloudflare code) where the strongest isolation matters. Later: OpenRouter tool-using agents with API keys. D: https://github.com/apple/container |
| **Docker on colima** (installed: 0.10.3, Virtualization.framework, virtiofs mounts, P) | Same as above, with a mature image ecosystem. `--network none` and egress control via a proxy container. | Same Linux-only limit. The docker socket is root-equivalent on the VM, so never expose it to a sandboxed run. VM must be running, and adds startup latency. | Same as `container`, if the user prefers existing tooling. Also a fallback if Seatbelt ever disappears. |
| Per-task macOS user account | Real uid separation; kill-by-uid is trivial | Needs admin rights to create users. Keychain and subscription logins are per user, so it breaks the login decision. | Not recommended |
| Full macOS VM (Virtualization.framework, macOS guest) | Xcode inside a real boundary | Heavy (GBs, minutes). Apple licence limits to 2 concurrent macOS VMs (S). Logins live inside the VM. | Only if a threat model requires it later |

Decision rule: macOS-target projects (Xcode/Swift/macOS-only tools) must use Seatbelt.
Linux-target projects may opt into the VM backend for the verifier stage, behind the same
trait. Agent stages stay on Seatbelt while they depend on host subscription logins.

---

## 6. Process-tree termination on macOS

Facts:
- No PID namespaces or cgroups on macOS.
- `kqueue` `NOTE_TRACK`/`NOTE_CHILD` are "no longer supported as of 10.5" (P: SDK `sys/event.h`).
- A sandboxed child that double-forks with `setsid()` **escapes `killpg`** (P). After
  `killpg(leader, SIGKILL)` the grandchild was still alive with PPID 1 and its own PGID.
- It **keeps the sandbox**: `sandbox_check(pid, NULL, 0)` returned 1 for it, 0 for my shell (P).
  But "is sandboxed" alone cannot identify *our* run, since other apps and other runs are
  sandboxed too. Path-specific `sandbox_check` against another PID returned 1 for everything,
  including unsandboxed processes, so it is unusable (P).

Options:

1. **Deny `setsid`/`setpgid` in the profile** (`syscall-unix`, numbers 147 and 82). Probe: it
   blocks `setsid` (P). Then `killpg` is complete, apart from launchd-spawned work.
   - Risk: Node `detached: true`, shells with job control and some test runners call these
     and would break (S). Codex likely uses `setpgid` for its own children (UNVERIFIED).
   - Make it a per-profile flag; turn it on only after the agents are probed live.
2. **Responsibility-PID sweep (recommended).** On macOS every process has a "responsible
   process", which children inherit regardless of reparenting. A spawn attribute
   (`responsibility_spawnattrs_setdisclaim`) makes the spawned process its own responsible root.
   - Both functions are exported from `/usr/lib/system/libquarantine.dylib` (P:
     `dyld_info -exports`) together with `responsibility_get_pid_responsible_for_pid`.
   - These are **private SPI** (not in public headers). Chromium and Terminal-style apps use
     them (S).
   - Probe (P, `1a-probes/resp2.c`):
     1. `posix_spawn` a supervisor (`/bin/sh`) with `POSIX_SPAWN_SETSID` + disclaim.
     2. It starts `sandbox-exec ... python` which double-forks + `setsid`s.
     3. After 4 s, enumerate `proc_listallpids` and SIGKILL every pid whose responsible pid
        equals the supervisor's. Loop until zero.
     4. Result: "killed 2 members / sweep complete", and `pgrep` found **no survivors**.
   - **Caveat found (P):** once the supervisor had died, the escaped grandchild reported
     *itself* as its responsible pid, so the sweep found nothing (`resp.c` run). **The
     supervisor must stay alive until the sweep completes.**
3. Polling ancestry (`proc_listchildpids` recursively). Racy: a process can fork and the
   parent exit between polls. Use only as a fallback when the SPI is missing.

Recommended termination protocol (the worktree lock is released only at step 6):

1. The harness spawns a tiny **supervisor**: the harness binary itself in a `__supervise`
   mode, unsandboxed.
   - Spawn flags: `POSIX_SPAWN_SETSID` + responsibility disclaim.
   - The supervisor `posix_spawn`s `/usr/bin/sandbox-exec -f <profile> -D... -- <argv>` as
     its child, in its own process group.
   - Rlimits are set before exec (`RLIMIT_NPROC`, `RLIMIT_CPU`, `RLIMIT_FSIZE`, `RLIMIT_NOFILE`).
     `RLIMIT_NPROC` is per-uid, so it only caps fork bombs coarsely (S).
   - The supervisor holds the stdio pipes and reports exit status.
2. On completion, deadline or cancel: `killpg(child_pgid, SIGTERM)`, then a grace period
   (e.g. 5 s), then `killpg(SIGKILL)`.
3. Sweep: loop { list all pids → kill -9 those whose responsible pid == supervisor pid →
   sleep 50 ms } until one empty pass. Then do a **second empty pass** after 200 ms to catch
   in-flight forks. Bounded by a timeout. On timeout, mark the task `QUARANTINED`, keep the
   lock, and alert the human. Do not release.
4. Optional extra evidence: no process has an open file or cwd under the worktree (`proc_pidinfo`
   `PROC_PIDVNODEPATHINFO` over candidates). Report, don't block.
5. The supervisor exits.
6. The harness records "tree empty" in the audit trail (intent → act), then releases the
   worktree lock.

Launchd escapes (`launchctl submit`, `open`, XPC services that spawn work) are outside any
process tree. Keep them blocked in the profile: deny `appleevent-send`, keep the
`mach-lookup` allow-list tight, deny writes to `~/Library/LaunchAgents`. Both escape attempts
failed under my profile (P). Add both as acceptance tests.

If the private SPI disappears in a future macOS, `doctor` must detect it (`dlsym` failure) and
**fail closed** for agent stages, or fall back to option 1 for verifier stages.

---

## 7. Rust interface

Keep the surface tiny. Policy in, handle out. The backend owns profile rendering, the proxy
port binding and cleanup. All types are `serde`-serializable so runs can be replayed and
audited.

```rust
/// What a run may touch. Backend-neutral; paths are canonicalized by `SandboxProfile::new`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SandboxProfile {
    pub stage: StageKind,                 // Implement | Review | Verify | Advise
    pub worktree: CanonPath,              // cwd; writable iff `worktree_writable`
    pub worktree_writable: bool,
    pub task_home: CanonPath,             // always writable: HOME/TMPDIR/agent config dir
    pub extra_read: Vec<CanonPath>,       // e.g. toolchains, the Git common dir (read-only)
    pub extra_write: Vec<CanonPath>,      // rare; e.g. a build cache owned by this task
    pub deny_read: Vec<CanonPath>,        // other worktrees root, state store, ~/.ssh, ~/.aws, ...
    pub deny_write: Vec<CanonPath>,       // .git file, Git common dir, hooks; wins over allows
    pub network: NetworkPolicy,
    pub env: EnvPolicy,                   // explicit allowlist; proxy vars injected by backend
    pub limits: Limits,                   // wall deadline, grace, rlimits, max output bytes
    pub process: ProcessPolicy,           // deny_new_session: bool (setsid/setpgid), etc.
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetworkPolicy {
    None,
    Loopback,                                    // verifier opt-in: localhost only
    Proxy { allow_hosts: Vec<HostPattern> },     // implementer/reviewer: vendor hosts only
}

pub struct SpawnSpec { pub argv: Vec<OsString>, pub stdin: Stdio, pub stdout: Stdio, pub stderr: Stdio }

pub trait Sandbox: Send + Sync {
    /// Capability check run by `doctor` and before every run; Err => nothing runs (fail closed).
    fn probe(&self) -> Result<BackendInfo, SandboxError>;
    /// Start `spec` under `profile`. Returns only once the child is confined (or Err).
    fn spawn(&self, profile: &SandboxProfile, spec: SpawnSpec) -> Result<Box<dyn SandboxedChild>, SandboxError>;
}

pub trait SandboxedChild: Send {
    fn id(&self) -> RunId;
    fn stdio(&mut self) -> ChildStdio;                       // async pipes
    fn wait(&mut self) -> BoxFuture<'_, Result<ExitReport, SandboxError>>;
    /// TERM → grace → KILL → sweep until verified empty. Ok only if no descendant survives.
    fn terminate(&mut self, grace: Duration) -> BoxFuture<'_, Result<TreeGone, SandboxError>>;
    fn egress_log(&self) -> Vec<EgressDecision>;             // proxy allow/deny records for the audit trail
}
```

- `TreeGone` is a zero-sized proof token. The worktree-lock API takes `TreeGone` by value, so
  the lock **cannot be released in code** without a completed sweep.
- `ExitReport` carries the exit code or signal, timing, whether the run was killed by the
  deadline, and sandbox-violation hints.
- Seatbelt denials are logged by the kernel to the unified log
  (`log stream --predicate 'sender=="Sandbox"'`) (S). Collecting them is optional diagnostics.

### macOS backend (`SeatbeltSandbox`)

- Renders an `.sb` string from the profile, using a static base fragment (Codex-derived,
  attributed) plus generated rules. Passes paths only via `-D` params, never by string
  interpolation (no profile injection through odd path characters).
- Spawns supervisor → `/usr/bin/sandbox-exec -p <profile> -D... -- argv` (§6).
- For `NetworkPolicy::Proxy`, starts an in-process tokio CONNECT proxy on 127.0.0.1:0 for this
  run, injects `HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY`/`NODE_USE_ENV_PROXY=1` and empty
  `NO_PROXY`, and fails closed if the bind fails.
- `probe()` checks:
  - that a canary profile denies a write and a network connect;
  - that `/usr/bin/sandbox-exec` exists and its signature is Apple's (`codesign -v` (S));
  - that `dlsym` finds the responsibility functions;
  - and records the macOS version.

### Linux backend (`BwrapSandbox`), mapping from `harness/local/sandbox.mjs`

| Profile field | bubblewrap |
|---|---|
| worktree / `worktree_writable` | `--bind` or `--ro-bind <wt> /work`, `--chdir /work` |
| `deny_write` `.git` | `--ro-bind /dev/null /work/.git` (as the pilot does). The common dir is simply not mounted, or mounted `--ro-bind` |
| `deny_read` | A path that is never mounted is unreadable. Linux is allow-list by construction (`--ro-bind` of runtime paths only) |
| task_home | `--bind <home> /home/task`, `HOME`/`TMPDIR` set; pilot uses `--tmpfs /tmp` |
| env | `--clearenv` + `--setenv` allowlist (pilot) |
| `NetworkPolicy::None` | `--unshare-all` (new netns, loopback only) |
| `NetworkPolicy::Loopback` | same, since the netns has its own lo; the host's loopback is unreachable (stronger than macOS) |
| `NetworkPolicy::Proxy` | netns + a Unix socket bind-mounted in, bridged by a small in-sandbox forwarder to `127.0.0.1:P` (srt's approach, S), or `--share-net` + seccomp. Open item in `specs/claude-adapter` already |
| process tree | `--unshare-pid --die-with-parent --new-session`; killing the bwrap init kills the namespace, so `TreeGone` is cheap and exact |
| limits | `prlimit`/cgroup v2 scope via systemd-run (optional) |

Path semantics differ: Linux rewrites paths to `/work`, while macOS keeps real paths. The
trait must not assume either. Adapters receive the in-sandbox worktree path from
`SandboxedChild` / `BackendInfo`, not by recomputing it.

---

## 8. Other findings the core needs to know

- **Subscription logins vs. isolation.** Claude Code keeps OAuth credentials in the macOS
  Keychain (S). Codex keeps `~/.codex/auth.json` or the OS keyring (S). So:
  1. The implementer/reviewer profile must allow `mach-lookup com.apple.SecurityServer`, plus
     read+write of the agent's own config dir: `~/.claude*`, `~/.codex`, `~/.cursor`. That
     dir is also where session transcripts are written.
  2. Shell commands run by the agent can then read that agent's own credential. The `security`
     CLI path is UNVERIFIED and was not probed, per the brief. The only remaining exfil
     destination is the allowed vendor hosts. **This is accepted residual risk of the
     subscription decision.** Document it, and keep the host lists minimal.
  3. Redirecting `HOME` to the task home (as in my probe) means the CLIs will **not find their
     logins**. Each adapter must point only the agent's config dir at the real one, or use
     `CLAUDE_CONFIG_DIR`/`CODEX_HOME` with a login, which the human must do once per config
     dir. Decide per adapter after a live probe.
- **Shared Git objects are readable.** Granting read on the Git common dir (needed for
  `git status`/`diff` in a worktree) lets a run read **committed** objects of other tasks'
  branches. Uncommitted files in other worktrees stay unreadable (P). If that matters, give
  each task a separate clone (`git clone --no-hardlinks` from a bare mirror) instead of
  `git worktree`.
- **Fail-closed checks** for `doctor`: sandbox-exec present; canary deny works; nesting
  canary (to detect an outer sandbox, e.g. the harness itself run inside Claude Code's
  sandbox, in which case refuse); SPI present; proxy bind works.

---

## Sources

- https://code.claude.com/docs/en/sandboxing (D, page truncated at 100k chars; read the first part)
- https://github.com/anthropic-experimental/sandbox-runtime (D, README truncated at 100k chars; macOS limitations section not read)
- https://github.com/openai/codex/tree/main/codex-rs/sandboxing/src, `seatbelt.rs`,
  `seatbelt_base_policy.sbpl`, `seatbelt_network_policy.sbpl` (raw.githubusercontent.com), LICENSE (D)
- https://learn.chatgpt.com/docs/sandboxing (D; redirected from developers.openai.com/codex/concepts/sandboxing; it does not document `network_access` or the bypass flag)
- https://cursor.com/blog/agent-sandboxing, https://cursor.com/docs/reference/sandbox (D); `/docs/reference/sandbox.md` returned 404
- https://github.com/apple/container (D)
- Cursor forum thread on zsh/macOS 26 (S)
