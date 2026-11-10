#![allow(clippy::expect_used)]

use std::collections::BTreeSet;

use harness_core::artifact::Provenance;
use harness_core::finding::{
    FindingCategory, RawFinding, RawReview, ReviewRejected, Severity, Verdict, review_json_schema,
    validate_review,
};
use harness_core::ids::{ArtifactId, CommitSha, EffectId, TaskId, Timestamp};
use harness_core::types::{Actor, Envelope, RuntimeKind, WIRE_VERSION};
use serde::{Deserialize, Serialize};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn commit() -> CommitSha {
    match CommitSha::parse(SHA) {
        Ok(c) => c,
        Err(e) => panic!("valid sha rejected: {e}"),
    }
}

fn finding(severity: Severity) -> RawFinding {
    RawFinding {
        severity,
        category: FindingCategory::Correctness,
        file: Some("src/lib.rs".into()),
        line_start: Some(3),
        line_end: Some(7),
        claim: "off by one".into(),
        failure_scenario: "empty input panics".into(),
        recommendation: "guard the empty case".into(),
        suggested_check: None,
        confidence: 0.8,
    }
}

fn review(verdict: Verdict, findings: Vec<RawFinding>) -> RawReview {
    RawReview {
        verdict,
        summary: "summary".into(),
        findings,
    }
}

fn validate(raw: RawReview) -> Result<harness_core::finding::Review, ReviewRejected> {
    validate_review(raw, &commit(), RuntimeKind::Codex, Severity::High)
}

#[test]
fn artifact_id_is_sha256_hex() {
    assert_eq!(
        ArtifactId::of(b"abc").as_str(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TestEvent {
    TaskCreated { title: String },
}

#[test]
fn envelope_roundtrips_with_flattened_body() {
    let env = Envelope {
        v: WIRE_VERSION,
        id: "ev-1".into(),
        task: TaskId::new("task-1"),
        at: Timestamp(1_762_646_400_000_000),
        actor: Actor::Effect {
            effect: EffectId::new("eff-1"),
        },
        body: TestEvent::TaskCreated {
            title: "hello".into(),
        },
    };
    let json = match serde_json::to_string(&env) {
        Ok(s) => s,
        Err(e) => panic!("serialize: {e}"),
    };
    let back: Envelope<TestEvent> = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(e) => panic!("deserialize {json}: {e}"),
    };
    assert_eq!(back, env);

    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(e) => panic!("parse: {e}"),
    };
    assert_eq!(value["v"], 1);
    assert_eq!(value["type"], "task_created");
    assert_eq!(value["title"], "hello");
    assert_eq!(value["task"], "task-1");
    assert_eq!(value["at"], 1_762_646_400_000_000_i64);
    assert_eq!(value["actor"]["type"], "effect");
    assert_eq!(value["actor"]["effect"], "eff-1");
    assert!(value.get("body").is_none());
}

#[test]
fn unknown_field_in_raw_review_is_rejected() {
    let ok = r#"{"verdict":"pass","summary":"s","findings":[]}"#;
    assert!(serde_json::from_str::<RawReview>(ok).is_ok());

    let extra_top = r#"{"verdict":"pass","summary":"s","findings":[],"blocking":true}"#;
    assert!(serde_json::from_str::<RawReview>(extra_top).is_err());

    let extra_finding = r#"{"verdict":"changes_requested","summary":"s","findings":[{
        "severity":"high","category":"correctness","file":null,"line_start":null,
        "line_end":null,"claim":"c","failure_scenario":"f","recommendation":"r",
        "suggested_check":null,"confidence":0.5,"blocking":false}]}"#;
    assert!(serde_json::from_str::<RawReview>(extra_finding).is_err());
}

#[test]
fn only_two_provenances_are_evidence() {
    let all = [
        Provenance::Input,
        Provenance::RuntimeClaim,
        Provenance::HarnessEvidence,
        Provenance::ReviewFindings,
        Provenance::HumanStatement,
    ];
    let evidence: Vec<Provenance> = all.into_iter().filter(|p| p.is_evidence()).collect();
    assert_eq!(
        evidence,
        vec![Provenance::HarnessEvidence, Provenance::ReviewFindings]
    );
}

#[test]
fn blocking_is_derived_from_threshold_not_model() {
    let raw = review(
        Verdict::ChangesRequested,
        vec![finding(Severity::High), finding(Severity::Medium)],
    );
    let r = match validate(raw) {
        Ok(r) => r,
        Err(e) => panic!("rejected: {e}"),
    };
    assert_eq!(r.findings.len(), 2);
    assert!(r.findings[0].blocking);
    assert!(!r.findings[1].blocking);
    assert_eq!(r.findings[0].reviewer, RuntimeKind::Codex);
    assert_eq!(r.findings[0].reviewed_commit, commit());
}

#[test]
fn finding_path_outside_repo_is_rejected() {
    for bad in ["../x", "/etc/passwd", "a/../../b"] {
        let mut f = finding(Severity::Low);
        f.file = Some(bad.into());
        let res = validate(review(Verdict::ChangesRequested, vec![f]));
        assert!(
            matches!(res, Err(ReviewRejected::PathOutsideRepo { .. })),
            "{bad:?} gave {res:?}"
        );
    }
}

#[test]
fn malformed_finding_path_is_rejected() {
    for bad in ["", "src\\lib.rs", "src/\0lib.rs", "..", "a/.."] {
        let mut f = finding(Severity::Low);
        f.file = Some(bad.into());
        let res = validate(review(Verdict::ChangesRequested, vec![f]));
        assert!(
            matches!(res, Err(ReviewRejected::PathOutsideRepo { .. })),
            "{bad:?} gave {res:?}"
        );
    }
}

#[test]
fn pass_with_blocking_finding_is_rejected() {
    let res = validate(review(Verdict::Pass, vec![finding(Severity::Critical)]));
    assert!(
        matches!(res, Err(ReviewRejected::PassWithBlocking { .. })),
        "{res:?}"
    );
    // A non-blocking finding is allowed alongside Pass.
    assert!(validate(review(Verdict::Pass, vec![finding(Severity::Low)])).is_ok());
}

#[test]
fn inverted_line_range_is_rejected() {
    let mut f = finding(Severity::Low);
    f.line_start = Some(9);
    f.line_end = Some(2);
    let res = validate(review(Verdict::ChangesRequested, vec![f]));
    assert!(
        matches!(res, Err(ReviewRejected::BadLineRange { .. })),
        "{res:?}"
    );
}

#[test]
fn line_end_without_start_or_lines_without_file_are_rejected() {
    let mut no_start = finding(Severity::Low);
    no_start.line_start = None;
    let mut no_file = finding(Severity::Low);
    no_file.file = None;
    let mut zero = finding(Severity::Low);
    zero.line_start = Some(0);
    for f in [no_start, no_file, zero] {
        let res = validate(review(Verdict::ChangesRequested, vec![f]));
        assert!(
            matches!(res, Err(ReviewRejected::BadLineRange { .. })),
            "{res:?}"
        );
    }
}

#[test]
fn confidence_outside_unit_interval_is_rejected() {
    for bad in [-0.01_f32, 1.01, f32::NAN, f32::INFINITY] {
        let mut f = finding(Severity::Low);
        f.confidence = bad;
        let res = validate(review(Verdict::ChangesRequested, vec![f]));
        assert!(
            matches!(res, Err(ReviewRejected::ConfidenceOutOfRange { .. })),
            "{bad} gave {res:?}"
        );
    }
    for good in [0.0_f32, 1.0] {
        let mut f = finding(Severity::Low);
        f.confidence = good;
        assert!(validate(review(Verdict::ChangesRequested, vec![f])).is_ok());
    }
}

#[test]
fn findings_get_sequential_ids_locations_and_truncated_claims() {
    let mut whole_file = finding(Severity::Low);
    whole_file.line_start = None;
    whole_file.line_end = None;
    let mut single_line = finding(Severity::Low);
    single_line.line_end = None;
    let mut no_location = finding(Severity::Low);
    no_location.file = None;
    no_location.line_start = None;
    no_location.line_end = None;
    no_location.claim = "é".repeat(2_500);

    let r = match validate(review(
        Verdict::ChangesRequested,
        vec![whole_file, single_line, no_location],
    )) {
        Ok(r) => r,
        Err(e) => panic!("rejected: {e}"),
    };
    let ids: Vec<&str> = r.findings.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["F1", "F2", "F3"]);

    let loc0 = r.findings[0]
        .location
        .as_ref()
        .map(|l| (l.line_start, l.line_end));
    assert_eq!(loc0, Some((0, 0)));
    let loc1 = r.findings[1]
        .location
        .as_ref()
        .map(|l| (l.line_start, l.line_end));
    assert_eq!(loc1, Some((3, 3)));
    assert!(r.findings[2].location.is_none());
    assert_eq!(r.findings[2].claim.chars().count(), 2_000);
}

#[test]
fn severity_orders_critical_highest() {
    assert!(Severity::Critical > Severity::High);
    assert!(Severity::High > Severity::Medium);
    assert!(Severity::Medium > Severity::Low);
    let mut v = vec![
        Severity::Medium,
        Severity::Critical,
        Severity::Low,
        Severity::High,
    ];
    v.sort();
    assert_eq!(
        v,
        [
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical
        ]
    );
}

#[test]
fn artifact_id_and_commit_sha_reject_malformed_input() {
    let good = ArtifactId::of(b"abc");
    assert_eq!(ArtifactId::parse(good.as_str()).ok(), Some(good.clone()));
    let upper = good.as_str().to_uppercase();
    for bad in ["", "abc", upper.as_str(), &"g".repeat(64), &"a".repeat(65)] {
        assert!(ArtifactId::parse(bad).is_err(), "{bad:?}");
    }
    let upper_sha = SHA.to_uppercase();
    for bad in [
        "",
        &SHA[..39],
        upper_sha.as_str(),
        &"z".repeat(40),
        &"a".repeat(64),
    ] {
        assert!(CommitSha::parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn deserialize_validates_hex_ids() {
    let ok = format!("\"{SHA}\"");
    assert!(serde_json::from_str::<CommitSha>(&ok).is_ok());
    assert!(serde_json::from_str::<CommitSha>("\"not-a-sha\"").is_err());
    let upper = format!("\"{}\"", ArtifactId::of(b"x").as_str().to_uppercase());
    assert!(serde_json::from_str::<ArtifactId>(&upper).is_err());
    let roundtrip = match serde_json::to_string(&ArtifactId::of(b"x")) {
        Ok(s) => s,
        Err(e) => panic!("serialize: {e}"),
    };
    assert_eq!(
        serde_json::from_str::<ArtifactId>(&roundtrip).ok(),
        Some(ArtifactId::of(b"x"))
    );
}

fn names(v: &serde_json::Value) -> BTreeSet<String> {
    v.as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

fn finding_schema(schema: &serde_json::Value) -> &serde_json::Value {
    &schema["properties"]["findings"]["items"]
}

#[test]
fn review_schema_names_every_raw_finding_field() {
    let schema = review_json_schema();
    let finding_props = names(&finding_schema(&schema)["properties"]);
    let expected: BTreeSet<String> = [
        "severity",
        "category",
        "file",
        "line_start",
        "line_end",
        "claim",
        "failure_scenario",
        "recommendation",
        "suggested_check",
        "confidence",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(finding_props, expected);

    let review_props = names(&schema["properties"]);
    let expected_review: BTreeSet<String> = ["verdict", "summary", "findings"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(review_props, expected_review);
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(finding_schema(&schema)["additionalProperties"], false);
}

/// Keywords OpenAI Structured Outputs (strict) accepts and this schema uses.
/// Anything else (`format`, `minimum`, `$ref`, `$defs`, `$schema`, `oneOf`, ...)
/// is refused so the schema works with `codex exec --output-schema`.
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

/// Visits every subschema (not property names).
fn walk(node: &serde_json::Value, visit: &mut dyn FnMut(&serde_json::Value)) {
    visit(node);
    if let Some(props) = node.get("properties").and_then(|p| p.as_object()) {
        for v in props.values() {
            walk(v, visit);
        }
    }
    if let Some(items) = node.get("items") {
        walk(items, visit);
    }
    if let Some(any) = node.get("anyOf").and_then(|a| a.as_array()) {
        for v in any {
            walk(v, visit);
        }
    }
}

#[test]
fn review_schema_is_structured_outputs_strict() {
    let schema = review_json_schema();
    assert_eq!(schema["type"], "object");
    let mut objects = 0;
    walk(&schema, &mut |node| {
        let obj = node.as_object().expect("every subschema is an object");
        for k in obj.keys() {
            assert!(
                STRICT_KEYWORDS.contains(&k.as_str()),
                "unsupported keyword {k:?} in {node}"
            );
        }
        if obj.contains_key("properties") || node["type"] == "object" {
            objects += 1;
            let required: BTreeSet<String> = node["required"]
                .as_array()
                .expect("object lists required")
                .iter()
                .map(|v| v.as_str().expect("required is strings").to_owned())
                .collect();
            assert_eq!(required, names(&node["properties"]), "required in {node}");
            assert_eq!(node["additionalProperties"], false, "in {node}");
        }
    });
    assert_eq!(objects, 2, "RawReview and RawFinding");
}

fn allows_null(node: &serde_json::Value) -> bool {
    let ty_null = |t: &serde_json::Value| {
        t == "null" || t.as_array().is_some_and(|a| a.iter().any(|x| x == "null"))
    };
    ty_null(&node["type"])
        || node["anyOf"]
            .as_array()
            .is_some_and(|a| a.iter().any(|s| ty_null(&s["type"])))
}

#[test]
fn review_schema_marks_exactly_the_option_fields_nullable() {
    let schema = review_json_schema();
    let props = finding_schema(&schema)["properties"]
        .as_object()
        .expect("finding properties");
    let nullable: BTreeSet<&str> = props
        .iter()
        .filter(|(_, v)| allows_null(v))
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(
        nullable,
        BTreeSet::from(["file", "line_start", "line_end", "suggested_check"])
    );
    for (k, v) in schema["properties"].as_object().expect("review properties") {
        assert!(!allows_null(v), "{k} must not be nullable");
    }
}

#[test]
fn review_with_explicit_nulls_parses_into_raw_review() {
    let json = r#"{
        "verdict": "changes_requested",
        "summary": "s",
        "findings": [{
            "severity": "high",
            "category": "tests",
            "file": null,
            "line_start": null,
            "line_end": null,
            "claim": "c",
            "failure_scenario": "f",
            "recommendation": "r",
            "suggested_check": null,
            "confidence": 0.5
        }]
    }"#;
    let raw: RawReview = serde_json::from_str(json).expect("explicit nulls parse");
    let f = &raw.findings[0];
    assert_eq!(
        (&f.file, f.line_start, f.line_end, &f.suggested_check),
        (&None, None, None, &None)
    );
    let back: RawReview =
        serde_json::from_value(serde_json::to_value(&raw).expect("serialize")).expect("parse");
    assert_eq!(back, raw);
}
