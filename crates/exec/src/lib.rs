//! Process supervision, sandbox and Git. The only crate that spawns processes.
//!
//! Module ownership in the rust-spine plan: `plan` (shared, integration commit),
//! `supervisor` + `shim` (Task 5), `sandbox_macos` + `egress` (Task 6),
//! `workspace` + `hostgit` + `secretscan` (Task 7).

pub mod egress;
pub mod hostgit;
pub mod plan;
pub mod sandbox_macos;
pub mod secretscan;
pub mod shim;
pub mod supervisor;
pub mod workspace;

pub use plan::{LaunchPlan, TreeGone};
