//! Static description of what a runtime adapter supports.

use serde::{Deserialize, Serialize};

use crate::types::{ModelFamily, Role, RuntimeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnSandbox {
    MustDisable,
    Compatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredOutput {
    None,
    JsonSchemaFile,
    JsonSchemaInline,
}

/// Compiled into each adapter as a `const`. Serialize-only: the `&'static`
/// fields cannot be deserialized from owned input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Capabilities {
    pub runtime: RuntimeKind,
    pub model_family: ModelFamily,
    pub pinned_version: &'static str,
    pub roles: &'static [Role],
    pub structured_output: StructuredOutput,
    pub session_resume: bool,
    pub reports_usage: bool,
    pub own_sandbox: OwnSandbox,
}
