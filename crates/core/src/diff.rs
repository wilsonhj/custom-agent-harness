//! Splitting a unified `git diff` into review chunks (design decision 15: review accuracy
//! falls on large diffs, so the reviewer sees bounded chunks).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffChunk {
    /// 0-based, in output order; matches `EffectSpec::AgentRun.chunk`.
    pub index: u32,
    pub files: Vec<String>,
    /// The chunk's files' sections, byte for byte. Concatenating every chunk's `text`
    /// reproduces the input.
    pub text: String,
    pub changed_lines: u32,
}

struct FileSection<'a> {
    name: String,
    text: &'a str,
    changed: u32,
}

/// Splits `unified` at each `diff --git ` line and packs whole files, in input order, into
/// chunks of at most `max_lines` changed lines (`+`/`-` lines inside hunks; the `---`/`+++`
/// headers are not counted). A file with more than `max_lines` changed lines is its own chunk.
/// An empty or whitespace-only diff gives zero chunks; `decide` then skips review and treats
/// it as clean.
pub fn chunk_diff(unified: &str, max_lines: u32) -> Vec<DiffChunk> {
    if unified.trim().is_empty() {
        return Vec::new();
    }
    let mut chunks: Vec<DiffChunk> = Vec::new();
    let mut current: Option<DiffChunk> = None;
    for file in sections(unified) {
        if let Some(c) =
            current.take_if(|c| c.changed_lines.saturating_add(file.changed) > max_lines)
        {
            chunks.push(c);
        }
        let c = current.get_or_insert_with(|| DiffChunk {
            index: u32::try_from(chunks.len()).unwrap_or(u32::MAX),
            files: Vec::new(),
            text: String::new(),
            changed_lines: 0,
        });
        c.files.push(file.name);
        c.text.push_str(file.text);
        c.changed_lines = c.changed_lines.saturating_add(file.changed);
    }
    chunks.extend(current);
    chunks
}

/// One section per `diff --git ` line. Text before the first header (git emits none) stays
/// with the first section so nothing is dropped.
fn sections(unified: &str) -> Vec<FileSection<'_>> {
    let mut starts: Vec<usize> = Vec::new();
    let mut offset = 0;
    for line in unified.split_inclusive('\n') {
        if line.starts_with("diff --git ") && offset > 0 {
            starts.push(offset);
        }
        offset += line.len();
    }
    let mut bounds = vec![0];
    bounds.extend(starts);
    bounds.push(unified.len());
    bounds
        .windows(2)
        .map(|w| {
            let text = &unified[w[0]..w[1]];
            let (name, changed) = scan(text);
            FileSection {
                name,
                text,
                changed,
            }
        })
        .collect()
}

/// The file's name and its changed-line count. Git ends a `---`/`+++` path that contains a
/// space with a tab, which is dropped.
fn scan(section: &str) -> (String, u32) {
    let mut header_name = None;
    let mut plus_name = None;
    let mut minus_name = None;
    let mut in_hunk = false;
    let mut changed: u32 = 0;
    for line in section.lines() {
        if in_hunk {
            if line.starts_with('+') || line.starts_with('-') {
                changed = changed.saturating_add(1);
            }
        } else if line.starts_with("@@") {
            in_hunk = true;
        } else if let Some(rest) = line.strip_prefix("diff --git ") {
            header_name = Some(name_from_header(rest));
        } else if let Some(p) = line.strip_prefix("+++ b/") {
            plus_name = Some(p.trim_end_matches('\t').to_owned());
        } else if let Some(p) = line.strip_prefix("--- a/") {
            minus_name = Some(p.trim_end_matches('\t').to_owned());
        }
    }
    let name = plus_name.or(minus_name).or(header_name).unwrap_or_default();
    (name, changed)
}

/// `a/P b/P` names `P` (spaces allowed). A rename's header has two different paths; its
/// `+++` line, preferred above, names the new one.
fn name_from_header(rest: &str) -> String {
    let half = rest.len().saturating_sub(1) / 2;
    let (left, right) = (rest.get(..half), rest.get(half + 1..));
    match (
        left.and_then(|l| l.strip_prefix("a/")),
        right.and_then(|r| r.strip_prefix("b/")),
    ) {
        (Some(l), Some(r)) if l == r => l.to_owned(),
        _ => rest.to_owned(),
    }
}
