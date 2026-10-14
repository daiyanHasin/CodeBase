//! The context-pack builder: CodeBase's "librarian" job.
//!
//! You describe a task or paste an error. CodeBase picks the files that matter, follows
//! the links between them, and fills a token budget you choose (4k, 8k, 16k, 32k):
//!
//!   1. Start files   paths named in a stack trace, names you typed that a file defines,
//!                    then a BM25 keyword search over file names, definitions and summaries.
//!   2. Neighbours    what those files use, what uses them, their tests, and files that
//!                    Git says change together with them.
//!   3. Budget        full code for the top files, an outline (definitions only) for
//!                    neighbours, a one-line summary for the rest. Tokens = bytes / 4.
//!
//! Everything is plain Rust over the scan result: no AI, no network.

use crate::analyze::{self, FileInfo};
use crate::memory::Memory;
use crate::scan::ScanResult;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

/// Rough token estimate used everywhere: 4 bytes per token, rounded up.
pub fn estimate_tokens(s: &str) -> usize {
    s.len().div_ceil(4)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Full,
    Excerpt,
    Outline,
    Summary,
    Off,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        Some(match s {
            "full" => Mode::Full,
            "excerpt" => Mode::Excerpt,
            "outline" => Mode::Outline,
            "summary" => Mode::Summary,
            "off" => Mode::Off,
            _ => return None,
        })
    }
}

#[derive(Default)]
pub struct PackRequest {
    pub task: String,
    pub budget: usize,
    /// Files the user pinned (Shift+click on the map, "Add to Ask AI").
    pub pins: Vec<String>,
    /// Files to start from without a task ("Copy context for this file / folder / feature").
    pub seeds: Vec<String>,
    pub seed_reason: String,
    /// Per-file choices the user made in the list.
    pub modes: HashMap<String, Mode>,
    pub plain: bool,
    pub done: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct PackItem {
    pub path: String,
    pub mode: Mode,
    pub tokens: usize,
    /// Tokens this file would cost as full code (so the UI can say what switching costs).
    #[serde(rename = "fullTokens")]
    pub full_tokens: usize,
    pub score: f32,
    pub reasons: Vec<String>,
    /// false when the user chose the mode
    pub auto: bool,
    pub layer: &'static str,
    pub summary: String,
}

#[derive(Serialize, Debug)]
pub struct Pack {
    pub text: String,
    pub tokens: usize,
    pub budget: usize,
    #[serde(rename = "briefTokens")]
    pub brief_tokens: usize,
    pub items: Vec<PackItem>,
    /// Files and lines found in a pasted error.
    pub trace: Vec<TraceHit>,
    pub notes: Vec<String>,
    #[serde(rename = "overBudget")]
    pub over_budget: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct TraceHit {
    pub path: String,
    pub line: Option<usize>,
}

// =====================================================================
// Words
// =====================================================================

static CAMEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([a-z0-9])([A-Z])").unwrap());
static IDENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[A-Za-z_][A-Za-z0-9_]{3,}\b").unwrap());

/// English filler plus words every error message contains.
static QUERY_STOP: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    "a an the and or but if then so to of in on at by for with from into onto as is are was were be been being it its this that these those \
     me my we our you your he she they them their what which who whom when where why how can could should would will shall may might must \
     do does did done have has had not no yes please want need make made let also just more less very some any all each every there here than \
     too like same other another one two first last get got go going thing things way work works working broken break doesn don isn \
     error errors exception exceptions line lines file files null undefined call called calling method function class object value type \
     stack trace traceback most recent uncaught unhandled thrown throw at cannot can't unable failed fail fails failure warning fatal \
     js ts php py cs java src app http https www com org net".split_whitespace().collect()
});

pub fn tokenize(text: &str) -> Vec<String> {
    let spaced = CAMEL.replace_all(text, "$1 $2");
    spaced
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| w.len() >= 2 && !w.chars().all(|c| c.is_ascii_digit()))
        .map(|w| analyze::singular(&w))
        .filter(|w| !QUERY_STOP.contains(w.as_str()))
        .collect()
}

// =====================================================================
// Stack traces
// =====================================================================

const CODE_EXT: &str = "blade\\.php|php|jsx|tsx|mjs|cjs|js|ts|vue|svelte|py|rb|go|rs|java|kt|kts|cs|fs|vb|cpp|cc|hpp|c|h|swift|dart|scala|ex|exs|lua|scss|css|html|razor|cshtml|feature";

static TRACE_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?i)((?:[a-z]:)?(?:[\w.\-@~+]+[/\\])*[\w.\-@+]+\.(?:{CODE_EXT}))\b")).unwrap()
});
static TRACE_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(?::(\d+)|"?,? line (\d+)|\((\d+)[,):]|:line (\d+)| on line (\d+)| line (\d+))"#).unwrap()
});
/// .NET / Java frames without a path: "at MyApp.Services.BillService.Total(Bill b)"
static TRACE_FRAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bat\s+([\w.`<>+]+)\.([\w<>`]+)\s*\(").unwrap());

/// Finds project files (and line numbers) named in a pasted error or stack trace.
/// Matches by the longest path suffix, so absolute paths from another machine still work.
pub fn find_trace(text: &str, infos: &BTreeMap<String, FileInfo>) -> Vec<TraceHit> {
    let mut by_name: HashMap<String, Vec<&String>> = HashMap::new();
    for p in infos.keys() {
        by_name.entry(p.rsplit('/').next().unwrap_or(p).to_lowercase()).or_default().push(p);
    }
    let mut out: Vec<TraceHit> = Vec::new();
    let push = |path: &str, line: Option<usize>, out: &mut Vec<TraceHit>| {
        if let Some(h) = out.iter_mut().find(|h| h.path == path) {
            if h.line.is_none() { h.line = line; }
            return;
        }
        if out.len() < 25 { out.push(TraceHit { path: path.to_string(), line }); }
    };
    for m in TRACE_PATH.captures_iter(text) {
        let whole = m.get(1).unwrap();
        let raw = whole.as_str().replace('\\', "/");
        let after = &text[whole.end()..];
        let line = TRACE_LINE.captures(after).and_then(|c| (1..=6).find_map(|i| c.get(i)).and_then(|g| g.as_str().parse().ok()));
        let segs: Vec<&str> = raw.split('/').filter(|s| !s.is_empty() && *s != "." && !s.ends_with(':')).collect();
        let Some(name) = segs.last() else { continue };
        let Some(cands) = by_name.get(&name.to_lowercase()) else { continue };
        // Pick the candidate sharing the longest run of trailing folders with the trace path.
        let score = |p: &str| -> usize {
            let ps: Vec<&str> = p.split('/').collect();
            ps.iter().rev().zip(segs.iter().rev()).take_while(|(a, b)| a.eq_ignore_ascii_case(b)).count()
        };
        let best = cands.iter().map(|c| (score(c), *c)).max_by(|a, b| a.0.cmp(&b.0).then(b.1.len().cmp(&a.1.len())));
        if let Some((s, p)) = best {
            let unique = cands.len() == 1;
            if s >= 2 || unique { push(p, line, &mut out); }
        }
    }
    // Frames that name a class but no file (Release builds, JVM without debug info).
    let mut classes: HashMap<&str, Vec<&String>> = HashMap::new();
    for (p, i) in infos {
        for s in i.symbols.iter().filter(|s| s.kind == "class") { classes.entry(s.name.as_str()).or_default().push(p); }
    }
    for c in TRACE_FRAME.captures_iter(text) {
        let qualified = &c[1];
        let class = qualified.rsplit('.').next().unwrap_or(qualified).split(['`', '<', '+']).next().unwrap_or("");
        if let Some(files) = classes.get(class) {
            if files.len() == 1 { push(files[0], None, &mut out); }
        }
    }
    out
}

// =====================================================================
// Keyword search (BM25)
// =====================================================================

struct Doc<'a> {
    path: &'a str,
    tf: HashMap<String, f64>,
    len: f64,
}

/// Ranks files for the query words. Fields are weighted: file name x3, definitions x2,
/// folder names, summaries, your notes and the last commit message x1.
pub fn search(terms: &[String], scan: &ScanResult, notes: &HashMap<String, String>) -> Vec<(String, f64, Vec<String>)> {
    if terms.is_empty() { return vec![]; }
    let mut docs: Vec<Doc> = Vec::new();
    for (path, info) in &scan.infos {
        if info.kind != "code" || info.generated || info.layer == "media" { continue; }
        let mut tf: HashMap<String, f64> = HashMap::new();
        let mut add = |text: &str, w: f64| { for t in tokenize(text) { *tf.entry(t).or_default() += w; } };
        let (dir, file) = match path.rfind('/') { Some(i) => (&path[..i], &path[i + 1..]), None => ("", path.as_str()) };
        add(file.split('.').next().unwrap_or(file), 3.0);
        add(dir, 1.0);
        for s in info.symbols.iter().take(60) { add(&s.name, 2.0); }
        add(&info.summary, 1.0);
        if let Some(n) = notes.get(path) { add(n, 1.5); }
        if let Some(c) = scan.git.last_commit.get(path) { add(c, 0.5); }
        let len = tf.values().sum::<f64>().max(1.0);
        docs.push(Doc { path, tf, len });
    }
    let n = docs.len() as f64;
    if n == 0.0 { return vec![]; }
    let avg = docs.iter().map(|d| d.len).sum::<f64>() / n;
    let mut uniq: Vec<&String> = terms.iter().collect();
    uniq.sort();
    uniq.dedup();
    let idf: HashMap<&String, f64> = uniq.iter().map(|t| {
        let df = docs.iter().filter(|d| d.tf.contains_key(*t)).count() as f64;
        (*t, (1.0 + (n - df + 0.5) / (df + 0.5)).ln())
    }).collect();
    let (k1, b) = (1.2, 0.75);
    let mut out: Vec<(String, f64, Vec<String>)> = docs.iter().filter_map(|d| {
        let mut score = 0.0;
        let mut matched = Vec::new();
        for t in &uniq {
            if let Some(f) = d.tf.get(*t) {
                score += idf[t] * (f * (k1 + 1.0)) / (f + k1 * (1.0 - b + b * d.len / avg));
                matched.push((*t).clone());
            }
        }
        (score > 0.0).then(|| (d.path.to_string(), score, matched))
    }).collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    out
}

// =====================================================================
// Project brief (~500 tokens)
// =====================================================================

fn read(root: &Path, rel: &str) -> Option<String> {
    fs::read_to_string(root.join(rel)).ok()
}

fn how_to_run(root: &Path, scan: &ScanResult) -> Vec<String> {
    let has = |f: &str| scan.infos.contains_key(f);
    let mut cmds: Vec<String> = Vec::new();
    if let Some(p) = read(root, "package.json").and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
        let pm = if has("pnpm-lock.yaml") || root.join("pnpm-lock.yaml").exists() { "pnpm" } else if root.join("yarn.lock").exists() { "yarn" } else { "npm" };
        cmds.push(format!("{pm} install"));
        for k in ["dev", "start", "build", "test"] {
            if p["scripts"].get(k).is_some() { cmds.push(if pm == "npm" && k != "start" && k != "test" { format!("npm run {k}") } else { format!("{pm} {k}") }); }
        }
    }
    if has("artisan") {
        cmds.extend(["composer install", "php artisan migrate", "php artisan serve", "php artisan test"].map(String::from));
    } else if has("composer.json") {
        cmds.push("composer install".into());
    }
    if has("manage.py") { cmds.extend(["python manage.py migrate", "python manage.py runserver"].map(String::from)); }
    if has("requirements.txt") { cmds.insert(0, "pip install -r requirements.txt".into()); }
    if has("Cargo.toml") { cmds.extend(["cargo run", "cargo test"].map(String::from)); }
    if has("go.mod") { cmds.extend(["go run .", "go test ./..."].map(String::from)); }
    if has("pubspec.yaml") { cmds.extend(["flutter pub get", "flutter run"].map(String::from)); }
    if scan.infos.keys().any(|f| f.ends_with(".sln") || f.ends_with(".csproj")) {
        cmds.extend(["dotnet build", "dotnet test"].map(String::from));
        let runnable = scan.infos.keys().filter(|f| f.ends_with(".csproj")).find(|f| {
            read(root, f).map(|s| s.contains("Microsoft.NET.Sdk.Web") || s.contains("<OutputType>Exe") || s.contains("<OutputType>WinExe")).unwrap_or(false)
        });
        if let Some(p) = runnable { cmds.push(format!("dotnet run --project {p}")); }
    }
    if cmds.is_empty() {
        // Fall back to the commands in the README's install/run section.
        if let Some(readme) = scan.infos.keys().filter(|f| !f.contains('/') && f.to_lowercase().starts_with("readme")).min() {
            if let Some(text) = read(root, readme) {
                let mut in_section = false;
                let mut in_code = false;
                for line in text.lines() {
                    let t = line.trim();
                    if t.starts_with('#') {
                        let h = t.to_lowercase();
                        in_section = ["run", "start", "install", "getting started", "usage", "setup", "build"].iter().any(|k| h.contains(k));
                        continue;
                    }
                    if t.starts_with("```") { in_code = !in_code; continue; }
                    if in_section && in_code && !t.is_empty() && cmds.len() < 5 { cmds.push(t.to_string()); }
                }
            }
        }
    }
    cmds.dedup();
    cmds.truncate(7);
    cmds
}

/// A short "what is this project" block that goes at the top of every pack.
pub fn brief(root: &Path, scan: &ScanResult, memory: &Memory) -> String {
    let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "project".into());
    let mut out = vec![format!("Project: {name}")];
    let root_note = memory.view(".", None).note;
    if let Some(d) = root_note.or_else(|| scan.description.clone()) {
        out.push(format!("What it does: {}", clip(&d, 320)));
    }
    if !scan.stack.is_empty() { out.push(format!("Built with: {}", scan.stack.join(", "))); }

    let mut langs: BTreeMap<&str, usize> = BTreeMap::new();
    for i in scan.infos.values().filter(|i| i.kind == "code" && !matches!(i.layer, "docs" | "setup" | "media")) {
        *langs.entry(i.lang.as_str()).or_default() += 1;
    }
    let mut lv: Vec<(&str, usize)> = langs.into_iter().collect();
    lv.sort_by(|a, b| b.1.cmp(&a.1));
    let lang_text = lv.iter().take(4).map(|(l, n)| format!("{l} {n}")).collect::<Vec<_>>().join(", ");
    out.push(format!("Size: {} files{}", scan.infos.len(), if lang_text.is_empty() { String::new() } else { format!(" (code: {lang_text})") }));

    // Main folders: your notes first, otherwise what kind of files they hold.
    let mut folders: BTreeMap<&str, (usize, BTreeMap<&str, usize>)> = BTreeMap::new();
    for (p, i) in &scan.infos {
        if let Some(idx) = p.find('/') {
            let e = folders.entry(&p[..idx]).or_default();
            e.0 += 1;
            *e.1.entry(i.layer).or_default() += 1;
        }
    }
    let mut fv: Vec<_> = folders.into_iter().collect();
    fv.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    let lines: Vec<String> = fv.iter().take(8).map(|(d, (n, layers))| {
        let note = memory.view(d, None).note;
        let what = note.unwrap_or_else(|| {
            let top = layers.iter().max_by_key(|x| x.1).map(|x| *x.0).unwrap_or("logic");
            format!("{n} files, mostly {}", layer_word(top))
        });
        format!("- {d}/: {}", clip(&what, 140))
    }).collect();
    if !lines.is_empty() { out.push(format!("Main folders:\n{}", lines.join("\n"))); }

    // The design, not the folders: what kind of project, and the path data takes through it.
    let a = &scan.arch.json;
    let pipe: Vec<&str> = a["pipeline"].as_array().into_iter().flatten().filter_map(|c| c["name"].as_str()).collect();
    if let Some(kind) = a["archetype"]["name"].as_str() {
        out.push(if pipe.len() >= 2 { format!("Architecture: {kind}. Data moves: {}", pipe.join(" → ")) } else { format!("Architecture: {kind}") });
    }
    let journeys: Vec<&str> = scan.flows.journeys.iter().filter_map(|j| j["name"].as_str()).filter(|n| !n.ends_with("(not placed)")).take(10).collect();
    if !journeys.is_empty() { out.push(format!("User journeys, in order: {}", journeys.join(" → "))); }
    else {
        let feats: Vec<&str> = scan.features.iter().filter_map(|f| f["name"].as_str()).take(10).collect();
        if !feats.is_empty() { out.push(format!("Features (names used across files): {}", feats.join(", "))); }
    }

    let tests: Vec<&String> = scan.infos.iter().filter(|(_, i)| i.layer == "tests").map(|(p, _)| p).collect();
    if !tests.is_empty() {
        let mut dirs: Vec<&str> = tests.iter().map(|p| p.split('/').next().unwrap_or("")).collect();
        dirs.sort();
        dirs.dedup();
        out.push(format!("Tests: {} files, in {}", tests.len(), dirs.iter().take(4).map(|d| format!("{d}/")).collect::<Vec<_>>().join(", ")));
    }
    let run = how_to_run(root, scan);
    if !run.is_empty() { out.push(format!("How to run: {}", run.join(" · "))); }

    let mut text = out.join("\n");
    // Hard cap around 500 tokens.
    if text.len() > 2100 {
        let mut cut = 2100;
        while !text.is_char_boundary(cut) { cut -= 1; }
        text.truncate(cut);
        text.push('…');
    }
    text
}

fn layer_word(l: &str) -> &'static str {
    match l {
        "ui" => "screens",
        "server" => "server code",
        "data" => "data and database code",
        "tests" => "tests",
        "setup" => "settings and build files",
        "docs" => "documentation",
        "scripts" => "scripts",
        "media" => "images and media",
        _ => "logic",
    }
}

fn clip(s: &str, n: usize) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > n { format!("{}…", one.chars().take(n - 1).collect::<String>()) } else { one }
}

// =====================================================================
// Rendering one file in each mode
// =====================================================================

fn fence(content: &str) -> String {
    let mut longest = 2;
    let mut run = 0;
    for c in content.chars() {
        if c == '`' { run += 1; longest = longest.max(run); } else { run = 0; }
    }
    "`".repeat((longest + 1).max(3))
}

fn fence_lang(lang: &str) -> &str {
    match lang {
        "blade" => "php",
        "gherkin" => "gherkin",
        "config" => "ini",
        "shell" => "bash",
        other => other,
    }
}

fn file_text(root: &Path, path: &str) -> Option<String> {
    let bytes = fs::read(root.join(path)).ok()?;
    if bytes.iter().take(8000).any(|b| *b == 0) { return None; }
    Some(String::from_utf8_lossy(&bytes).replace("\r\n", "\n"))
}

fn render_full(path: &str, lang: &str, content: &str) -> String {
    let f = fence(content);
    format!("<file path=\"{path}\" included=\"full code\">\n{f}{}\n{}\n{f}\n</file>\n\n", fence_lang(lang), content.trim_end_matches('\n'))
}

/// Lines around the places an error points at, merged into a few windows.
fn render_excerpt(path: &str, lang: &str, content: &str, lines: &[usize], window: usize) -> String {
    let all: Vec<&str> = content.lines().collect();
    let mut ranges: Vec<(usize, usize)> = lines.iter().filter(|l| **l >= 1).map(|l| (l.saturating_sub(window).max(1), (l + window).min(all.len()))).collect();
    ranges.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for r in ranges {
        match merged.last_mut() {
            Some(last) if r.0 <= last.1 + 1 => last.1 = last.1.max(r.1),
            _ => merged.push(r),
        }
    }
    let mut body = String::new();
    for (a, b) in &merged {
        if *a > *b || *a > all.len() { continue; }
        body.push_str(&format!("// lines {a}-{b} of {}\n", all.len()));
        body.push_str(&all[a - 1..*b].join("\n"));
        body.push('\n');
    }
    let f = fence(&body);
    format!("<file path=\"{path}\" included=\"part of the file, around the error\">\n{f}{}\n{}{f}\n</file>\n\n", fence_lang(lang), body)
}

fn render_outline(path: &str, info: &FileInfo, summary: &str, uses: &[String]) -> String {
    let mut s = format!("<file path=\"{path}\" included=\"outline only (definitions, not the full file)\">\n{summary}\n");
    for sym in info.symbols.iter().filter(|s| s.kind != "heading" || info.lang == "markdown").take(40) {
        s.push_str("  ");
        s.push_str(&sym.sig);
        s.push('\n');
    }
    if !uses.is_empty() {
        s.push_str(&format!("  uses: {}\n", uses.iter().take(10).cloned().collect::<Vec<_>>().join(", ")));
    }
    s.push_str("</file>\n\n");
    s
}

fn render_summary(path: &str, summary: &str) -> String {
    format!("- {path}: {summary}\n")
}

// =====================================================================
// Building the pack
// =====================================================================

struct Cand {
    path: String,
    score: f32,
    reasons: Vec<String>,
    tier: u8, // 0 = start file, 1 = neighbour, 2 = weaker match
    lines: Vec<usize>,
}

fn short(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Keeps a long pasted log readable and inside the budget: head and tail.
fn trim_task(task: &str, max_tokens: usize) -> String {
    if estimate_tokens(task) <= max_tokens { return task.trim().to_string(); }
    let lines: Vec<&str> = task.lines().collect();
    let mut head = 40usize.min(lines.len());
    let mut tail = 15usize.min(lines.len().saturating_sub(head));
    loop {
        let t = format!("{}\n… ({} lines left out) …\n{}", lines[..head].join("\n"), lines.len() - head - tail, lines[lines.len() - tail..].join("\n"));
        if estimate_tokens(&t) <= max_tokens || head <= 3 {
            let mut t = t;
            let cap = max_tokens * 4;
            if t.len() > cap { let mut c = cap; while !t.is_char_boundary(c) { c -= 1; } t.truncate(c); t.push('…'); }
            return t;
        }
        head = head * 2 / 3;
        tail = tail * 2 / 3;
    }
}

pub fn build(req: &PackRequest, scan: &ScanResult, memory: &Memory, root: &Path) -> Pack {
    let budget = req.budget.clamp(1000, 200_000);
    let notes: HashMap<String, String> = scan.infos.iter().filter_map(|(p, i)| memory.view(p, Some(&i.hash)).note.map(|n| (p.clone(), n))).collect();
    let summary_of = |p: &str| -> String {
        notes.get(p).cloned().unwrap_or_else(|| scan.infos.get(p).map(|i| i.summary.clone()).unwrap_or_default())
    };
    let task = req.task.trim();
    let mut notes_out: Vec<String> = Vec::new();

    // ---------- 1. start files ----------
    let mut cands: Vec<Cand> = Vec::new();
    let upsert = |cands: &mut Vec<Cand>, path: &str, score: f32, reason: String, tier: u8, line: Option<usize>| {
        if !scan.infos.contains_key(path) { return; }
        if let Some(c) = cands.iter_mut().find(|c| c.path == path) {
            if score > c.score { c.score = score; }
            if tier < c.tier { c.tier = tier; }
            if !reason.is_empty() && !c.reasons.contains(&reason) && c.reasons.len() < 4 { c.reasons.push(reason); }
            if let Some(l) = line { if !c.lines.contains(&l) { c.lines.push(l); } }
            return;
        }
        cands.push(Cand { path: path.to_string(), score, reasons: if reason.is_empty() { vec![] } else { vec![reason] }, tier, lines: line.into_iter().collect() });
    };

    for p in &req.pins { upsert(&mut cands, p, 2.0, "You pinned it".into(), 0, None); }
    let seed_reason = if req.seed_reason.is_empty() { "You asked for this".to_string() } else { req.seed_reason.clone() };
    for (i, p) in req.seeds.iter().enumerate() {
        let imp = scan.importance.get(p).copied().unwrap_or(0.0);
        upsert(&mut cands, p, 1.6 - (i as f32 * 0.01).min(0.3) + imp * 0.1, seed_reason.clone(), 0, None);
    }

    let trace = if task.is_empty() { vec![] } else { find_trace(task, &scan.infos) };
    for (i, h) in trace.iter().enumerate() {
        let why = match h.line { Some(l) => format!("Named in the error (line {l})"), None => "Named in the error".to_string() };
        upsert(&mut cands, &h.path, 1.5 - (i as f32 * 0.05).min(0.5), why, 0, h.line);
    }
    if !trace.is_empty() {
        notes_out.push(format!("Found {} in the error you pasted.", plural(trace.len(), "project file")));
    }

    // Exact names: "BillService" or "calculateTotal" typed in the task.
    if !task.is_empty() {
        let mut defs: HashMap<&str, Vec<&String>> = HashMap::new();
        for (p, i) in &scan.infos {
            if i.generated { continue; }
            for s in i.symbols.iter().filter(|s| s.kind != "heading" && s.kind != "section") { defs.entry(s.name.as_str()).or_default().push(p); }
        }
        let mut seen = HashSet::new();
        for m in IDENT.find_iter(task) {
            let w = m.as_str();
            if !seen.insert(w) || w.len() < 5 || QUERY_STOP.contains(w.to_lowercase().as_str()) { continue; }
            if let Some(files) = defs.get(w) {
                if files.len() <= 3 {
                    for f in files { upsert(&mut cands, f, 1.3, format!("Defines {w}"), 0, None); }
                }
            }
        }
    }

    // Keyword search.
    let terms = tokenize(task);
    let hits = search(&terms, scan, &notes);
    if let Some(best) = hits.first().map(|h| h.1) {
        let strong_limit = if cands.iter().any(|c| c.tier == 0) { 2 } else { 5 };
        let mut strong = 0;
        for (path, score, matched) in hits.iter().take(30) {
            let rel = (score / best) as f32;
            let why = format!("Matches your words: {}", matched.iter().take(4).cloned().collect::<Vec<_>>().join(", "));
            if rel >= 0.45 && strong < strong_limit {
                strong += 1;
                upsert(&mut cands, path, 0.6 + 0.5 * rel, why, 0, None);
            } else if rel >= 0.15 {
                upsert(&mut cands, path, 0.1 + 0.25 * rel, why, 2, None);
            }
        }
    }
    if cands.is_empty() && !task.is_empty() {
        notes_out.push("No file clearly matches your words. Try the names used in the app, paste the error, or add files yourself.".into());
    }

    // ---------- 2. neighbours of start files ----------
    let starts: Vec<(String, f32)> = cands.iter().filter(|c| c.tier == 0).map(|c| (c.path.clone(), c.score)).collect();
    let empty: Vec<String> = Vec::new();
    for (s, sc) in starts.iter().take(12) {
        let base = sc.min(1.2);
        for t in scan.imports_of.get(s).unwrap_or(&empty) { upsert(&mut cands, t, base * 0.5, format!("{} uses it", short(s)), 1, None); }
        for t in scan.imported_by.get(s).unwrap_or(&empty).iter().take(15) { upsert(&mut cands, t, base * 0.42, format!("It uses {}", short(s)), 1, None); }
        for t in scan.tested_by.get(s).unwrap_or(&empty) { upsert(&mut cands, t, base * 0.48, format!("Tests {}", short(s)), 1, None); }
        for t in scan.tests_of.get(s).unwrap_or(&empty) { upsert(&mut cands, t, base * 0.45, format!("Checked by {}", short(s)), 1, None); }
        if let Some(co) = scan.git.cochange.get(s) {
            for (t, strength, n) in co { upsert(&mut cands, t, base * 0.4 * strength, format!("Changed together with {} ({n} commits)", short(s)), 1, None); }
        }
    }
    // Tie-break by importance so central files win among equals.
    for c in cands.iter_mut() { c.score += scan.importance.get(&c.path).copied().unwrap_or(0.0) * 0.05; }
    cands.retain(|c| scan.infos.get(&c.path).map(|i| i.kind == "code" && !i.generated).unwrap_or(false) || c.tier == 0);
    cands.sort_by(|a, b| a.tier.min(1).cmp(&b.tier.min(1)).then(b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)).then(a.path.cmp(&b.path)));
    // User-chosen files that the search did not find still belong in the list.
    for (p, _) in &req.modes { upsert(&mut cands, p, 0.05, "You added it".into(), 2, None); }
    cands.truncate(80);

    // ---------- 3. fixed parts ----------
    let brief_text = brief(root, scan, memory);
    let brief_block = format!("<project_brief>\n{brief_text}\n</project_brief>\n\n");
    let stack = if scan.stack.is_empty() { String::new() } else { format!(" ({})", scan.stack.iter().take(3).cloned().collect::<Vec<_>>().join(", ")) };
    let is_error = !trace.is_empty();
    let task_cap = (budget / 4).max(250);
    let task_text = if task.is_empty() {
        "(Describe here what you want to change.)".to_string()
    } else {
        trim_task(task, task_cap)
    };
    let task_block = if is_error { format!("<error>\n{task_text}\n</error>\n\n") } else { format!("<task>\n{task_text}\n</task>\n\n") };
    let done_block = if req.done.trim().is_empty() { String::new() } else { format!("<done_when>\n{}\n</done_when>\n\n", req.done.trim()) };

    let instructions = |editable: &[String]| -> String {
        let may = if editable.is_empty() { "none yet: ask me for the files you need".to_string() } else { editable.join(", ") };
        format!(
"<instructions>
You are helping me change an existing project{stack}.{plain}
A local tool (CodeBase) picked these files for the task and fitted them to your context window.
- Files marked \"full code\" are complete. You may change these: {may}.
- \"Outline\" files show only definitions; \"related files\" show one line each. Do not rewrite them from memory. If you need one in full, reply NEED FILES: path1, path2 and stop.
- Change only what the task needs. Keep names, structure and style as they are.
- Return every file you change in full, under a heading with its exact path. Never write \"rest of the code unchanged\".
- Do not invent file paths. If a new file is needed, give its full path and full content.
- The links between files were found by reading the code, not running it, so run-time wiring may be missing. If the cause is probably in a file not shown, say which one.
</instructions>\n\n",
            plain = if req.plain { " I am not a professional programmer: explain in plain words and define any technical term you use." } else { "" },
        )
    };
    let answer = if is_error && req.plain {
        "<answer_format>\n1. What causes the error, in two or three plain sentences.\n2. Every changed file in full, each under a heading with its exact path.\n3. Step by step: which files to replace and any command to run (say exactly where to type it).\n4. How to check the error is gone.\n</answer_format>\n"
    } else if is_error {
        "<answer_format>\n1. Root cause in two to four lines.\n2. Every changed file in full, each under a heading with its exact path.\n3. Anything to run afterwards, or \"Nothing to run\".\n</answer_format>\n"
    } else if req.plain {
        "<answer_format>\n1. In two or three plain sentences: what you will change and why.\n2. Every changed file in full, each under a heading with its exact path.\n3. Step by step: which files to replace and any command to run (say exactly where to type it).\n4. How to check it worked: what I should see on screen.\n</answer_format>\n"
    } else {
        "<answer_format>\n1. Your plan in two to four lines.\n2. Every changed file in full, each under a heading with its exact path.\n3. Anything to run afterwards (migrations, installs, cache clears), or \"Nothing to run\".\n</answer_format>\n"
    };

    // The instruction block lists editable files, so reserve room for the list up front.
    let reserve_paths: usize = cands.iter().take(8).map(|c| c.path.len() + 2).sum::<usize>().div_ceil(4) + 4;
    let mut fixed = estimate_tokens(&instructions(&[])) + reserve_paths + estimate_tokens(&task_block) + estimate_tokens(&done_block) + estimate_tokens(answer);
    let mut brief_used = brief_block.clone();
    if fixed + estimate_tokens(&brief_used) > budget / 2 {
        // Tiny budgets: keep only the first lines of the brief.
        let short_brief: String = brief_text.lines().take(3).collect::<Vec<_>>().join("\n");
        brief_used = format!("<project_brief>\n{short_brief}\n</project_brief>\n\n");
    }
    fixed += estimate_tokens(&brief_used);
    let section_overhead = 12; // "<related_files>" wrappers
    let available = budget.saturating_sub(fixed + section_overhead);
    let mut remaining = available;

    // ---------- 4. fill ----------
    struct Slot {
        mode: Mode,
        chosen: bool,
        full: Option<String>,
        excerpt: Option<String>,
        outline: Option<String>,
        summary_line: String,
        summary: String,
    }
    impl Slot {
        fn text(&self, m: Mode) -> Option<&str> {
            match m {
                Mode::Full => self.full.as_deref(),
                Mode::Excerpt => self.excerpt.as_deref(),
                Mode::Outline => self.outline.as_deref(),
                Mode::Summary => Some(self.summary_line.as_str()),
                Mode::Off => Some(""),
            }
        }
        fn cost(&self, m: Mode) -> Option<usize> { self.text(m).map(estimate_tokens) }
    }

    let mut slots: Vec<Slot> = Vec::new();
    let mut over = false;
    let mut first_start = true;
    let mut summaries = 0;
    for c in &cands {
        let info = &scan.infos[&c.path];
        let summary = clip(&summary_of(&c.path), 160);
        let content = if info.kind == "code" { file_text(root, &c.path) } else { None };
        let uses = scan.imports_of.get(&c.path).cloned().unwrap_or_default();
        let slot = Slot {
            mode: Mode::Off,
            chosen: false,
            full: content.as_ref().map(|t| render_full(&c.path, &info.lang, t)),
            excerpt: match (&content, c.lines.is_empty()) {
                (Some(t), false) => Some(render_excerpt(&c.path, &info.lang, t, &c.lines, 30)),
                _ => None,
            },
            outline: (!info.symbols.is_empty()).then(|| render_outline(&c.path, info, &summary, &uses)),
            summary_line: render_summary(&c.path, &summary),
            summary,
        };
        let fits = |m: Mode, limit: usize| slot.cost(m).map(|x| x <= limit).unwrap_or(false);
        let chosen = req.modes.get(&c.path).copied();
        let mode = match chosen {
            Some(m) if slot.cost(m).is_some() => m,
            Some(Mode::Full) | Some(Mode::Excerpt) => if slot.outline.is_some() { Mode::Outline } else { Mode::Summary },
            Some(_) => Mode::Summary,
            None if c.tier == 0 => {
                // Start files: full code if it fits (any one file may take at most ~60% of what is
                // left, except the first), else the part around the error, else an outline.
                let cap = if first_start { remaining } else { remaining * 6 / 10 };
                if fits(Mode::Full, cap) { Mode::Full }
                else if fits(Mode::Excerpt, remaining * 6 / 10) { Mode::Excerpt }
                else if fits(Mode::Outline, remaining / 3) { Mode::Outline }
                else if fits(Mode::Summary, remaining) { Mode::Summary }
                else { Mode::Off }
            }
            None if c.tier == 1 && remaining > available / 5 && fits(Mode::Outline, remaining / 4) => Mode::Outline,
            None if summaries < 40 && fits(Mode::Summary, remaining) => Mode::Summary,
            None => Mode::Off,
        };
        if c.tier == 0 && matches!(mode, Mode::Full | Mode::Excerpt) { first_start = false; }
        if mode == Mode::Summary { summaries += 1; }
        let used = slot.cost(mode).unwrap_or(0);
        if used > remaining { over = true; remaining = 0; } else { remaining -= used; }
        slots.push(Slot { mode, chosen: chosen.is_some(), ..slot });
    }

    // Spare room: upgrade the best-ranked neighbours to full code (then outlines), in rank order,
    // so the budget you picked is actually used. One file may take at most a third of the budget.
    for target in [Mode::Full, Mode::Outline] {
        for (i, c) in cands.iter().enumerate() {
            let slot = &slots[i];
            if slot.chosen || c.tier > 1 || slot.mode == Mode::Full || (target == Mode::Outline && slot.mode != Mode::Summary && slot.mode != Mode::Off) { continue; }
            if c.tier == 1 && target == Mode::Full && c.score < 0.2 { continue; }
            let (Some(new_cost), Some(old_cost)) = (slot.cost(target), slot.cost(slot.mode)) else { continue };
            if new_cost <= old_cost { continue; }
            let extra = new_cost - old_cost;
            if extra <= remaining && new_cost <= available / 3 {
                remaining -= extra;
                slots[i].mode = target;
            }
        }
    }

    let mut items: Vec<PackItem> = Vec::new();
    let mut chunks_full: Vec<String> = Vec::new();
    let mut chunks_outline: Vec<String> = Vec::new();
    let mut chunks_summary: Vec<String> = Vec::new();
    let mut editable: Vec<String> = Vec::new();
    for (c, slot) in cands.iter().zip(&slots) {
        let text = slot.text(slot.mode).unwrap_or("").to_string();
        match slot.mode {
            Mode::Full | Mode::Excerpt => { chunks_full.push(text.clone()); editable.push(c.path.clone()); }
            Mode::Outline => chunks_outline.push(text.clone()),
            Mode::Summary => chunks_summary.push(text.clone()),
            Mode::Off => {}
        }
        let mut reasons = c.reasons.clone();
        if !slot.chosen && slot.mode == Mode::Off { reasons.push("Left out: no room in the budget".into()); }
        items.push(PackItem {
            path: c.path.clone(), mode: slot.mode, tokens: estimate_tokens(&text), full_tokens: slot.cost(Mode::Full).unwrap_or(0),
            score: (c.score * 100.0).round() / 100.0, reasons, auto: !slot.chosen, layer: scan.infos[&c.path].layer, summary: slot.summary.clone(),
        });
    }

    // ---------- 5. assemble ----------
    let mut text = String::new();
    text.push_str(&instructions(&editable));
    text.push_str(&brief_used);
    text.push_str(&task_block);
    text.push_str(&done_block);
    for c in &chunks_full { text.push_str(c); }
    for c in &chunks_outline { text.push_str(c); }
    if !chunks_summary.is_empty() {
        text.push_str("<related_files>\n");
        for c in &chunks_summary { text.push_str(c); }
        text.push_str("</related_files>\n\n");
    }
    text.push_str(answer);

    let tokens = estimate_tokens(&text);
    if tokens > budget { over = true; }
    if over { notes_out.push("Your choices go over the budget. Switch some files to Outline or Summary, or pick a bigger budget.".into()); }
    if items.iter().any(|i| i.auto && i.mode == Mode::Off) {
        notes_out.push("Some related files did not fit and were left out. Turn them on if they matter.".into());
    }
    Pack { text, tokens, budget, brief_tokens: estimate_tokens(&brief_used), items, trace, notes: notes_out, over_budget: over }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::Scanner;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    fn scan_fixture(name: &str) -> (std::path::PathBuf, ScanResult, Memory) {
        let root = fixture(name);
        let mut memory = Memory::load(&root);
        let mut s = Scanner::new(root.clone());
        let r = s.scan(&mut memory);
        (root, r, memory)
    }

    #[test]
    fn tokens_are_bytes_over_four() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    #[test]
    fn tokenizer_splits_camel_case_and_drops_filler() {
        let t = tokenize("Add a paidOn date to the BillService please");
        assert!(t.contains(&"paid".to_string()));
        assert!(t.contains(&"bill".to_string()));
        assert!(t.contains(&"service".to_string()));
        assert!(!t.contains(&"the".to_string()));
    }

    #[test]
    fn python_traceback_finds_file_and_line() {
        let (_, scan, _) = scan_fixture("python");
        let err = "Traceback (most recent call last):\n  File \"/home/someone/shop/app/billing.py\", line 7, in total\n    return sum(x.amount for x in items)\nAttributeError: 'NoneType' object has no attribute 'amount'";
        let hits = find_trace(err, &scan.infos);
        assert_eq!(hits.first(), Some(&TraceHit { path: "app/billing.py".into(), line: Some(7) }));
    }

    #[test]
    fn dotnet_trace_finds_file_and_line() {
        let (_, scan, _) = scan_fixture("csharp");
        let err = "System.NullReferenceException: Object reference not set\n   at MyApp.Services.BillService.Total(Bill bill) in C:\\work\\MyApp\\Services\\BillService.cs:line 9";
        let hits = find_trace(err, &scan.infos);
        assert_eq!(hits.first(), Some(&TraceHit { path: "MyApp/Services/BillService.cs".into(), line: Some(9) }));
    }

    #[test]
    fn js_trace_with_line_and_column() {
        let (_, scan, _) = scan_fixture("javascript");
        let err = "TypeError: Cannot read properties of undefined (reading 'total')\n    at formatBill (webpack-internal:///./src/utils/format.js:3:20)";
        let hits = find_trace(err, &scan.infos);
        assert_eq!(hits.first(), Some(&TraceHit { path: "src/utils/format.js".into(), line: Some(3) }));
    }

    #[test]
    fn php_trace_on_line() {
        let (_, scan, _) = scan_fixture("php");
        let err = "ErrorException: Undefined variable $bill in /var/www/html/app/Http/Controllers/BillController.php on line 12";
        let hits = find_trace(err, &scan.infos);
        assert_eq!(hits.first(), Some(&TraceHit { path: "app/Http/Controllers/BillController.php".into(), line: Some(12) }));
    }

    #[test]
    fn search_ranks_the_obvious_file_first() {
        let (_, scan, _) = scan_fixture("javascript");
        let hits = search(&tokenize("change how the bill list looks"), &scan, &HashMap::new());
        assert_eq!(hits[0].0, "src/components/BillList.jsx");
    }

    #[test]
    fn pack_never_exceeds_budget_in_auto_mode() {
        for fx in ["javascript", "python", "php", "csharp", "rust"] {
            let (root, scan, memory) = scan_fixture(fx);
            for budget in [1000, 2000, 4000, 8000, 32000] {
                let req = PackRequest { task: "fix the bill total and the bill list".into(), budget, plain: true, ..Default::default() };
                let pack = build(&req, &scan, &memory, &root);
                assert!(pack.tokens <= budget, "{fx} at {budget}: {} tokens", pack.tokens);
                assert_eq!(pack.tokens, estimate_tokens(&pack.text));
            }
        }
    }

    #[test]
    fn pack_puts_the_error_file_in_full_and_its_neighbours_after() {
        let (root, scan, memory) = scan_fixture("csharp");
        let err = "at MyApp.Services.BillService.Total(Bill bill) in C:\\x\\MyApp\\Services\\BillService.cs:line 9";
        let pack = build(&PackRequest { task: err.into(), budget: 8000, ..Default::default() }, &scan, &memory, &root);
        let first = &pack.items[0];
        assert_eq!(first.path, "MyApp/Services/BillService.cs");
        assert_eq!(first.mode, Mode::Full);
        let paths: Vec<&str> = pack.items.iter().map(|i| i.path.as_str()).collect();
        assert!(paths.contains(&"MyApp/Bill.cs"), "uses Bill: {paths:?}");
        assert!(paths.contains(&"MyApp.Tests/BillServiceTests.cs"), "its test: {paths:?}");
        assert!(pack.text.contains("<error>"));
    }

    #[test]
    fn user_choices_override_auto_modes() {
        let (root, scan, memory) = scan_fixture("javascript");
        let mut modes = HashMap::new();
        modes.insert("src/components/BillList.jsx".to_string(), Mode::Off);
        modes.insert("src/api/client.js".to_string(), Mode::Full);
        let pack = build(&PackRequest { task: "bill list".into(), budget: 8000, modes, ..Default::default() }, &scan, &memory, &root);
        let get = |p: &str| pack.items.iter().find(|i| i.path == p).unwrap();
        assert_eq!(get("src/components/BillList.jsx").mode, Mode::Off);
        assert!(!get("src/components/BillList.jsx").auto);
        assert_eq!(get("src/api/client.js").mode, Mode::Full);
        assert!(!pack.text.contains("<file path=\"src/components/BillList.jsx\""));
    }

    #[test]
    fn seeds_work_without_a_task() {
        let (root, scan, memory) = scan_fixture("python");
        let req = PackRequest { seeds: vec!["app/billing.py".into()], budget: 4000, ..Default::default() };
        let pack = build(&req, &scan, &memory, &root);
        assert_eq!(pack.items[0].path, "app/billing.py");
        assert_eq!(pack.items[0].mode, Mode::Full);
        assert!(pack.text.contains("Describe here what you want to change"));
    }

    #[test]
    fn brief_mentions_stack_and_how_to_run() {
        let (root, scan, memory) = scan_fixture("javascript");
        let b = brief(&root, &scan, &memory);
        assert!(b.contains("React"), "{b}");
        assert!(b.contains("npm run dev"), "{b}");
        assert!(estimate_tokens(&b) <= 560);
    }
}
