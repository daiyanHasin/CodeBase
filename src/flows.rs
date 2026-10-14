//! End-to-end flows: how one action moves through the project.
//!
//!   scenario  a Gherkin scenario: each line -> its step definition -> page objects -> driver
//!   maestro   a Maestro flow: its commands, sub-flows and data
//!   test      a named test case -> page objects -> driver (test projects and e2e folders)
//!   request   "POST /api/rides" -> controller -> services -> models -> database
//!   screen    a screen -> its components and state -> the API calls it makes -> the
//!             server routes that answer them (stitched across front end and back end)
//!
//! Flows are grouped into journeys (login, ride, payment…) and ordered by where they
//! usually happen in an app's life: sign up, log in, browse, create, assign, pay, finish.

use crate::analyze::{split_words, FileInfo, RouteDef};
use crate::arch::{self, Arch, VIRTUAL};
use crate::links::step_regex;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::LazyLock;

pub struct Flows {
    pub list: Vec<Value>,
    pub journeys: Vec<Value>,
    pub undefined_steps: Vec<(String, usize, String)>,
    pub unused_steps: Vec<(String, String)>,
    pub reached: BTreeSet<String>,
}

/// Where in an app's life a word usually belongs. Lower comes first.
pub fn lifecycle(word: &str) -> f32 {
    const ORDER: &[&[&str]] = &[
        &["setup", "install", "onboard", "onboarding", "splash", "welcome", "launch", "start", "landing"],
        &["register", "registration", "signup", "join", "invite"],
        &["login", "signin", "auth", "authenticate", "otp", "verify", "verification", "session", "token", "password", "forgot", "reset"],
        &["profile", "account", "user", "setting", "preference"],
        &["home", "dashboard", "search", "browse", "list", "catalog", "map", "location", "explore", "feed", "menu", "product"],
        &["create", "request", "book", "booking", "order", "cart", "add", "new", "upload", "submit", "apply", "draft"],
        &["plan", "route", "quote", "estimate", "price", "pricing", "fare", "schedule", "calculate"],
        &["match", "assign", "dispatch", "approve", "approval", "review", "process", "accept", "confirm", "allocate"],
        &["track", "tracking", "trip", "ride", "status", "progress", "notify", "notification", "message", "chat", "deliver", "delivery"],
        &["pay", "payment", "checkout", "invoice", "bill", "billing", "wallet", "charge", "refund", "transaction"],
        &["complete", "finish", "rate", "rating", "feedback", "receipt", "history", "report", "summary", "export"],
        &["logout", "signout", "delete", "cancel", "close", "deactivate", "archive"],
    ];
    ORDER.iter().position(|g| g.contains(&word)).map(|i| i as f32).unwrap_or(6.5)
}

fn journey_rank(words: &[String]) -> f32 {
    words.iter().map(|w| lifecycle(w)).fold(6.5f32, |a, b| if b != 6.5 && (a == 6.5 || b < a) { b } else { a })
}

const GENERIC_SEG: &[&str] = &["api", "v1", "v2", "v3", "v4", "web", "admin", "auth"];

pub fn norm_url(u: &str) -> Vec<String> {
    static ORIGIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z]+://[^/]+").unwrap());
    let u = ORIGIN.replace(u, "");
    let u = u.split(['?', '#']).next().unwrap_or("");
    let mut out = Vec::new();
    for (i, seg) in u.split('/').filter(|s| !s.is_empty()).enumerate() {
        let var = seg.starts_with(':') || seg.starts_with('{') || seg.starts_with('<') || seg.starts_with('[') || seg.contains("${") || seg.contains('{')
            || seg.chars().all(|c| c.is_ascii_digit()) || seg == "*" || seg.starts_with('$');
        if var && i == 0 && (seg.contains("${") || seg.starts_with('{') || seg.starts_with('$')) && !seg.contains('}') { continue; }
        if var && i == 0 && seg.contains("${") && seg.ends_with('}') && out.is_empty() { continue; }
        out.push(if var { "*".to_string() } else { seg.to_lowercase() });
    }
    out
}

/// 2 = exact, 1 = one side has a prefix (base URL or mount point) the other lacks, 0 = no match.
pub fn url_match(call: &[String], route: &[String]) -> u8 {
    let eq = |a: &[String], b: &[String]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x == y || x == "*" || y == "*");
    if call.is_empty() || route.is_empty() { return u8::from(call.is_empty() && route.is_empty()) * 2; }
    if eq(call, route) { return 2; }
    let (long, short) = if call.len() > route.len() { (call, route) } else { (route, call) };
    if short.len() >= 1 && short.iter().any(|s| s != "*") && eq(&long[long.len() - short.len()..], short) { return 1; }
    0
}

struct Ctx<'a> {
    imports_of: &'a HashMap<String, Vec<String>>,
    arch: &'a Arch,
}

impl Ctx<'_> {
    fn comp(&self, f: &str) -> &'static str { self.arch.comp_of.get(f).copied().unwrap_or("") }

    /// Files reached from `start` by following "uses" links, staying on the way down the data path.
    fn downstream(&self, start: &str, allow: &[&str], depth: usize, cap: usize) -> Vec<String> {
        let a = self.arch.archetype;
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut out = Vec::new();
        let mut q: VecDeque<(String, usize)> = VecDeque::from([(start.to_string(), 0)]);
        seen.insert(start.to_string());
        while let Some((f, d)) = q.pop_front() {
            if d >= depth { continue; }
            let rf = arch::rank(a, self.comp(&f));
            let Some(deps) = self.imports_of.get(&f) else { continue };
            let mut deps: Vec<&String> = deps.iter().collect();
            deps.sort_by_key(|x| arch::rank(a, self.comp(x)).unwrap_or(99));
            for dep in deps {
                if seen.contains(dep) { continue; }
                let cd = self.comp(dep);
                if !allow.contains(&cd) { continue; }
                if let (Some(r1), Some(r2)) = (rf, arch::rank(a, cd)) { if r2 < r1 { continue; } }
                seen.insert(dep.clone());
                out.push(dep.clone());
                if out.len() >= cap { return out; }
                q.push_back((dep.clone(), d + 1));
            }
        }
        out
    }

    fn stages(&self, groups: Vec<(&'static str, Vec<(String, String)>)>) -> Vec<Value> {
        let mut merged: Vec<(&'static str, Vec<(String, String)>)> = Vec::new();
        for (c, files) in groups {
            if files.is_empty() && !VIRTUAL.contains(&c) { continue; }
            match merged.iter_mut().find(|(x, _)| *x == c) {
                Some((_, v)) => for f in files {
                    match v.iter_mut().find(|(p, _)| *p == f.0) {
                        // Same file reached twice (two routes in one router): keep both notes.
                        Some((_, note)) => if !f.1.is_empty() && !note.split("; ").any(|n| n == f.1) { if note.is_empty() { *note = f.1 } else { note.push_str("; "); note.push_str(&f.1); } },
                        None => v.push(f),
                    }
                },
                None => merged.push((c, files)),
            }
        }
        merged.into_iter().map(|(c, files)| {
            let k = arch::comp(c);
            json!({ "comp": c, "name": k.name, "virtual": VIRTUAL.contains(&c),
                "files": files.into_iter().map(|(p, note)| json!({ "path": p, "note": note })).collect::<Vec<_>>() })
        }).collect()
    }

    /// Splits files into stages in data-path order.
    fn by_comp(&self, files: &[String]) -> Vec<(&'static str, Vec<(String, String)>)> {
        let a = self.arch.archetype;
        let mut by: BTreeMap<(usize, &'static str), Vec<(String, String)>> = BTreeMap::new();
        for f in files {
            let c = self.comp(f);
            let r = arch::rank(a, c).unwrap_or(50 + a.sides.iter().position(|(s, _)| *s == c).unwrap_or(40));
            by.entry((r, c)).or_default().push((f.clone(), String::new()));
        }
        by.into_iter().map(|((_, c), v)| (c, v)).collect()
    }
}

fn file_words(path: &str) -> Vec<String> {
    let stem = path.rsplit('/').next().unwrap_or(path).split('.').next().unwrap_or("");
    split_words(stem).into_iter().filter(|w| !["screen", "page", "view", "test", "spec", "steps", "step", "flow", "feature"].contains(&w.as_str())).collect()
}

/// "RideRequestScreen" -> "Ride request", "checkout-page" -> "Checkout".
fn screen_title(stem: &str) -> String {
    static CAMEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([a-z0-9])([A-Z])").unwrap());
    let spaced = CAMEL.replace_all(stem, "$1 $2").replace(['_', '-', '.'], " ");
    let words: Vec<String> = spaced.split_whitespace().map(|w| w.to_lowercase())
        .filter(|w| !matches!(w.as_str(), "screen" | "page" | "view" | "index" | "route")).collect();
    if words.is_empty() { return title_case(stem); }
    title_case(&words.join(" "))
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() { Some(f) => f.to_uppercase().collect::<String>() + c.as_str(), None => String::new() }
}

fn flow_json(id: String, kind: &str, title: String, subtitle: String, entry: &str, line: usize, journey: String, words: &[String], stages: Vec<Value>, extra: Value) -> Value {
    let mut files: Vec<String> = Vec::new();
    for s in &stages {
        for f in s["files"].as_array().into_iter().flatten() {
            if let Some(p) = f["path"].as_str() { if !files.iter().any(|x| x == p) { files.push(p.to_string()); } }
        }
    }
    let mut v = json!({
        "id": id, "kind": kind, "title": title, "subtitle": subtitle, "entry": entry, "line": line,
        "journey": journey, "rank": journey_rank(words), "stages": stages, "files": files, "gaps": [],
    });
    if let (Some(o), Value::Object(e)) = (v.as_object_mut(), extra) { for (k, x) in e { o.insert(k, x); } }
    v
}

pub fn build(infos: &BTreeMap<String, FileInfo>, imports_of: &HashMap<String, Vec<String>>, arch: &Arch, custom: Option<&Value>) -> Flows {
    let cx = Ctx { imports_of, arch };
    let a = arch.archetype;
    let mut list: Vec<Value> = Vec::new();
    let mut undefined_steps = Vec::new();

    // Files of a kind, used by every flow of that kind.
    let all_of = |c: &str| -> Vec<String> { infos.keys().filter(|p| cx.comp(p) == c).cloned().collect() };
    let hooks = all_of("hooks");

    // ---------- test automation ----------
    let mut defs: Vec<(String, String, Regex)> = Vec::new();
    for (p, i) in infos.iter().filter(|(_, i)| i.lang != "gherkin" && !i.steps.is_empty()) {
        for s in &i.steps { if let Some(re) = step_regex(s) { defs.push((p.clone(), s.clone(), re)); } }
    }
    let mut used: BTreeSet<(String, String)> = BTreeSet::new();
    let test_down: &[&str] = &["pages", "driver", "testutil", "testdata", "steps"];

    let driver_files = all_of("driver");
    let reach_driver = |from: &[String]| -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for f in from {
            for d in cx.downstream(f, &["pages", "testutil", "driver"], 3, 30) { if cx.comp(&d) == "driver" { out.insert(d); } }
        }
        if out.is_empty() { for f in &hooks { for d in imports_of.get(f).into_iter().flatten() { if cx.comp(d) == "driver" { out.insert(d.clone()); } } } }
        if out.is_empty() && driver_files.len() <= 3 { out.extend(driver_files.iter().cloned()); }
        out.into_iter().collect()
    };
    let data_for = |from: &[String]| -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for f in from {
            for d in imports_of.get(f).into_iter().flatten() { if cx.comp(d) == "testdata" { out.insert(d.clone()); } }
        }
        out.into_iter().take(8).collect()
    };

    for (path, info) in infos.iter().filter(|(_, i)| i.lang == "gherkin") {
        for sc in &info.scenarios {
            let mut steps_json = Vec::new();
            let mut def_files: Vec<String> = Vec::new();
            let mut gaps = Vec::new();
            for st in &sc.steps {
                let hit = defs.iter().find(|(_, _, re)| re.is_match(&st.text));
                if let Some((f, pat, _)) = hit {
                    used.insert((f.clone(), pat.clone()));
                    if !def_files.contains(f) { def_files.push(f.clone()); }
                } else {
                    undefined_steps.push((path.clone(), st.line, st.text.clone()));
                    gaps.push(format!("Line {}: \"{}\" has no step definition", st.line, st.text));
                }
                steps_json.push(json!({ "kw": st.kw, "text": st.text, "line": st.line, "def": hit.map(|h| h.0.clone()), "pattern": hit.map(|h| h.1.clone()) }));
            }
            let mut below: Vec<String> = Vec::new();
            for f in &def_files { for d in cx.downstream(f, test_down, 3, 20) { if !below.contains(&d) && !def_files.contains(&d) { below.push(d); } } }
            let pages: Vec<String> = below.iter().filter(|f| cx.comp(f) == "pages").cloned().collect();
            let mut from = def_files.clone();
            from.extend(pages.iter().cloned());
            let drivers = reach_driver(&from);
            let mut data_from = def_files.clone();
            data_from.extend(below.iter().cloned());
            let data = data_for(&data_from);
            let note_for = |f: &String| -> String {
                let n = sc.steps.iter().filter(|s| steps_json.iter().any(|j| j["line"] == s.line && j["def"].as_str() == Some(f))).count();
                format!("{n} of the steps")
            };
            let groups = vec![
                ("scenarios", vec![(path.clone(), format!("line {}", sc.line))]),
                ("hooks", hooks.iter().map(|h| (h.clone(), "before and after".to_string())).collect()),
                ("steps", def_files.iter().map(|f| (f.clone(), note_for(f))).collect()),
                ("pages", pages.iter().map(|f| (f.clone(), String::new())).collect()),
                ("testutil", below.iter().filter(|f| cx.comp(f) == "testutil").take(6).map(|f| (f.clone(), String::new())).collect()),
                ("testdata", data.iter().map(|f| (f.clone(), String::new())).collect()),
                ("driver", drivers.iter().map(|f| (f.clone(), String::new())).collect()),
                ("app_under_test", vec![]),
            ];
            let mut words = split_words(&sc.group);
            words.extend(split_words(&sc.name));
            let journey = if sc.group.is_empty() { file_words(path).first().cloned().unwrap_or_else(|| "tests".into()) } else { sc.group.clone() };
            let mut f = flow_json(format!("s:{path}#{}", sc.line), "scenario", sc.name.clone(), sc.group.clone(), path, sc.line, journey, &words, cx.stages(groups), json!({ "steps": steps_json }));
            f["gaps"] = json!(gaps);
            list.push(f);
            if list.len() >= 400 { break; }
        }
    }
    let unused_steps: Vec<(String, String)> = if infos.values().any(|i| i.lang == "gherkin") {
        defs.iter().filter(|(f, p, _)| !used.contains(&(f.clone(), p.clone()))).map(|(f, p, _)| (f.clone(), p.clone())).take(60).collect()
    } else { vec![] };

    for (path, info) in infos.iter().filter(|(_, i)| i.signals.contains(&"maestro")) {
        for sc in &info.scenarios {
            let subs: Vec<String> = imports_of.get(path).into_iter().flatten().filter(|d| infos.get(*d).map(|i| i.signals.contains(&"maestro")).unwrap_or(false)).cloned().collect();
            let data: Vec<String> = imports_of.get(path).into_iter().flatten().filter(|d| cx.comp(d) == "testdata").cloned().collect();
            let steps_json: Vec<Value> = sc.steps.iter().map(|s| json!({ "kw": s.kw, "text": s.text, "line": s.line, "def": Value::Null })).collect();
            let groups = vec![
                ("scenarios", std::iter::once((path.clone(), format!("{} commands", sc.steps.len()))).chain(subs.iter().map(|s| (s.clone(), "sub-flow".to_string()))).collect()),
                ("testdata", data.iter().map(|f| (f.clone(), String::new())).collect()),
                ("app_under_test", vec![]),
            ];
            let words = file_words(path);
            let journey = words.first().cloned().unwrap_or_else(|| "flows".into());
            list.push(flow_json(format!("m:{path}"), "maestro", sc.name.clone(), "Maestro flow".into(), path, 1, journey, &words, cx.stages(groups), json!({ "steps": steps_json })));
        }
    }

    let e2e_path = |p: &str| { let l = p.to_lowercase(); ["e2e", "ui-tests", "uitests", "acceptance", "playwright", "cypress", "selenium", "automation"].iter().any(|k| l.contains(k)) };
    for (path, info) in infos.iter().filter(|(p, i)| i.lang != "gherkin" && !i.signals.contains(&"maestro") && !i.scenarios.is_empty() && (a.id == "test_automation" || e2e_path(p))) {
        let below = cx.downstream(path, test_down, 3, 20);
        let pages: Vec<String> = below.iter().filter(|f| cx.comp(f) == "pages").cloned().collect();
        let mut from = vec![path.clone()];
        from.extend(pages.iter().cloned());
        let drivers = reach_driver(&from);
        let mut data_from = vec![path.clone()];
        data_from.extend(below.iter().cloned());
        let data = data_for(&data_from);
        for sc in info.scenarios.iter().take(60) {
            let groups = vec![
                ("testcases", vec![(path.clone(), format!("line {}", sc.line))]),
                ("hooks", hooks.iter().map(|h| (h.clone(), "before and after".to_string())).collect()),
                ("pages", pages.iter().map(|f| (f.clone(), String::new())).collect()),
                ("testdata", data.iter().map(|f| (f.clone(), String::new())).collect()),
                ("driver", drivers.iter().map(|f| (f.clone(), String::new())).collect()),
                ("app_under_test", vec![]),
            ];
            let mut words = split_words(&sc.group);
            words.extend(split_words(&sc.name));
            let journey = if !sc.group.is_empty() { sc.group.clone() } else { file_words(path).first().cloned().unwrap_or_else(|| "tests".into()) };
            list.push(flow_json(format!("t:{path}#{}", sc.line), "test", sc.name.clone(), path.rsplit('/').next().unwrap_or(path).to_string(), path, sc.line, journey, &words, cx.stages(groups), json!({})));
        }
        if list.len() >= 500 { break; }
    }

    // ---------- requests ----------
    let mut class_index: HashMap<&str, Vec<&String>> = HashMap::new();
    let mut fn_index: HashMap<&str, Vec<&String>> = HashMap::new();
    for (p, i) in infos {
        for s in &i.symbols {
            if s.kind == "class" { class_index.entry(s.name.as_str()).or_default().push(p); }
            if s.kind == "function" { fn_index.entry(s.name.as_str()).or_default().push(p); }
        }
    }
    let mut routes: Vec<(String, RouteDef)> = Vec::new();
    for (p, i) in infos {
        if i.layer == "tests" || arch::is_test_path(&p.to_lowercase()) { continue; }
        for r in &i.routes { routes.push((p.clone(), r.clone())); }
    }
    // Next.js and similar: the file path is the address.
    static NEXT_API: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|/)(?:pages/(api/.+?)|app/((?:.+/)?)route)\.(?:js|ts|mjs)$").unwrap());
    for (p, i) in infos {
        if let Some(c) = NEXT_API.captures(p) {
            let raw = c.get(1).or_else(|| c.get(2)).map(|m| m.as_str()).unwrap_or("");
            let route = format!("/{}", raw.trim_end_matches('/').trim_end_matches("/index").replace('[', "{").replace(']', "}"));
            let methods: Vec<&str> = i.symbols.iter().map(|s| s.name.as_str()).filter(|n| matches!(*n, "GET" | "POST" | "PUT" | "PATCH" | "DELETE")).collect();
            for m in if methods.is_empty() { vec!["ANY"] } else { methods } {
                routes.push((p.clone(), RouteDef { method: m.into(), path: route.clone(), handler: String::new(), action: m.into(), line: 1 }));
            }
        }
    }
    // Express-style mounts: app.use('/api/rides', rides) puts every route in routes/rides.js under /api/rides.
    let mut mounts: HashMap<String, String> = HashMap::new();
    for (file, r) in routes.iter().filter(|(_, r)| r.method == "USE") {
        let ident = r.handler.rsplit('.').next().unwrap_or(&r.handler).to_lowercase();
        if ident.is_empty() { continue; }
        let target = imports_of.get(file).into_iter().flatten().find(|d| {
            let stem = d.rsplit('/').next().unwrap_or(d).split('.').next().unwrap_or("").to_lowercase();
            stem == ident || stem.trim_end_matches("routes").trim_end_matches("router").trim_end_matches(['.', '_', '-']) == ident.trim_end_matches("routes").trim_end_matches("router")
        });
        if let Some(t) = target { mounts.insert(t.clone(), r.path.trim_end_matches('/').to_string()); }
    }
    routes.retain(|(_, r)| r.method != "USE");
    for (file, r) in routes.iter_mut() {
        if let Some(prefix) = mounts.get(file.as_str()) {
            r.path = if r.path == "/" { prefix.clone() } else { format!("{prefix}/{}", r.path.trim_start_matches('/')) };
        }
    }
    let server_down: &[&str] = &["middleware", "controllers", "validation", "services", "integrations", "repos", "models", "db", "views", "jobs"];
    let mut route_ix: Vec<(usize, String, Vec<String>)> = Vec::new(); // (index in list, method, normalized path)
    for (file, r) in routes.iter().take(400) {
        let local = imports_of.get(file).cloned().unwrap_or_default();
        let pick = |cands: Option<&Vec<&String>>| -> Option<String> {
            let c = cands?;
            c.iter().find(|x| local.contains(x)).or_else(|| (c.len() == 1).then(|| &c[0])).map(|x| (*x).clone())
        };
        let mut handler_file = if !r.handler.is_empty() { pick(class_index.get(r.handler.as_str())) } else { None };
        if handler_file.is_none() && !r.handler.is_empty() {
            let h = r.handler.rsplit('.').next().unwrap_or(&r.handler).to_lowercase();
            handler_file = local.iter().find(|d| d.rsplit('/').next().unwrap_or(d).split('.').next().unwrap_or("").to_lowercase() == h).cloned();
        }
        if handler_file.is_none() && !r.action.is_empty() && !infos[file].symbols.iter().any(|s| s.name == r.action) {
            handler_file = pick(fn_index.get(r.action.as_str()));
        }
        let handler = handler_file.unwrap_or_else(|| file.clone());
        let below = cx.downstream(&handler, server_down, 4, 14);
        let mut groups = vec![];
        let route_note = format!("{} {}  line {}", r.method, r.path, r.line);
        if handler != *file { groups.push(("routes", vec![(file.clone(), route_note.clone())])); }
        let action_note = if r.action.is_empty() { if handler == *file { route_note.clone() } else { String::new() } } else { format!("{}()", r.action) };
        groups.push((cx.comp(&handler), vec![(handler.clone(), action_note)]));
        groups.extend(cx.by_comp(&below));
        let segs = norm_url(&r.path);
        let words: Vec<String> = segs.iter().filter(|s| *s != "*" && !GENERIC_SEG.contains(&s.as_str())).flat_map(|s| split_words(s)).chain(split_words(&r.action)).collect();
        let journey = segs.iter().filter(|s| *s != "*" && !GENERIC_SEG.contains(&s.as_str())).flat_map(|s| split_words(s)).next().unwrap_or_else(|| "home".into());
        let sub = if r.handler.is_empty() { r.action.clone() } else if r.action.is_empty() { r.handler.clone() } else { format!("{}.{}", r.handler, r.action) };
        let id = format!("r:{file}#{}:{}:{}", r.line, r.method, r.path);
        route_ix.push((list.len(), r.method.clone(), segs));
        list.push(flow_json(id, "request", format!("{} {}", r.method, r.path), sub, file, r.line, journey, &words, cx.stages(groups), json!({ "method": r.method, "path": r.path })));
    }

    // ---------- screens: stitched to the routes they call ----------
    let has_routes = !route_ix.is_empty();
    let screens: Vec<&String> = infos.keys().filter(|p| cx.comp(p) == "screens" && infos[*p].kind == "code" && infos[*p].lang != "html").collect();
    for s in screens.iter().take(200) {
        let below = cx.downstream(s, &["components", "state", "apiclient", "models", "services"], 4, 16);
        let mut calls = Vec::new();
        let mut matched: Vec<usize> = Vec::new();
        let mut gaps = Vec::new();
        for f in std::iter::once(*s).chain(below.iter()) {
            for c in &infos[f].calls {
                let segs = norm_url(&c.url);
                let best = route_ix.iter().filter(|(_, m, _)| m == "ANY" || *m == c.method || c.method == "GET" && m == "HEAD")
                    .map(|(i, _, r)| (url_match(&segs, r), *i)).filter(|(sc, _)| *sc > 0).max_by_key(|(sc, _)| *sc);
                let id = best.map(|(_, i)| list[i]["id"].clone()).unwrap_or(Value::Null);
                if let Some((_, i)) = best { if !matched.contains(&i) { matched.push(i); } }
                else if has_routes { gaps.push(format!("{} calls {} {} but no route in this project answers it", f.rsplit('/').next().unwrap_or(f), c.method, c.url)); }
                calls.push(json!({ "method": c.method, "url": c.url, "file": f, "line": c.line, "route": id }));
            }
        }
        let mut groups = vec![("screens", vec![((*s).clone(), String::new())])];
        groups.extend(cx.by_comp(&below));
        if !calls.is_empty() && !has_routes { groups.push(("server", vec![])); }
        for i in &matched {
            let r = &list[*i];
            for st in r["stages"].as_array().into_iter().flatten() {
                let c = arch::COMPS.iter().find(|k| Some(k.id) == st["comp"].as_str()).map(|k| k.id).unwrap_or("services");
                let files: Vec<(String, String)> = st["files"].as_array().into_iter().flatten().map(|f| (f["path"].as_str().unwrap_or("").to_string(), f["note"].as_str().unwrap_or("").to_string())).collect();
                groups.push((c, files));
            }
        }
        let name = s.rsplit('/').next().unwrap_or(s).split('.').next().unwrap_or("");
        let url = infos[*s].screen_url.clone();
        let url_words: Vec<String> = norm_url(&url).into_iter().filter(|w| w != "*" && !GENERIC_SEG.contains(&w.as_str())).flat_map(|w| split_words(&w)).collect();
        let generic_name = matches!(name.to_lowercase().as_str(), "index" | "app" | "main" | "page" | "default" | "layout" | "root");
        let mut words = file_words(s);
        if words.is_empty() || generic_name { words = url_words.clone(); }
        let journey = words.first().cloned().unwrap_or_else(|| "home".into());
        let title = if generic_name && !url.is_empty() {
            if url_words.is_empty() { "Home".to_string() } else { title_case(&url_words.join(" ")) }
        } else { screen_title(name) };
        let sub = if url.is_empty() { (*s).clone() } else { format!("{url}  ·  {s}") };
        let mut f = flow_json(format!("v:{s}"), "screen", title, sub, s, 1, journey, &words,
            cx.stages(groups), json!({ "calls": calls, "routes": matched.iter().map(|i| list[*i]["id"].clone()).collect::<Vec<_>>() }));
        if f["title"].as_str().map(|t| t.is_empty()).unwrap_or(true) { f["title"] = json!(name); }
        f["gaps"] = json!(gaps);
        list.push(f);
    }

    let mut reached: BTreeSet<String> = BTreeSet::new();
    for f in &list { for p in f["files"].as_array().into_iter().flatten() { if let Some(s) = p.as_str() { reached.insert(s.to_string()); } } }

    let journeys = journeys(&list, custom);
    Flows { list, journeys, undefined_steps, unused_steps, reached }
}

/// Groups flows into journeys and orders them through an app's life.
/// A saved custom ordering (from you or your AI chat) wins where it still fits.
pub fn journeys(list: &[Value], custom: Option<&Value>) -> Vec<Value> {
    let ids: BTreeSet<&str> = list.iter().filter_map(|f| f["id"].as_str()).collect();
    let mut out: Vec<Value> = Vec::new();
    let mut placed: BTreeSet<String> = BTreeSet::new();
    if let Some(saved) = custom.and_then(|c| c["journeys"].as_array()) {
        for (i, j) in saved.iter().enumerate() {
            let flows: Vec<String> = j["flows"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).filter(|x| ids.contains(x) && !placed.contains(*x)).map(String::from).collect();
            if flows.is_empty() { continue; }
            for f in &flows { placed.insert(f.clone()); }
            out.push(json!({ "id": format!("custom-{i}"), "name": j["name"].as_str().unwrap_or("Journey"), "about": j["about"].as_str().unwrap_or(""), "flows": flows, "custom": true }));
        }
    }
    let mut by: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for f in list {
        let id = f["id"].as_str().unwrap_or("");
        if placed.contains(id) { continue; }
        by.entry(f["journey"].as_str().unwrap_or("other").to_string()).or_default().push(f);
    }
    let mut auto: Vec<(f32, String, Vec<&Value>)> = by.into_iter().map(|(k, mut v)| {
        // Where people start (screens, scenarios) before what answers them; create before read before change.
        let kind_order = |f: &Value| match f["kind"].as_str() { Some("scenario" | "maestro" | "test") => 0, Some("screen") => 1, _ => 2 };
        let method_order = |f: &Value| match f["method"].as_str() { Some("POST") => 0, Some("GET") => if f["path"].as_str().map(|p| p.contains([':', '{', '<'])).unwrap_or(false) { 2 } else { 1 }, Some("PUT" | "PATCH") => 3, Some("DELETE") => 4, _ => 1 };
        v.sort_by(|a, b| a["rank"].as_f64().unwrap_or(6.5).partial_cmp(&b["rank"].as_f64().unwrap_or(6.5)).unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| kind_order(a).cmp(&kind_order(b)))
            .then_with(|| method_order(a).cmp(&method_order(b)))
            .then_with(|| a["title"].as_str().cmp(&b["title"].as_str())));
        let r = v.iter().map(|f| f["rank"].as_f64().unwrap_or(6.5) as f32).fold(f32::MAX, f32::min);
        let r = r.min(lifecycle(&crate::analyze::singular(&k.to_lowercase())));
        (r, k, v)
    }).collect();
    auto.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(b.2.len().cmp(&a.2.len())).then(a.1.cmp(&b.1)));
    let custom_mode = !out.is_empty();
    for (_, k, v) in auto {
        let name = if k.contains(' ') { k.clone() } else { title_case(&k) };
        out.push(json!({ "id": format!("auto-{k}"), "name": if custom_mode { format!("{name} (not placed)") } else { name }, "about": "", "flows": v.iter().map(|f| f["id"].clone()).collect::<Vec<_>>(), "custom": false }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_normalise_and_match_across_prefixes() {
        assert_eq!(norm_url("/api/rides/{id}/assign"), vec!["api", "rides", "*", "assign"]);
        assert_eq!(norm_url("${BASE_URL}/rides/${rideId}"), vec!["rides", "*"]);
        assert_eq!(norm_url("https://x.com/api/rides?x=1"), vec!["api", "rides"]);
        assert_eq!(url_match(&norm_url("/api/rides/7"), &norm_url("/api/rides/:id")), 2);
        assert_eq!(url_match(&norm_url("/rides"), &norm_url("/api/rides")), 1);
        assert_eq!(url_match(&norm_url("/api/drivers"), &norm_url("/api/rides")), 0);
    }

    #[test]
    fn lifecycle_orders_login_before_payment() {
        assert!(lifecycle("login") < lifecycle("ride"));
        assert!(lifecycle("assign") < lifecycle("payment"));
        assert!(lifecycle("payment") < lifecycle("logout"));
    }
}
