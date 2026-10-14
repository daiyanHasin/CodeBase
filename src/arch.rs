//! Project architecture, not folder layout.
//!
//! Every file gets a *component*: the part it plays in the design (scenario, step
//! definition, page object, driver, controller, service, model…). Components are
//! found from names, folders and what the code does (creates a WebDriver, finds
//! elements, queries a database, defines routes), so they work even when a project
//! keeps everything in one folder.
//!
//! The project gets an *archetype* (test automation, API, full-stack app, ML
//! pipeline…) which says in what order data moves through the components. That
//! order drives the pipeline diagram, the flows, and the design checks.
//!
//! Deterministic: no AI, no network.

use crate::analyze::{split_words, FileInfo};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub struct Comp {
    pub id: &'static str,
    pub name: &'static str,
    pub plain: &'static str,
}

pub const COMPS: &[Comp] = &[
    // test automation
    Comp { id: "scenarios", name: "Scenarios", plain: "Test cases written as plain steps people can read" },
    Comp { id: "testcases", name: "Test cases", plain: "Test functions that each check one behaviour" },
    Comp { id: "steps", name: "Step definitions", plain: "The code behind each scenario line" },
    Comp { id: "hooks", name: "Hooks", plain: "Run before and after every test: start the app, take screenshots, clean up" },
    Comp { id: "pages", name: "Page objects", plain: "One class per screen: where its buttons and fields are and how to use them" },
    Comp { id: "driver", name: "Driver", plain: "Starts the browser or device and sends it clicks and keystrokes" },
    Comp { id: "testdata", name: "Test data", plain: "Inputs the tests use: users, amounts, spreadsheets" },
    Comp { id: "testconfig", name: "Test settings", plain: "Which browser or device, which environment, timeouts" },
    Comp { id: "testutil", name: "Test helpers", plain: "Shared waits, readers, loggers and small tools for tests" },
    Comp { id: "reports", name: "Reports", plain: "Results, screenshots and logs from test runs" },
    Comp { id: "app_under_test", name: "App under test", plain: "The real app the tests drive (not in this project)" },
    // applications
    Comp { id: "entry", name: "Start-up", plain: "Where the program begins and wires its parts together" },
    Comp { id: "screens", name: "Screens", plain: "Full pages people navigate to" },
    Comp { id: "components", name: "UI components", plain: "Reusable pieces of the screen" },
    Comp { id: "state", name: "State", plain: "Data shared between screens" },
    Comp { id: "apiclient", name: "API client", plain: "Sends requests from the screens to the server" },
    Comp { id: "server", name: "Server", plain: "The backend the screens talk to (not in this project)" },
    Comp { id: "routes", name: "Routes", plain: "The addresses the server answers and who handles each" },
    Comp { id: "middleware", name: "Guards", plain: "Checks that run before a request is handled, like login" },
    Comp { id: "controllers", name: "Controllers", plain: "Receive a request and decide what happens" },
    Comp { id: "validation", name: "Validation", plain: "Checks that submitted data is correct" },
    Comp { id: "services", name: "Business logic", plain: "The rules and calculations the app is about" },
    Comp { id: "jobs", name: "Background jobs", plain: "Work that runs later, on a schedule, or after an event" },
    Comp { id: "integrations", name: "External services", plain: "Talks to payments, email, maps and other outside systems" },
    Comp { id: "repos", name: "Data access", plain: "Reads and writes the database" },
    Comp { id: "models", name: "Models", plain: "The shape of stored things: users, rides, bills" },
    Comp { id: "db", name: "Database", plain: "Tables, migrations and seed data" },
    Comp { id: "views", name: "Templates", plain: "Pages the server fills with data before sending" },
    Comp { id: "styles", name: "Styles", plain: "Colours, spacing and layout rules" },
    Comp { id: "cli", name: "Commands", plain: "The commands a user types to run the tool" },
    // machine learning
    Comp { id: "ml_data", name: "Data loading", plain: "Reads and prepares datasets" },
    Comp { id: "ml_model", name: "Model", plain: "The network or algorithm definition" },
    Comp { id: "ml_train", name: "Training", plain: "Teaches the model from data" },
    Comp { id: "ml_eval", name: "Evaluation", plain: "Measures how good the model is" },
    Comp { id: "notebooks", name: "Notebooks", plain: "Experiments and exploration" },
    // everywhere
    Comp { id: "utils", name: "Shared helpers", plain: "Small tools used in many places" },
    Comp { id: "config", name: "Settings", plain: "Configuration and the list of libraries used" },
    Comp { id: "infra", name: "Build and deploy", plain: "Docker, CI and scripts that build or ship it" },
    Comp { id: "docs", name: "Docs", plain: "Written explanations" },
    Comp { id: "assets", name: "Media", plain: "Images, fonts and other files" },
];

pub fn comp(id: &str) -> &'static Comp {
    COMPS.iter().find(|c| c.id == id).unwrap_or(&COMPS[COMPS.len() - 1])
}

/// Components that are not files in this project; they close a pipeline.
pub const VIRTUAL: &[&str] = &["app_under_test", "server"];

pub struct Archetype {
    pub id: &'static str,
    pub name: &'static str,
    /// The main path data takes, in order.
    pub pipeline: &'static [&'static str],
    /// Supporting parts and what they do for the pipeline.
    pub sides: &'static [(&'static str, &'static str)],
}

pub const ARCHETYPES: &[Archetype] = &[
    Archetype {
        id: "test_automation", name: "Test automation framework",
        pipeline: &["scenarios", "testcases", "steps", "pages", "driver", "app_under_test"],
        sides: &[("hooks", "start and stop the driver around every test"), ("testdata", "feeds inputs into the steps"),
            ("testconfig", "tells the driver which browser, device and environment"), ("testutil", "shared waits and readers"),
            ("reports", "collect the results"), ("config", "project setup")],
    },
    Archetype {
        id: "fullstack", name: "Full-stack app",
        pipeline: &["screens", "components", "state", "apiclient", "routes", "middleware", "controllers", "validation", "services", "integrations", "repos", "models", "db"],
        sides: &[("entry", "starts the app"), ("views", "server pages"), ("jobs", "work done later"), ("styles", "look and feel"), ("utils", "shared helpers"), ("config", "settings")],
    },
    Archetype {
        id: "web_mvc", name: "Server-rendered web app",
        pipeline: &["routes", "middleware", "controllers", "validation", "services", "integrations", "repos", "models", "db", "views"],
        sides: &[("entry", "starts the app"), ("jobs", "work done later"), ("components", "screen pieces"), ("styles", "look and feel"), ("utils", "shared helpers"), ("config", "settings")],
    },
    Archetype {
        id: "api", name: "Backend API",
        pipeline: &["entry", "routes", "middleware", "controllers", "validation", "services", "integrations", "repos", "models", "db"],
        sides: &[("jobs", "work done later"), ("utils", "shared helpers"), ("config", "settings")],
    },
    Archetype {
        id: "mobile", name: "Mobile app",
        pipeline: &["entry", "screens", "components", "state", "apiclient", "server"],
        sides: &[("models", "data shapes"), ("services", "app logic"), ("styles", "look and feel"), ("utils", "shared helpers"), ("config", "settings")],
    },
    Archetype {
        id: "frontend", name: "Front-end app",
        pipeline: &["entry", "screens", "components", "state", "apiclient", "server"],
        sides: &[("models", "data shapes"), ("services", "app logic"), ("styles", "look and feel"), ("utils", "shared helpers"), ("config", "settings")],
    },
    Archetype {
        id: "ml", name: "Machine learning project",
        pipeline: &["ml_data", "ml_model", "ml_train", "ml_eval"],
        sides: &[("notebooks", "experiments"), ("utils", "shared helpers"), ("config", "settings"), ("entry", "start-up")],
    },
    Archetype {
        id: "cli", name: "Command-line tool",
        pipeline: &["entry", "cli", "services", "integrations", "models"],
        sides: &[("utils", "shared helpers"), ("config", "settings"), ("views", "output pages"), ("screens", "user interface")],
    },
    Archetype {
        id: "library", name: "Library or app",
        pipeline: &["entry", "screens", "controllers", "services", "integrations", "models", "db"],
        sides: &[("utils", "shared helpers"), ("config", "settings")],
    },
];

pub fn archetype(id: &str) -> &'static Archetype {
    ARCHETYPES.iter().find(|a| a.id == id).unwrap_or(&ARCHETYPES[ARCHETYPES.len() - 1])
}

/// Position of a component along the archetype's data path. Side parts sort after.
pub fn rank(a: &Archetype, comp: &str) -> Option<usize> {
    a.pipeline.iter().position(|c| *c == comp)
}

#[derive(Clone, Copy, Default)]
pub struct Ctx {
    pub test_project: bool,
    pub ml_project: bool,
}

const TEST_DIRS: &[&str] = &["test", "tests", "spec", "specs", "e2e", "automation", "qa", "cypress", "playwright", "features", "acceptance", "ui-tests", "uitests", "integration-tests", "__tests__", "maestro", ".maestro"];

pub fn is_test_path(p: &str) -> bool {
    let segs: Vec<&str> = p.split('/').collect();
    let dirs = &segs[..segs.len().saturating_sub(1)];
    dirs.iter().any(|d| {
        TEST_DIRS.contains(d)
            || [".tests", ".specs", ".automation", ".e2e", ".acceptancetests", ".uitests", "tests", "specs"].iter().any(|s| d.len() > s.len() + 1 && d.ends_with(s))
    })
}

fn ends_any(s: &str, list: &[&str]) -> bool { list.iter().any(|x| s.len() > x.len() && s.ends_with(x)) || list.contains(&s) }

const FRONT_EXT: &[&str] = &["jsx", "tsx", "vue", "svelte", "dart"];

/// What part a file plays in the design.
pub fn classify(path: &str, info: &FileInfo, ctx: &Ctx) -> &'static str {
    let p = path.to_lowercase();
    let segs: Vec<&str> = p.split('/').collect();
    let fname = *segs.last().unwrap_or(&"");
    let dirs = &segs[..segs.len().saturating_sub(1)];
    let in_dir = |names: &[&str]| dirs.iter().any(|d| names.contains(d));
    let stem_orig = path.rsplit('/').next().unwrap_or(path).split('.').next().unwrap_or("");
    let stem = stem_orig.to_lowercase();
    let ext = if fname.contains('.') { fname.rsplit('.').next().unwrap_or("") } else { "" };
    let sig = |s: &str| info.signals.contains(&s);
    let lang = info.lang.as_str();
    let role = info.role.unwrap_or("");
    let serves = info.routes.iter().any(|r| r.method != "USE");

    // Same everywhere.
    if lang == "markdown" || (lang == "text" && !matches!(ext, "csv" | "tsv")) { return "docs"; }
    if lang == "notebook" { return "notebooks"; }
    if fname == "dockerfile" || fname.starts_with("docker-compose") || fname == "makefile" || fname == "procfile"
        || matches!(ext, "bat" | "cmd" | "ps1" | "sh") || in_dir(&[".github", ".gitlab", ".circleci", "deploy", "k8s", "helm", "terraform", "ci", "scripts", "docker"]) {
        return "infra";
    }

    let data_ext = matches!(ext, "xlsx" | "xls" | "csv" | "tsv");
    let in_tests = ctx.test_project || info.layer == "tests" || is_test_path(&p);
    if in_tests {
        if lang == "gherkin" || sig("maestro") { return "scenarios"; }
        if in_dir(&["hooks", "hook"]) || ends_any(&stem, &["hooks", "hook"]) || stem == "conftest" || (stem == "environment" && lang == "python") { return "hooks"; }
        if in_dir(&["stepdefinitions", "step_definitions", "stepdefs", "steps", "glue", "step-definitions"]) || ends_any(&stem, &["steps", "stepdefinitions", "stepdefs", "_steps", "step"]) {
            if info.kind == "code" && lang != "json" { return "steps"; }
        }
        if !info.steps.is_empty() { return "steps"; }
        let code = info.kind == "code" && !matches!(lang, "json" | "yaml" | "xml" | "toml" | "config" | "csv");
        if code && (in_dir(&["pages", "pageobjects", "page_objects", "page-objects", "pageobject", "pom", "screens", "screenobjects", "screen_objects"])
            || ends_any(&stem, &["page", "pageobject", "screen", "po"]) && stem_orig.chars().next().map(|c| c.is_uppercase() || lang == "python" || lang == "javascript" || lang == "typescript").unwrap_or(false)) {
            return "pages";
        }
        if code && (in_dir(&["driver", "drivers", "browser", "browsers", "session", "webdriver"]) || ["driver", "browser", "webdriver"].iter().any(|k| stem.contains(k)) && ends_any(&stem, &["driver", "browser", "factory", "manager", "setup", "context", "provider", "helper", "session"])) {
            return "driver";
        }
        if code && sig("hook") && info.scenarios.is_empty() { return "hooks"; }
        if code && sig("driver") && info.scenarios.is_empty() { return "driver"; }
        if code && sig("locator") && !sig("assert") && info.scenarios.is_empty() { return "pages"; }
        if in_dir(&["reports", "report", "allure-results", "test-results", "testresults", "screenshots", "logs"]) { return "reports"; }
        let config_name = stem.contains("config") || stem.contains("setting") || stem.starts_with("appsettings") || stem.contains("capabilit")
            || matches!(ext, "properties" | "runsettings" | "ini" | "csproj" | "fsproj" | "toml" | "gradle")
            || ["specflow", "reqnroll", "cucumber", "wdio", "pytest", "testng", "package", "requirements", "pom", "tsconfig", "jsconfig", ".env"].iter().any(|k| stem.starts_with(k) || fname.starts_with(k));
        if in_dir(&["testdata", "test_data", "test-data", "data", "datasets", "fixtures", "payloads", "inputs", "testfiles"]) && !config_name { return "testdata"; }
        if data_ext || (matches!(lang, "json" | "yaml" | "xml" | "csv") && !config_name) { return "testdata"; }
        if config_name || info.layer == "setup" { return "testconfig"; }
        if in_dir(&["utils", "utilities", "util", "helpers", "helper", "common", "support", "extensions", "core", "base", "framework", "lib", "tools"]) && code { return "testutil"; }
        if !info.scenarios.is_empty() || info.layer == "tests" { return "testcases"; }
        if ctx.test_project {
            return match info.layer { "media" => "assets", "scripts" => "infra", _ => "testutil" };
        }
    }

    if info.kind == "asset" { return if data_ext && ctx.ml_project { "ml_data" } else { "assets" }; }
    if data_ext || lang == "csv" { return if ctx.ml_project { "ml_data" } else { "db" }; }

    if lang == "python" && info.signals.iter().any(|s| s.starts_with("ml_")) {
        if sig("ml_train") && (stem.contains("train") || !sig("ml_model")) { return "ml_train"; }
        if sig("ml_eval") && (stem.contains("eval") || stem.contains("metric") || stem.contains("test") || stem.contains("bench")) { return "ml_eval"; }
        if sig("ml_model") { return "ml_model"; }
        if sig("ml_data") { return "ml_data"; }
        if sig("ml_eval") { return "ml_eval"; }
        return "ml_train";
    }

    // Front-end code that sends requests to this project's own server.
    let relative_call = info.calls.iter().any(|c| !c.url.starts_with("http") || c.url.contains("localhost"));
    if matches!(lang, "javascript" | "typescript" | "dart") && info.routes.is_empty() && relative_call
        && !FRONT_EXT.contains(&ext) && !matches!(role, "Page" | "UI component" | "View component" | "Hook" | "State management")
        && !in_dir(&["server", "backend", "functions", "lambda", "api-server", "pages", "screens", "components", "views"]) && !sig("db") {
        return "apiclient";
    }
    // The file's own name beats the folder it sits in: feature folders keep
    // article.controller.ts, article.service.ts and article.model.ts side by side.
    if !FRONT_EXT.contains(&ext) && info.kind == "code" {
        if fname.ends_with(".d.ts") { return "models"; }
        let file_orig = path.rsplit('/').next().unwrap_or(path);
        let base = file_orig.rsplit_once('.').map(|(b, _)| b).unwrap_or(file_orig);
        if let Some(c) = name_says(base, lang) {
            let c = if c == "services" && relative_call && matches!(lang, "javascript" | "typescript" | "dart") && !in_dir(&["server", "backend", "api"]) { "apiclient" } else { c };
            if c != "routes" || !info.routes.is_empty() || info.symbols.is_empty() { return c; }
        }
    }
    let routes_file = !info.routes.is_empty() || !info.mounts.is_empty() || matches!(stem.as_str(), "routes" | "router" | "web" | "api" | "urls" | "index" | "channels" | "console");
    match role {
        "Route definitions" if routes_file => return "routes",
        "Middleware" | "Authorization policy" => return "middleware",
        "Form request validation" => return "validation",
        "Database migration" | "Database seeder" | "Model factory" => return "db",
        "Background job or event" => return "jobs",
        "Service provider" => return "entry",
        "Project documentation" => return "docs",
        "Configuration" | "Dependency manifest" | "Build or tool config" => return "config",
        _ => {}
    }
    if info.layer == "setup" { return "config"; }
    if lang == "sql" || ext == "prisma" || in_dir(&["migrations", "seeds", "seeders"]) { return "db"; }
    if matches!(lang, "css" | "scss") { return "styles"; }
    if lang == "blade" || matches!(ext, "cshtml" | "razor" | "erb" | "twig" | "hbs" | "jinja" | "jinja2" | "ejs" | "njk")
        || (lang == "html" && in_dir(&["templates", "views", "layouts", "partials"])) {
        return "views";
    }

    let front = FRONT_EXT.contains(&ext) || matches!(lang, "vue" | "svelte" | "dart")
        || (matches!(lang, "javascript" | "typescript") && !serves && in_dir(&["components", "pages", "screens", "views", "hooks", "store", "stores", "composables", "context", "contexts", "redux", "slices", "features", "app", "widgets"]) && !in_dir(&["server", "backend", "api"]));
    if sig("screen") && info.kind == "code" { return "screens"; }
    if front {
        if matches!(role, "Page") || in_dir(&["pages", "screens", "views"]) || stem == "page" { return "screens"; }
        if in_dir(&["components", "widgets", "ui", "layouts", "shared"]) { return "components"; }
        if ends_any(&stem, &["screen", "page"]) && stem_orig.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) { return "screens"; }
        if matches!(role, "Hook" | "State management") || in_dir(&["store", "stores", "redux", "state", "context", "contexts", "providers", "slices", "composables", "hooks", "bloc", "blocs", "cubit", "riverpod"]) { return "state"; }
        if !info.calls.is_empty() || in_dir(&["api", "services", "service", "clients", "client", "repositories", "data"]) { return "apiclient"; }
        if matches!(role, "UI component" | "View component" | "Page layout") || in_dir(&["components", "widgets", "ui", "layouts"]) { return "components"; }
        if role == "Entry point" || matches!(stem.as_str(), "main" | "index" | "app" | "_app" | "layout" | "router" | "routes") { return "entry"; }
        if matches!(role, "Helper") || in_dir(&["utils", "util", "helpers", "lib", "common", "shared"]) { return "utils"; }
        if info.layer == "data" || in_dir(&["models", "types", "entities"]) { return "models"; }
        return "components";
    }

    if role == "Entry point" && !serves { return "entry"; }
    if role == "Controller" || serves || in_dir(&["handlers", "handler", "endpoints", "resolvers", "controllers"]) {
        return if serves && matches!(stem.as_str(), "routes" | "router" | "urls" | "web" | "api" | "endpoints") { "routes" } else { "controllers" };
    }
    if in_dir(&["validators", "validation", "validations"]) || ends_any(&stem, &["validator", "request"]) && lang == "php" { return "validation"; }
    if in_dir(&["repositories", "repository", "repos", "dao", "dal", "data_access", "persistence"]) || ends_any(&stem, &["repository", "repo", "dao"]) { return "repos"; }
    if role == "Model" || in_dir(&["entities", "entity", "domain", "dto", "dtos", "models"]) { return "models"; }
    if in_dir(&["integrations", "integration", "gateways", "gateway", "adapters", "external", "thirdparty", "third_party"]) || ends_any(&stem, &["gateway", "adapter"]) || sig("external") { return "integrations"; }
    if lang == "html" { return "screens"; }
    if role == "Entry point" || matches!(stem.as_str(), "program" | "startup" | "main" | "lib" | "server" | "wsgi" | "asgi" | "manage" | "__main__" | "bootstrap" | "kernel") { return "entry"; }
    if sig("cli") || in_dir(&["commands", "cmd", "cli", "console"]) { return "cli"; }
    if sig("db") && info.layer == "logic" { return "repos"; }
    if in_dir(&["services", "service", "usecases", "use_cases", "application", "managers", "core", "engine", "logic", "business", "features"])
        || ends_any(&stem, &["service", "manager", "engine", "usecase", "interactor", "processor", "calculator", "runner", "worker"]) {
        return "services";
    }
    if role == "Helper" || in_dir(&["utils", "util", "helpers", "helper", "lib", "common", "shared", "extensions", "tools"]) { return "utils"; }
    match info.layer {
        "ui" => "components",
        "server" => "controllers",
        "data" => "models",
        "tests" => "testcases",
        "docs" => "docs",
        "scripts" => "infra",
        "media" => "assets",
        _ => "services",
    }
}

/// The part a file's own name claims: "ArticleService", "article.service", "bill_repository".
fn name_says(stem: &str, lang: &str) -> Option<&'static str> {
    let last = if stem.contains('.') { stem.rsplit('.').next().unwrap_or(stem).to_lowercase() } else {
        let words = split_camel(stem);
        words.last().cloned().unwrap_or_default()
    };
    if stem.eq_ignore_ascii_case(&last) && !stem.contains('.') {
        // A one-word name only counts when the word itself names a part (routes.js, models.py).
        if !matches!(last.as_str(), "routes" | "router" | "urls" | "models" | "schemas" | "middleware" | "validators" | "utils" | "helpers" | "services" | "repositories" | "jobs" | "tasks" | "worker" | "seed" | "seeds" | "seeder") { return None; }
    }
    Some(match last.as_str() {
        "service" | "services" | "usecase" | "interactor" | "engine" | "calculator" | "processor" => "services",
        "controller" | "controllers" | "handler" | "handlers" | "resolver" | "resolvers" | "endpoint" | "endpoints" | "resource" if lang != "xml" => "controllers",
        "model" | "models" | "entity" | "entities" | "schema" | "schemas" | "dto" | "dtos" | "record" => "models",
        "repository" | "repositories" | "repo" | "dao" => "repos",
        "middleware" | "middlewares" | "guard" | "guards" | "interceptor" | "policy" => "middleware",
        "validator" | "validators" | "validation" => "validation",
        "util" | "utils" | "helper" | "helpers" | "mapper" | "mappers" | "formatter" | "constants" | "extensions" => "utils",
        "job" | "jobs" | "worker" | "queue" | "listener" | "consumer" | "cron" | "scheduler" | "tasks" => "jobs",
        "gateway" | "adapter" | "integration" => "integrations",
        "routes" | "router" | "urls" | "routing" => "routes",
        "migration" | "migrations" | "seeder" | "seed" | "seeds" => "db",
        "component" => "components",
        "reducer" | "slice" | "effects" | "actions" | "selectors" => "state",
        _ => return None,
    })
}

fn split_camel(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let boundary = c == '_' || c == '-' || (c.is_uppercase() && i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit()));
        if boundary && !cur.is_empty() { out.push(cur.to_lowercase()); cur.clear(); }
        if c != '_' && c != '-' { cur.push(c); }
    }
    if !cur.is_empty() { out.push(cur.to_lowercase()); }
    out
}

/// Decides what kind of project this is from the files' behaviour and the libraries it uses.
pub fn detect(infos: &BTreeMap<String, FileInfo>, stack: &[String]) -> (&'static Archetype, Ctx) {
    let has = |s: &str| stack.iter().any(|x| x == s);
    let (mut t, mut app, mut front, mut back, mut views, mut ml, mut cli) = (0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    for (path, i) in infos {
        if i.kind != "code" { continue; }
        let p = path.to_lowercase();
        let ext = p.rsplit('.').next().unwrap_or("");
        if i.lang == "gherkin" || i.signals.contains(&"maestro") || !i.steps.is_empty() || i.signals.contains(&"driver") || i.signals.contains(&"locator") { t += 1; }
        if i.layer == "tests" || is_test_path(&p) { continue; }
        if i.routes.iter().any(|r| r.method != "USE") || matches!(i.role, Some("Controller" | "Route definitions" | "Database migration")) { back += 1; }
        if i.role == Some("Model") { back += 1; }
        if FRONT_EXT.contains(&ext) || matches!(i.lang.as_str(), "vue" | "svelte") || i.role == Some("Page") { front += 1; }
        if i.lang == "blade" || p.contains("/templates/") || ext == "cshtml" { views += 1; }
        if i.signals.iter().any(|s| s.starts_with("ml_")) { ml += 1; }
        if i.signals.contains(&"cli") { cli += 1; }
    }
    app += back + front + views;
    for k in ["Selenium", "Playwright", "Appium", "Reqnroll", "SpecFlow", "Cypress", "WebdriverIO", "Cucumber", "Maestro", "behave", "pytest-bdd"] {
        if has(k) { t += 3; }
    }
    let ctx_ml = ml >= 2 && ml * 2 >= app;
    let id = if t >= 3 && app <= t {
        "test_automation"
    } else if ctx_ml {
        "ml"
    } else if front >= 2 && back >= 2 {
        "fullstack"
    } else if back >= 2 && views >= 2 {
        "web_mvc"
    } else if back >= 1 {
        "api"
    } else if front >= 2 || has("React") || has("Vue") || has("Svelte") || has("Angular") || has("Flutter") || has("React Native") {
        if has("Flutter") || has("React Native") || has("Expo") || has(".NET MAUI") { "mobile" } else { "frontend" }
    } else if cli >= 1 {
        "cli"
    } else {
        "library"
    };
    (archetype(id), Ctx { test_project: id == "test_automation", ml_project: ctx_ml })
}

/// The architecture: components, how they connect, the data path, and the m-ary tree.
pub struct Arch {
    pub archetype: &'static Archetype,
    pub ctx: Ctx,
    pub comp_of: HashMap<String, &'static str>,
    pub json: Value,
}

fn group_files(files: &[&String], comp_id: &str) -> Vec<(String, Vec<String>)> {
    if files.len() <= 10 { return vec![]; }
    // 1. By sub-folder under the folder these files share.
    let split: Vec<Vec<&str>> = files.iter().map(|f| f.split('/').collect()).collect();
    let mut common = 0;
    'outer: loop {
        let first = match split[0].get(common) { Some(s) => *s, None => break };
        for s in &split {
            if s.len() <= common + 1 || s[common] != first { break 'outer; }
        }
        common += 1;
    }
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (f, s) in files.iter().zip(&split) {
        let key = if s.len() > common + 1 { s[common].to_string() } else { String::new() };
        by.entry(key).or_default().push((*f).clone());
    }
    let real = by.keys().filter(|k| !k.is_empty()).count();
    if real >= 2 && real <= files.len() / 2 {
        return by.into_iter().map(|(k, v)| (if k.is_empty() { "(top)".to_string() } else { k }, v)).collect();
    }
    // 2. By the leading word of the file name (LoginPage, LoginSteps -> "login").
    let mut words: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in files {
        let stem = f.rsplit('/').next().unwrap_or(f).split('.').next().unwrap_or("");
        let w = split_words(stem).into_iter().find(|w| !comp_id.contains(w.as_str()) && !["page", "step", "screen", "spec"].contains(&w.as_str())).unwrap_or_default();
        words.entry(w).or_default().push((*f).clone());
    }
    let multi: Vec<(String, Vec<String>)> = words.iter().filter(|(k, v)| !k.is_empty() && v.len() >= 2).map(|(k, v)| (k.clone(), v.clone())).collect();
    if multi.len() >= 2 {
        let mut rest: Vec<String> = Vec::new();
        let keep: BTreeSet<&String> = multi.iter().map(|(k, _)| k).collect();
        for (k, v) in &words { if !keep.contains(k) { rest.extend(v.iter().cloned()); } }
        let mut out = multi;
        if !rest.is_empty() { out.push(("other".into(), rest)); }
        return out;
    }
    vec![]
}

pub fn build(
    name: &str,
    infos: &BTreeMap<String, FileInfo>,
    imports_of: &HashMap<String, Vec<String>>,
    tests_of: &HashMap<String, Vec<String>>,
    stack: &[String],
) -> Arch {
    let (arch, ctx) = detect(infos, stack);
    let comp_of: HashMap<String, &'static str> = infos.iter().map(|(p, i)| (p.clone(), classify(p, i, &ctx))).collect();

    let mut by_comp: BTreeMap<&'static str, Vec<&String>> = BTreeMap::new();
    for (p, c) in &comp_of { by_comp.entry(c).or_default().push(p); }
    for v in by_comp.values_mut() { v.sort(); }

    // Component-to-component links, from imports, test links and step matching.
    let mut edges: BTreeMap<(&str, &str), (usize, Vec<(String, String)>)> = BTreeMap::new();
    let mut add_edge = |fa: &String, fb: &String| {
        let (Some(ca), Some(cb)) = (comp_of.get(fa), comp_of.get(fb)) else { return };
        if ca == cb { return; }
        let e = edges.entry((*ca, *cb)).or_default();
        e.0 += 1;
        if e.1.len() < 6 { e.1.push((fa.clone(), fb.clone())); }
    };
    for (f, deps) in imports_of { for d in deps { add_edge(f, d); } }
    for (f, deps) in tests_of { for d in deps { add_edge(f, d); } }

    let present = |c: &str| by_comp.contains_key(c);
    let mut pipeline: Vec<&'static str> = arch.pipeline.iter().copied().filter(|c| present(c) || VIRTUAL.contains(c)).collect();
    // A virtual end only makes sense when something real leads to it.
    if pipeline.iter().all(|c| VIRTUAL.contains(c)) { pipeline.clear(); }
    if arch.id == "test_automation" && !present("driver") && !present("pages") && !present("steps") && !present("scenarios") { pipeline.retain(|c| *c != "app_under_test"); }
    if matches!(arch.id, "frontend" | "mobile") && !present("apiclient") { pipeline.retain(|c| *c != "server"); }

    let link_between = |a: &str, b: &str| -> usize {
        edges.get(&(a, b)).map(|e| e.0).unwrap_or(0) + edges.get(&(b, a)).map(|e| e.0).unwrap_or(0)
    };
    let mut path_links: Vec<Value> = Vec::new();
    for w in pipeline.windows(2) {
        let (a, b) = (w[0], w[1]);
        let seen = if VIRTUAL.contains(&b) { 1 } else { link_between(a, b) };
        path_links.push(json!({ "from": a, "to": b, "links": seen, "seen": seen > 0 }));
    }

    let sides: Vec<Value> = arch.sides.iter().filter(|(c, _)| present(c) && !pipeline.contains(c))
        .map(|(c, how)| json!({ "id": c, "how": how })).collect();

    // Display order: data path, then its supporting parts, then everything else by size.
    let mut order: Vec<&'static str> = pipeline.iter().copied().filter(|c| !VIRTUAL.contains(c)).collect();
    for (c, _) in arch.sides { if present(c) && !order.contains(c) { order.push(c); } }
    let mut rest: Vec<(&'static str, usize)> = by_comp.iter().filter(|(c, _)| !order.contains(c)).map(|(c, v)| (*c, v.len())).collect();
    let tail = ["config", "infra", "docs", "assets"];
    rest.sort_by(|a, b| tail.contains(&a.0).cmp(&tail.contains(&b.0)).then(b.1.cmp(&a.1)).then(a.0.cmp(b.0)));
    order.extend(rest.into_iter().map(|(c, _)| c));

    let comps: Vec<Value> = order.iter().map(|c| {
        let k = comp(c);
        let files = &by_comp[c];
        let mut uses: BTreeMap<&str, usize> = BTreeMap::new();
        let mut used_by: BTreeMap<&str, usize> = BTreeMap::new();
        for ((a, b), (n, _)) in &edges {
            if a == c { uses.insert(b, *n); }
            if b == c { used_by.insert(a, *n); }
        }
        json!({
            "id": c, "name": k.name, "plain": k.plain, "count": files.len(),
            "stage": rank(arch, c).filter(|_| pipeline.contains(c)).map(|_| pipeline.iter().position(|x| x == c)),
            "uses": uses, "usedBy": used_by,
        })
    }).collect();

    // The m-ary tree: project -> components -> groups -> files.
    let tree_children: Vec<Value> = order.iter().map(|c| {
        let files = &by_comp[c];
        let groups = group_files(files, c);
        let children: Vec<Value> = if groups.is_empty() {
            files.iter().map(|f| json!({ "id": f, "kind": "file" })).collect()
        } else {
            groups.into_iter().map(|(g, fs)| json!({
                "id": format!("g:{c}:{g}"), "kind": "group", "name": g, "count": fs.len(),
                "children": fs.iter().map(|f| json!({ "id": f, "kind": "file" })).collect::<Vec<_>>(),
            })).collect()
        };
        json!({ "id": format!("c:{c}"), "kind": "component", "comp": c, "name": comp(c).name, "count": files.len(), "children": children })
    }).collect();

    let edge_list: Vec<Value> = edges.iter().map(|((a, b), (n, ex))| json!({
        "from": a, "to": b, "links": n, "examples": ex.iter().map(|(x, y)| json!([x, y])).collect::<Vec<_>>(),
    })).collect();

    let json = json!({
        "archetype": { "id": arch.id, "name": arch.name },
        "pipeline": pipeline.iter().map(|c| json!({ "id": c, "name": comp(c).name, "plain": comp(c).plain, "count": by_comp.get(c).map(|v| v.len()).unwrap_or(0), "virtual": VIRTUAL.contains(c) })).collect::<Vec<_>>(),
        "path": path_links,
        "sides": sides,
        "components": comps,
        "edges": edge_list,
        "tree": { "id": "root", "kind": "root", "name": name, "children": tree_children },
    });
    Arch { archetype: arch, ctx, comp_of, json }
}

/// Classifies a path alone (no file content), for files in older versions.
pub fn classify_path(path: &str, ctx: &Ctx) -> &'static str {
    let info = crate::analyze::non_text(path, 0, String::new(), "code", String::new());
    classify(path, &info, ctx)
}

// ---------------------------------------------------------------------------
// Design checks
// ---------------------------------------------------------------------------

/// "1 controller queries", "3 controllers query": check titles read as sentences.
fn count(n: usize, one: &str, many: &str) -> String { format!("{n} {}", if n == 1 { one } else { many }) }

fn check(sev: &str, kind: &str, title: String, detail: String, fix: &str, files: Vec<String>) -> Value {
    json!({ "severity": sev, "kind": kind, "title": title, "detail": detail, "fix": fix, "files": files })
}

/// Import cycles: groups of files that all depend on each other (Tarjan).
fn cycles(files: &[&String], imports_of: &HashMap<String, Vec<String>>) -> Vec<Vec<String>> {
    let idx: HashMap<&str, usize> = files.iter().enumerate().map(|(i, f)| (f.as_str(), i)).collect();
    let n = files.len();
    let adj: Vec<Vec<usize>> = files.iter().map(|f| imports_of.get(*f).map(|d| d.iter().filter_map(|x| idx.get(x.as_str()).copied()).collect()).unwrap_or_default()).collect();
    let (mut index, mut low, mut on, mut stack, mut out) = (vec![usize::MAX; n], vec![0; n], vec![false; n], Vec::new(), Vec::new());
    let mut counter = 0;
    for s in 0..n {
        if index[s] != usize::MAX { continue; }
        // Iterative Tarjan: (node, next child position)
        let mut work: Vec<(usize, usize)> = vec![(s, 0)];
        while let Some(&mut (v, ref mut i)) = work.last_mut() {
            if *i == 0 && index[v] == usize::MAX {
                index[v] = counter; low[v] = counter; counter += 1;
                stack.push(v); on[v] = true;
            }
            if *i < adj[v].len() {
                let w = adj[v][*i];
                *i += 1;
                if index[w] == usize::MAX { work.push((w, 0)); }
                else if on[w] { low[v] = low[v].min(index[w]); }
            } else {
                work.pop();
                if let Some(&(parent, _)) = work.last() { low[parent] = low[parent].min(low[v]); }
                if low[v] == index[v] {
                    let mut comp = Vec::new();
                    while let Some(w) = stack.pop() { on[w] = false; comp.push(files[w].clone()); if w == v { break; } }
                    if comp.len() >= 2 { comp.sort(); out.push(comp); }
                }
            }
        }
    }
    out.sort_by(|a, b| b.len().cmp(&a.len()));
    out
}

pub struct CheckInput<'a> {
    pub infos: &'a BTreeMap<String, FileInfo>,
    pub imports_of: &'a HashMap<String, Vec<String>>,
    pub imported_by: &'a HashMap<String, Vec<String>>,
    pub tests_of: &'a HashMap<String, Vec<String>>,
    pub arch: &'a Arch,
    /// Step lines with no matching definition: (feature file, line, text)
    pub undefined_steps: &'a [(String, usize, String)],
    /// Step definition patterns no scenario uses: (file, pattern)
    pub unused_steps: &'a [(String, String)],
    /// Files that a route, screen or test reaches.
    pub reached: &'a BTreeSet<String>,
}

pub fn checks(i: &CheckInput) -> Vec<Value> {
    let a = i.arch.archetype;
    let c = |f: &str| i.arch.comp_of.get(f).copied().unwrap_or("");
    let mut out = Vec::new();

    if !i.undefined_steps.is_empty() {
        let files: BTreeSet<String> = i.undefined_steps.iter().map(|(f, _, _)| f.clone()).collect();
        let sample: Vec<String> = i.undefined_steps.iter().take(6).map(|(f, l, t)| format!("{}:{l}  {t}", f.rsplit('/').next().unwrap_or(f))).collect();
        out.push(check("error", "undefined-steps", format!("{} no step definition", count(i.undefined_steps.len(), "scenario step has", "scenario steps have")),
            format!("These lines will fail with \"step not found\" when the test runs.\n{}", sample.join("\n")),
            "Add a step definition whose pattern matches each line, or fix the wording so it matches an existing one.", files.into_iter().collect()));
    }

    // Step definitions that find elements themselves skip the page-object layer.
    let has_pages = i.arch.comp_of.values().any(|x| *x == "pages");
    let leaky: Vec<String> = i.infos.iter().filter(|(p, f)| c(p) == "steps" && f.signals.contains(&"locator")).map(|(p, _)| p.clone()).collect();
    if !leaky.is_empty() && has_pages {
        out.push(check("warn", "steps-skip-pages", format!("{} screen elements themselves", count(leaky.len(), "step definition file finds", "step definition files find")),
            "They use locators (By.Id, XPath, getByRole…) directly instead of asking a page object. When a screen changes you will have to fix it in several places.".into(),
            "Move the locator and the click/type into the page object for that screen and call it from the step.", leaky));
    }
    let asserting: Vec<String> = i.infos.iter().filter(|(p, f)| c(p) == "pages" && f.signals.contains(&"assert")).map(|(p, _)| p.clone()).collect();
    if !asserting.is_empty() {
        out.push(check("info", "pages-assert", format!("{} results themselves", count(asserting.len(), "page object checks", "page objects check")),
            "Page objects usually only describe and operate a screen; deciding pass or fail belongs in steps or tests, so failures point at the right place.".into(),
            "Return the value from the page object and assert in the step definition.", asserting));
    }

    // Data flowing backwards: a later part of the path depends on an earlier one.
    let mut upstream: BTreeMap<(&str, &str), Vec<String>> = BTreeMap::new();
    for (f, deps) in i.imports_of {
        let cf = c(f);
        let Some(rf) = rank(a, cf) else { continue };
        for d in deps {
            let cd = c(d);
            let Some(rd) = rank(a, cd) else { continue };
            if rf > rd + 1 || (rf > rd && !matches!((cf, cd), ("models", "repos") | ("components", "screens") | ("state", "components"))) {
                if cf == "db" || cd == "entry" { continue; }
                let e = upstream.entry((cf, cd)).or_default();
                if e.len() < 8 { e.push(format!("{f} → {d}")); }
            }
        }
    }
    for ((from, to), pairs) in upstream {
        out.push(check("warn", "upstream", format!("{} depends on {}", comp(from).name, comp(to).name),
            format!("Data normally moves {} → {} in this kind of project. Depending backwards makes the two hard to change separately.\n{}", comp(to).name, comp(from).name, pairs.join("\n")),
            "Pass what is needed in as a parameter, or move the shared code into a helper both can use.",
            pairs.iter().map(|p| p.split(" → ").next().unwrap_or("").to_string()).collect()));
    }

    if a.id != "test_automation" {
        let has_lower = i.arch.comp_of.values().any(|x| matches!(*x, "repos" | "services"));
        let direct: Vec<String> = i.infos.iter().filter(|(p, f)| c(p) == "controllers" && f.signals.contains(&"db")).map(|(p, _)| p.clone()).collect();
        if !direct.is_empty() && has_lower {
            out.push(check("info", "controller-db", format!("{} the database directly", count(direct.len(), "controller queries", "controllers query")),
                "Other parts of this project go through business logic or data-access classes. Mixing both makes the rules harder to find.".into(),
                "Move the query into the matching service or repository.", direct));
        }
    }

    let code: Vec<&String> = i.infos.iter().filter(|(_, f)| f.kind == "code" && !matches!(f.lang.as_str(), "markdown" | "text" | "json" | "yaml" | "toml" | "config" | "xml" | "csv" | "gherkin")).map(|(p, _)| p).collect();
    for cyc in cycles(&code, i.imports_of).into_iter().take(8) {
        out.push(check("warn", "cycle", format!("{} files depend on each other in a loop", cyc.len()),
            format!("{}\nA change in any of them can ripple through all of them, and some languages fail to load loops.", cyc.join("\n")),
            "Find the one link that points the wrong way and move what it needs into a separate file.", cyc));
    }

    if !i.unused_steps.is_empty() {
        out.push(check("info", "unused-steps", format!("{} never used by a scenario", count(i.unused_steps.len(), "step definition is", "step definitions are")),
            i.unused_steps.iter().take(8).map(|(f, p)| format!("{}: {p}", f.rsplit('/').next().unwrap_or(f))).collect::<Vec<_>>().join("\n"),
            "Delete them, or check whether a scenario's wording drifted.", i.unused_steps.iter().map(|(f, _)| f.clone()).collect::<BTreeSet<_>>().into_iter().collect()));
    }

    let skip = ["docs", "config", "infra", "assets", "entry", "routes", "scenarios", "testcases", "hooks", "steps", "db", "notebooks", "screens", "styles", "views", "testconfig", "testdata", "reports", "cli", "jobs", "middleware", "ml_train", "ml_eval"];
    let weak_langs = ["php", "blade", "python", "html", "css", "scss", "shell", "c", "cpp", "ruby"];
    let orphans: Vec<String> = code.iter().filter(|p| {
        let f = &i.infos[p.as_str()];
        !skip.contains(&c(p)) && !weak_langs.contains(&f.lang.as_str()) && !f.generated && f.layer != "tests"
            && i.imported_by.get(p.as_str()).map(|v| v.is_empty()).unwrap_or(true)
            && !i.tests_of.values().any(|v| v.contains(p)) && !i.reached.contains(p.as_str())
    }).map(|p| (*p).clone()).take(40).collect();
    if orphans.len() >= 1 {
        out.push(check("info", "orphans", format!("{} not used by anything CodeBase can see", count(orphans.len(), "file is", "files are")),
            "No other file imports them and no route, screen or test reaches them. They may be unused, or loaded by the framework by name.".into(),
            "Check each one: delete it if it is dead, or note what loads it so the next person knows.", orphans));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::analyze_text;

    fn c(path: &str, text: &str, ctx: Ctx) -> &'static str { classify(path, &analyze_text(path, text, text.len() as u64), &ctx) }

    #[test]
    fn test_framework_parts_are_found_by_name_and_behaviour() {
        let t = Ctx { test_project: true, ml_project: false };
        assert_eq!(c("Features/Login.feature", "Feature: Login\nScenario: ok\nGiven x", t), "scenarios");
        assert_eq!(c("StepDefinitions/LoginSteps.cs", "[Given(@\"I log in\")] public void A(){}", t), "steps");
        assert_eq!(c("Hooks/Hooks.cs", "[BeforeScenario] public void Start(){}", t), "hooks");
        assert_eq!(c("Pages/LoginPage.cs", "class LoginPage { By user = By.Id(\"u\"); }", t), "pages");
        assert_eq!(c("Drivers/DriverFactory.cs", "var d = new ChromeDriver();", t), "driver");
        assert_eq!(c("TestData/users.json", "{}", t), "testdata");
        assert_eq!(c("appsettings.json", "{}", t), "testconfig");
        // Behaviour wins when names say nothing.
        assert_eq!(c("Framework/Thing.cs", "class Thing { void Go(){ var d = new ChromeDriver(); } }", t), "driver");
        assert_eq!(c("Framework/Login.cs", "class Login { void Go(){ driver.FindElement(By.Id(\"x\")).Click(); } }", t), "pages");
    }

    #[test]
    fn app_parts_are_found() {
        let a = Ctx::default();
        assert_eq!(c("app/Http/Controllers/BillController.php", "<?php class BillController {}", a), "controllers");
        assert_eq!(c("app/Models/Bill.php", "<?php class Bill {}", a), "models");
        assert_eq!(c("routes/web.php", "<?php Route::get('/bills', [BillController::class, 'index']);", a), "routes");
        assert_eq!(c("src/screens/RideRequestScreen.tsx", "export default function RideRequestScreen(){}", a), "screens");
        assert_eq!(c("src/api/rides.ts", "export const book = () => axios.post('/api/rides', {})", a), "apiclient");
        assert_eq!(c("server/services/MatchingService.js", "module.exports = {}", a), "services");
        assert_eq!(c("database/migrations/2024_create_bills.php", "<?php", a), "db");
    }

    #[test]
    fn cycles_are_found() {
        let files: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let refs: Vec<&String> = files.iter().collect();
        let mut imp = HashMap::new();
        imp.insert("a".to_string(), vec!["b".to_string()]);
        imp.insert("b".to_string(), vec!["c".to_string()]);
        imp.insert("c".to_string(), vec!["a".to_string()]);
        imp.insert("d".to_string(), vec!["a".to_string()]);
        let cy = cycles(&refs, &imp);
        assert_eq!(cy, vec![vec!["a".to_string(), "b".to_string(), "c".to_string()]]);
    }
}
