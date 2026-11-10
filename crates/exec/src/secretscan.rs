//! Token-shaped strings in the added lines of a unified diff.
//!
//! Only added lines are scanned: a secret that the candidate removes is not a leak. Lines are
//! classified by parsing hunk headers, so an added line whose own text starts with `++` is
//! content, not a `+++` file header.

/// One rule matched on one added line. `line` is the 1-based line in the new file, or 0 when
/// the `+` line was not inside a well-formed hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretHit {
    pub file: String,
    pub line: u32,
    pub rule: &'static str,
}

/// Minimum length of the token body after a known key prefix, and of a JSON token value.
const MIN_TOKEN_CHARS: usize = 20;

const PREFIX_RULES: &[(&str, &str)] = &[
    ("sk-ant-", "anthropic-key"),
    ("sk-proj-", "openai-project-key"),
    ("sk-or-", "openrouter-key"),
    ("ghp_", "github-token"),
    ("gho_", "github-token"),
    ("github_pat_", "github-token"),
];

const JSON_TOKEN_KEYS: &[&str] = &["accessToken", "refreshToken", "id_token"];

/// Every rule hit on every added line of `diff`, in diff order (one hit per rule per line).
pub fn scan_for_secrets(diff: &str) -> Vec<SecretHit> {
    let mut hits = Vec::new();
    let mut file = String::new();
    let (mut old_left, mut new_left, mut new_line) = (0u32, 0u32, 0u32);

    for line in diff.split('\n') {
        if old_left > 0 || new_left > 0 {
            match line.as_bytes().first() {
                Some(b'+') => {
                    record(&mut hits, &file, new_line, &line[1..]);
                    new_left = new_left.saturating_sub(1);
                    new_line = new_line.saturating_add(1);
                    continue;
                }
                Some(b'-') => {
                    old_left = old_left.saturating_sub(1);
                    continue;
                }
                // An empty line is a context line whose trailing space was stripped.
                Some(b' ') | None => {
                    old_left = old_left.saturating_sub(1);
                    new_left = new_left.saturating_sub(1);
                    new_line = new_line.saturating_add(1);
                    continue;
                }
                Some(b'\\') => continue, // "\ No newline at end of file"
                // Malformed hunk: fall through and treat the line as a header.
                Some(_) => {
                    old_left = 0;
                    new_left = 0;
                }
            }
        }
        if line.starts_with("diff ") {
            file.clear();
        } else if let Some(name) = line.strip_prefix("+++ ") {
            file = header_path(name);
        } else if let Some(rest) = line.strip_prefix("@@ ") {
            if let Some((old, new)) = hunk_ranges(rest) {
                old_left = old.1;
                new_left = new.1;
                new_line = new.0;
            }
        } else if let Some(text) = line.strip_prefix('+') {
            // Outside any hunk: still an added line as far as we can tell; fail towards finding.
            record(&mut hits, &file, 0, text);
        }
    }
    hits
}

fn record(hits: &mut Vec<SecretHit>, file: &str, line: u32, text: &str) {
    let mut push = |rule: &'static str| {
        if !hits
            .iter()
            .any(|h| h.rule == rule && h.line == line && h.file == file)
        {
            hits.push(SecretHit {
                file: file.to_owned(),
                line,
                rule,
            });
        }
    };
    let bytes = text.as_bytes();
    for &(prefix, rule) in PREFIX_RULES {
        let found = text.match_indices(prefix).any(|(at, _)| {
            at_word_start(bytes, at)
                && token_run(&bytes[at + prefix.len()..], is_token_char) >= MIN_TOKEN_CHARS
        });
        if found {
            push(rule);
        }
    }
    if text.match_indices("AKIA").any(|(at, _)| {
        token_run(&bytes[at + 4..], |c| {
            c.is_ascii_digit() || c.is_ascii_uppercase()
        }) >= 16
    }) {
        push("aws-access-key");
    }
    if text
        .find("-----BEGIN ")
        .is_some_and(|at| text[at..].contains("PRIVATE KEY-----"))
    {
        push("private-key");
    }
    if JSON_TOKEN_KEYS.iter().any(|key| {
        text.match_indices(key).any(|(at, _)| {
            at_word_start(bytes, at) && json_value_len(&bytes[at + key.len()..]) >= MIN_TOKEN_CHARS
        })
    }) {
        push("json-token");
    }
}

fn is_token_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

fn at_word_start(bytes: &[u8], at: usize) -> bool {
    at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_')
}

fn token_run(bytes: &[u8], ok: impl Fn(u8) -> bool) -> usize {
    bytes.iter().take_while(|&&c| ok(c)).count()
}

/// After a key name: optional closing quote, `:`, then a double-quoted value. Returns the
/// value's length in bytes (to the closing quote or end of line), or 0 if the shape differs.
fn json_value_len(rest: &[u8]) -> usize {
    let mut i = 0;
    if rest.first() == Some(&b'"') {
        i += 1;
    }
    let skip_ws = |mut i: usize| {
        while rest.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
            i += 1;
        }
        i
    };
    i = skip_ws(i);
    if rest.get(i) != Some(&b':') {
        return 0;
    }
    i = skip_ws(i + 1);
    if rest.get(i) != Some(&b'"') {
        return 0;
    }
    let value = &rest[i + 1..];
    let mut len = 0;
    while len < value.len() {
        match value[len] {
            b'\\' => len += 2,
            b'"' => break,
            _ => len += 1,
        }
    }
    len.min(value.len())
}

/// `-a,b +c,d @@ ...` => ((a, b), (c, d)); a missing count is 1.
type Range = (u32, u32);
fn hunk_ranges(rest: &str) -> Option<(Range, Range)> {
    let mut parts = rest.split(' ');
    let old = range(parts.next()?.strip_prefix('-')?)?;
    let new = range(parts.next()?.strip_prefix('+')?)?;
    Some((old, new))
}

fn range(s: &str) -> Option<Range> {
    match s.split_once(',') {
        Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
        None => Some((s.parse().ok()?, 1)),
    }
}

/// `b/path`, `"b/quoted\tpath"` or `/dev/null` (Git appends a tab to names with spaces).
fn header_path(name: &str) -> String {
    let name = name.strip_suffix('\t').unwrap_or(name);
    let name = match name.strip_prefix('"').and_then(|n| n.strip_suffix('"')) {
        Some(quoted) => unquote(quoted),
        None => name.to_owned(),
    };
    match name.strip_prefix("b/") {
        Some(path) => path.to_owned(),
        None => name,
    }
}

/// Git's C-style path quoting: `\\`, `\"`, `\t`, `\n`, and `\ooo` octal bytes.
fn unquote(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' || i + 1 == b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let c = b[i + 1];
        let octal = b
            .get(i + 1..i + 4)
            .filter(|d| d.iter().all(|x| (b'0'..=b'7').contains(x)));
        if let Some(d) = octal {
            let v = d.iter().fold(0u16, |acc, x| acc * 8 + u16::from(x - b'0'));
            out.push(u8::try_from(v).unwrap_or(b'?'));
            i += 4;
            continue;
        }
        out.push(match c {
            b't' => b'\t',
            b'n' => b'\n',
            b'r' => b'\r',
            other => other,
        });
        i += 2;
    }
    String::from_utf8_lossy(&out).into_owned()
}
