//! Reviewer output: the raw shape a model emits, and the validated shape the
//! harness keeps.

use std::cmp::Ordering;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::CommitSha;
use crate::types::RuntimeKind;

/// Maximum characters kept from a finding's `claim`.
pub const MAX_CLAIM_CHARS: usize = 2_000;

/// Ordered `Critical > High > Medium > Low`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

impl Severity {
    fn rank(self) -> u8 {
        match self {
            Severity::Critical => 3,
            Severity::High => 2,
            Severity::Medium => 1,
            Severity::Low => 0,
        }
    }
}

impl Ord for Severity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank().cmp(&other.rank())
    }
}

impl PartialOrd for Severity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingCategory {
    Correctness,
    Security,
    SpecMismatch,
    Tests,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    ChangesRequested,
}

/// A repo-relative location. Lines are 1-based and inclusive; `0..0` means
/// the whole file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Location {
    pub path: String,
    pub line_start: u32,
    pub line_end: u32,
}

/// One finding as the reviewer model emits it. Untrusted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RawFinding {
    pub severity: Severity,
    pub category: FindingCategory,
    pub file: Option<String>,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub claim: String,
    pub failure_scenario: String,
    pub recommendation: String,
    pub suggested_check: Option<String>,
    pub confidence: f32,
}

/// A review as the reviewer model emits it. Untrusted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RawReview {
    pub verdict: Verdict,
    pub summary: String,
    pub findings: Vec<RawFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub blocking: bool,
    pub category: FindingCategory,
    pub location: Option<Location>,
    pub claim: String,
    pub failure_scenario: String,
    pub recommendation: String,
    pub reviewed_commit: CommitSha,
    pub reviewer: RuntimeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Review {
    pub verdict: Verdict,
    pub summary: String,
    pub findings: Vec<Finding>,
}

/// Why a raw review was refused. `finding` is the 1-based index (`F<n>`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReviewRejected {
    #[error("finding F{finding}: path {path:?} is not a repo-relative path")]
    PathOutsideRepo { finding: usize, path: String },
    #[error("verdict is pass but finding F{finding} is blocking")]
    PassWithBlocking { finding: usize },
    #[error("finding F{finding}: invalid line range")]
    BadLineRange { finding: usize },
    #[error("finding F{finding}: confidence is not within 0.0..=1.0")]
    ConfidenceOutOfRange { finding: usize },
}

/// Lexical check only: core has no filesystem. Rejects empty, absolute, any
/// `..` component, backslashes and NUL. Symlinks that escape the repository
/// are checked later, in `harness-exec`, against the real worktree.
fn is_repo_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && !path.split('/').any(|c| c == "..")
}

fn location(n: usize, raw: &RawFinding) -> Result<Option<Location>, ReviewRejected> {
    let bad_lines = ReviewRejected::BadLineRange { finding: n };
    let Some(path) = &raw.file else {
        return match (raw.line_start, raw.line_end) {
            (None, None) => Ok(None),
            _ => Err(bad_lines),
        };
    };
    if !is_repo_relative(path) {
        return Err(ReviewRejected::PathOutsideRepo {
            finding: n,
            path: path.chars().take(200).collect(),
        });
    }
    let (line_start, line_end) = match (raw.line_start, raw.line_end) {
        (None, None) => (0, 0),
        (Some(s), None) => (s, s),
        (Some(s), Some(e)) => (s, e),
        (None, Some(_)) => return Err(bad_lines),
    };
    if raw.line_start == Some(0) || line_start > line_end {
        return Err(bad_lines);
    }
    Ok(Some(Location {
        path: path.clone(),
        line_start,
        line_end,
    }))
}

/// Turns untrusted reviewer output into findings the harness can act on.
/// `blocking` is derived from `threshold`, never taken from the model.
pub fn validate_review(
    raw: RawReview,
    commit: &CommitSha,
    reviewer: RuntimeKind,
    threshold: Severity,
) -> Result<Review, ReviewRejected> {
    let mut findings = Vec::with_capacity(raw.findings.len());
    for (i, f) in raw.findings.into_iter().enumerate() {
        let n = i + 1;
        if !(0.0..=1.0).contains(&f.confidence) {
            return Err(ReviewRejected::ConfidenceOutOfRange { finding: n });
        }
        let location = location(n, &f)?;
        let blocking = f.severity >= threshold;
        if blocking && raw.verdict == Verdict::Pass {
            return Err(ReviewRejected::PassWithBlocking { finding: n });
        }
        findings.push(Finding {
            id: format!("F{n}"),
            severity: f.severity,
            blocking,
            category: f.category,
            location,
            claim: f.claim.chars().take(MAX_CLAIM_CHARS).collect(),
            failure_scenario: f.failure_scenario,
            recommendation: f.recommendation,
            reviewed_commit: commit.clone(),
            reviewer,
        });
    }
    Ok(Review {
        verdict: raw.verdict,
        summary: raw.summary,
        findings,
    })
}

/// JSON Schema for [`RawReview`], handed to the reviewer runtime.
///
/// Shaped for OpenAI Structured Outputs in strict mode (what
/// `codex exec --output-schema` uses), which Claude's `--json-schema` also
/// accepts: `$ref`s are inlined, every object lists all its properties in
/// `required` with `additionalProperties: false` (an `Option` field is
/// required but nullable, `["<type>", "null"]`), and keywords strict mode
/// refuses (`format`, `minimum`, `$schema`, ...) are dropped. The lost bounds
/// are enforced by [`validate_review`].
pub fn review_json_schema() -> serde_json::Value {
    let mut root = schemars::schema_for!(RawReview).to_value();
    let defs = root
        .as_object_mut()
        .and_then(|o| o.remove("$defs"))
        .unwrap_or_default();
    strict(&root, &defs)
}

const STRICT_KEYWORDS: [&str; 9] = [
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "anyOf",
    "description",
    "title",
];

fn strict(node: &serde_json::Value, defs: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    let Some(obj) = node.as_object() else {
        return node.clone();
    };
    if let Some(name) = obj
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|r| r.strip_prefix("#/$defs/"))
    {
        return strict(&defs[name], defs);
    }
    let mut out = serde_json::Map::new();
    for (k, v) in obj {
        let v = match k.as_str() {
            "properties" => Value::Object(
                v.as_object()
                    .into_iter()
                    .flatten()
                    .map(|(name, s)| (name.clone(), strict(s, defs)))
                    .collect(),
            ),
            "items" => strict(v, defs),
            "anyOf" => Value::Array(
                v.as_array()
                    .into_iter()
                    .flatten()
                    .map(|s| strict(s, defs))
                    .collect(),
            ),
            k if STRICT_KEYWORDS.contains(&k) => v.clone(),
            _ => continue,
        };
        out.insert(k.clone(), v);
    }
    if let Some(Value::Object(props)) = out.get("properties") {
        let all = props.keys().cloned().map(Value::String).collect();
        out.insert("required".into(), Value::Array(all));
        out.insert("additionalProperties".into(), Value::Bool(false));
    }
    Value::Object(out)
}
