//! Exit codes and the one-object-per-command `--json` shape.

use serde_json::{Map, Value, json};

pub const OK: i32 = 0;
pub const USAGE: i32 = 2;
pub const REJECTED: i32 = 3;
pub const AWAITING_HUMAN: i32 = 4;
pub const FAILED: i32 = 5;
pub const INTERNAL: i32 = 70;

/// What a command reports. `ok`, `task`, `state` and `error` are always present in the JSON
/// object; `extra` adds command-specific fields.
pub struct Report {
    pub ok: bool,
    pub task: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub extra: Map<String, Value>,
}

impl Report {
    pub fn ok() -> Report {
        Report {
            ok: true,
            task: None,
            state: None,
            error: None,
            extra: Map::new(),
        }
    }

    pub fn err(error: impl Into<String>) -> Report {
        Report {
            ok: false,
            error: Some(error.into()),
            ..Report::ok()
        }
    }

    pub fn task(mut self, task: &str, state: Option<String>) -> Report {
        self.task = Some(task.to_owned());
        self.state = state;
        self
    }

    pub fn with(mut self, key: &str, value: Value) -> Report {
        self.extra.insert(key.to_owned(), value);
        self
    }

    pub fn to_json(&self) -> Value {
        let mut obj = Map::new();
        obj.insert("ok".into(), json!(self.ok));
        obj.insert("task".into(), json!(self.task));
        obj.insert("state".into(), json!(self.state));
        obj.insert("error".into(), json!(self.error));
        for (k, v) in &self.extra {
            obj.insert(k.clone(), v.clone());
        }
        Value::Object(obj)
    }
}

/// Prints the report (JSON on stdout, or `text` on stdout and the error on stderr) and
/// returns `code`.
pub fn emit(json: bool, report: &Report, text: &str, code: i32) -> i32 {
    if json {
        println!("{}", report.to_json());
    } else {
        if !text.is_empty() {
            println!("{text}");
        }
        if let Some(e) = &report.error {
            eprintln!("eng: {e}");
        }
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_object_always_has_the_four_fields() {
        let v = Report::err("boom").to_json();
        assert_eq!(v["ok"], json!(false));
        assert_eq!(v["task"], Value::Null);
        assert_eq!(v["state"], Value::Null);
        assert_eq!(v["error"], json!("boom"));
        let v = Report::ok()
            .task("t1", Some("verified".into()))
            .with("n", json!(3))
            .to_json();
        assert_eq!(
            (v["task"].as_str(), v["state"].as_str(), v["n"].as_i64()),
            (Some("t1"), Some("verified"), Some(3))
        );
    }
}
