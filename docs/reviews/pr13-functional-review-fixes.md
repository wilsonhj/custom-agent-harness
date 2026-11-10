# PR13 functional fixes: independent review brief

Base: `b900dfa`; branch: `codex/review-fixes-pr13`. Review this commit's diff separately from the parallel security branch, then review the combined head. This is an implementation brief, not an approval.

## Intended behavior

1. Preserve canonical Node discovery for tools and verification independently of native/JavaScript Codex invocation. `codex_uses_node` selects only the argv prefix.
2. Enumerate the committed packet identically from the repository root and nested repository paths with `ls-tree --full-tree`.
3. Make status/list/cancel reach the live `eng run` owner. `harness-engine` owns the private transient Unix socket; the CLI only renders returned snapshots/receipts. `Store::open` remains exclusive. Only status and human Cancel are accepted; no decision/approval or general command RPC exists.

## Review focus

- Native/shell Codex launches directly with Node still on PATH/read-allowed; JS Codex and login retain their Node prefix.
- Packet bytes and digest remain bound to the base commit, even for a nested cwd.
- Cancellation follows the existing token/supervisor/TreeGone path. The pinned worker future is awaited after a listener failure; it is not dropped while processes are running. A committed cancellation can be visible before cleanup finishes, with the worktree still locked.
- Socket directory 0700; endpoint pointer 0600; size/time bounds; malformed/partial requests; cleanup after exit/crash; long state paths; owner-exit/client-connect races.
- The endpoint exists only during an explicit foreground run. No daemon, second writer, credentials, live agents, or containment/egress bypass was introduced. Sources: design §§5.1, 5.4, 5.5, 6.1 and R-11.

## Evidence

All three reported functional regressions were observed failing before implementation; the additional native login regression and a concurrent snapshot sequence regression also failed before their fixes. Details are in `specs/rust-spine/handoff.md`. Rust workspace: **246 passed, 5 existing ignores**. Conformance: **13 passed**. Dependency direction, formatting, clippy, workspace build and JS syntax checks passed. Node 24.20.0 was selected explicitly; fake-agent sandbox tests ran outside the tool's outer sandbox to avoid nested Seatbelt failure.

The existing Linux Node pilot suite is not green on this macOS host: **44 passed, 44 failed, 2 skipped**. Failures stem from system runtime directory restrictions and Linux Bubblewrap requirements. Neither pilot source nor pilot tests changed. Full local output is `/private/tmp/pr13-functional-npm-test.log`.

## Assumptions and limits

The state owner is trusted, and file permissions plus the harness sandbox protect the private control endpoint. A transient foreground-run status/cancel endpoint stays within the CLI slice; general command transport and the daemon remain deferred. No authenticated runtime compatibility or stronger containment claim follows from fake-agent evidence. Parallel security work may intentionally gate these production flows; do not add bypass flags to preserve positive fixtures. The parent session owns integration and independent external review.

## Named Node pilot failures on this host

- bug2a: an ignored file inside the packet directory still yields a successful run
- bug2b: a packet file with mode 0600 in the source still yields a successful run
- bug3: a candidate file with mode 0o654 passes the snapshot check
- bug4: reviewer patch ends with a newline and applies with git apply --check
- bug6b: a verifier/reviewer argv containing {workspace} is rejected
- bug6c: a decision statement with a duplicate "decision" key is rejected
- bug7: an ignored worker artifact fails the run and the error names the path
- bug8: a packet file missing from the commit is rejected even if present and ignored on disk
- bug11a: a reviewer run by relative path cannot execute a worker-written review
- bug11b: a verifier/reviewer argument under the sandbox /work path is rejected
- bug11c: verifier options that do not name candidate files still run
- bug11d: an inline script longer than a filename is not treated as a candidate path
- bug12: extensionless Node entrypoints cannot execute candidate files (parent of the two failing fixtures)
- fixture: verifier: review
- fixture: reviewer: review
- bug13: absolute candidate aliases are rejected at configuration load
- bug13: trusted absolute arguments and non-path operands still run
- full local flow: separate processes, evidence, fixture human signature, restart and handoff
- reject decision is distinct from accept and is durably recorded
- unsigned, wrong-principal and wrong-namespace decisions are rejected
- a non-canonical decision statement cannot flip reject into accept
- signed decision bound to another candidate or evidence is rejected
- another run cannot consume a valid signature
- worker nonzero exit retains failure evidence and blocks decision
- verifier failure retains failure evidence and blocks decision
- worker timeout retains failure evidence and blocks decision
- excessive output retains failure evidence and blocks decision
- invalid reviewer JSON retains failure evidence and blocks decision
- stale reviewer result retains failure evidence and blocks decision
- review asks for changes retains failure evidence and blocks decision
- passing review with findings retains failure evidence and blocks decision
- worker packet rewrite retains failure evidence and blocks decision
- verifier mutates candidate retains failure evidence and blocks decision
- altered candidate after verification invalidates the decision request
- ignored files cannot influence verified bytes without being in the candidate commit
- Git assume-unchanged flags cannot hide modified candidate bytes
- content-addressed evidence corruption is detected on restart and import
- modified trust/config/tools require a new run
- changed configuration and signer list invalidate a pending decision
- worktree, config, trust path and packet preparation boundaries
- prepared candidate may not change before execution
- exclusive lease prevents a second controller and interrupted jobs are not replayed
- sandbox argv is offline, minimally mounted, and does not expose controller state
- default CLI execution fails closed when isolation is unavailable
