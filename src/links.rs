//! Relationships between files that are not plain import lines. All of it is
//! deterministic: no AI, no network.
//!
//!   type references  C#, Java, Kotlin, Go, Swift, Dart import whole namespaces, so we link
//!                    a file to the file that declares a type it mentions (BillService -> BillService.cs)
//!   tests            BillServiceTests.cs -> BillService.cs, test_bill.py -> bill.py
//!   test steps       SpecFlow / Reqnroll / Cucumber / behave: a .feature line -> the step definition file
//!   co-change        files that were changed in the same Git commits
//!   importance       PageRank over the import graph: files many others depend on score high

use crate::analyze::FileInfo;
use regex::Regex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::process::Command;

/// Which languages share a type namespace for reference linking.
fn family(lang: &str) -> Option<&'static str> {
    match lang {
        "csharp" => Some("dotnet"),
        "java" | "kotlin" => Some("jvm"),
        "go" => Some("go"),
        "swift" => Some("swift"),
        "dart" => Some("dart"),
        _ => None,
    }
}

/// Links files to the files that declare the types they mention.
/// A type counts only if at most 3 files declare it (partial classes are fine,
/// a name declared everywhere tells us nothing).
pub fn type_ref_edges(infos: &BTreeMap<String, FileInfo>) -> HashMap<String, Vec<String>> {
    let mut declared: HashMap<(&'static str, &str), Vec<&String>> = HashMap::new();
    for (path, info) in infos {
        let Some(fam) = family(&info.lang) else { continue };
        for s in info.symbols.iter().filter(|s| s.kind == "class") {
            declared.entry((fam, s.name.as_str())).or_default().push(path);
        }
    }
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for (path, info) in infos {
        let Some(fam) = family(&info.lang) else { continue };
        let mut targets: Vec<String> = Vec::new();
        for r in &info.refs {
            if let Some(files) = declared.get(&(fam, r.as_str())) {
                if files.len() > 3 { continue; }
                for f in files {
                    if *f != path && !targets.contains(f) { targets.push((*f).clone()); }
                }
            }
            if targets.len() >= 40 { break; }
        }
        if !targets.is_empty() { targets.sort(); out.insert(path.clone(), targets); }
    }
    out
}

/// "BillServiceTests.cs" -> "billservice", "test_bill.py" -> "bill", "bill.spec.ts" -> "bill".
pub fn test_subject_name(path: &str) -> Option<String> {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.split('.').next().unwrap_or(file);
    let lower_file = file.to_lowercase();
    let mut s = stem.to_string();
    let mut changed = false;
    for suf in ["IntegrationTests", "UnitTests", "Tests", "Test", "Specs", "Spec", "Steps", "StepDefinitions"] {
        if s.len() > suf.len() && s.ends_with(suf) { s.truncate(s.len() - suf.len()); changed = true; break; }
    }
    if !changed {
        if let Some(rest) = s.strip_prefix("test_") { s = rest.to_string(); changed = true; }
        else if let Some(rest) = s.strip_suffix("_test") { s = rest.to_string(); changed = true; }
        else if lower_file.contains(".test.") || lower_file.contains(".spec.") { changed = true; }
    }
    let s = s.trim_end_matches(['_', '-']).to_lowercase();
    (changed && s.len() >= 3).then_some(s)
}

/// Turns a step definition into a regex. Handles plain regexes (SpecFlow),
/// Cucumber expressions ({int}, {string}) and behave's {name} placeholders.
pub fn step_regex(pattern: &str) -> Option<Regex> {
    if let Some(re) = pattern.strip_prefix("re:") {
        return Regex::new(re).ok();
    }
    let looks_regex = pattern.starts_with('^') || pattern.ends_with('$') || pattern.contains("(.*)") || pattern.contains("(\\d+)") || pattern.contains("(.+)");
    if looks_regex {
        let p = pattern.trim_start_matches('^').trim_end_matches('$');
        return Regex::new(&format!("^(?:{p})$")).ok();
    }
    // Cucumber expression: escape literal text, swap {placeholders} for patterns.
    let mut out = String::from("^");
    let mut rest = pattern;
    while let Some(i) = rest.find('{') {
        out.push_str(&regex::escape(&rest[..i]));
        let Some(j) = rest[i..].find('}') else { break };
        let name = &rest[i + 1..i + j];
        out.push_str(match name {
            "int" => r"-?\d+",
            "float" | "double" | "decimal" => r"-?[\d.]+",
            "word" => r"\S+",
            "string" => r#"(?:"[^"]*"|'[^']*')"#,
            _ => ".*",
        });
        rest = &rest[i + j + 1..];
    }
    out.push_str(&regex::escape(rest));
    out.push('$');
    Regex::new(&out).ok()
}

/// Test links: test file -> code file it checks, and .feature file -> step definition files.
pub fn test_edges(infos: &BTreeMap<String, FileInfo>) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let mut add = |from: &str, to: &str| {
        let e = out.entry(from.to_string()).or_default();
        if from != to && !e.iter().any(|x| x == to) { e.push(to.to_string()); }
    };

    // 1. Name convention: BillServiceTests.cs -> BillService.cs
    let mut by_stem: HashMap<String, Vec<&String>> = HashMap::new();
    for (path, info) in infos.iter().filter(|(_, i)| i.kind == "code" && i.layer != "tests") {
        let file = path.rsplit('/').next().unwrap_or(path);
        let stem = file.split('.').next().unwrap_or(file).to_lowercase();
        by_stem.entry(stem).or_default().push(path);
        let _ = info;
    }
    for (path, info) in infos.iter().filter(|(_, i)| i.layer == "tests" && i.lang != "gherkin") {
        if let Some(name) = test_subject_name(path) {
            if let Some(cands) = by_stem.get(&name) {
                // Prefer the same language; skip when the name is ambiguous.
                let same: Vec<&&String> = cands.iter().filter(|c| infos[**c].lang == info.lang).collect();
                let pick: Vec<&String> = if !same.is_empty() { same.into_iter().copied().collect() } else { cands.clone() };
                if pick.len() <= 2 { for p in pick { add(path, p); } }
            }
        }
    }

    // 2. Gherkin steps -> step definitions.
    let mut defs: Vec<(&String, Regex)> = Vec::new();
    for (path, info) in infos.iter().filter(|(_, i)| i.lang != "gherkin" && !i.steps.is_empty()) {
        for p in &info.steps {
            if let Some(re) = step_regex(p) { defs.push((path, re)); }
        }
        if defs.len() > 5000 { break; }
    }
    if !defs.is_empty() {
        for (path, info) in infos.iter().filter(|(_, i)| i.lang == "gherkin") {
            let mut hit: HashSet<&String> = HashSet::new();
            for step in &info.steps {
                for (f, re) in &defs {
                    if !hit.contains(f) && re.is_match(step) { hit.insert(f); }
                }
            }
            let mut v: Vec<&&String> = hit.iter().collect();
            v.sort();
            for f in v { add(path, f); }
        }
    }
    out
}

/// What Git knows about files.
#[derive(Default, Clone)]
pub struct GitInfo {
    /// file -> [(other file, strength 0..1, shared commits)]
    pub cochange: HashMap<String, Vec<(String, f32, u32)>>,
    /// file -> subject line of the newest commit that touched it
    pub last_commit: HashMap<String, String>,
    /// file -> how many of the scanned commits touched it
    pub churn: HashMap<String, u32>,
}

const RECORD: char = '\u{1e}';

/// Parses `git log --name-only --format=%x1e%s` output.
pub fn parse_git_log(text: &str) -> GitInfo {
    let mut info = GitInfo::default();
    let mut pair: HashMap<(String, String), u32> = HashMap::new();
    for record in text.split(RECORD).filter(|r| !r.trim().is_empty()) {
        let mut lines = record.lines();
        let subject = lines.next().unwrap_or("").trim().to_string();
        let mut files: Vec<String> = lines.map(|l| l.trim().replace('\\', "/")).filter(|l| !l.is_empty()).collect();
        files.sort();
        files.dedup();
        for f in &files {
            *info.churn.entry(f.clone()).or_default() += 1;
            info.last_commit.entry(f.clone()).or_insert_with(|| subject.clone());
        }
        // Huge commits (renames, formatting, dependency bumps) say nothing about coupling.
        if files.len() < 2 || files.len() > 30 { continue; }
        for i in 0..files.len() {
            for j in i + 1..files.len() {
                *pair.entry((files[i].clone(), files[j].clone())).or_default() += 1;
            }
        }
    }
    for ((a, b), n) in pair {
        if n < 2 { continue; }
        let ca = info.churn.get(&a).copied().unwrap_or(1);
        let cb = info.churn.get(&b).copied().unwrap_or(1);
        let strength = n as f32 / ca.min(cb).max(1) as f32;
        if strength < 0.3 { continue; }
        info.cochange.entry(a.clone()).or_default().push((b.clone(), strength, n));
        info.cochange.entry(b).or_default().push((a, strength, n));
    }
    for v in info.cochange.values_mut() {
        v.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap_or(std::cmp::Ordering::Equal).then(y.2.cmp(&x.2)).then(x.0.cmp(&y.0)));
        v.truncate(8);
    }
    info
}

/// Current commit, or None when this is not a Git folder or Git is missing.
pub fn git_head(root: &Path) -> Option<String> {
    if !root.join(".git").exists() && !root.ancestors().any(|a| a.join(".git").exists()) { return None; }
    let out = Command::new("git").args(["rev-parse", "HEAD"]).current_dir(root).output().ok()?;
    if !out.status.success() { return None; }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Reads the last 500 commits. Paths come back relative to `root`, even when
/// the project is a sub-folder of a bigger repository.
pub fn read_git(root: &Path) -> Option<GitInfo> {
    let out = Command::new("git")
        .args(["log", "--no-merges", "-n", "500", "--relative", "--name-only", "--format=%x1e%s"])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() { return None; }
    Some(parse_git_log(&String::from_utf8_lossy(&out.stdout)))
}

/// PageRank over "A uses B" edges. Returns 0..1 per file, 1 = most depended on.
pub fn importance(files: &[String], uses: &HashMap<String, Vec<String>>) -> HashMap<String, f32> {
    let n = files.len();
    if n == 0 { return HashMap::new(); }
    let index: HashMap<&str, usize> = files.iter().enumerate().map(|(i, f)| (f.as_str(), i)).collect();
    let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (from, tos) in uses {
        let Some(&i) = index.get(from.as_str()) else { continue };
        for t in tos {
            if let Some(&j) = index.get(t.as_str()) { if i != j { out_edges[i].push(j); } }
        }
    }
    let d = 0.85f64;
    let mut rank = vec![1.0 / n as f64; n];
    for _ in 0..40 {
        let dangling: f64 = (0..n).filter(|i| out_edges[*i].is_empty()).map(|i| rank[i]).sum();
        let mut next = vec![(1.0 - d) / n as f64 + d * dangling / n as f64; n];
        for i in 0..n {
            if out_edges[i].is_empty() { continue; }
            let share = d * rank[i] / out_edges[i].len() as f64;
            for &j in &out_edges[i] { next[j] += share; }
        }
        rank = next;
    }
    let min = rank.iter().cloned().fold(f64::MAX, f64::min);
    let max = rank.iter().cloned().fold(0.0, f64::max);
    let span = (max - min).max(1e-12);
    files.iter().enumerate().map(|(i, f)| (f.clone(), (((rank[i] - min) / span).sqrt()) as f32)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_names_map_to_subjects() {
        assert_eq!(test_subject_name("MyApp.Tests/BillServiceTests.cs").as_deref(), Some("billservice"));
        assert_eq!(test_subject_name("tests/test_bill.py").as_deref(), Some("bill"));
        assert_eq!(test_subject_name("src/bill.spec.ts").as_deref(), Some("bill"));
        assert_eq!(test_subject_name("pkg/bill_test.go").as_deref(), Some("bill"));
        assert_eq!(test_subject_name("src/bill.ts"), None);
    }

    #[test]
    fn step_patterns_match_feature_lines() {
        assert!(step_regex(r"I have entered (.*) into the calculator").unwrap().is_match("I have entered 50 into the calculator"));
        assert!(step_regex("I have {int} cukes in my belly").unwrap().is_match("I have 42 cukes in my belly"));
        assert!(step_regex("the user {string} logs in").unwrap().is_match("the user \"sakib\" logs in"));
        assert!(!step_regex("I have {int} cukes").unwrap().is_match("I have many cukes"));
        assert!(step_regex(r"re:^the total is (\d+)$").unwrap().is_match("the total is 7"));
    }

    #[test]
    fn git_log_finds_files_that_change_together() {
        let log = "\u{1e}Add paid date\nsrc/Bill.cs\nsrc/BillView.cs\n\u{1e}Fix total\nsrc/Bill.cs\nsrc/BillView.cs\n\u{1e}Readme\nREADME.md\n";
        let g = parse_git_log(log);
        let co = &g.cochange["src/Bill.cs"];
        assert_eq!(co[0].0, "src/BillView.cs");
        assert_eq!(co[0].2, 2);
        assert_eq!(g.last_commit["src/Bill.cs"], "Add paid date");
        assert!(!g.cochange.contains_key("README.md"));
    }

    #[test]
    fn importance_favours_files_many_others_use() {
        let files: Vec<String> = ["a", "b", "c", "core"].iter().map(|s| s.to_string()).collect();
        let mut uses = HashMap::new();
        for f in ["a", "b", "c"] { uses.insert(f.to_string(), vec!["core".to_string()]); }
        let imp = importance(&files, &uses);
        assert!(imp["core"] > imp["a"]);
        assert!((imp["core"] - 1.0).abs() < 1e-6);
    }
}
