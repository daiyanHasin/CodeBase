//! Walks the repo (respecting .gitignore, built-in junk rules and your hidden list),
//! analyzes files (cached by modified time + size), links imports into
//! file-to-file edges, and builds the graph the UI renders.

use crate::analyze::{self, FileInfo};
use crate::arch::{self, Arch};
use crate::flows::{self, Flows};
use crate::links::{self, GitInfo};
use crate::memory::Memory;
use crate::understand;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::UNIX_EPOCH;

pub const MAX_FILES: usize = 8000;
const MAX_TEXT_BYTES: u64 = 1_000_000;

pub const BUILTIN_DIRS: &[&str] = &[
    ".git", ".hg", ".svn", "node_modules", "vendor", "bower_components", "dist", "build", "out", "target",
    ".next", ".nuxt", ".svelte-kit", ".output", "__pycache__", ".pytest_cache", ".mypy_cache", ".venv", "venv",
    ".idea", ".vscode", "coverage", ".cache", ".parcel-cache", ".turbo", ".gradle", "Pods", ".dart_tool",
    ".codebase", ".canopy", ".terraform", ".expo", ".angular",
    // .NET build output and IDE state
    "obj", ".vs", "TestResults", "BenchmarkDotNet.Artifacts",
];

/// True for folders that are never source code. `bin/` is only junk when it holds
/// .NET build output (Debug/Release/net8.0…); elsewhere it often holds real scripts.
pub fn is_junk_dir(name: &str, abs: &Path) -> bool {
    if BUILTIN_DIRS.contains(&name) { return true; }
    if name == "bin" {
        if ["Debug", "Release", "x64", "x86"].iter().any(|d| abs.join(d).is_dir()) { return true; }
        if let Ok(rd) = fs::read_dir(abs) {
            for e in rd.flatten().take(200) {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if n.starts_with("net") && e.path().is_dir() { return true; }
                if n.ends_with(".dll") || n.ends_with(".pdb") || n.ends_with(".exe") { return true; }
            }
        }
    }
    false
}
const BUILTIN_PATHS: &[&str] = &["storage/framework", "storage/logs", "bootstrap/cache", "public/build", "public/hot"];

pub static BUILTIN_FILES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|/)(package-lock\.json|yarn\.lock|pnpm-lock\.yaml|composer\.lock|poetry\.lock|Pipfile\.lock|Cargo\.lock|Gemfile\.lock|bun\.lockb|packages\.lock\.json|uv\.lock|go\.sum|pubspec\.lock|flake\.lock)$|\.min\.(js|css)$|\.map$|(^|/)\.DS_Store$|(^|/)Thumbs\.db$|\.log$|\.(pyc|pyo|class|o|obj|so|dll|pdb|exe|nupkg|suo|user|cache)$|\.(feature|razor|cshtml)\.cs$").unwrap()
});

pub struct ScanResult {
    pub graph: Value,
    pub infos: BTreeMap<String, FileInfo>,
    /// "A uses B": import lines plus type references.
    pub imports_of: HashMap<String, Vec<String>>,
    pub imported_by: HashMap<String, Vec<String>>,
    /// test file -> code (or step definitions) it checks
    pub tests_of: HashMap<String, Vec<String>>,
    /// code file -> tests that check it
    pub tested_by: HashMap<String, Vec<String>>,
    pub git: GitInfo,
    /// 0..1, 1 = the file most others depend on
    pub importance: HashMap<String, f32>,
    pub dirs: BTreeSet<String>,
    pub stack: Vec<String>,
    pub description: Option<String>,
    pub features: Vec<Value>,
    /// The design: components, data path and the m-ary tree (src/arch.rs).
    pub arch: Arch,
    /// End-to-end flows and journeys (src/flows.rs).
    pub flows: Flows,
}

pub struct Scanner {
    pub root: PathBuf,
    cache: HashMap<String, (u128, u64, FileInfo)>,
    git_cache: Option<(String, GitInfo)>,
}

fn rel_of(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

fn user_matcher(root: &Path, patterns: &[String]) -> Gitignore {
    let mut b = GitignoreBuilder::new(root);
    for p in patterns { let _ = b.add_line(None, p); }
    b.build().unwrap_or_else(|_| Gitignore::empty())
}

impl Scanner {
    pub fn new(root: PathBuf) -> Scanner { Scanner { root, cache: HashMap::new(), git_cache: None } }

    /// Git history, re-read only when the current commit changes.
    fn git(&mut self) -> Option<GitInfo> {
        let head = links::git_head(&self.root)?;
        if let Some((h, info)) = &self.git_cache {
            if *h == head { return Some(info.clone()); }
        }
        let info = links::read_git(&self.root)?;
        self.git_cache = Some((head, info.clone()));
        Some(info)
    }

    fn walk(&self, hidden: &[String]) -> (Vec<String>, bool) {
        let root = self.root.clone();
        let user = Arc::new(user_matcher(&root, hidden));
        let walker = WalkBuilder::new(&root)
            .hidden(false)
            .git_ignore(true)
            .git_exclude(true)
            .git_global(false)
            .ignore(false)
            .parents(false)
            .require_git(false)
            .follow_links(false)
            .filter_entry({
                let root = root.clone();
                let user = user.clone();
                move |e| {
                    let rel = rel_of(&root, e.path());
                    if rel.is_empty() { return true; }
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    if is_dir && is_junk_dir(e.file_name().to_string_lossy().as_ref(), e.path()) { return false; }
                    if BUILTIN_PATHS.iter().any(|b| rel == *b || rel.starts_with(&format!("{b}/"))) { return false; }
                    !user.matched_path_or_any_parents(&rel, is_dir).is_ignore()
                }
            })
            .build();
        let mut files = Vec::new();
        let mut truncated = false;
        for entry in walker.flatten() {
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
            let rel = rel_of(&root, entry.path());
            if BUILTIN_FILES.is_match(&rel) { continue; }
            if files.len() >= MAX_FILES { truncated = true; break; }
            files.push(rel);
        }
        files.sort();
        (files, truncated)
    }

    fn analyze(&mut self, rel: &str) -> Option<FileInfo> {
        let abs = self.root.join(rel);
        let meta = fs::metadata(&abs).ok()?;
        let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
        let size = meta.len();
        if let Some((m, s, info)) = self.cache.get(rel) {
            if *m == mtime && *s == size { return Some(info.clone()); }
        }
        let stamp = format!("{size}-{mtime}");
        let info = if analyze::is_asset_path(rel) {
            analyze::non_text(rel, size, stamp, "asset", "Image, font or other media (not read)".into())
        } else if size > MAX_TEXT_BYTES {
            analyze::non_text(rel, size, stamp, "large", format!("Large file ({} KB), not analyzed", size / 1024))
        } else {
            let buf = fs::read(&abs).ok()?;
            if buf.iter().take(8000).any(|b| *b == 0) {
                analyze::non_text(rel, size, stamp, "asset", "Binary file (not read)".into())
            } else {
                analyze::analyze_text(rel, &String::from_utf8_lossy(&buf), size)
            }
        };
        self.cache.insert(rel.to_string(), (mtime, size, info.clone()));
        Some(info)
    }

    fn read(&self, f: &str) -> Option<String> { fs::read_to_string(self.root.join(f)).ok() }

    fn psr4(&self) -> Vec<(String, String)> {
        let mut map = Vec::new();
        if let Some(c) = self.read("composer.json").and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
            for key in ["autoload", "autoload-dev"] {
                if let Some(obj) = c[key]["psr-4"].as_object() {
                    for (ns, dirs) in obj {
                        let list: Vec<String> = match dirs {
                            Value::String(s) => vec![s.clone()],
                            Value::Array(a) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
                            _ => vec![],
                        };
                        for d in list { map.push((ns.clone(), format!("{}/", d.trim_end_matches('/')))); }
                    }
                }
            }
        }
        map.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        map
    }

    fn stack(&self, set: &HashSet<String>) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        fn add(out: &mut Vec<String>, s: &str) { if !out.iter().any(|x| x == s) { out.push(s.to_string()) } }
        macro_rules! push { ($s:expr) => { add(&mut out, $s) }; }
        if set.contains("composer.json") {
            if let Some(c) = self.read("composer.json").and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
                let has = |k: &str| c["require"].get(k).is_some() || c["require-dev"].get(k).is_some();
                if has("laravel/framework") { push!("Laravel"); }
                if c["require"].as_object().map(|o| o.keys().any(|k| k.starts_with("symfony/"))).unwrap_or(false) && !has("laravel/framework") { push!("Symfony"); }
            }
            push!("PHP");
        }
        if set.contains("package.json") {
            if let Some(p) = self.read("package.json").and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
                let has = |k: &str| p["dependencies"].get(k).is_some() || p["devDependencies"].get(k).is_some();
                for (k, label) in [("next", "Next.js"), ("react", "React"), ("react-native", "React Native"), ("expo", "Expo"), ("vue", "Vue"), ("svelte", "Svelte"),
                    ("@angular/core", "Angular"), ("electron", "Electron"), ("express", "Express"), ("@nestjs/core", "NestJS"), ("vite", "Vite"),
                    ("tailwindcss", "Tailwind CSS"), ("prisma", "Prisma"), ("@prisma/client", "Prisma"), ("typescript", "TypeScript")] {
                    if has(k) { push!(label); }
                }
            }
            if !out.iter().any(|s| s == "TypeScript") { push!("JavaScript"); }
        }
        let py = ["requirements.txt", "pyproject.toml"].iter().filter(|f| set.contains(**f)).filter_map(|f| self.read(f)).collect::<String>().to_lowercase();
        if !py.is_empty() || set.iter().any(|f| f.ends_with(".py")) {
            for (k, label) in [("django", "Django"), ("fastapi", "FastAPI"), ("flask", "Flask"), ("streamlit", "Streamlit"), ("torch", "PyTorch"), ("pandas", "pandas")] {
                if py.contains(k) { push!(label); }
            }
            push!("Python");
        }
        if let Some(proj) = set.iter().filter(|f| f.ends_with(".csproj") || f.ends_with(".fsproj")).min() {
            let all: String = set.iter().filter(|f| f.ends_with(".csproj") || f.ends_with(".fsproj")).take(30).filter_map(|f| self.read(f)).collect();
            if all.contains("Microsoft.NET.Sdk.Web") || all.contains("Microsoft.AspNetCore") { push!("ASP.NET Core"); }
            for (k, label) in [("Microsoft.Maui", ".NET MAUI"), ("Avalonia", "Avalonia"), ("EntityFrameworkCore", "Entity Framework"), ("Reqnroll", "Reqnroll"),
                ("SpecFlow", "SpecFlow"), ("xunit", "xUnit"), ("NUnit", "NUnit"), ("MSTest", "MSTest"), ("Selenium", "Selenium"), ("Playwright", "Playwright")] {
                if all.contains(k) { push!(label); }
            }
            push!(if proj.ends_with(".fsproj") { "F#" } else { "C#" });
        }
        if set.contains("Cargo.toml") { push!("Rust"); }
        if set.contains("go.mod") { push!("Go"); }
        if set.contains("pom.xml") || set.contains("build.gradle") { push!("Java"); }
        if set.contains("pubspec.yaml") { push!("Flutter"); }
        out
    }

    /// One-line "what is this project" from README or package manifests.
    fn description(&self, set: &HashSet<String>) -> (Option<String>, &'static str) {
        static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!?\[([^\]]*)\]\([^)]*\)").unwrap());
        static MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[*_`>]+").unwrap());
        static TOML_DESC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?m)^description\s*=\s*"([^"]+)""#).unwrap());
        if let Some(readme) = set.iter().filter(|f| !f.contains('/') && f.to_lowercase().starts_with("readme")).min() {
            if let Some(text) = self.read(readme) {
                let mut para: Vec<String> = Vec::new();
                let mut in_code = false;
                for line in text.lines() {
                    let t = line.trim();
                    if t.starts_with("```") { in_code = !in_code; continue; }
                    if in_code { continue; }
                    let skip = t.starts_with('#') || t.starts_with('<') || t.starts_with("[![") || t.starts_with("![") || t.starts_with("---") || t.starts_with('|') || t.starts_with("- [");
                    if t.is_empty() || skip {
                        if !para.is_empty() { break; }
                        continue;
                    }
                    para.push(t.to_string());
                }
                let joined = MARK.replace_all(&LINK.replace_all(&para.join(" "), "$1"), "").trim().trim_start_matches('#').trim().to_string();
                if joined.len() > 20 {
                    let clipped: String = joined.chars().take(320).collect();
                    return (Some(clipped), "README");
                }
            }
        }
        for f in ["package.json", "composer.json"] {
            if let Some(d) = self.read(f).and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v["description"].as_str().map(String::from)) {
                if d.len() > 10 { return (Some(d), if f == "package.json" { "package.json" } else { "composer.json" }); }
            }
        }
        for f in ["Cargo.toml", "pyproject.toml"] {
            if let Some(c) = self.read(f).and_then(|s| TOML_DESC.captures(&s).map(|c| c[1].to_string())) {
                if c.len() > 10 { return (Some(c), if f == "Cargo.toml" { "Cargo.toml" } else { "pyproject.toml" }); }
            }
        }
        (None, "")
    }

    fn resolve(rel: &str, info: &FileInfo, set: &HashSet<String>, psr4: &[(String, String)]) -> Vec<String> {
        let dir = match rel.rfind('/') { Some(i) => &rel[..i], None => "" };
        let try_paths = |cands: Vec<String>| -> Option<String> { cands.into_iter().map(|c| normalize(&c)).find(|c| set.contains(c)) };
        const JS_EXT: &[&str] = &["", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".vue", ".svelte", ".json", ".css", ".scss"];
        let with_exts = |base: &str| -> Vec<String> {
            let mut v: Vec<String> = JS_EXT.iter().map(|e| format!("{base}{e}")).collect();
            v.extend(JS_EXT[1..].iter().map(|e| format!("{base}/index{e}")));
            v
        };
        let mut out = BTreeSet::new();
        for spec in &info.imports {
            let hit = if let Some(fqn) = spec.strip_prefix("php:") {
                psr4.iter().find(|(ns, _)| fqn.starts_with(ns.as_str()))
                    .and_then(|(ns, d)| try_paths(vec![format!("{d}{}.php", fqn[ns.len()..].replace('\\', "/"))]))
            } else if let Some(v) = spec.strip_prefix("view:") {
                let v = v.replace("::", "/").replace('.', "/");
                try_paths(vec![format!("resources/views/{v}.blade.php"), format!("resources/views/{v}/index.blade.php")])
            } else if let Some(v) = spec.strip_prefix("component:") {
                let v = v.replace('.', "/");
                try_paths(vec![format!("resources/views/components/{v}.blade.php"), format!("resources/views/components/{v}/index.blade.php")])
            } else if let Some(m) = spec.strip_prefix("crate:") {
                // use crate::scan -> <crate>/src/scan.rs
                let src = match rel.find("src/") { Some(i) => &rel[..i + 4], None => "" };
                try_paths(vec![format!("{src}{m}.rs"), format!("{src}{m}/mod.rs")])
            } else if let Some(j) = spec.strip_prefix("java:") {
                // import com.acme.bill.Bill -> any file ending in com/acme/bill/Bill.java (or .kt)
                let tail = j.replace('.', "/");
                let found = [".java", ".kt"].iter().find_map(|ext| {
                    let want = format!("{tail}{ext}");
                    set.iter().filter(|f| *f == &want || f.ends_with(&format!("/{want}"))).min().cloned()
                });
                found
            } else if let Some(d) = spec.strip_prefix("data:") {
                // A data file named by path in a string: try from this folder, from the root, then by ending.
                let near = if dir.is_empty() { d.to_string() } else { format!("{dir}/{d}") };
                try_paths(vec![near, d.to_string()]).or_else(|| {
                    let tail = format!("/{d}");
                    let mut hits: Vec<&String> = set.iter().filter(|f| f.ends_with(&tail)).collect();
                    hits.sort_by_key(|f| f.len());
                    hits.first().map(|f| (*f).clone())
                })
            } else if spec.starts_with("ns:") {
                None // C# namespaces are linked through type references instead
            } else if let Some(m) = spec.strip_prefix("mod:") {
                let base = if dir.is_empty() { String::new() } else { format!("{dir}/") };
                try_paths(vec![format!("{base}{m}.rs"), format!("{base}{m}/mod.rs")])
            } else if info.lang == "python" {
                if spec.starts_with('.') {
                    let dots = spec.chars().take_while(|c| *c == '.').count();
                    let mut base: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
                    let keep = base.len().saturating_sub(dots - 1);
                    base.truncate(keep);
                    let module = spec[dots..].replace('.', "/");
                    let mut parts: Vec<String> = base.iter().map(|s| s.to_string()).collect();
                    if !module.is_empty() { parts.push(module); }
                    let p = parts.join("/");
                    try_paths(vec![format!("{p}.py"), format!("{p}/__init__.py")])
                } else {
                    let p = spec.replace('.', "/");
                    try_paths(vec![format!("{p}.py"), format!("{p}/__init__.py"), format!("src/{p}.py"), format!("src/{p}/__init__.py")])
                }
            } else if spec.starts_with('.') {
                let base = if dir.is_empty() { spec.clone() } else { format!("{dir}/{spec}") };
                let mut hit = try_paths(with_exts(&base));
                if hit.is_none() && info.lang == "scss" {
                    let n = normalize(&base);
                    let (d, f) = match n.rfind('/') { Some(i) => (&n[..=i], &n[i + 1..]), None => ("", n.as_str()) };
                    hit = try_paths(vec![format!("{d}_{f}.scss"), format!("{n}.scss")]);
                }
                hit
            } else if let Some(s) = spec.strip_prefix("@/").or_else(|| spec.strip_prefix("~/")) {
                let mut c = with_exts(&format!("src/{s}"));
                c.extend(with_exts(&format!("resources/js/{s}")));
                c.extend(with_exts(s));
                try_paths(c)
            } else if let Some(s) = spec.strip_prefix('/') {
                try_paths(vec![s.to_string(), format!("public/{s}")])
            } else {
                None
            };
            if let Some(h) = hit { if h != rel { out.insert(h); } }
        }
        out.into_iter().collect()
    }

    pub fn scan(&mut self, memory: &mut Memory) -> ScanResult {
        let hidden = memory.config.hidden.clone();
        let (files, truncated) = self.walk(&hidden);
        let set: HashSet<String> = files.iter().cloned().collect();
        self.cache.retain(|k, _| set.contains(k));

        let mut infos: BTreeMap<String, FileInfo> = BTreeMap::new();
        for f in &files { if let Some(i) = self.analyze(f) { infos.insert(f.clone(), i); } }

        let psr4 = self.psr4();
        let refs = links::type_ref_edges(&infos);
        let mut imports_of: HashMap<String, Vec<String>> = HashMap::new();
        let mut imported_by: HashMap<String, Vec<String>> = HashMap::new();
        for (f, info) in &infos {
            let mut deps = Self::resolve(f, info, &set, &psr4);
            if let Some(r) = refs.get(f) {
                for t in r { if !deps.contains(t) { deps.push(t.clone()); } }
                deps.sort();
            }
            for d in &deps { imported_by.entry(d.clone()).or_default().push(f.clone()); }
            imports_of.insert(f.clone(), deps);
        }
        for v in imported_by.values_mut() { v.sort(); }

        // Mounted routers: prefix their routes with the mount path, so "/" in routes/rides.js
        // reads as "/api/rides". Laravel's routes/api.php is served under /api.
        let mut prefixes: HashMap<String, String> = HashMap::new();
        for (f, info) in &infos {
            for (prefix, spec) in &info.mounts {
                let mut probe = info.clone();
                probe.imports = vec![spec.clone()];
                if let Some(target) = Self::resolve(f, &probe, &set, &psr4).into_iter().next() { prefixes.insert(target, prefix.clone()); }
            }
        }
        if infos.contains_key("routes/api.php") { prefixes.entry("routes/api.php".into()).or_insert_with(|| "/api".into()); }
        for (f, prefix) in &prefixes {
            if let Some(info) = infos.get_mut(f) {
                for r in info.routes.iter_mut() {
                    if r.path.starts_with(prefix.as_str()) { continue; }
                    let base = prefix.trim_end_matches('/');
                    r.path = if r.path == "/" || r.path.is_empty() { base.to_string() } else { format!("{base}{}", r.path) };
                }
            }
        }

        // Screens a router shows are screens, wherever they live (CRA keeps them in components/).
        let mut by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        for (f, info) in &infos { for sy in info.symbols.iter().filter(|s| s.kind == "class" || s.kind == "function") { by_symbol.entry(sy.name.clone()).or_default().push(f.clone()); } }
        let mut screens: Vec<(String, String)> = Vec::new();
        for (f, info) in &infos {
            for (url, spec, ident) in &info.pages {
                let mut target = None;
                if !spec.is_empty() {
                    let mut probe = info.clone();
                    probe.imports = vec![spec.clone()];
                    target = Self::resolve(f, &probe, &set, &psr4).into_iter().next();
                }
                if target.is_none() && !ident.is_empty() {
                    target = by_symbol.get(ident).filter(|v| v.len() == 1).map(|v| v[0].clone())
                        .or_else(|| infos.keys().find(|k| k.rsplit('/').next().and_then(|n| n.split('.').next()) == Some(ident.as_str())).cloned());
                }
                if let Some(t) = target { if &t != f { screens.push((t, url.clone())); } }
            }
        }
        for (t, url) in &screens {
            if let Some(i) = infos.get_mut(t) {
                if !i.signals.contains(&"screen") { i.signals.push("screen"); }
                if i.screen_url.is_empty() || url.len() < i.screen_url.len() { i.screen_url = url.clone(); }
            }
        }

        let tests_of = links::test_edges(&infos);
        let mut tested_by: HashMap<String, Vec<String>> = HashMap::new();
        for (t, subjects) in &tests_of { for s in subjects { tested_by.entry(s.clone()).or_default().push(t.clone()); } }
        for v in tested_by.values_mut() { v.sort(); }

        let git_raw = self.git();
        let has_git = git_raw.is_some();
        let mut git = git_raw.unwrap_or_default();
        git.cochange.retain(|k, _| infos.contains_key(k));
        for v in git.cochange.values_mut() { v.retain(|(o, _, _)| infos.contains_key(o)); }
        git.cochange.retain(|_, v| !v.is_empty());
        git.last_commit.retain(|k, _| infos.contains_key(k));
        git.churn.retain(|k, _| infos.contains_key(k));

        let code_files: Vec<String> = infos.iter().filter(|(_, i)| i.kind == "code").map(|(k, _)| k.clone()).collect();
        let importance = links::importance(&code_files, &imports_of);

        // ---- the design: what part each file plays, and how data moves through them ----
        let name = self.root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "project".into());
        let stack = self.stack(&set);
        let design = arch::build(&name, &infos, &imports_of, &tests_of, &stack);
        let flow_set = flows::build(&infos, &imports_of, &design, memory.journeys());
        let checks = arch::checks(&arch::CheckInput {
            infos: &infos, imports_of: &imports_of, imported_by: &imported_by, tests_of: &tests_of, arch: &design,
            undefined_steps: &flow_set.undefined_steps, unused_steps: &flow_set.unused_steps, reached: &flow_set.reached,
        });
        memory.hashes = infos.iter().map(|(f, i)| (f.clone(), i.hash.clone())).collect();

        // ---- nodes and links ----
        let mut nodes: Vec<Value> = Vec::new();
        let mut links: Vec<Value> = Vec::new();
        let mut dirs: BTreeSet<String> = BTreeSet::new();
        dirs.insert(".".into());
        let root_note = memory.view(".", None);
        nodes.push(json!({ "id": ".", "type": "dir", "name": name, "depth": 0, "note": root_note.note, "noteSource": root_note.source }));

        fn ensure_dir(d: &str, dirs: &mut BTreeSet<String>, nodes: &mut Vec<Value>, links: &mut Vec<Value>, memory: &Memory) {
            if dirs.contains(d) { return; }
            let parent = match d.rfind('/') { Some(i) => d[..i].to_string(), None => ".".into() };
            ensure_dir(&parent, dirs, nodes, links, memory);
            dirs.insert(d.to_string());
            let m = memory.view(d, None);
            nodes.push(json!({ "id": d, "type": "dir", "name": d.rsplit('/').next().unwrap_or(d), "depth": d.split('/').count(), "note": m.note, "noteSource": m.source }));
            links.push(json!({ "source": parent, "target": d, "type": "tree" }));
        }

        for (f, info) in &infos {
            let parent = match f.rfind('/') { Some(i) => f[..i].to_string(), None => ".".into() };
            ensure_dir(&parent, &mut dirs, &mut nodes, &mut links, memory);
            let m = memory.view(f, Some(&info.hash));
            nodes.push(json!({
                "id": f, "type": if info.kind == "asset" { "asset" } else { "file" }, "name": f.rsplit('/').next().unwrap_or(f),
                "lang": info.lang, "lines": info.lines, "bytes": info.bytes, "summary": info.summary, "role": info.role,
                "layer": info.layer, "words": info.words, "generated": info.generated, "large": info.kind == "large",
                "note": m.note, "noteSource": m.source, "noteStale": m.stale,
                "inDeg": imported_by.get(f).map(|v| v.len()).unwrap_or(0), "outDeg": imports_of.get(f).map(|v| v.len()).unwrap_or(0),
                "importance": (importance.get(f).copied().unwrap_or(0.0) * 1000.0).round() / 1000.0,
                "tests": tested_by.get(f).map(|v| v.len()).unwrap_or(0),
                "churn": git.churn.get(f).copied().unwrap_or(0),
                "depth": f.split('/').count(),
                "comp": design.comp_of.get(f).copied().unwrap_or("utils"),
            }));
            links.push(json!({ "source": parent, "target": f, "type": "tree" }));
        }
        let mut import_count = 0;
        for (f, deps) in &imports_of {
            for d in deps { links.push(json!({ "source": f, "target": d, "type": "import" })); import_count += 1; }
        }
        let mut test_count = 0;
        for (t, subjects) in &tests_of {
            for s in subjects { links.push(json!({ "source": t, "target": s, "type": "test" })); test_count += 1; }
        }
        let mut co_count = 0;
        for (a, others) in &git.cochange {
            for (b, strength, n) in others {
                if a < b { links.push(json!({ "source": a, "target": b, "type": "cochange", "strength": strength, "commits": n })); co_count += 1; }
            }
        }

        let (description, description_source) = self.description(&set);
        let features = understand::features(&infos, &imports_of);
        let graph = json!({
            "meta": {
                "name": name, "root": self.root.to_string_lossy(), "stack": stack,
                "files": infos.len(), "imports": import_count, "testLinks": test_count, "cochangeLinks": co_count,
                "git": has_git, "truncated": truncated, "maxFiles": MAX_FILES, "hidden": hidden,
                "description": description, "descriptionSource": description_source,
                "features": features, "scannedAt": crate::memory::now_ms(),
                "archetype": { "id": design.archetype.id, "name": design.archetype.name },
                "flowCount": flow_set.list.len(), "journeyCount": flow_set.journeys.len(),
                "checkCount": checks.len(),
                "checkErrors": checks.iter().filter(|c| c["severity"] == "error").count(),
            },
            "nodes": nodes, "links": links,
            "arch": design.json.clone(), "checks": checks,
            "flows": flow_set.list.clone(), "journeys": flow_set.journeys.clone(),
        });
        ScanResult {
            graph, infos, imports_of, imported_by, tests_of, tested_by, git, importance, dirs,
            stack, description, features, arch: design, flows: flow_set,
        }
    }
}

fn normalize(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => { out.pop(); }
            s => out.push(s),
        }
    }
    out.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(name: &str) -> ScanResult {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        let mut memory = Memory::load(&root);
        Scanner::new(root).scan(&mut memory)
    }
    fn uses(r: &ScanResult, from: &str) -> Vec<String> {
        r.imports_of.get(from).cloned().unwrap_or_default()
    }

    #[test]
    fn junk_folders_and_lockfiles_are_skipped() {
        let r = scan("csharp");
        assert!(r.infos.keys().all(|p| !p.contains("/obj/") && !p.contains("/bin/") && !p.starts_with(".vs/")), "{:?}", r.infos.keys());
        let r = scan("javascript");
        assert!(r.infos.keys().all(|p| !p.starts_with("node_modules/") && !p.starts_with("dist/") && p != "package-lock.json"), "{:?}", r.infos.keys());
        let r = scan("php");
        assert!(r.infos.keys().all(|p| !p.starts_with("vendor/")));
    }

    #[test]
    fn javascript_imports_resolve() {
        let r = scan("javascript");
        assert_eq!(uses(&r, "src/components/BillList.jsx"), vec!["src/api/client.js", "src/utils/format.js"]);
        assert_eq!(r.tests_of["src/utils/format.test.js"], vec!["src/utils/format.js"]);
    }

    #[test]
    fn python_imports_resolve_including_from_dot_import() {
        let r = scan("python");
        assert_eq!(uses(&r, "app/billing.py"), vec!["app/models.py"]);
        let t = uses(&r, "tests/test_billing.py");
        assert!(t.contains(&"app/billing.py".to_string()) && t.contains(&"app/models.py".to_string()), "{t:?}");
        assert_eq!(r.tests_of["tests/test_billing.py"], vec!["app/billing.py"]);
    }

    #[test]
    fn php_namespaces_and_views_resolve() {
        let r = scan("php");
        let c = uses(&r, "app/Http/Controllers/BillController.php");
        assert!(c.contains(&"app/Models/Bill.php".to_string()) && c.contains(&"resources/views/bills/index.blade.php".to_string()), "{c:?}");
        assert_eq!(uses(&r, "routes/web.php"), vec!["app/Http/Controllers/BillController.php"]);
        assert_eq!(r.tests_of["tests/Feature/BillControllerTest.php"], vec!["app/Http/Controllers/BillController.php"]);
    }

    #[test]
    fn csharp_type_references_tests_and_steps_link() {
        let r = scan("csharp");
        assert_eq!(uses(&r, "MyApp/Services/BillService.cs"), vec!["MyApp/Bill.cs"]);
        assert!(uses(&r, "MyApp.Tests/BillServiceTests.cs").contains(&"MyApp/Services/BillService.cs".to_string()));
        assert_eq!(r.infos["MyApp.Tests/BillServiceTests.cs"].layer, "tests");
        assert_eq!(r.tests_of["MyApp.Tests/BillServiceTests.cs"], vec!["MyApp/Services/BillService.cs"]);
        assert_eq!(r.tests_of["MyApp.Tests/Features/Billing.feature"], vec!["MyApp.Tests/StepDefinitions/BillingSteps.cs"]);
        let methods: Vec<&str> = r.infos["MyApp/Services/BillService.cs"].symbols.iter().filter(|s| s.kind == "function").map(|s| s.name.as_str()).collect();
        assert_eq!(methods, vec!["Total", "Format"]);
        assert!(r.stack.contains(&"ASP.NET Core".to_string()) && r.stack.contains(&"Reqnroll".to_string()), "{:?}", r.stack);
    }

    #[test]
    fn rust_mod_and_use_crate_resolve() {
        let r = scan("rust");
        assert_eq!(uses(&r, "src/main.rs"), vec!["src/bill.rs"]);
    }

    fn flow<'a>(r: &'a ScanResult, id_part: &str) -> &'a Value {
        r.graph["flows"].as_array().unwrap().iter().find(|f| f["id"].as_str().unwrap().contains(id_part) || f["title"] == id_part)
            .unwrap_or_else(|| panic!("no flow {id_part}: {:?}", r.graph["flows"].as_array().unwrap().iter().map(|f| f["id"].clone()).collect::<Vec<_>>()))
    }
    fn stage_files(f: &Value, comp: &str) -> Vec<String> {
        f["stages"].as_array().unwrap().iter().filter(|s| s["comp"] == comp)
            .flat_map(|s| s["files"].as_array().unwrap().iter().map(|x| x["path"].as_str().unwrap().to_string())).collect()
    }

    #[test]
    fn test_automation_architecture_and_scenario_flows() {
        let r = scan("automation");
        assert_eq!(r.arch.archetype.id, "test_automation");
        let comp = |p: &str| r.arch.comp_of[p];
        assert_eq!(comp("Features/Login.feature"), "scenarios");
        assert_eq!(comp("StepDefinitions/LoginSteps.cs"), "steps");
        assert_eq!(comp("Pages/LoginPage.cs"), "pages");
        assert_eq!(comp("Drivers/DriverFactory.cs"), "driver");
        assert_eq!(comp("Hooks/Hooks.cs"), "hooks");
        assert_eq!(comp("TestData/users.json"), "testdata");
        let pipe: Vec<&str> = r.graph["arch"]["pipeline"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
        assert_eq!(pipe, vec!["scenarios", "steps", "pages", "driver", "app_under_test"]);

        // One scenario, traced line by line down to the driver, with its data file.
        let f = flow(&r, "Wrong password is rejected");
        assert_eq!(f["kind"], "scenario");
        assert_eq!(stage_files(f, "steps"), vec!["StepDefinitions/LoginSteps.cs"]);
        assert!(stage_files(f, "pages").contains(&"Pages/LoginPage.cs".to_string()), "{f}");
        assert_eq!(stage_files(f, "driver"), vec!["Drivers/DriverFactory.cs"]);
        assert_eq!(stage_files(f, "testdata"), vec!["TestData/users.json"]);
        assert_eq!(stage_files(f, "hooks"), vec!["Hooks/Hooks.cs"]);
        // Background steps are folded into every scenario; "I reset my password" has no definition.
        assert_eq!(f["steps"][0]["text"], "the browser is open");
        assert_eq!(f["gaps"].as_array().unwrap().len(), 1);
        let checks = r.graph["checks"].as_array().unwrap();
        assert!(checks.iter().any(|c| c["kind"] == "undefined-steps"), "{checks:?}");
        assert!(checks.iter().any(|c| c["kind"] == "unused-steps"), "{checks:?}");
    }

    #[test]
    fn fullstack_screens_are_stitched_to_server_routes() {
        let r = scan("rideshare");
        assert_eq!(r.arch.archetype.id, "fullstack");
        assert_eq!(r.arch.comp_of["client/src/screens/RideRequestScreen.jsx"], "screens");
        assert_eq!(r.arch.comp_of["client/src/api/rides.js"], "apiclient");
        assert_eq!(r.arch.comp_of["server/routes/rides.js"], "routes");
        assert_eq!(r.arch.comp_of["server/controllers/rideController.js"], "controllers");
        assert_eq!(r.arch.comp_of["server/services/matchingService.js"], "services");
        assert_eq!(r.arch.comp_of["server/models/Driver.js"], "models");
        assert_eq!(r.arch.comp_of["server/index.js"], "entry");

        // Mount prefix applied: router.post('/') under app.use('/api/rides').
        let req = flow(&r, "POST /api/rides");
        assert_eq!(stage_files(req, "controllers"), vec!["server/controllers/rideController.js"]);
        assert!(stage_files(req, "services").contains(&"server/services/matchingService.js".to_string()), "{req}");
        assert!(stage_files(req, "models").contains(&"server/models/Driver.js".to_string()), "{req}");

        // The screen reaches the same controller and models through its API call.
        let screen = flow(&r, "v:client/src/screens/RideRequestScreen.jsx");
        let reached: Vec<&str> = screen["routes"].as_array().unwrap().iter().filter_map(|x| x.as_str()).collect();
        assert!(reached.iter().any(|id| id.contains(":POST:/api/rides")), "{screen}");
        assert!(stage_files(screen, "apiclient").contains(&"client/src/api/rides.js".to_string()));
        assert!(stage_files(screen, "services").contains(&"server/services/matchingService.js".to_string()), "{screen}");

        // Journeys: login comes before rides.
        let names: Vec<&str> = r.graph["journeys"].as_array().unwrap().iter().map(|j| j["name"].as_str().unwrap()).collect();
        let li = names.iter().position(|n| *n == "Login" || *n == "Auth").unwrap();
        let ri = names.iter().position(|n| *n == "Ride").unwrap();
        assert!(li < ri, "{names:?}");
    }

    #[test]
    fn importance_is_highest_for_shared_files() {
        let r = scan("csharp");
        let bill = r.importance["MyApp/Bill.cs"];
        let steps = r.importance["MyApp.Tests/StepDefinitions/BillingSteps.cs"];
        assert!(bill > steps, "{bill} vs {steps}");
    }
}
