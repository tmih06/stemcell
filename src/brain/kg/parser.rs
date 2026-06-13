//! Pure markdown knowledge-graph parser.
//!
//! Extracts the structure StemCell reasons over from an Obsidian-compatible
//! note: YAML frontmatter, `[[wikilinks]]` (with `|alias`, `#heading`, `#^block`
//! and `![[embed]]` variants), inline `#tags`, typed `## Observations` bullets
//! (`- [category] text #tag (context)`), and typed `## Relations` bullets
//! (`- relation_type [[Target]]`, bare `- [[Target]]` = `links_to`).
//!
//! This module is intentionally I/O-free and dependency-light (regex + serde_json
//! only) so it is trivially unit-testable. `pulldown-cmark` is unsuitable here —
//! it treats `[[` as ordinary text — so the scanner is hand-rolled.

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

static WIKILINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(!?)\[\[([^\[\]]+)\]\]").unwrap());
static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)#([A-Za-z0-9_][A-Za-z0-9_/-]*)").unwrap());
static BLOCK_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s\^([A-Za-z0-9][A-Za-z0-9-]*)\s*$").unwrap());

/// A parsed `[[wikilink]]` and its components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    /// The target note name (text before `#` and `|`).
    pub target: String,
    /// `#heading` portion, if present (not a block ref).
    pub heading: Option<String>,
    /// `#^block` portion, if present.
    pub block_id: Option<String>,
    /// `|alias` display text, if present.
    pub alias: Option<String>,
    /// True for `![[embed]]` transclusions.
    pub embed: bool,
}

/// A typed observation bullet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub category: Option<String>,
    pub content: String,
    pub tags: Vec<String>,
    pub context: Option<String>,
}

/// A typed relation (graph edge) to a target note name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub relation_type: String,
    pub target: String,
    pub context: Option<String>,
}

/// A markdown heading and the 0-based line it appears on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: usize,
    pub text: String,
    pub line: usize,
}

/// Parsed YAML frontmatter. Known keys are lifted into typed fields; every key
/// is also preserved in `fields` for JSON storage / cheap fact extraction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub note_type: Option<String>,
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
    pub fields: serde_json::Map<String, serde_json::Value>,
}

impl Frontmatter {
    /// Serialize the raw frontmatter fields to a JSON object string, or `None`
    /// when there was no frontmatter.
    pub fn to_json(&self) -> Option<String> {
        if self.fields.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(self.fields.clone()).to_string())
        }
    }
}

/// The full parse of a note.
#[derive(Debug, Clone, Default)]
pub struct ParsedNote {
    pub frontmatter: Frontmatter,
    /// Title hint: frontmatter `title`, else the first H1. `None` → caller
    /// should fall back to the filename stem.
    pub title: Option<String>,
    pub observations: Vec<Observation>,
    pub relations: Vec<Relation>,
    pub links: Vec<WikiLink>,
    pub tags: Vec<String>,
    pub headings: Vec<Heading>,
    /// `(block_id, line_index)` pairs for `^block` anchors.
    pub block_ids: Vec<(String, usize)>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Section {
    None,
    Observations,
    Relations,
    Other,
}

/// The body text of a note with any leading frontmatter removed.
pub fn body_after_frontmatter(content: &str) -> String {
    split_frontmatter(content).1
}

/// Parse a complete note (frontmatter + body).
pub fn parse(content: &str) -> ParsedNote {
    let (frontmatter, body) = split_frontmatter(content);

    let mut section = Section::None;
    let mut observations = Vec::new();
    let mut relations = Vec::new();
    let mut links: Vec<WikiLink> = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    let mut headings = Vec::new();
    let mut block_ids = Vec::new();
    let mut first_h1: Option<String> = None;

    for (i, line) in body.lines().enumerate() {
        if let Some((level, text)) = parse_heading(line) {
            if level == 1 && first_h1.is_none() {
                first_h1 = Some(text.clone());
            }
            let lower = text.to_lowercase();
            section = if lower.starts_with("observation") {
                Section::Observations
            } else if lower.starts_with("relation") {
                Section::Relations
            } else {
                Section::Other
            };
            headings.push(Heading {
                level,
                text,
                line: i,
            });
            continue;
        }

        for wl in scan_wikilinks(line) {
            links.push(wl);
        }
        for t in scan_tags(line) {
            if !tags.contains(&t) {
                tags.push(t);
            }
        }
        if let Some(id) = block_id_on_line(line) {
            block_ids.push((id, i));
        }

        let trimmed = line.trim_start();
        let bullet = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "));
        if let Some(bullet) = bullet {
            match section {
                Section::Observations => {
                    if let Some(o) = parse_observation(bullet) {
                        observations.push(o);
                    }
                }
                Section::Relations => {
                    if let Some(r) = parse_relation(bullet) {
                        relations.push(r);
                    }
                }
                _ => {}
            }
        }
    }

    // Fold every other body wikilink into a `links_to` relation so the graph
    // captures prose mentions too — but never double-count a target that
    // already has a typed relation.
    let mut covered: HashSet<String> = relations
        .iter()
        .map(|r| r.target.trim().to_lowercase())
        .collect();
    for wl in &links {
        let key = wl.target.trim().to_lowercase();
        if key.is_empty() || covered.contains(&key) {
            continue;
        }
        covered.insert(key);
        relations.push(Relation {
            relation_type: "links_to".to_string(),
            target: wl.target.clone(),
            context: None,
        });
    }

    // Merge frontmatter tags into the tag set.
    for t in &frontmatter.tags {
        if !tags.contains(t) {
            tags.push(t.clone());
        }
    }

    let title = frontmatter.title.clone().or(first_h1);

    ParsedNote {
        frontmatter,
        title,
        observations,
        relations,
        links,
        tags,
        headings,
        block_ids,
    }
}

/// Split leading `---`-fenced YAML frontmatter from the body. Returns the parsed
/// frontmatter (empty if none) and the remaining body text.
fn split_frontmatter(content: &str) -> (Frontmatter, String) {
    let c = content.strip_prefix('\u{feff}').unwrap_or(content);
    let (first_line, after_first) = match c.find('\n') {
        Some(p) => (&c[..p], &c[p + 1..]),
        None => (c, ""),
    };
    if first_line.trim_end() != "---" {
        return (Frontmatter::default(), content.to_string());
    }

    let mut offset = 0usize;
    for line in after_first.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']).trim() == "---" {
            let fm_block = &after_first[..offset];
            let body = &after_first[offset + line.len()..];
            return (parse_yaml(fm_block), body.to_string());
        }
        offset += line.len();
    }
    // No closing fence — treat the whole thing as body.
    (Frontmatter::default(), content.to_string())
}

enum FieldVal {
    Str(String),
    List(Vec<String>),
}

/// Minimal YAML parser for the controlled frontmatter subset: `key: scalar`,
/// `key: [a, b]` inline lists, and `key:` followed by `  - item` block lists.
fn parse_yaml(block: &str) -> Frontmatter {
    let mut fm = Frontmatter::default();
    let mut lines = block.lines().peekable();
    while let Some(line) = lines.next() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim().to_string();
        if key.is_empty() {
            continue;
        }
        let val = v.trim();
        let field = if val.is_empty() {
            // Possible block list of `- item` lines.
            let mut items = Vec::new();
            while let Some(next) = lines.peek() {
                let nt = next.trim_start();
                if let Some(item) = nt.strip_prefix("- ") {
                    items.push(unquote(item.trim()));
                    lines.next();
                } else {
                    break;
                }
            }
            if items.is_empty() {
                FieldVal::Str(String::new())
            } else {
                FieldVal::List(items)
            }
        } else if val.starts_with('[') && val.ends_with(']') {
            let inner = &val[1..val.len() - 1];
            let items: Vec<String> = inner
                .split(',')
                .map(|s| unquote(s.trim()))
                .filter(|s| !s.is_empty())
                .collect();
            FieldVal::List(items)
        } else {
            FieldVal::Str(unquote(val))
        };
        set_field(&mut fm, &key, field);
    }
    fm
}

fn set_field(fm: &mut Frontmatter, key: &str, val: FieldVal) {
    let json = match &val {
        FieldVal::Str(s) => serde_json::Value::String(s.clone()),
        FieldVal::List(l) => {
            serde_json::Value::Array(l.iter().cloned().map(serde_json::Value::String).collect())
        }
    };
    fm.fields.insert(key.to_string(), json);

    let as_list = |v: &FieldVal| -> Vec<String> {
        match v {
            FieldVal::List(l) => l.clone(),
            FieldVal::Str(s) if !s.is_empty() => vec![s.clone()],
            FieldVal::Str(_) => Vec::new(),
        }
    };

    match key {
        "title" => {
            if let FieldVal::Str(s) = &val
                && !s.is_empty()
            {
                fm.title = Some(s.clone());
            }
        }
        "type" => {
            if let FieldVal::Str(s) = &val
                && !s.is_empty()
            {
                fm.note_type = Some(s.clone());
            }
        }
        "tags" => fm.tags = as_list(&val),
        "aliases" => fm.aliases = as_list(&val),
        _ => {}
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Parse a markdown ATX heading line into `(level, text)`.
pub fn parse_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &trimmed[level..];
    // ATX headings require a space after the `#` run.
    if !rest.starts_with(' ') {
        return None;
    }
    Some((level, rest.trim().to_string()))
}

/// Extract a trailing `^block-id` from a line, if any.
pub fn block_id_on_line(line: &str) -> Option<String> {
    BLOCK_ID_RE
        .captures(line)
        .map(|c| c.get(1).unwrap().as_str().to_string())
}

/// Scan a line for all `[[wikilinks]]` / `![[embeds]]`.
pub fn scan_wikilinks(line: &str) -> Vec<WikiLink> {
    WIKILINK_RE
        .captures_iter(line)
        .map(|c| {
            let embed = !c.get(1).unwrap().as_str().is_empty();
            parse_wikilink_inner(c.get(2).unwrap().as_str(), embed)
        })
        .collect()
}

/// Parse the inside of a `[[...]]` (already stripped of brackets).
fn parse_wikilink_inner(inner: &str, embed: bool) -> WikiLink {
    // Split off `|alias` first.
    let (target_part, alias) = match inner.split_once('|') {
        Some((t, a)) => (t.trim(), Some(a.trim().to_string())),
        None => (inner.trim(), None),
    };
    // Then split off `#heading` or `#^block`.
    let (target, heading, block_id) = match target_part.split_once('#') {
        Some((t, anchor)) => {
            let t = t.trim().to_string();
            if let Some(block) = anchor.strip_prefix('^') {
                (t, None, Some(block.trim().to_string()))
            } else {
                (t, Some(anchor.trim().to_string()), None)
            }
        }
        None => (target_part.to_string(), None, None),
    };
    WikiLink {
        target,
        heading,
        block_id,
        alias,
        embed,
    }
}

/// Scan a line for inline `#tags`.
pub fn scan_tags(line: &str) -> Vec<String> {
    TAG_RE
        .captures_iter(line)
        .map(|c| c.get(1).unwrap().as_str().to_string())
        .collect()
}

fn strip_tags(s: &str) -> String {
    // Remove `#tag` tokens (TAG_RE includes the leading space in the match),
    // then normalise whitespace.
    let replaced = TAG_RE.replace_all(s, " ");
    replaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Pull a trailing parenthetical `(context)` off a string, requiring whitespace
/// before the opening paren so we don't eat `fn()`-style content. Returns the
/// `(stripped_text, Option<context>)`.
fn split_trailing_context(s: &str) -> (String, Option<String>) {
    let s = s.trim_end();
    if !s.ends_with(')') {
        return (s.to_string(), None);
    }
    if let Some(open) = s.rfind('(') {
        let before = &s[..open];
        // Require a whitespace boundary before the paren.
        if open == 0 || before.ends_with(char::is_whitespace) {
            let ctx = s[open + 1..s.len() - 1].trim().to_string();
            return (
                before.trim_end().to_string(),
                if ctx.is_empty() { None } else { Some(ctx) },
            );
        }
    }
    (s.to_string(), None)
}

fn parse_observation(bullet: &str) -> Option<Observation> {
    let s = bullet.trim();
    if s.is_empty() {
        return None;
    }
    let mut rest = s;
    let mut category = None;
    if let Some(after) = rest.strip_prefix('[')
        && let Some(end) = after.find(']')
    {
        category = Some(after[..end].trim().to_string());
        rest = after[end + 1..].trim_start();
    }
    let tags = scan_tags(rest);
    let (content_no_ctx, context) = split_trailing_context(rest);
    let content = strip_tags(&content_no_ctx);
    if content.is_empty() && tags.is_empty() {
        return None;
    }
    Some(Observation {
        category,
        content,
        tags,
        context,
    })
}

fn parse_relation(bullet: &str) -> Option<Relation> {
    let s = bullet.trim();
    let caps = WIKILINK_RE.captures(s)?;
    let whole = caps.get(0).unwrap();
    let inner = caps.get(2).unwrap().as_str();
    let wl = parse_wikilink_inner(inner, !caps.get(1).unwrap().as_str().is_empty());

    let prefix = s[..whole.start()].trim();
    let relation_type = if prefix.is_empty() {
        "links_to".to_string()
    } else {
        normalize_relation_type(prefix)
    };

    let after = s[whole.end()..].trim();
    let (_, context) = split_trailing_context(after);

    Some(Relation {
        relation_type,
        target: wl.target,
        context,
    })
}

/// Normalise a relation label into a snake_case type: lowercase, trim a trailing
/// colon, collapse spaces/hyphens to underscores.
fn normalize_relation_type(s: &str) -> String {
    let s = s.trim().trim_end_matches(':').trim().to_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut prev_us = false;
    for c in s.chars() {
        if c.is_whitespace() || c == '-' {
            if !prev_us {
                out.push('_');
                prev_us = true;
            }
        } else {
            out.push(c);
            prev_us = false;
        }
    }
    out.trim_matches('_').to_string()
}
