# Glossary

Domain terms used in code and specs. Keep entries short; link the module that owns each.

**Open question**: what a task is waiting for a human to answer. It is either spec approval
(the task is `Planned`) or a human gate (the stage run is `AwaitingHuman`). At most one is
open at a time. Owned by `crates/core/src/gate.rs` (`OpenQuestion`).

**Human gate**: why a stage run stopped to wait for a human. One of `ReviewClean`,
`RepairLimitReached`, `LimitExhausted` or `LostOnRestart` (`crates/core/src/stage.rs`).

**Subject**: the packet digest or commit an answer is bound to. Spec approval and answers
before any checkpoint bind the packet digest. Other answers bind the last checkpoint's
commit (`gate::subject`).

**Answer**: a human's response to the open question: a decision kind (`approve-spec`,
`accept`, `reject`, `send-back`) or `run`, which retries the work a gate stopped. Legal
answers per question:

| Open question | Legal answers |
|---|---|
| Spec approval | approve-spec |
| ReviewClean | accept, reject, send-back |
| RepairLimitReached | reject, send-back |
| LimitExhausted | run, reject |
| LostOnRestart | run, reject |
