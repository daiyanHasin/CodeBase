//! The project's story, from the first commit to the edit you just saved.
//!
//!   commits     every commit with the parts of the design it touched, in plain words
//!   pending     edits not committed yet (git status)
//!   journal     automatic save points for folders without Git
//!   at(rev)     the architecture at any past commit or tag, compared with today
//!
//! Read-only: CodeBase never commits, pushes, pulls, or checks anything out.

use crate::arch::{self, Ctx};
use crate::memory::JournalEntry;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

const REC: char = '\u{1e}';
const SEP: char = '\u{1f}';

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).current_dir(root).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

/// "src/{old => new}/a.rs" and "old.rs => new.rs" both become the new path.
fn renamed(p: &str) -> String {
    static BRACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{[^{}]*? => ([^{}]*)\}").unwrap());
    let s = BRACE.replace_all(p, "$1").replace("//", "/");
    match s.split_once(" => ") { Some((_, b)) => b.to_string(), None => s }
}

pub fn kind_of(subject: &str, comps: &[&str]) -> &'static str {
    static CONV: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(feat|feature|fix|bugfix|hotfix|refactor|test|tests|docs|doc|chore|perf|style|build|ci|deps)(\([^)]*\))?!?:").unwrap());
    if let Some(c) = CONV.captures(subject) {
        return match c[1].to_lowercase().as_str() {
            "feat" | "feature" => "feature", "fix" | "bugfix" | "hotfix" => "fix", "refactor" | "perf" | "style" => "cleanup",
            "test" | "tests" => "tests", "docs" | "doc" => "docs", _ => "setup",
        };
    }
    let s = subject.to_lowercase();
    let has = |ws: &[&str]| ws.iter().any(|w| s.split(|c: char| !c.is_alphanumeric()).any(|x| x == *w));
    if has(&["fix", "fixed", "fixes", "bug", "bugs", "error", "crash", "issue", "broken", "hotfix", "patch"]) { return "fix"; }
    if has(&["refactor", "refactored", "cleanup", "clean", "rename", "renamed", "move", "moved", "tidy", "restructure", "simplify"]) { return "cleanup"; }
    if has(&["add", "added", "adds", "new", "implement", "implemented", "create", "created", "support", "introduce", "feature", "build", "built"]) { return "feature"; }
    if has(&["test", "tests", "spec", "specs", "coverage"]) || (!comps.is_empty() && comps.iter().all(|c| matches!(*c, "testcases" | "scenarios" | "steps"))) { return "tests"; }
    if has(&["doc", "docs", "readme", "documentation"]) || (!comps.is_empty() && comps.iter().all(|c| *c == "docs")) { return "docs"; }
    if has(&["bump", "upgrade", "dependency", "dependencies", "deps", "version", "release"]) || (!comps.is_empty() && comps.iter().all(|c| matches!(*c, "config" | "infra" | "testconfig"))) { return "setup"; }
    "change"
}

fn join_names(names: &[String]) -> String {
    match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        _ => format!("{} and {}", names[..names.len() - 1].join(", "), names[names.len() - 1]),
    }
}

/// "Changed step definitions and page objects. New: LoginPage.cs"
pub fn summarize(files: &[(String, &'static str, &'static str)]) -> String {
    let mut by: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, c, _) in files { *by.entry(c).or_default() += 1; }
    let mut top: Vec<(&str, usize)> = by.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let names: Vec<String> = top.iter().take(3).map(|(c, _)| arch::comp(c).name.to_lowercase()).collect();
    let more = top.len().saturating_sub(3);
    let mut s = format!("Touched {}{}", join_names(&names), if more > 0 { format!(" and {more} more part{}", if more == 1 { "" } else { "s" }) } else { String::new() });
    let short = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
    let added: Vec<String> = files.iter().filter(|f| f.2 == "added").take(3).map(|f| short(&f.0)).collect();
    let removed: Vec<String> = files.iter().filter(|f| f.2 == "removed").take(3).map(|f| short(&f.0)).collect();
    if !added.is_empty() { s += &format!(". New: {}", added.join(", ")); }
    if !removed.is_empty() { s += &format!(". Removed: {}", removed.join(", ")); }
    s.push('.');
    s
}

fn comps_json(files: &[(String, &'static str, &'static str)]) -> Vec<Value> {
    let mut by: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, c, _) in files { *by.entry(c).or_default() += 1; }
    let mut v: Vec<(&str, usize)> = by.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.into_iter().map(|(c, n)| json!({ "id": c, "name": arch::comp(c).name, "files": n })).collect()
}

/// Parses `git log --numstat --summary --format=%x1e%H%x1f%h%x1f%an%x1f%at%x1f%s%x1f%D`.
pub fn parse_log(text: &str, comp_for: &dyn Fn(&str) -> &'static str) -> Vec<Value> {
    let mut out = Vec::new();
    for rec in text.split(REC).filter(|r| !r.trim().is_empty()) {
        let mut lines = rec.lines();
        let head: Vec<&str> = lines.next().unwrap_or("").split(SEP).collect();
        if head.len() < 5 { continue; }
        let tags: Vec<String> = head.get(5).unwrap_or(&"").split(", ").filter_map(|r| r.trim().strip_prefix("tag: ").map(String::from)).collect();
        let mut stats: Vec<(String, i64, i64)> = Vec::new();
        let mut created: BTreeSet<String> = BTreeSet::new();
        let mut deleted: BTreeSet<String> = BTreeSet::new();
        for l in lines {
            let t = l.trim();
            if let Some(p) = t.strip_prefix("create mode ") { if let Some((_, path)) = p.split_once(' ') { created.insert(path.to_string()); } continue; }
            if let Some(p) = t.strip_prefix("delete mode ") { if let Some((_, path)) = p.split_once(' ') { deleted.insert(path.to_string()); } continue; }
            let parts: Vec<&str> = l.splitn(3, '\t').collect();
            if parts.len() == 3 {
                stats.push((renamed(parts[2]).replace('\\', "/"), parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0)));
            }
        }
        let files: Vec<(String, &'static str, &'static str)> = stats.iter().map(|(p, _, _)| {
            let st = if created.contains(p) { "added" } else if deleted.contains(p) { "removed" } else { "modified" };
            (p.clone(), comp_for(p), st)
        }).collect();
        let comps: Vec<&str> = files.iter().map(|f| f.1).collect();
        let subject = head[4].to_string();
        out.push(json!({
            "hash": head[0], "short": head[1], "author": head[2], "at": head[3].parse::<u64>().unwrap_or(0) * 1000,
            "subject": subject, "tags": tags, "kind": kind_of(&subject, &comps),
            "add": stats.iter().map(|s| s.1).sum::<i64>(), "del": stats.iter().map(|s| s.2).sum::<i64>(),
            "summary": if files.is_empty() { "No file changes in this folder.".to_string() } else { summarize(&files) },
            "comps": comps_json(&files),
            "files": stats.iter().zip(&files).take(300).map(|((p, a, d), f)| json!({ "path": p, "add": a, "del": d, "comp": f.1, "status": f.2 })).collect::<Vec<_>>(),
        }));
    }
    out
}

pub fn timeline(root: &Path, comp_for: &dyn Fn(&str) -> &'static str, journal: &[JournalEntry]) -> Value {
    let journal_json: Vec<Value> = journal.iter().rev().take(200).map(|j| {
        let mut files: Vec<(String, &'static str, &'static str)> = Vec::new();
        for p in &j.added { files.push((p.clone(), comp_for(p), "added")); }
        for p in &j.modified { files.push((p.clone(), comp_for(p), "modified")); }
        for p in &j.removed { files.push((p.clone(), comp_for(p), "removed")); }
        json!({ "at": j.at, "summary": summarize(&files), "comps": comps_json(&files), "kind": "save",
            "files": files.iter().map(|(p, c, s)| json!({ "path": p, "comp": c, "status": s })).collect::<Vec<_>>() })
    }).collect();

    let Some(head) = crate::links::git_head(root) else {
        return json!({ "git": false, "commits": [], "journal": journal_json });
    };
    let log = git(root, &["log", "--no-merges", "-n", "400", "--relative", "--numstat", "--summary", "--format=%x1e%H%x1f%h%x1f%an%x1f%at%x1f%s%x1f%D"]).unwrap_or_default();
    let commits = parse_log(&log, comp_for);
    let total: u64 = git(root, &["rev-list", "--count", "--no-merges", "HEAD", "--", "."]).and_then(|s| s.trim().parse().ok()).unwrap_or(commits.len() as u64);
    let shallow = git(root, &["rev-parse", "--is-shallow-repository"]).map(|s| s.trim() == "true").unwrap_or(false);
    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_default();

    // Uncommitted edits, limited to this folder.
    let prefix = git(root, &["rev-parse", "--show-prefix"]).map(|s| s.trim().to_string()).unwrap_or_default();
    let mut pending: Vec<(String, &'static str, &'static str)> = Vec::new();
    for l in git(root, &["status", "--porcelain=v1", "--untracked-files=all", "--", "."]).unwrap_or_default().lines() {
        if l.len() < 4 { continue; }
        let code = &l[..2];
        let raw = l[3..].trim_matches('"');
        let path = renamed(&raw.replace(" -> ", " => "));
        let Some(rel) = path.strip_prefix(&prefix) else { continue };
        let st = if code.contains('?') || code.contains('A') { "added" } else if code.contains('D') { "removed" } else { "modified" };
        pending.push((rel.to_string(), comp_for(rel), st));
        if pending.len() >= 300 { break; }
    }
    json!({
        "git": true, "head": head, "branch": branch, "shallow": shallow, "total": total, "commits": commits,
        "pending": if pending.is_empty() { Value::Null } else { json!({
            "summary": summarize(&pending), "comps": comps_json(&pending),
            "files": pending.iter().map(|(p, c, s)| json!({ "path": p, "comp": c, "status": s })).collect::<Vec<_>>(),
        }) },
        "journal": journal_json,
    })
}

pub fn valid_rev(rev: &str) -> bool {
    !rev.is_empty() && rev.len() <= 100 && !rev.starts_with('-') && !rev.contains("..") && rev.chars().all(|c| c.is_ascii_alphanumeric() || "._/-~^".contains(c))
}

/// The architecture at a past commit, next to today's.
pub fn at(root: &Path, rev: &str, ctx: &Ctx, now: &HashMap<String, &'static str>) -> Result<Value, String> {
    if !valid_rev(rev) { return Err("That is not a commit or tag name.".into()); }
    let list = git(root, &["ls-tree", "-r", "--name-only", rev]).ok_or("Git could not read that version.")?;
    let then: Vec<String> = list.lines().map(|l| l.trim_matches('"').to_string())
        .filter(|p| !p.split('/').any(|seg| crate::scan::BUILTIN_DIRS.contains(&seg)) && !crate::scan::BUILTIN_FILES.is_match(p))
        .collect();
    let then_set: BTreeSet<&String> = then.iter().collect();
    let mut counts: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    let mut added: BTreeMap<&'static str, Vec<&String>> = BTreeMap::new();
    let mut removed: BTreeMap<&'static str, Vec<&String>> = BTreeMap::new();
    for p in &then {
        let c = now.get(p).copied().unwrap_or_else(|| arch::classify_path(p, ctx));
        counts.entry(c).or_default().0 += 1;
        if !now.contains_key(p) { removed.entry(c).or_default().push(p); }
    }
    for (p, c) in now {
        counts.entry(c).or_default().1 += 1;
        if !then_set.contains(p) { added.entry(c).or_default().push(p); }
    }
    let mut comps: Vec<Value> = counts.iter().map(|(c, (a, b))| json!({
        "id": c, "name": arch::comp(c).name, "then": a, "now": b,
        "added": added.get(c).map(|v| { let mut v = v.clone(); v.sort(); v.into_iter().take(15).cloned().collect::<Vec<_>>() }).unwrap_or_default(),
        "removed": removed.get(c).map(|v| { let mut v = v.clone(); v.sort(); v.into_iter().take(15).cloned().collect::<Vec<_>>() }).unwrap_or_default(),
    })).collect();
    comps.sort_by(|a, b| b["now"].as_u64().cmp(&a["now"].as_u64()));
    Ok(json!({ "rev": rev, "filesThen": then.len(), "filesNow": now.len(), "components": comps }))
}

/// What one commit changed in one file, as a patch.
pub fn diff(root: &Path, rev: &str, path: &str) -> Result<Value, String> {
    if !valid_rev(rev) { return Err("That is not a commit or tag name.".into()); }
    if path.is_empty() || path.starts_with('-') || path.contains("..") || path.len() > 400 { return Err("That is not a file in this project.".into()); }
    let text = git(root, &["show", "--format=%H%x1f%an%x1f%at%x1f%s", "--no-color", "--relative", rev, "--", path]).ok_or("Git could not read that change.")?;
    let (head, patch) = text.split_once('\n').unwrap_or((&text, ""));
    let h: Vec<&str> = head.split(SEP).collect();
    let mut patch = patch.trim_start_matches('\n').to_string();
    let cut = patch.len() > 200_000;
    if cut { let mut n = 200_000; while !patch.is_char_boundary(n) { n -= 1; } patch.truncate(n); }
    Ok(json!({ "rev": rev, "path": path, "hash": h.first(), "author": h.get(1), "at": h.get(2).and_then(|x| x.parse::<u64>().ok()).map(|x| x * 1000),
        "subject": h.get(3), "patch": patch, "truncated": cut }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_parses_into_plain_summaries() {
        let log = "\u{1e}abc123\u{1f}abc\u{1f}Sakib\u{1f}1700000000\u{1f}Add login page\u{1f}tag: v0.2, origin/main\n\n12\t0\tPages/LoginPage.cs\n3\t1\tStepDefinitions/LoginSteps.cs\n create mode 100644 Pages/LoginPage.cs\n\u{1e}def456\u{1f}def\u{1f}Sakib\u{1f}1690000000\u{1f}fix: wrong total\u{1f}\n\n1\t1\tsrc/{old => new}/Bill.cs\n";
        let comp = |p: &str| -> &'static str { if p.contains("Pages") { "pages" } else if p.contains("Step") { "steps" } else { "models" } };
        let c = parse_log(log, &comp);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0]["tags"][0], "v0.2");
        assert_eq!(c[0]["kind"], "feature");
        assert_eq!(c[0]["files"][0]["status"], "added");
        assert!(c[0]["summary"].as_str().unwrap().contains("New: LoginPage.cs"), "{}", c[0]["summary"]);
        assert_eq!(c[1]["kind"], "fix");
        assert_eq!(c[1]["files"][0]["path"], "src/new/Bill.cs");
    }

    #[test]
    fn revisions_are_validated() {
        assert!(valid_rev("v1.2.0") && valid_rev("a1b2c3d") && valid_rev("HEAD~3"));
        assert!(!valid_rev("--output=x") && !valid_rev("a..b") && !valid_rev("a b"));
    }
}
