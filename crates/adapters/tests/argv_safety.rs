#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use harness_adapters::{AgentRunSpec, adapter_for};
use harness_core::types::{Role, RuntimeKind};

const FORBIDDEN: [&str; 6] = [
    "--bare",
    "--dangerously-skip-permissions",
    "--allow-dangerously-skip-permissions",
    "--add-dir",
    "bypassPermissions",
    "--dangerously-bypass-approvals-and-sandbox",
];

#[test]
fn no_adapter_argv_contains_a_forbidden_flag() {
    for kind in [RuntimeKind::Claude, RuntimeKind::Codex] {
        for role in [Role::Implementer, Role::Reviewer] {
            let spec = AgentRunSpec {
                role,
                prompt: "p".into(),
                worktree: PathBuf::from("/wt"),
                config_dir: PathBuf::from("/cfg"),
                mcp_config: PathBuf::from("/cfg/mcp.json"),
                review_schema: Some(PathBuf::from("/cfg/schema.json")),
                max_turns: 5,
            };
            let plan = adapter_for(kind).plan(&spec).unwrap();
            for arg in &plan.argv {
                for bad in FORBIDDEN {
                    assert!(
                        !arg.contains(bad),
                        "{kind:?}/{role:?} argv has {bad}: {arg}"
                    );
                }
            }
        }
    }
}
