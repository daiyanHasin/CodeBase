//! CodeBase: see and understand any codebase.
//!
//! Usage:
//!   codebase                     open the last project (or pick a folder on first run)
//!   codebase <folder>            open a local folder (live updates)
//!   codebase <git-url>           clone or update a repository, then open it
//!   options: --pick  --port 4321  --host 127.0.0.1  --no-open

mod analyze;
mod arch;
mod context;
mod flows;
mod history;
mod links;
mod memory;
mod scan;
mod understand;

use memory::Memory;
use notify::{RecursiveMode, Watcher};
use scan::{ScanResult, Scanner};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};

const VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------- embedded web files: the whole app ships inside one executable ----------
struct Asset {
    path: &'static str,
    mime: &'static str,
    body: &'static [u8],
}
const ASSETS: &[Asset] = &[
    Asset { path: "/", mime: "text/html; charset=utf-8", body: include_bytes!("../web/app.html") },
    Asset { path: "/app.css", mime: "text/css; charset=utf-8", body: include_bytes!("../web/app.css") },
    Asset { path: "/app.js", mime: "text/javascript; charset=utf-8", body: include_bytes!("../web/app.js") },
    Asset { path: "/tokens.css", mime: "text/css; charset=utf-8", body: include_bytes!("../web/tokens.css") },
    Asset { path: "/favicon.svg", mime: "image/svg+xml", body: include_bytes!("../web/favicon.svg") },
    Asset { path: "/vendor/3d-force-graph.js", mime: "text/javascript; charset=utf-8", body: include_bytes!("../web/vendor/3d-force-graph.min.js") },
    Asset { path: "/vendor/highlight.js", mime: "text/javascript; charset=utf-8", body: include_bytes!("../web/vendor/highlight.min.js") },
    Asset { path: "/docs", mime: "text/html; charset=utf-8", body: include_bytes!("../site/docs.html") },
    Asset { path: "/welcome", mime: "text/html; charset=utf-8", body: include_bytes!("../site/index.html") },
    Asset { path: "/site.css", mime: "text/css; charset=utf-8", body: include_bytes!("../site/site.css") },
];

// ---------- project state ----------
struct Project {
    id: u64,
    root: PathBuf,
    memory: Memory,
    scanner: Scanner,
    last: ScanResult,
    version: u64,
    events: VecDeque<Value>,
    _watcher: Option<notify::RecommendedWatcher>,
}

struct App {
    project: Option<Project>,
    next_id: u64,
}

type Shared = Arc<Mutex<App>>;

/// Locks the app state. If an earlier request panicked while holding the lock,
/// carry on with the data instead of failing every request after it.
fn lock(app: &Shared) -> std::sync::MutexGuard<'_, App> {
    app.lock().unwrap_or_else(|e| e.into_inner())
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}
fn data_dir() -> PathBuf {
    home_dir().join(".codebase")
}

fn recent_list() -> Vec<String> {
    fs::read_to_string(data_dir().join("recent.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
fn remember(target: &str) {
    let mut list = recent_list();
    list.retain(|x| x != target);
    list.insert(0, target.to_string());
    list.truncate(10);
    let _ = fs::create_dir_all(data_dir());
    let _ = fs::write(data_dir().join("recent.json"), serde_json::to_string_pretty(&list).unwrap_or_default());
}

fn is_git_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://") || s.starts_with("git@") || s.starts_with("ssh://") || s.ends_with(".git")
}

/// Windows canonicalize() returns \\?\C:\... which confuses editors and links.
fn strip_unc(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy().to_string();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => p,
    }
}

/// Turns a folder path or a Git URL into a local folder.
fn prepare_root(target: &str) -> Result<PathBuf, String> {
    let mut t = target.trim().trim_matches('"').to_string();
    if t.starts_with("github.com/") || t.starts_with("www.github.com/") {
        t = format!("https://{t}");
    }
    if !is_git_url(&t) {
        let p = PathBuf::from(&t);
        let abs = if p.is_absolute() { p } else { std::env::current_dir().map_err(|e| e.to_string())?.join(p) };
        let abs = abs.canonicalize().map_err(|_| format!("Folder not found: {t}"))?;
        if !abs.is_dir() {
            return Err(format!("Not a folder: {}", abs.display()));
        }
        return Ok(strip_unc(abs));
    }
    let parts: Vec<&str> = t.trim_end_matches('/').trim_end_matches(".git").split(['/', ':']).filter(|s| !s.is_empty()).collect();
    let name = parts.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join("-");
    let dest = data_dir().join("repos").join(name);
    let _ = fs::create_dir_all(data_dir().join("repos"));
    if Command::new("git").arg("--version").output().is_err() {
        return Err("Git is not installed, so CodeBase cannot download repositories. Install it from https://git-scm.com or open a folder instead.".into());
    }
    // Full history, so the History view can show every version. Older CodeBase builds
    // cloned only the newest commit; those clones are deepened on the next update.
    let status = if dest.join(".git").exists() {
        println!("Updating {} ...", dest.display());
        let shallow = Command::new("git").args(["rev-parse", "--is-shallow-repository"]).current_dir(&dest).output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true").unwrap_or(false);
        if shallow { let _ = Command::new("git").args(["fetch", "--unshallow", "--tags"]).current_dir(&dest).status(); }
        Command::new("git").args(["pull", "--ff-only", "--tags"]).current_dir(&dest).status()
    } else {
        println!("Downloading {t} with its full history ...");
        Command::new("git").args(["clone", "--no-single-branch", &t]).arg(&dest).status()
    };
    match status {
        Ok(_) if dest.join(".git").exists() => Ok(dest),
        _ => Err("Could not download the repository. Check the link and your internet connection.".into()),
    }
}

/// Native folder picker (Windows). Returns None if cancelled or unavailable.
fn pick_folder(start: Option<&str>) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let start = start.filter(|s| !is_git_url(s)).unwrap_or("").replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
         $d.Description = 'Choose the project folder to open in CodeBase'; \
         $d.ShowNewFolderButton = $false; \
         if ('{start}' -ne '' -and (Test-Path -LiteralPath '{start}')) {{ $d.SelectedPath = '{start}' }}; \
         $o = New-Object System.Windows.Forms.Form; $o.TopMost = $true; \
         if ($d.ShowDialog($o) -eq [System.Windows.Forms.DialogResult]::OK) {{ [Console]::Out.Write($d.SelectedPath) }}"
    );
    let out = Command::new("powershell").args(["-NoProfile", "-STA", "-ExecutionPolicy", "Bypass", "-Command", &script]).output().ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!path.is_empty()).then_some(path)
}

fn hashes(r: &ScanResult) -> HashMap<String, String> {
    r.infos.iter().map(|(k, v)| (k.clone(), v.hash.clone())).collect()
}

fn rescan(p: &mut Project, reason: &str) {
    let before = hashes(&p.last);
    p.last = p.scanner.scan(&mut p.memory);
    let after = hashes(&p.last);
    let mut added: Vec<String> = after.keys().filter(|k| !before.contains_key(*k)).cloned().collect();
    let mut removed: Vec<String> = before.keys().filter(|k| !after.contains_key(*k)).cloned().collect();
    let mut modified: Vec<String> = after.iter().filter(|(k, v)| before.get(*k).map(|b| b != *v).unwrap_or(false)).map(|(k, _)| k.clone()).collect();
    if reason == "watch" && added.is_empty() && removed.is_empty() && modified.is_empty() {
        return;
    }
    added.sort();
    removed.sort();
    modified.sort();
    if reason == "watch" { p.memory.record(&added, &modified, &removed); }
    p.version += 1;
    p.events.push_back(json!({ "v": p.version, "reason": reason, "at": memory::now_ms(), "added": added, "removed": removed, "modified": modified }));
    while p.events.len() > 60 {
        p.events.pop_front();
    }
}

fn start_watcher(root: &Path, id: u64, app: Shared) -> Option<notify::RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<()>();
    let filter_root = root.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            // Ignore churn inside build output, dependencies and caches (same rules as the scanner),
            // so a running build does not trigger endless rescans.
            let relevant = ev.paths.iter().any(|p| {
                let rel = p.strip_prefix(&filter_root).unwrap_or(p);
                let mut abs = filter_root.clone();
                !rel.components().any(|c| {
                    abs.push(c);
                    let name = c.as_os_str().to_string_lossy();
                    name == ".git" || scan::is_junk_dir(&name, &abs)
                })
            });
            if relevant {
                let _ = tx.send(());
            }
        }
    })
    .ok()?;
    watcher.watch(root, RecursiveMode::Recursive).ok()?;
    thread::spawn(move || {
        // Debounce: wait for 350 ms of quiet, then rescan once.
        while rx.recv().is_ok() {
            loop {
                match rx.recv_timeout(Duration::from_millis(350)) {
                    Ok(()) => continue,
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
            let mut a = lock(&app);
            match a.project.as_mut() {
                Some(p) if p.id == id => rescan(p, "watch"),
                _ => return,
            }
        }
    });
    Some(watcher)
}

fn open_project(app: &Shared, target: &str) -> Result<(), String> {
    let root = prepare_root(target)?;
    let started = Instant::now();
    let mut memory = Memory::load(&root);
    let mut scanner = Scanner::new(root.clone());
    let last = scanner.scan(&mut memory);
    println!("Mapped {} files in {} ({} ms)", last.infos.len(), root.display(), started.elapsed().as_millis());
    let id = {
        let mut a = lock(&app);
        a.next_id += 1;
        a.project = None; // stops the previous project's watcher
        a.next_id
    };
    let watcher = start_watcher(&root, id, app.clone());
    if watcher.is_none() {
        println!("Live updates are not available for this folder; use Refresh after changes.");
    }
    lock(&app).project = Some(Project { id, root: root.clone(), memory, scanner, last, version: 1, events: VecDeque::new(), _watcher: watcher });
    remember(if is_git_url(target) { target.trim() } else { root.to_str().unwrap_or(target) });
    Ok(())
}

// ---------- HTTP helpers ----------
fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn send(req: Request, code: u16, mime: &str, body: Vec<u8>) {
    let resp = Response::from_data(body)
        .with_status_code(code)
        .with_header(header("Content-Type", mime))
        .with_header(header("Cache-Control", "no-store"));
    let _ = req.respond(resp);
}
fn send_json(req: Request, code: u16, v: &Value) {
    send(req, code, "application/json", serde_json::to_vec(v).unwrap_or_default());
}
fn err(req: Request, code: u16, msg: &str) {
    send_json(req, code, &json!({ "error": msg }));
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if b[i] == b'%' && i + 3 <= b.len() {
            match std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(v) => { out.push(v); i += 3; }
                None => { out.push(b'%'); i += 1; }
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn query(url: &str, key: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    q.split('&').filter_map(|kv| kv.split_once('=')).find(|(k, _)| *k == key).map(|(_, v)| decode(v))
}

fn body_json(req: &mut Request) -> Value {
    let mut s = String::new();
    let _ = req.as_reader().take(5_000_000).read_to_string(&mut s);
    serde_json::from_str(&s).unwrap_or(Value::Null)
}

fn safe_rel(p: &Project, raw: &str, allow_dir: bool) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    let raw = raw.replace('\\', "/");
    for seg in raw.split('/') {
        match seg {
            "" | "." => {}
            ".." => return None,
            s => parts.push(s),
        }
    }
    let rel = if parts.is_empty() { ".".to_string() } else { parts.join("/") };
    if p.last.infos.contains_key(&rel) || (allow_dir && p.last.dirs.contains(&rel)) { Some(rel) } else { None }
}

fn file_detail(p: &Project, rel: &str) -> Value {
    let info = &p.last.infos[rel];
    let content = if info.kind == "code" { fs::read(p.root.join(rel)).ok().map(|b| String::from_utf8_lossy(&b).to_string()) } else { None };
    let m = p.memory.view(rel, Some(&info.hash));
    json!({
        "path": rel, "abs": p.root.join(rel).to_string_lossy(), "lang": info.lang, "lines": info.lines, "bytes": info.bytes,
        "kind": info.kind, "summary": info.summary, "role": info.role, "layer": info.layer, "generated": info.generated,
        "symbols": info.symbols, "imports": p.last.imports_of.get(rel).cloned().unwrap_or_default(),
        "importedBy": p.last.imported_by.get(rel).cloned().unwrap_or_default(),
        "testedBy": p.last.tested_by.get(rel).cloned().unwrap_or_default(),
        "checks": p.last.tests_of.get(rel).cloned().unwrap_or_default(),
        "cochange": p.last.git.cochange.get(rel).map(|v| v.iter().map(|(f, s, n)| json!({ "path": f, "strength": s, "commits": n })).collect::<Vec<_>>()).unwrap_or_default(),
        "lastCommit": p.last.git.last_commit.get(rel),
        "importance": p.last.importance.get(rel).copied().unwrap_or(0.0),
        "note": m.note, "noteSource": m.source, "noteStale": m.stale,
        "history": m.history.iter().map(|h| json!({ "task": h.task, "at": h.at })).collect::<Vec<_>>(),
        "content": content,
    })
}

fn dir_detail(p: &Project, rel: &str) -> Value {
    let m = p.memory.view(rel, None);
    json!({ "path": rel, "type": "dir", "note": m.note, "noteSource": m.source })
}

fn handle(mut req: Request, app: Shared) {
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or("/").to_string();
    let method = req.method().clone();

    if !path.starts_with("/api/") {
        let key = match path.as_str() {
            "/docs/" | "/docs.html" => "/docs",
            "/index.html" => "/",
            "/welcome/" | "/site" | "/site/" => "/welcome",
            other => other,
        };
        return match ASSETS.iter().find(|a| a.path == key) {
            Some(a) => send(req, 200, a.mime, a.body.to_vec()),
            None => err(req, 404, "Not found"),
        };
    }

    // Endpoints that work with or without an open project.
    match (&method, path.as_str()) {
        (Method::Get, "/api/projects") => {
            let current = lock(&app).project.as_ref().map(|p| p.root.to_string_lossy().to_string());
            return send_json(req, 200, &json!({ "recent": recent_list(), "current": current, "canPick": cfg!(windows), "version": VERSION }));
        }
        (Method::Post, "/api/open") => {
            let body = body_json(&mut req);
            let target = if body["pick"].as_bool() == Some(true) {
                let start = lock(&app).project.as_ref().map(|p| p.root.to_string_lossy().to_string());
                match pick_folder(start.as_deref()) {
                    Some(t) => t,
                    None => return send_json(req, 200, &json!({ "cancelled": true })),
                }
            } else {
                body["target"].as_str().unwrap_or("").to_string()
            };
            if target.trim().is_empty() {
                return err(req, 400, "Choose a folder or paste a repository link.");
            }
            return match open_project(&app, &target) {
                Ok(()) => send_json(req, 200, &json!({ "ok": true })),
                Err(e) => err(req, 400, &e),
            };
        }
        _ => {}
    }

    // History reads Git, which can take a moment: copy what it needs and release the lock first.
    if method == Method::Get && matches!(path.as_str(), "/api/timeline" | "/api/at" | "/api/diff") {
        let snap = {
            let guard = lock(&app);
            guard.project.as_ref().map(|p| (p.root.clone(), p.last.arch.comp_of.clone(), p.last.arch.ctx, p.memory.journal.clone()))
        };
        let Some((root, comp_of, ctx, journal)) = snap else { return err(req, 409, "No project is open yet."); };
        let comp_for = |f: &str| -> &'static str { comp_of.get(f).copied().unwrap_or_else(|| arch::classify_path(f, &ctx)) };
        return match path.as_str() {
            "/api/timeline" => send_json(req, 200, &history::timeline(&root, &comp_for, &journal)),
            "/api/at" => match history::at(&root, &query(&url, "rev").unwrap_or_default(), &ctx, &comp_of) {
                Ok(v) => send_json(req, 200, &v),
                Err(e) => err(req, 400, &e),
            },
            _ => match history::diff(&root, &query(&url, "rev").unwrap_or_default(), &query(&url, "path").unwrap_or_default()) {
                Ok(v) => send_json(req, 200, &v),
                Err(e) => err(req, 400, &e),
            },
        };
    }

    let mut guard = lock(&app);
    let Some(p) = guard.project.as_mut() else {
        drop(guard);
        return err(req, 409, "No project is open yet.");
    };

    let (code, value) = match (method, path.as_str()) {
        (Method::Get, "/api/graph") => {
            let mut g = p.last.graph.clone();
            g["meta"]["project"] = json!(p.id);
            g["meta"]["version"] = json!(p.version);
            (200, g)
        }
        (Method::Get, "/api/poll") => {
            let since: u64 = query(&url, "since").and_then(|s| s.parse().ok()).unwrap_or(0);
            let events: Vec<&Value> = p.events.iter().filter(|e| e["v"].as_u64().unwrap_or(0) > since).collect();
            (200, json!({ "project": p.id, "version": p.version, "events": events }))
        }
        (Method::Get, "/api/file") => match query(&url, "path").and_then(|r| safe_rel(p, &r, false)) {
            Some(rel) => (200, file_detail(p, &rel)),
            None => (404, json!({ "error": "That file is not in the current map." })),
        },
        (Method::Post, "/api/files") => {
            let body = body_json(&mut req);
            let list: Vec<Value> = body["paths"].as_array().cloned().unwrap_or_default().iter()
                .filter_map(|v| v.as_str()).take(80).filter_map(|r| safe_rel(p, r, false)).map(|r| file_detail(p, &r)).collect();
            (200, Value::Array(list))
        }
        (Method::Post, "/api/note") => {
            let body = body_json(&mut req);
            match body["path"].as_str().and_then(|r| safe_rel(p, r, true)) {
                Some(rel) => {
                    if body["confirm"].as_bool() == Some(true) { p.memory.confirm_note(&rel) } else { p.memory.set_note(&rel, body["note"].as_str().unwrap_or(""), "you") }
                    rescan(p, "note");
                    (200, if p.last.infos.contains_key(&rel) { file_detail(p, &rel) } else { dir_detail(p, &rel) })
                }
                None => (404, json!({ "error": "That file is not in the current map." })),
            }
        }
        (Method::Post, "/api/notes") => {
            let body = body_json(&mut req);
            let mut saved = 0;
            let mut unknown: Vec<String> = Vec::new();
            if let Some(obj) = body["notes"].as_object() {
                for (k, v) in obj {
                    match (safe_rel(p, k, true), v.as_str()) {
                        (Some(rel), Some(text)) if !text.trim().is_empty() => { p.memory.set_note(&rel, text, "ai"); saved += 1; }
                        _ => unknown.push(k.clone()),
                    }
                }
            }
            rescan(p, "note");
            (200, json!({ "saved": saved, "unknown": unknown }))
        }
        (Method::Post, "/api/history") => {
            let body = body_json(&mut req);
            let paths: Vec<String> = body["paths"].as_array().cloned().unwrap_or_default().iter().filter_map(|v| v.as_str()).filter_map(|r| safe_rel(p, r, false)).collect();
            p.memory.add_history(&paths, body["task"].as_str().unwrap_or(""));
            (200, json!({ "ok": true }))
        }
        (Method::Post, "/api/hidden") => {
            let body = body_json(&mut req);
            let list: Vec<String> = body["hidden"].as_array().cloned().unwrap_or_default().iter().filter_map(|v| v.as_str().map(String::from)).collect();
            p.memory.set_hidden(list);
            rescan(p, "hidden");
            (200, json!({ "hidden": p.memory.config.hidden }))
        }
        (Method::Post, "/api/context") => {
            let body = body_json(&mut req);
            let strs = |k: &str| -> Vec<String> {
                body[k].as_array().cloned().unwrap_or_default().iter().filter_map(|v| v.as_str()).filter_map(|r| safe_rel(p, r, false)).collect()
            };
            let modes = body["modes"].as_object().map(|o| {
                o.iter().filter_map(|(k, v)| Some((safe_rel(p, k, false)?, context::Mode::parse(v.as_str()?)?))).collect()
            }).unwrap_or_default();
            let creq = context::PackRequest {
                task: body["task"].as_str().unwrap_or("").chars().take(60_000).collect(),
                budget: body["budget"].as_u64().unwrap_or(8000) as usize,
                pins: strs("pins"),
                seeds: strs("seeds"),
                seed_reason: body["seedReason"].as_str().unwrap_or("").chars().take(120).collect(),
                modes,
                plain: body["plain"].as_bool().unwrap_or(true),
                done: body["done"].as_str().unwrap_or("").chars().take(2000).collect(),
            };
            let pack = context::build(&creq, &p.last, &p.memory, &p.root);
            (200, serde_json::to_value(&pack).unwrap_or(Value::Null))
        }
        (Method::Get, "/api/brief") => {
            let b = context::brief(&p.root, &p.last, &p.memory);
            (200, json!({ "text": b, "tokens": context::estimate_tokens(&b) }))
        }
        (Method::Post, "/api/journeys") => {
            // {"journeys":[{"name","about","flows":[ids]}]} from you or your AI chat; null resets.
            let body = body_json(&mut req);
            let v = body.get("journeys").filter(|j| j.is_array()).map(|j| json!({ "journeys": j }));
            p.memory.set_journeys(v);
            rescan(p, "note");
            (200, json!({ "journeys": p.last.flows.journeys }))
        }
        (Method::Post, "/api/rescan") => {
            rescan(p, "manual");
            (200, json!({ "ok": true }))
        }
        _ => (404, json!({ "error": "Unknown request" })),
    };
    drop(guard);
    send_json(req, code, &value);
}

fn fatal(msg: &str) -> ! {
    eprintln!("\n{msg}");
    if cfg!(windows) {
        eprintln!("Press Enter to close.");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    std::process::exit(1);
}

fn open_browser(url: &str) {
    let r = if cfg!(windows) {
        Command::new("cmd").args(["/c", "start", "", url]).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
    if r.is_err() {
        println!("Open {url} in your browser.");
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut take = |name: &str| -> Option<String> {
        let i = args.iter().position(|a| a == name)?;
        let v = args.get(i + 1).cloned();
        let end = (i + 2).min(args.len());
        args.drain(i..end);
        v
    };
    let port: u16 = take("--port").and_then(|v| v.parse().ok()).unwrap_or(4321);
    let host = take("--host").unwrap_or_else(|| "127.0.0.1".into());
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("codebase {VERSION}");
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("CodeBase {VERSION}\n\nUsage: codebase [folder | git-url] [--pick] [--port 4321] [--host 127.0.0.1] [--no-open]\n\nWith no folder, CodeBase reopens your last project, or asks for one.");
        return;
    }
    let no_open = args.iter().any(|a| a == "--no-open");
    let force_pick = args.iter().any(|a| a == "--pick");
    let target = args.into_iter().find(|a| !a.starts_with("--"));

    println!("CodeBase {VERSION}");
    let app: Shared = Arc::new(Mutex::new(App { project: None, next_id: 0 }));

    let chosen = match target {
        Some(t) => Some(t),
        None if force_pick => pick_folder(recent_list().first().map(|s| s.as_str())),
        None => recent_list()
            .into_iter()
            .find(|r| !is_git_url(r) && Path::new(r).is_dir())
            .or_else(|| pick_folder(None))
            .or_else(|| if cfg!(windows) { None } else { Some(".".into()) }),
    };
    match chosen {
        Some(t) => {
            if let Err(e) = open_project(&app, &t) {
                fatal(&e)
            }
        }
        None => println!("No folder chosen yet. You can open one in the browser."),
    }

    let mut bound = None;
    for attempt in 0..20u16 {
        if let Ok(s) = Server::http(format!("{host}:{}", port + attempt)) {
            bound = Some((s, port + attempt));
            break;
        }
    }
    let Some((server, port)) = bound else { fatal("Could not start the local server: every port from 4321 to 4340 is busy.") };
    let shown_host = if host == "0.0.0.0" { "localhost" } else { host.as_str() };
    let link = format!("http://{shown_host}:{port}");
    println!("CodeBase is running at {link}\nKeep this window open while you use it. Close it (or press Ctrl+C) to stop.");
    if !no_open {
        open_browser(&link);
    }
    for req in server.incoming_requests() {
        let app = app.clone();
        thread::spawn(move || handle(req, app));
    }
}
