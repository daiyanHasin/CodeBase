//! Per-file analysis with light regex parsers. No AI and no network.
//! For each file: language, symbols, raw imports, role, layer (plain-language
//! category), domain words (for feature detection) and a one-line summary.

use regex::Regex;
use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::LazyLock;

#[derive(Clone, Serialize)]
pub struct Symbol {
    pub kind: &'static str,
    pub name: String,
    pub sig: String,
    pub exported: bool,
}

/// A web address the server answers: `POST /api/rides` handled by `RideController.store`.
#[derive(Clone, Serialize)]
pub struct RouteDef {
    pub method: String,
    pub path: String,
    /// Class or module that handles it, when the route names one (`RideController`).
    pub handler: String,
    /// Function or method that handles it (`store`, `create_ride`).
    pub action: String,
    pub line: usize,
}

/// A request the code sends: `axios.post('/api/rides')`.
#[derive(Clone, Serialize)]
pub struct HttpCall {
    pub method: String,
    pub url: String,
    pub line: usize,
}

#[derive(Clone, Serialize)]
pub struct ScenarioStep {
    pub kw: String,
    pub text: String,
    pub line: usize,
}

/// One test case: a Gherkin scenario, a Maestro flow, or a named test function.
#[derive(Clone, Serialize)]
pub struct Scenario {
    pub name: String,
    /// Feature, describe block or test class it belongs to.
    pub group: String,
    pub line: usize,
    pub steps: Vec<ScenarioStep>,
}

#[derive(Clone)]
pub struct FileInfo {
    pub kind: &'static str, // "code" | "asset" | "large"
    pub lang: String,
    pub lines: usize,
    pub bytes: u64,
    pub hash: String,
    pub imports: Vec<String>,
    pub symbols: Vec<Symbol>,
    pub summary: String,
    pub role: Option<&'static str>,
    pub layer: &'static str,
    pub generated: bool,
    pub words: Vec<String>,
    /// Test steps: for .feature files the step lines, for code files the step-definition patterns.
    pub steps: Vec<String>,
    /// Capitalised identifiers this file mentions (C#, Java, Kotlin, Go, Swift, Dart), used to link
    /// files that use a type declared elsewhere, because those languages import whole namespaces.
    pub refs: Vec<String>,
    /// What the code does, found from its content: "driver", "locator", "assert", "hook", "db",
    /// "external", "cli", "maestro", "ml_data", "ml_model", "ml_train", "ml_eval".
    pub signals: Vec<&'static str>,
    pub routes: Vec<RouteDef>,
    pub calls: Vec<HttpCall>,
    pub scenarios: Vec<Scenario>,
    /// Routers mounted under a prefix: `app.use('/api/rides', rides)` -> ("/api/rides", "./routes/rides").
    pub mounts: Vec<(String, String)>,
    /// Screens a front-end router shows: (address, import path or "", component name).
    pub pages: Vec<(String, String, String)>,
    /// The address a router shows this screen at, when one does ("/article/:id").
    pub screen_url: String,
}

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).unwrap());
    };
}

pub fn detect_language(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    if name.ends_with(".blade.php") { return Some("blade"); }
    if name == "dockerfile" { return Some("docker"); }
    if name == "makefile" { return Some("make"); }
    if name.ends_with(".feature") { return Some("gherkin"); }
    if name.starts_with(".env") || name == ".gitignore" || name == ".editorconfig" { return Some("config"); }
    let ext = if name.contains('.') { name.rsplit('.').next().unwrap_or("") } else { "" };
    Some(match ext {
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "vue" => "vue",
        "svelte" => "svelte",
        "py" => "python",
        "php" => "php",
        "rb" => "ruby",
        "go" => "go",
        "rs" => "rust",
        "java" => "java",
        "kt" => "kotlin",
        "cs" => "csharp",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" => "cpp",
        "swift" => "swift",
        "dart" => "dart",
        "css" | "less" => "css",
        "scss" | "sass" => "scss",
        "html" | "htm" => "html",
        "md" | "mdx" => "markdown",
        "txt" | "rst" => "text",
        "json" => "json",
        "yml" | "yaml" => "yaml",
        "toml" => "toml",
        "ini" | "env" => "config",
        "xml" | "csproj" | "fsproj" | "vbproj" | "props" | "targets" | "xaml" | "razor" | "cshtml" => "xml",
        "sql" => "sql",
        "sh" | "bash" | "ps1" | "bat" | "cmd" => "shell",
        _ => return None,
    })
}

pub fn is_asset_path(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    matches!(ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg" | "avif" | "tif" | "tiff" | "psd"
        | "mp4" | "webm" | "mov" | "avi" | "mp3" | "wav" | "ogg" | "flac"
        | "woff" | "woff2" | "ttf" | "otf" | "eot"
        | "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
        | "zip" | "gz" | "tar" | "rar" | "7z" | "jar" | "apk" | "ipa" | "dmg" | "iso"
        | "db" | "sqlite" | "sqlite3" | "bin" | "dat" | "wasm" | "exe" | "dll")
}

pub fn hash_text(text: &[u8]) -> String {
    let mut h = sha1_smol::Sha1::new();
    h.update(text);
    h.digest().to_string()[..12].to_string()
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() > n { format!("{}…", s.chars().take(n - 1).collect::<String>()) } else { s.to_string() }
}
fn one_line(s: &str) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ") }

// ---------- JavaScript / TypeScript ----------
re!(JS_IMPORT, r#"\bimport\s+(?:[\w*{}\s,$]+\s+from\s+)?['"]([^'"]+)['"]"#);
re!(JS_EXPORT_FROM, r#"\bexport\s+(?:\*|\{[^}]*\})\s+from\s+['"]([^'"]+)['"]"#);
re!(JS_REQUIRE, r#"\brequire\(\s*['"]([^'"]+)['"]\s*\)"#);
re!(JS_DYN, r#"\bimport\(\s*['"]([^'"]+)['"]\s*\)"#);
re!(JS_FN, r#"(?m)^\s*(export\s+(?:default\s+)?)?(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)\s*\(([^)]*)\)"#);
re!(JS_CLASS, r#"(?m)^\s*(export\s+(?:default\s+)?)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)"#);
re!(JS_ARROW, r#"(?m)^\s*(export\s+)?const\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?(?:\(([^)]*)\)|[A-Za-z_$][\w$]*)\s*=>"#);
re!(JS_TYPE, r#"(?m)^\s*export\s+(?:interface|type)\s+([A-Za-z_$][\w$]*)"#);
// ---------- Python ----------
re!(PY_FROM, r#"(?m)^\s*from\s+([.\w]+)\s+import[ \t]+\(?([\w \t,]*)"#);
re!(PY_IMPORT, r#"(?m)^\s*import[ \t]+([\w.]+(?:[ \t]*,[ \t]*[\w.]+)*)"#);
re!(PY_FROM_DOTS, r#"(?m)^\s*from\s+(\.+)\s+import[ \t]+\(?([\w \t,]+)"#);
re!(PY_DEF, r#"(?m)^def\s+(\w+)\s*\(([^)]*)\)"#);
re!(PY_CLASS, r#"(?m)^class\s+(\w+)\s*(\([^)]*\))?"#);
// ---------- PHP / Blade ----------
re!(PHP_USE, r#"(?m)^use\s+([\w\\]+)(?:\s+as\s+\w+)?\s*;"#);
re!(PHP_REQ, r#"\b(?:require|include)(?:_once)?\s*\(?\s*(?:__DIR__\s*\.\s*)?['"]([^'"]+\.php)['"]"#);
re!(PHP_VIEW, r#"\b(?:view|View::make)\(\s*['"]([\w.\-/:]+)['"]"#);
re!(PHP_CLASS, r#"(?m)^\s*(?:final\s+|abstract\s+|readonly\s+)*(class|interface|trait|enum)\s+(\w+)([^{\n]*)"#);
re!(PHP_FN, r#"(?m)^\s*(?:(public|protected|private)\s+)?(?:static\s+)?function\s+(\w+)\s*\(([^)]*)\)"#);
re!(BLADE_INC, r#"@(?:extends|include|includeIf|includeWhen|component|each)\(\s*['"]([\w.\-/:]+)['"]"#);
re!(BLADE_X, r#"<x-([\w.\-]+)"#);
re!(BLADE_VITE, r#"@vite\(\s*\[?\s*['"]([^'"]+)['"]"#);
re!(BLADE_SECTION, r#"@section\(\s*['"]([\w\-]+)['"]"#);
// ---------- CSS / HTML / Markdown / other ----------
re!(CSS_IMPORT, r#"@(?:import|use|forward)\s+(?:url\()?['"]([^'"]+)['"]"#);
re!(HTML_SCRIPT, r#"<script[^>]+src=["']([^"']+)["']"#);
re!(HTML_LINK, r#"<link[^>]+href=["']([^"']+\.css)["']"#);
re!(MD_HEAD, r#"(?m)^(#{1,3})\s+(.+)$"#);
re!(GEN_FN, r#"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:func|fn|def|sub)\s+(?:\([^)]*\)\s*)?(\w+)"#);
re!(GEN_CLASS, r#"(?m)^\s*(?:pub\s+|public\s+|private\s+|internal\s+)?(?:abstract\s+|final\s+|static\s+)*(?:class|struct|interface|enum|trait)\s+(\w+)"#);
re!(RUST_USE, r#"(?m)^\s*(?:pub\s+)?mod\s+(\w+)\s*;"#);
re!(RUST_CRATE, r#"(?m)^\s*(?:pub\s+)?use\s+crate::(\w+)"#);
// ---------- C# / Java / Kotlin ----------
re!(JAVA_IMPORT, r#"(?m)^\s*import\s+(?:static\s+)?([\w.]+)\s*;?\s*$"#);
re!(CS_USING, r#"(?m)^\s*(?:global\s+)?using\s+(?:static\s+)?([\w.]+)\s*;"#);
re!(CLIKE_METHOD, r#"(?m)^[ \t]*(?:\[[^\]\n]*\][ \t]*)*(?:(public|private|protected|internal)[ \t]+)?(?:(?:static|virtual|override|async|abstract|sealed|final|synchronized|partial|extern|new|open|suspend)[ \t]+)*([\w<>\[\],.?]+(?:<[^>\n]*>)?)[ \t]+(\w+)[ \t]*(?:<[^>\n]*>)?\(([^)\n]*)\)"#);
re!(TYPE_REF, r#"\b[A-Z][A-Za-z0-9_]{2,}\b"#);
// ---------- test steps (SpecFlow / Reqnroll / Cucumber / behave) ----------
re!(CS_STEP, r#"\[\s*(?:Given|When|Then|And|But|StepDefinition)\s*\(\s*@?"((?:[^"]|"")*)""#);
re!(JS_STEP, r#"\b(?:Given|When|Then|defineStep)\s*\(\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)"|/((?:[^/\\\n]|\\.)+)/)"#);
re!(PY_STEP, r#"@(?:given|when|then|step)\(\s*(?:u|r)?['"](.+?)['"]"#);
re!(GHERKIN_STEP, r#"(?m)^\s*(?:Given|When|Then|And|But|\*)\s+(.+?)\s*$"#);
// ---------- doc comments ----------
re!(DOC_PY, r#"^\s*(?:#![^\n]*\n)?(?:#[^\n]*\n)*\s*(?:"""|''')([\s\S]*?)(?:"""|''')"#);
re!(DOC_BLOCK, r#"/\*\*?([\s\S]*?)\*/"#);
re!(DOC_LINES, r#"^(?:\s*(?:<\?php)?\s*\n)*((?:[ \t]*(?://[/!]?|#[^!\n])[^\n]*\n){1,4})"#);
re!(DOC_TRIPLE, r#"(?m)((?:^[ \t]*///[^\n]*\n)+)"#);
re!(XML_TAG, r#"</?[a-zA-Z]+[^>]*>"#);
re!(DOC_PREFIX, r#"(?m)^[ \t]*(?://[/!]?|#)\s?"#);
re!(GENERATED, r#"(?i)(@generated\b|\bauto-?generated\b|\bcode generated by\b|\bthis file (?:is|was|has been) (?:auto(?:matically)?[- ])?generated\b|\bautomatically generated\b)"#);
re!(DO_NOT_EDIT, r#"\bDO NOT EDIT\b"#);
re!(CAMEL, r#"([a-z0-9])([A-Z])"#);

fn caps(re: &Regex, text: &str, group: usize) -> Vec<String> {
    re.captures_iter(text).filter_map(|c| c.get(group).map(|m| m.as_str().to_string())).collect()
}

fn sym(kind: &'static str, name: &str, sig: String, exported: bool) -> Symbol {
    Symbol { kind, name: name.to_string(), sig: clip(&one_line(&sig), 140), exported }
}

/// Words that look like a method declaration to a regex but are statements.
const NOT_A_TYPE: &[&str] = &["return", "new", "await", "throw", "else", "case", "yield", "if", "for", "foreach", "while", "switch", "catch", "using", "lock", "fixed", "when", "is", "as", "in", "out", "ref", "goto", "do", "try", "typeof", "sizeof", "nameof", "default", "var", "let", "val"];

/// C#, Java and Kotlin methods: "public decimal Total(Bill b)".
fn clike_methods(text: &str, symbols: &mut Vec<Symbol>) {
    for c in CLIKE_METHOD.captures_iter(text) {
        let ty = &c[2];
        let name = &c[3];
        if NOT_A_TYPE.contains(&ty) || NOT_A_TYPE.contains(&name) { continue; }
        let vis = c.get(1).map(|m| m.as_str()).unwrap_or("");
        let sig = format!("{}{} {}({})", if vis.is_empty() { String::new() } else { format!("{vis} ") }, ty, name, &c[4]);
        symbols.push(sym("function", name, sig, vis == "public" || vis == "internal"));
    }
}

fn type_refs(text: &str) -> Vec<String> {
    let mut set = BTreeSet::new();
    for m in TYPE_REF.find_iter(text) {
        set.insert(m.as_str().to_string());
        if set.len() >= 400 { break; }
    }
    set.into_iter().collect()
}

/// Returns (imports, symbols, steps, refs).
fn parse(lang: &str, text: &str) -> (Vec<String>, Vec<Symbol>, Vec<String>, Vec<String>) {
    let mut imports = Vec::new();
    let mut symbols = Vec::new();
    let mut steps: Vec<String> = Vec::new();
    let mut refs: Vec<String> = Vec::new();
    match lang {
        "javascript" | "typescript" | "vue" | "svelte" => {
            for re in [&*JS_IMPORT, &*JS_EXPORT_FROM, &*JS_REQUIRE, &*JS_DYN] { imports.extend(caps(re, text, 1)); }
            for c in JS_FN.captures_iter(text) {
                symbols.push(sym("function", &c[2], format!("function {}({})", &c[2], &c[3]), c.get(1).is_some()));
            }
            for c in JS_CLASS.captures_iter(text) { symbols.push(sym("class", &c[2], format!("class {}", &c[2]), c.get(1).is_some())); }
            for c in JS_ARROW.captures_iter(text) {
                let args = c.get(3).map(|m| m.as_str()).unwrap_or("");
                symbols.push(sym("function", &c[2], format!("const {} = ({}) =>", &c[2], args), c.get(1).is_some()));
            }
            for c in JS_TYPE.captures_iter(text) { symbols.push(sym("type", &c[1], format!("type {}", &c[1]), true)); }
            for c in JS_STEP.captures_iter(text) {
                if let Some(m) = c.get(1).or_else(|| c.get(2)) { steps.push(m.as_str().to_string()); }
                else if let Some(m) = c.get(3) { steps.push(format!("re:{}", m.as_str())); }
            }
        }
        "python" => {
            for c in PY_FROM.captures_iter(text) {
                let module = &c[1];
                imports.push(module.to_string());
                // "from app import billing" may import the module app/billing.py
                if !module.chars().all(|ch| ch == '.') {
                    for name in c[2].split(',').map(|x| x.trim()).filter(|x| !x.is_empty() && x.chars().all(|ch| ch.is_alphanumeric() || ch == '_')).take(20) {
                        imports.push(format!("{module}.{name}"));
                    }
                }
            }
            for c in PY_IMPORT.captures_iter(text) { imports.extend(c[1].split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty())); }
            for c in PY_FROM_DOTS.captures_iter(text) {
                for name in c[2].split(',').map(|x| x.trim()).filter(|x| !x.is_empty() && x.chars().all(|ch| ch.is_alphanumeric() || ch == '_')) {
                    imports.push(format!("{}{}", &c[1], name));
                }
            }
            for c in PY_STEP.captures_iter(text) { steps.push(c[1].to_string()); }
            for c in PY_DEF.captures_iter(text) { symbols.push(sym("function", &c[1], format!("def {}({})", &c[1], &c[2]), !c[1].starts_with('_'))); }
            for c in PY_CLASS.captures_iter(text) {
                symbols.push(sym("class", &c[1], format!("class {}{}", &c[1], c.get(2).map(|m| m.as_str()).unwrap_or("")), true));
            }
        }
        "php" => {
            imports.extend(caps(&PHP_USE, text, 1).into_iter().map(|s| format!("php:{s}")));
            imports.extend(caps(&PHP_REQ, text, 1));
            imports.extend(caps(&PHP_VIEW, text, 1).into_iter().map(|s| format!("view:{s}")));
            for c in PHP_CLASS.captures_iter(text) { symbols.push(sym("class", &c[2], format!("{} {}{}", &c[1], &c[2], &c[3]), true)); }
            for c in PHP_FN.captures_iter(text) {
                let vis = c.get(1).map(|m| m.as_str()).unwrap_or("");
                let sig = if vis.is_empty() { format!("function {}({})", &c[2], &c[3]) } else { format!("{vis} function {}({})", &c[2], &c[3]) };
                symbols.push(sym("function", &c[2], sig, vis != "private"));
            }
        }
        "blade" => {
            imports.extend(caps(&BLADE_INC, text, 1).into_iter().map(|s| format!("view:{s}")));
            imports.extend(caps(&BLADE_X, text, 1).into_iter().map(|s| format!("component:{s}")));
            imports.extend(caps(&BLADE_VITE, text, 1).into_iter().map(|s| format!("/{s}")));
            imports.extend(caps(&PHP_VIEW, text, 1).into_iter().map(|s| format!("view:{s}")));
            for c in BLADE_SECTION.captures_iter(text) { symbols.push(sym("section", &c[1], format!("@section('{}')", &c[1]), true)); }
        }
        "css" | "scss" => imports.extend(caps(&CSS_IMPORT, text, 1)),
        "html" => {
            imports.extend(caps(&HTML_SCRIPT, text, 1));
            imports.extend(caps(&HTML_LINK, text, 1));
            imports.retain(|s| !s.starts_with("http:") && !s.starts_with("https:") && !s.starts_with("//"));
        }
        "markdown" => {
            for c in MD_HEAD.captures_iter(text).take(20) { symbols.push(sym("heading", &clip(&c[2], 80), format!("{} {}", &c[1], &c[2]), true)); }
        }
        "rust" => {
            imports.extend(caps(&RUST_USE, text, 1).into_iter().map(|s| format!("mod:{s}")));
            imports.extend(caps(&RUST_CRATE, text, 1).into_iter().map(|s| format!("crate:{s}")));
            for c in GEN_FN.captures_iter(text) { symbols.push(sym("function", &c[1], c[0].trim().to_string(), c[0].contains("pub"))); }
            for c in GEN_CLASS.captures_iter(text) { symbols.push(sym("class", &c[1], c[0].trim().to_string(), true)); }
        }
        "csharp" | "java" | "kotlin" => {
            for c in GEN_CLASS.captures_iter(text) { symbols.push(sym("class", &c[1], c[0].trim().to_string(), true)); }
            if lang == "kotlin" { for c in GEN_FN.captures_iter(text) { symbols.push(sym("function", &c[1], c[0].trim().to_string(), true)); } }
            clike_methods(text, &mut symbols);
            if lang == "csharp" {
                imports.extend(caps(&CS_USING, text, 1).into_iter().map(|s| format!("ns:{s}")));
                for c in CS_STEP.captures_iter(text) { steps.push(c[1].replace("\"\"", "\"")); }
            } else {
                imports.extend(caps(&JAVA_IMPORT, text, 1).into_iter().map(|s| format!("java:{s}")));
            }
            refs = type_refs(text);
        }
        "go" | "c" | "cpp" | "swift" | "dart" | "ruby" => {
            for c in GEN_FN.captures_iter(text) { symbols.push(sym("function", &c[1], c[0].trim().to_string(), true)); }
            for c in GEN_CLASS.captures_iter(text) { symbols.push(sym("class", &c[1], c[0].trim().to_string(), true)); }
            if matches!(lang, "go" | "swift" | "dart") { refs = type_refs(text); }
        }
        "gherkin" => {
            for c in GHERKIN_STEP.captures_iter(text).take(400) { steps.push(c[1].to_string()); }
            for line in text.lines() {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("Feature:").or_else(|| t.strip_prefix("Scenario:")).or_else(|| t.strip_prefix("Scenario Outline:")) {
                    symbols.push(sym("heading", rest.trim(), t.to_string(), true));
                }
            }
        }
        _ => {}
    }
    let mut seen = BTreeSet::new();
    imports.retain(|s| seen.insert(s.clone()));
    // Classes first so a file's outline starts with what it is.
    symbols.sort_by_key(|s| match s.kind { "class" => 0, "type" => 1, _ => 2 });
    symbols.truncate(80);
    steps.truncate(400);
    (imports, symbols, steps, refs)
}

fn first_doc(text: &str, lang: &str) -> Option<String> {
    let head: String = text.chars().take(3000).collect();
    if lang == "python" {
        if let Some(c) = DOC_PY.captures(&head) { return Some(c[1].to_string()); }
    }
    if let Some(c) = DOC_BLOCK.captures(&head) {
        let body: Vec<String> = c[1].lines()
            .map(|l| l.trim().trim_start_matches('*').trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('@'))
            .collect();
        if !body.is_empty() { return Some(body.join(" ")); }
    }
    if let Some(c) = DOC_LINES.captures(&head) { return Some(XML_TAG.replace_all(&DOC_PREFIX.replace_all(&c[1], " "), " ").to_string()); }
    // C#, Rust and Swift put "///" doc comments on the first type, not the file.
    if let Some(c) = DOC_TRIPLE.captures(&head) {
        let t = XML_TAG.replace_all(&DOC_PREFIX.replace_all(&c[1], " "), " ").to_string();
        if t.split_whitespace().count() >= 3 { return Some(t); }
    }
    None
}

static ROLES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"(^|/)[Tt]ests?/|\.(test|spec)\.\w+$|_test\.\w+$|(^|/)test_\w+\.py$|[A-Za-z0-9_]Tests?\.(php|cs|java|kt|swift|dart)$|[A-Za-z0-9_]Spec\.(cs|java|kt|scala)$|(^|/)__tests__/|\.(Tests?|Specs?|IntegrationTests|UnitTests)/|(^|/)(StepDefinitions|Steps)/|[A-Za-z0-9_]Steps\.(cs|java|kt|ts|js|py)$|\.feature$", "Test"),
        (r"(^|/)database/migrations/|(^|/)migrations/", "Database migration"),
        (r"(^|/)database/seeders/|(^|/)seeds?/", "Database seeder"),
        (r"(^|/)database/factories/", "Model factory"),
        (r"(^|/)routes/|(^|/)router\.\w+$|(^|/)urls\.py$", "Route definitions"),
        (r"(^|/)Http/Controllers/|(^|/)controllers?/|Controller\.\w+$|(^|/)views\.py$", "Controller"),
        (r"(^|/)(Http/)?Middleware/|(^|/)middlewares?/", "Middleware"),
        (r"(^|/)Http/Requests/", "Form request validation"),
        (r"(^|/)Models?/|(^|/)models?\.py$|(^|/)entities/|(^|/)schemas?/|\.prisma$", "Model"),
        (r"(^|/)Policies/", "Authorization policy"),
        (r"(^|/)Providers/", "Service provider"),
        (r"(^|/)(Jobs|Listeners|Events|Mail|Notifications|jobs|workers?)/", "Background job or event"),
        (r"(^|/)resources/views/components/", "View component"),
        (r"(^|/)(resources/views/)?layouts?/", "Page layout"),
        (r"\.blade\.php$|(^|/)templates/.*\.html$", "View template"),
        (r"(^|/)components?/", "UI component"),
        (r"(^|/)(pages|screens|views)/.*\.(jsx|tsx|vue|svelte|js|ts)$|(^|/)app/.*page\.(jsx|tsx)$", "Page"),
        (r"(^|/)hooks?/|(^|/)use[A-Z]\w+\.(js|ts|jsx|tsx)$", "Hook"),
        (r"(^|/)(store|stores|redux|state|context)/", "State management"),
        (r"(^|/)(api|services?|clients?)/", "Service or API client"),
        (r"(^|/)(utils?|helpers?|lib|common|shared)/", "Helper"),
        (r"(^|/)config/|(^|/)settings\.py$|\.config\.\w+$|rc\.\w+$|(^|/)\.env", "Configuration"),
        (r"(^|/)(README|CHANGELOG|CONTRIBUTING|LICENSE)[^/]*$", "Project documentation"),
        (r"(^|/)(package\.json|composer\.json|requirements\.txt|pyproject\.toml|Cargo\.toml|go\.mod|Gemfile|pom\.xml|build\.gradle)$", "Dependency manifest"),
        (r"(^|/)(vite|webpack|rollup|tailwind|postcss|babel|eslint|prettier|tsconfig|jest|vitest|Dockerfile|docker-compose|Makefile)[^/]*$|(^|/)\.github/", "Build or tool config"),
        (r"(^|/)(main|index|app|server|cli|manage|artisan)\.\w+$|(^|/)artisan$", "Entry point"),
    ]
    .into_iter()
    .map(|(p, r)| (Regex::new(p).unwrap(), r))
    .collect()
});

pub fn role_for(path: &str) -> Option<&'static str> {
    ROLES.iter().find(|(re, _)| re.is_match(path)).map(|(_, r)| *r)
}

/// Plain-language bucket a non-programmer can follow.
pub fn layer_for(path: &str, lang: &str, role: Option<&str>, asset: bool) -> &'static str {
    if asset { return "media"; }
    let p = path.to_lowercase();
    match role {
        Some("Test") => return "tests",
        Some("Database migration" | "Database seeder" | "Model factory" | "Model") => return "data",
        Some("Route definitions" | "Controller" | "Middleware" | "Form request validation" | "Authorization policy") => return "server",
        Some("View component" | "Page layout" | "View template" | "UI component" | "Page") => return "ui",
        Some("Configuration" | "Dependency manifest" | "Build or tool config" | "Service provider") => return "setup",
        Some("Project documentation") => return "docs",
        _ => {}
    }
    if lang == "sql" || p.contains("/db/") || p.contains("database") || p.contains("repositor") { return "data"; }
    if matches!(lang, "html" | "css" | "scss" | "blade" | "vue" | "svelte") || p.ends_with(".jsx") || p.ends_with(".tsx") { return "ui"; }
    if p.contains("server") || p.contains("/api/") || p.contains("handler") || p.contains("endpoint") || p.contains("/routes") { return "server"; }
    if matches!(lang, "json" | "yaml" | "toml" | "config" | "xml" | "docker" | "make") { return "setup"; }
    if matches!(lang, "markdown" | "text") { return "docs"; }
    if lang == "shell" || p.starts_with("scripts/") || p.contains("/scripts/") || p.starts_with("bin/") { return "scripts"; }
    "logic"
}

/// Generic words that never name a feature.
static STOP: LazyLock<BTreeSet<&'static str>> = LazyLock::new(|| {
    "app apps src lib libs index main core common shared util utils helper helpers test tests spec specs mock mocks fixture fixtures \
     component components page pages view views screen screens layout layouts template templates partial partials style styles css scss \
     controller controllers model models service services provider providers middleware request requests response route routes router \
     api apis http https web public private static asset assets image images img font fonts icon icons resource resources database db \
     migration migrations seed seeder seeders factory factories schema schemas config configs setting settings env dist build out bin \
     script scripts tool tools type types interface interfaces enum enums const constant constants default base abstract class function \
     create update delete store show edit list item items table tables data value values new old get set add remove handle handler handlers \
     hook hooks use context state redux slice action actions reducer reducers saga store stores event events listener listeners job jobs \
     the and for with from into this that json yaml yml toml php js ts jsx tsx vue py rs go java blade html md txt readme license changelog \
     docs doc guide example examples demo sample samples vendor node module modules init main run start setup \
     wrapper container provider logger log logs error errors exception exceptions client server worker workers task tasks util kernel \
     console command commands policy policies rule rules trait traits concern concerns contract contracts facade facades support \
     unit feature integration e2e cypress jest vitest storybook story stories gen generated lock tmp temp cache \
     prisma express react vue angular svelte next nuxt laravel django flask fastapi axios redux graphql supabase firebase tailwind \
     vite webpack eslint typescript javascript node npm yarn docker bootstrap jquery lodash moment"
        .split_whitespace()
        .collect()
});

pub fn singular(w: &str) -> String {
    if w.len() > 4 && w.ends_with("ies") { return format!("{}y", &w[..w.len() - 3]); }
    if w.len() > 4 && (w.ends_with("sses") || w.ends_with("xes") || w.ends_with("ches")) { return w[..w.len() - 2].to_string(); }
    if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") && !w.ends_with("us") && !w.ends_with("is") { return w[..w.len() - 1].to_string(); }
    w.to_string()
}

pub fn split_words(s: &str) -> Vec<String> {
    let spaced = CAMEL.replace_all(s, "$1 $2");
    spaced
        .split(|c: char| !c.is_ascii_alphabetic())
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| w.len() >= 3)
        .map(|w| singular(&w))
        .filter(|w| !STOP.contains(w.as_str()))
        .collect()
}

fn domain_words(path: &str, symbols: &[Symbol]) -> Vec<String> {
    let mut set = BTreeSet::new();
    let no_ext = path.split('/').map(|seg| seg.split('.').next().unwrap_or(seg)).collect::<Vec<_>>().join(" ");
    for w in split_words(&no_ext) { set.insert(w); }
    for s in symbols.iter().filter(|s| s.exported && s.kind != "heading").take(8) {
        for w in split_words(&s.name) { set.insert(w); }
    }
    set.into_iter().take(16).collect()
}

const LAYER_PHRASE: &[(&str, &str)] = &[
    ("ui", "Part of what people see"),
    ("server", "Handles requests"),
    ("logic", "Logic"),
    ("data", "Describes stored data"),
    ("setup", "Project setup"),
    ("tests", "Automatic check"),
    ("docs", "Written documentation"),
    ("scripts", "Helper script"),
    ("media", "Media file"),
];

pub fn analyze_text(path: &str, text: &str, bytes: u64) -> FileInfo {
    let lang = detect_language(path).unwrap_or("text");
    let (mut imports, symbols, steps, refs) = parse(lang, text);
    let lines = text.lines().count().max(1);
    let signals = signals_for(lang, text);
    let maestro = signals.contains(&"maestro");
    if maestro { imports.extend(maestro_imports(text)); }
    let routes = if matches!(lang, "markdown" | "text" | "gherkin" | "json" | "yaml" | "toml" | "config") { vec![] } else { routes_for(lang, text) };
    let calls = if matches!(lang, "javascript" | "typescript" | "vue" | "svelte" | "python" | "dart" | "csharp" | "kotlin" | "java" | "php" | "html" | "blade") { calls_for(text) } else { vec![] };
    let scenarios = scenarios_for(path, lang, text, maestro);
    let mounts = if matches!(lang, "javascript" | "typescript") { mounts_for(text) } else { vec![] };
    let pages = if matches!(lang, "javascript" | "typescript" | "vue" | "svelte" | "dart") { pages_for(text) } else { vec![] };
    if !matches!(lang, "markdown" | "text" | "json" | "csv") { imports.extend(data_refs(text)); }
    let head: String = text.chars().take(600).collect();
    let no_check = matches!(lang, "config" | "markdown" | "text" | "yaml" | "toml");
    let mut generated = !no_check && (GENERATED.is_match(&head) || DO_NOT_EDIT.is_match(&head));
    if matches!(lang, "javascript" | "typescript" | "css" | "scss" | "json") && text.len() > 2000 && text.lines().take(5).any(|l| l.len() > 1500) {
        generated = true;
    }
    let role = role_for(path);
    let layer = layer_for(path, lang, role, false);
    let named: Vec<&str> = symbols.iter().filter(|s| s.exported && s.kind != "heading" && s.kind != "section").take(4).map(|s| s.name.as_str()).collect();

    let doc = if matches!(lang, "config" | "yaml" | "toml" | "text" | "json") { None } else { first_doc(text, lang) };
    let summary = match doc.map(|d| one_line(&d)).filter(|d| d.len() > 8) {
        Some(d) => clip(&d, 160),
        None if lang == "markdown" && !symbols.is_empty() => format!("Document: {}", symbols[0].name),
        None if lang == "gherkin" && !symbols.is_empty() => {
            let scen: Vec<&str> = symbols.iter().skip(1).take(3).map(|s| s.name.as_str()).collect();
            if scen.is_empty() { format!("Test scenarios: {}", symbols[0].name) } else { format!("Test scenarios for {}: {}", symbols[0].name, scen.join("; ")) }
        }
        None => {
            let lead = role.map(|r| r.to_string())
                .unwrap_or_else(|| LAYER_PHRASE.iter().find(|(l, _)| *l == layer).map(|(_, p)| p.to_string()).unwrap_or_default());
            if named.is_empty() { format!("{lead} ({lang}, {lines} line{})", if lines == 1 { "" } else { "s" }) } else { format!("{lead}: {}", named.join(", ")) }
        }
    };

    FileInfo {
        kind: "code",
        lang: lang.to_string(),
        lines,
        bytes,
        hash: hash_text(text.as_bytes()),
        words: domain_words(path, &symbols),
        imports,
        symbols,
        summary,
        role,
        layer,
        generated,
        steps,
        refs,
        signals,
        routes,
        calls,
        scenarios,
        mounts,
        pages,
        screen_url: String::new(),
    }
}

pub fn non_text(path: &str, bytes: u64, stamp: String, kind: &'static str, summary: String) -> FileInfo {
    let lang = if kind == "asset" { "asset".to_string() } else { detect_language(path).unwrap_or("text").to_string() };
    let role = if kind == "asset" { None } else { role_for(path) };
    let layer = layer_for(path, &lang, role, kind == "asset");
    FileInfo {
        kind, lang, lines: 0, bytes, hash: stamp, imports: vec![], symbols: vec![], summary,
        role, layer, generated: false, words: if kind == "asset" { vec![] } else { domain_words(path, &[]) },
        steps: vec![], refs: vec![], signals: vec![], routes: vec![], calls: vec![], scenarios: vec![], mounts: vec![], pages: vec![], screen_url: String::new(),
    }
}

// =====================================================================
// Behaviour: what the code does, so architecture works without naming conventions
// =====================================================================

fn line_at(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())].iter().filter(|b| **b == b'\n').count() + 1
}

re!(SIG_DRIVER, r#"\bnew\s+(?:Chrome|Firefox|Edge|Safari|RemoteWeb|Appium|Android|IOS|Windows)Driver\b|\bwebdriver\.(?:Chrome|Firefox|Edge|Remote|Safari)\(|\b(?:chromium|firefox|webkit)\.launch\(|Playwright\.CreateAsync|\bplaywright\.(?:chromium|firefox|webkit)\b|\bDesiredCapabilities\b|\bUiAutomator2Options\b|\bXCUITestOptions\b|\bwdio\.conf|\bremote\(\s*\{|\bAppiumOptions\b|\bChromeOptions\b|\bwebdriver\.Remote\b|\bAppiumServiceBuilder\b"#);
re!(SIG_LOCATOR, r#"\bBy\.(?:Id|XPath|CssSelector|Name|ClassName|LinkText|TagName|id|xpath|cssSelector|css|name|className|linkText|ID|XPATH|CSS_SELECTOR|NAME|CLASS_NAME)\b|\bFindElements?\(|\bfind_elements?\(|\bfindElements?\(|\bgetBy(?:Role|Text|TestId|Label|Placeholder)\(|\bpage\.locator\(|\bcy\.get\(|\bMobileBy\.|\bAppiumBy\.|\baccessibilityId\(|@FindBy\b|\[FindsBy\b|\bpage\.(?:click|fill|tap)\("#);
re!(SIG_ASSERT, r#"\bAssert\.\w+\(|\bassert(?:Equals|True|False|That|NotNull)\(|(?m)^\s*assert\s|\bexpect\(|\.Should\(\)|\.should\(|\bAssertions\.|\bassertThat\("#);
re!(SIG_HOOK, r#"\[\s*(?:BeforeScenario|AfterScenario|BeforeTestRun|AfterTestRun|BeforeFeature|AfterFeature|BeforeStep|AfterStep|SetUp|TearDown|OneTimeSetUp|OneTimeTearDown|TestInitialize|TestCleanup|AssemblyInitialize)\b|@(?:Before|After|BeforeAll|AfterAll|BeforeMethod|AfterMethod|BeforeClass|AfterClass|BeforeEach|AfterEach|BeforeSuite|AfterSuite)\b|\b(?:Before|After|BeforeAll|AfterAll)\(\s*(?:async|function|\(|\{)|\bdef\s+(?:before|after)_(?:all|feature|scenario|step|tag)\b|@pytest\.fixture"#);
re!(SIG_DB, r#"\b(?:SELECT\s+[\w*,\s.]+\s+FROM|INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM)\b|\bDB::(?:table|select|insert|update|statement)\(|\bcursor\.execute\(|\bprisma\.\w+\.(?:find|create|update|delete|upsert)|\.objects\.(?:filter|get|all|create|exclude)\(|\bsession\.(?:query|add|commit)\(|\bnew\s+SqlConnection\b|\bDbContext\b|\bknex\(|\bmongoose\.model\(|\.(?:findOne|findMany|findById|findAll)\(|::(?:where|find|findOrFail|create|firstOrCreate)\(|\bsqlx::|\bdiesel::|\bJdbcTemplate\b|\bEntityManager\b"#);
re!(SIG_EXTERNAL, r#"(?i)\b(?:stripe|twilio|sendgrid|paypal|mailgun|razorpay|sslcommerz|bkash|boto3|S3Client|googleapis|firebase-admin|firebase_admin|onesignal|pusher|nodemailer|smtplib|openai|anthropic)\b"#);
re!(SIG_CLI, r#"\bargparse\b|\bclap::|#\[derive\([^)]*Parser|@click\.command|\bcommander\b|\byargs\b|\bsys\.argv\b|\bprocess\.argv\b|\bcobra\.Command\b|\bflag\.Parse\(\)|\bstatic\s+void\s+Main\s*\(\s*string\[\]"#);
re!(SIG_ML_MODEL, r#"\bnn\.Module\b|\bkeras\.(?:Sequential|Model)\b|\btf\.keras\.Model\b|\btorchvision\.models\b|\bsklearn\.(?:ensemble|linear_model|tree|svm|neighbors)\b|\b(?:XGB|LGBM|CatBoost)(?:Classifier|Regressor)\b|\bAutoModel\w*\.from_pretrained\b"#);
re!(SIG_ML_TRAIN, r#"\.backward\(\)|\boptimizer\.step\(\)|\bmodel\.fit\(|\.fit\(\s*X|\bmodel\.train\(\)|\bTrainer\(|\btrain_one_epoch\b|\bfor\s+epoch\s+in\b"#);
re!(SIG_ML_EVAL, r#"\b(?:accuracy_score|f1_score|roc_auc_score|precision_score|recall_score|confusion_matrix|classification_report|mean_squared_error)\b|\bmodel\.eval\(\)|\.evaluate\(|\bpsnr\b|\bssim\b"#);
re!(SIG_ML_DATA, r#"\b(?:Dataset|DataLoader|ImageFolder)\b|\bpd\.read_(?:csv|parquet|excel|json)\(|\bload_dataset\(|\btrain_test_split\(|\btransforms\.Compose\("#);
re!(MAESTRO_HEAD, r#"(?m)^\s*appId\s*:"#);
re!(MAESTRO_CMD, r#"(?m)^\s*-\s*(?:launchApp|tapOn|inputText|assertVisible|assertNotVisible|runFlow|scrollUntilVisible|swipe|back|pressKey|takeScreenshot|waitForAnimationToEnd|extendedWaitUntil|clearState|eraseText|openLink|runScript|evalScript|copyTextFrom|pasteText|hideKeyboard|stopApp)\b"#);
re!(MAESTRO_RUNFLOW, r#"(?m)runFlow\s*:\s*(?:\n\s+file\s*:\s*)?["']?([\w./\-]+\.ya?ml)"#);
re!(MAESTRO_FILE, r#"(?m)^\s*(?:file|script)\s*:\s*["']?([\w./\-]+\.(?:ya?ml|js|csv|json|xlsx))"#);
re!(MAESTRO_NAME, r#"(?m)^\s*name\s*:\s*["']?([^"'\n]+)"#);
re!(MAESTRO_STEP, r#"(?m)^\s*-\s*(\w+)\s*:?\s*(.*)$"#);

pub fn signals_for(lang: &str, text: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    if matches!(lang, "markdown" | "text" | "json" | "toml" | "config" | "csv" | "gherkin" | "css" | "scss") { return out; }
    if lang == "yaml" {
        if MAESTRO_HEAD.is_match(text) && MAESTRO_CMD.is_match(text) { out.push("maestro"); }
        return out;
    }
    let checks: [(&LazyLock<Regex>, &'static str); 7] = [
        (&SIG_DRIVER, "driver"), (&SIG_LOCATOR, "locator"), (&SIG_ASSERT, "assert"), (&SIG_HOOK, "hook"),
        (&SIG_DB, "db"), (&SIG_EXTERNAL, "external"), (&SIG_CLI, "cli"),
    ];
    for (re, name) in checks { if re.is_match(text) { out.push(name); } }
    if lang == "python" {
        for (re, name) in [(&SIG_ML_DATA, "ml_data"), (&SIG_ML_MODEL, "ml_model"), (&SIG_ML_TRAIN, "ml_train"), (&SIG_ML_EVAL, "ml_eval")] {
            if re.is_match(text) { out.push(name); }
        }
    }
    out
}

/// Sub-flows and data files a Maestro flow pulls in, as relative paths.
fn maestro_imports(text: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for re in [&*MAESTRO_RUNFLOW, &*MAESTRO_FILE] {
        for c in re.captures_iter(text) {
            let p = c[1].to_string();
            let p = if p.starts_with('.') || p.starts_with('/') { p } else { format!("./{p}") };
            if !v.contains(&p) { v.push(p); }
        }
    }
    v
}

// ---------- routes: what the server answers ----------
re!(R_LARAVEL, r#"Route::(get|post|put|patch|delete|any|match|resource|apiResource)\(\s*['"]([^'"]*)['"]\s*,\s*(?:\[\s*\\?([\w\\]+)::class\s*,\s*['"](\w+)['"]\s*\]|['"]([\w\\]+)@(\w+)['"]|\\?([\w\\]+)::class)?"#);
re!(R_EXPRESS, r#"\b(?:app|router|server|api|route|fastify|r)\.(get|post|put|patch|delete|all)\(\s*['"`]([^'"`]+)['"`]\s*,\s*(?:[\w.]+\s*,\s*)*([\w.]+)?"#);
re!(R_PY_DECO, r#"(?m)^\s*@\w+\.(get|post|put|patch|delete|route|api_route)\(\s*['"]([^'"]*)['"]([^)\n]*)\)\s*\n(?:\s*@[^\n]*\n)*\s*(?:async\s+)?def\s+(\w+)"#);
re!(R_DJANGO, r#"\b(?:path|re_path|url)\(\s*r?['"]([^'"]*)['"]\s*,\s*([\w.]+)"#);
re!(R_CS_ATTR, r#"\[\s*Http(Get|Post|Put|Patch|Delete)\s*(?:\(\s*"([^"]*)"\s*\))?\s*\][\s\S]{0,300}?\b(?:public|private|protected)\s+[\w<>\[\],.? ]+?\s+(\w+)\s*\("#);
re!(R_CS_ROUTE, r#"\[\s*(?:Route|ApiController\]\s*\[\s*Route)\s*\(\s*"([^"]*)"\s*\)\s*\][\s\S]{0,200}?\bclass\s+(\w+)"#);
re!(R_CS_MAP, r#"\.Map(Get|Post|Put|Patch|Delete)\(\s*"([^"]*)""#);
re!(R_SPRING, r#"@(Get|Post|Put|Patch|Delete|Request)Mapping\s*\(\s*(?:value\s*=\s*|path\s*=\s*)?\{?\s*"([^"]*)"[\s\S]{0,200}?\b(?:public|private|protected)?\s*[\w<>\[\],.? ]+?\s+(\w+)\s*\("#);
re!(R_SPRING_CLASS, r#"@RequestMapping\s*\(\s*(?:value\s*=\s*|path\s*=\s*)?"([^"]*)"\s*\)[\s\S]{0,200}?\bclass\s+(\w+)"#);
re!(R_GO, r#"\.(GET|POST|PUT|PATCH|DELETE|HandleFunc|Handle|Get|Post|Put|Patch|Delete)\(\s*"([^"]+)"\s*,\s*([\w.]+)"#);
re!(R_AXUM, r#"\.route\(\s*"([^"]+)"\s*,\s*(get|post|put|patch|delete)\(\s*([\w:]+)"#);
re!(R_ACTIX, r#"#\[(get|post|put|patch|delete)\(\s*"([^"]+)"\s*\)\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)"#);

fn short_class(s: &str) -> String { s.rsplit('\\').next().unwrap_or(s).rsplit('.').next().unwrap_or(s).to_string() }

pub fn routes_for(lang: &str, text: &str) -> Vec<RouteDef> {
    let mut out: Vec<RouteDef> = Vec::new();
    let mut push = |method: &str, path: &str, handler: String, action: String, off: usize| {
        if out.len() >= 300 { return; }
        let path = if path.starts_with('/') || path.is_empty() { path.to_string() } else { format!("/{path}") };
        out.push(RouteDef { method: method.to_uppercase(), path, handler, action, line: line_at(text, off) });
    };
    match lang {
        "php" => for c in R_LARAVEL.captures_iter(text) {
            let verb = &c[1];
            let off = c.get(0).unwrap().start();
            let (h, a) = if let Some(h) = c.get(3) { (short_class(h.as_str()), c.get(4).map(|m| m.as_str().to_string()).unwrap_or_default()) }
                else if let Some(h) = c.get(5) { (short_class(h.as_str()), c.get(6).map(|m| m.as_str().to_string()).unwrap_or_default()) }
                else if let Some(h) = c.get(7) { (short_class(h.as_str()), String::new()) } else { (String::new(), String::new()) };
            match verb {
                "resource" | "apiResource" => {
                    let base = &c[2];
                    for (m, suffix, act) in [("GET", "", "index"), ("POST", "", "store"), ("GET", "/{id}", "show"), ("PUT", "/{id}", "update"), ("DELETE", "/{id}", "destroy")] {
                        push(m, &format!("{base}{suffix}"), h.clone(), act.into(), off);
                    }
                }
                "match" | "any" => push("ANY", &c[2], h, a, off),
                v => push(v, &c[2], h, a, off),
            }
        },
        "javascript" | "typescript" => for c in R_EXPRESS.captures_iter(text) {
            let p = &c[2];
            if !p.starts_with('/') && p != "*" { continue; }
            // Inline handlers (`async (req, res) => {}`, `function (req, res)`) have no name.
            let h = c.get(3).map(|m| m.as_str()).filter(|h| !matches!(*h, "async" | "function" | "req" | "ctx" | "request")).unwrap_or("");
            let (handler, action) = match h.split_once('.') { Some((a, b)) => (a.to_string(), b.to_string()), None => (String::new(), h.to_string()) };
            push(if &c[1] == "all" { "ANY" } else { &c[1] }, p, handler, action, c.get(0).unwrap().start());
        },
        "python" => {
            for c in R_PY_DECO.captures_iter(text) {
                let verb = &c[1];
                let method = if matches!(verb, "route" | "api_route") {
                    let rest = c[3].to_uppercase();
                    ["POST", "PUT", "PATCH", "DELETE", "GET"].into_iter().find(|m| rest.contains(m)).unwrap_or("GET")
                } else { verb };
                push(method, &c[2], String::new(), c[4].to_string(), c.get(0).unwrap().start());
            }
            for c in R_DJANGO.captures_iter(text) {
                let target = &c[2];
                if target == "include" { continue; }
                let (handler, action) = match target.rsplit_once('.') { Some((a, b)) => (a.to_string(), b.to_string()), None => (String::new(), target.to_string()) };
                let action = action.trim_end_matches(".as_view").to_string();
                push("ANY", &c[1], handler, action, c.get(0).unwrap().start());
            }
        }
        "csharp" => {
            let (prefix, class) = R_CS_ROUTE.captures(text).map(|c| (c[1].to_string(), c[2].to_string())).unwrap_or_default();
            let ctrl = class.trim_end_matches("Controller").to_lowercase();
            for c in R_CS_ATTR.captures_iter(text) {
                let sub = c.get(2).map(|m| m.as_str()).unwrap_or("");
                let base = prefix.replace("[controller]", &ctrl);
                let full = if sub.starts_with('/') || sub.starts_with("~/") { sub.trim_start_matches('~').to_string() } else if sub.is_empty() { base.clone() } else { format!("{}/{sub}", base.trim_end_matches('/')) };
                push(&c[1], &full, class.clone(), c[3].to_string(), c.get(0).unwrap().start());
            }
            for c in R_CS_MAP.captures_iter(text) { push(&c[1], &c[2], String::new(), String::new(), c.get(0).unwrap().start()); }
        }
        "java" | "kotlin" => {
            let (prefix, class) = R_SPRING_CLASS.captures(text).map(|c| (c[1].to_string(), c[2].to_string())).unwrap_or_default();
            for c in R_SPRING.captures_iter(text) {
                if &c[1] == "Request" && R_SPRING_CLASS.is_match(&c[0]) { continue; }
                let m = if &c[1] == "Request" { "ANY" } else { &c[1] };
                let full = format!("{}/{}", prefix.trim_end_matches('/'), c[2].trim_start_matches('/'));
                push(m, &full, class.clone(), c[3].to_string(), c.get(0).unwrap().start());
            }
        }
        "go" => for c in R_GO.captures_iter(text) {
            if !c[2].starts_with('/') { continue; }
            let m = match &c[1] { "HandleFunc" | "Handle" => "ANY", x => x };
            let h = &c[3];
            let (handler, action) = match h.rsplit_once('.') { Some((a, b)) => (a.to_string(), b.to_string()), None => (String::new(), h.to_string()) };
            push(m, &c[2], handler, action, c.get(0).unwrap().start());
        },
        "rust" => {
            for c in R_AXUM.captures_iter(text) { push(&c[2], &c[1], String::new(), c[3].rsplit("::").next().unwrap_or("").to_string(), c.get(0).unwrap().start()); }
            for c in R_ACTIX.captures_iter(text) { push(&c[1], &c[2], String::new(), c[3].to_string(), c.get(0).unwrap().start()); }
        }
        _ => {}
    }
    out
}

re!(MOUNT, r#"\b(?:app|router|server|api)\.use\(\s*['"`](/[^'"`]*)['"`]\s*,\s*(?:[\w.]+\s*,\s*)*(\w+)\s*\)"#);
re!(BIND_REQUIRE, r#"\b(?:const|let|var)\s+(\w+)\s*=\s*require\(\s*['"]([^'"]+)['"]\s*\)"#);
re!(BIND_IMPORT, r#"\bimport\s+(\w+)\s+from\s+['"]([^'"]+)['"]"#);
re!(DATA_REF, r#"['"]([\w./\\\-]+\.(?:json|csv|tsv|xlsx|xls|yaml|yml|xml|properties|sql))['"]"#);

re!(JSX_ROUTE_TAG, r#"<Route\b[^>]*?/?>"#);
re!(ATTR_PATH, r#"\bpath\s*=\s*\{?\s*["'`]([^"'`]*)["'`]"#);
re!(ATTR_COMP, r#"\b(?:component|element|Component)\s*=\s*\{\s*<?\s*([A-Z]\w*)"#);
re!(OBJ_ROUTE, r#"\{\s*path\s*:\s*["'`]([^"'`]*)["'`]\s*,[^{}]*?(?:component|element|loadComponent|Component)\s*:\s*(?:\(\s*\)\s*=>\s*import\(\s*["']([^"']+)["']\s*\)|<?\s*([A-Z]\w*))"#);
re!(DART_ROUTE, r#"["'](/[\w/\-:]*)["']\s*:\s*\(\s*\w*\s*\)\s*=>\s*(?:const\s+)?([A-Z]\w*)\s*\("#);
re!(BIND_NAMED, r#"\bimport\s*(?:\w+\s*,\s*)?\{([^}]+)\}\s*from\s*['"]([^'"]+)['"]"#);

/// Screens a router shows: React Router, Vue Router, Angular, Flutter route maps.
fn pages_for(text: &str) -> Vec<(String, String, String)> {
    let mut binds: Vec<(String, String)> = Vec::new();
    for re in [&*BIND_REQUIRE, &*BIND_IMPORT] { for c in re.captures_iter(text) { binds.push((c[1].to_string(), c[2].to_string())); } }
    for c in BIND_NAMED.captures_iter(text) {
        for n in c[1].split(',') {
            let local = n.split(" as ").last().unwrap_or(n).trim();
            if !local.is_empty() { binds.push((local.to_string(), c[2].to_string())); }
        }
    }
    let spec_of = |ident: &str| binds.iter().find(|(n, _)| n == ident).map(|(_, s)| s.clone()).unwrap_or_default();
    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut add = |url: &str, spec: String, ident: String| {
        if out.len() < 200 && !out.iter().any(|(u, s, i)| u == url && *s == spec && *i == ident) { out.push((url.to_string(), spec, ident)); }
    };
    for t in JSX_ROUTE_TAG.find_iter(text) {
        let tag = t.as_str();
        if let (Some(p), Some(c)) = (ATTR_PATH.captures(tag), ATTR_COMP.captures(tag)) { add(&p[1], spec_of(&c[1]), c[1].to_string()); }
    }
    for c in OBJ_ROUTE.captures_iter(text) {
        if let Some(spec) = c.get(2) { add(&c[1], spec.as_str().to_string(), String::new()); }
        else if let Some(id) = c.get(3) { add(&c[1], spec_of(id.as_str()), id.as_str().to_string()); }
    }
    for c in DART_ROUTE.captures_iter(text) { add(&c[1], String::new(), c[2].to_string()); }
    out
}

/// `app.use('/api/rides', rides)` where `rides = require('./routes/rides')`.
fn mounts_for(text: &str) -> Vec<(String, String)> {
    let mut binds: Vec<(String, String)> = Vec::new();
    for re in [&*BIND_REQUIRE, &*BIND_IMPORT] { for c in re.captures_iter(text) { binds.push((c[1].to_string(), c[2].to_string())); } }
    MOUNT.captures_iter(text).filter_map(|c| {
        let var = &c[2];
        binds.iter().find(|(n, _)| n == var).map(|(_, spec)| (c[1].to_string(), spec.clone()))
    }).take(60).collect()
}

/// Data files named in strings: `File.ReadAllText("TestData/users.json")`, `pd.read_excel('data/rides.xlsx')`.
fn data_refs(text: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for c in DATA_REF.captures_iter(text) {
        let p = c[1].replace('\\', "/");
        let p = p.trim_start_matches("./").to_string();
        if p.starts_with("http") || p.contains("node_modules") || p.ends_with("package.json") || p.ends_with("tsconfig.json") { continue; }
        let spec = format!("data:{p}");
        if !v.contains(&spec) { v.push(spec); }
        if v.len() >= 40 { break; }
    }
    v
}

// ---------- calls: requests the code sends ----------
re!(C_FETCH, r#"\bfetch\(\s*(['"`])([^'"`]+)['"`]\s*(?:,\s*\{[^}]{0,200}?method\s*:\s*['"](\w+)['"])?"#);
re!(C_CLIENT, r#"\b(?:axios|api|http|client|request|\$http|this\.http|httpClient|instance|apiClient|requests|httpx|session|dio|_http|HttpClient|ky|superagent|\$axios|useFetch|\$fetch)\.(get|post|put|patch|delete)(?:<[^>]*>)?\(\s*(?:Uri\.parse\()?(?:f|r)?(['"`])([^'"`]+)['"`]"#);
re!(C_CS, r#"\.(Get|Post|Put|Patch|Delete)(?:FromJson|AsJson)?Async(?:<[^>]*>)?\(\s*\$?"([^"]+)""#);
re!(C_PHP, r#"\bHttp::(?:\w+\([^)]*\)->)*(get|post|put|patch|delete)\(\s*['"]([^'"]+)['"]"#);
re!(C_FORM, r#"<form[^>]*\baction\s*=\s*["']([^"'{]+)["'][^>]*?(?:method\s*=\s*["'](\w+)["'])?"#);

fn looks_like_url(u: &str) -> bool {
    let u = u.trim();
    if u.is_empty() || u.len() > 300 || u.contains(' ') || u.contains('\n') { return false; }
    u.starts_with('/') || u.starts_with("http://") || u.starts_with("https://") || u.starts_with("${") || u.starts_with("{") || u.contains("/api") || u.split('/').count() >= 2 && !u.contains('.')
}

pub fn calls_for(text: &str) -> Vec<HttpCall> {
    let mut out: Vec<HttpCall> = Vec::new();
    let mut push = |m: &str, u: &str, off: usize| {
        if out.len() >= 200 || !looks_like_url(u) { return; }
        if out.iter().any(|c| c.url == u && c.method.eq_ignore_ascii_case(m)) { return; }
        out.push(HttpCall { method: m.to_uppercase(), url: u.to_string(), line: line_at(text, off) });
    };
    for c in C_FETCH.captures_iter(text) { push(c.get(3).map(|m| m.as_str()).unwrap_or("GET"), &c[2], c.get(0).unwrap().start()); }
    for c in C_CLIENT.captures_iter(text) { push(&c[1], &c[3], c.get(0).unwrap().start()); }
    for c in C_CS.captures_iter(text) { push(&c[1], &c[2], c.get(0).unwrap().start()); }
    for c in C_PHP.captures_iter(text) { push(&c[1], &c[2], c.get(0).unwrap().start()); }
    for c in C_FORM.captures_iter(text) { push(c.get(2).map(|m| m.as_str()).unwrap_or("GET"), &c[1], c.get(0).unwrap().start()); }
    out
}

// ---------- scenarios: test cases, in plain names ----------
re!(T_JS_DESCRIBE, r#"\b(?:describe|context|suite)(?:\.only|\.skip)?\(\s*['"`]([^'"`]+)['"`]"#);
re!(T_JS_IT, r#"\b(?:it|test|specify)(?:\.only|\.skip|\.each\([^)]*\))?\(\s*['"`]([^'"`]+)['"`]"#);
re!(T_PY_DEF, r#"(?m)^(\s*)(?:async\s+)?def\s+(test_\w+)\s*\("#);
re!(T_PY_CLASS, r#"(?m)^class\s+(Test\w*)"#);
re!(T_CLIKE, r#"(?:\[\s*(?:Test|Fact|Theory|TestMethod|TestCase[^\]]*)\b[^\]]*\]|@(?:Test|ParameterizedTest)\b)[\s\S]{0,240}?\b(?:void|Task|async\s+Task|fun|IActionResult)\s+(\w+)\s*\("#);
re!(T_PHP, r#"(?:@test[\s\S]{0,80}?)?\bfunction\s+(test\w*)\s*\(|(?m)^\s*(?:it|test)\(\s*['"]([^'"]+)['"]"#);
re!(T_CLASS, r#"\bclass\s+(\w+)"#);

fn humanize(name: &str) -> String {
    let s = name.trim_start_matches("test_").trim_start_matches("test").trim_start_matches('_');
    let spaced = CAMEL.replace_all(s, "$1 $2").replace('_', " ");
    let t = spaced.trim();
    if t.is_empty() { name.to_string() } else {
        let mut c = t.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()).unwrap_or_default()
    }
}

fn gherkin_scenarios(text: &str) -> Vec<Scenario> {
    let mut out: Vec<Scenario> = Vec::new();
    let mut feature = String::new();
    let mut background: Vec<ScenarioStep> = Vec::new();
    let mut in_bg = false;
    let mut last_kw = String::from("Given");
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        let ln = i + 1;
        if let Some(r) = t.strip_prefix("Feature:") { feature = r.trim().to_string(); in_bg = false; continue; }
        if t.starts_with("Background:") { in_bg = true; continue; }
        if let Some(r) = t.strip_prefix("Scenario Outline:").or_else(|| t.strip_prefix("Scenario Template:")).or_else(|| t.strip_prefix("Scenario:")).or_else(|| t.strip_prefix("Example:")) {
            in_bg = false;
            out.push(Scenario { name: r.trim().to_string(), group: feature.clone(), line: ln, steps: background.clone() });
            continue;
        }
        if t.starts_with("Rule:") || t.starts_with("Examples:") || t.starts_with('|') || t.starts_with('#') || t.starts_with('@') { continue; }
        for kw in ["Given", "When", "Then", "And", "But", "*"] {
            if let Some(rest) = t.strip_prefix(kw) {
                if !rest.starts_with(' ') { continue; }
                let shown = if matches!(kw, "And" | "But" | "*") { last_kw.clone() } else { last_kw = kw.to_string(); kw.to_string() };
                let step = ScenarioStep { kw: if kw == "*" { shown } else { kw.to_string() }, text: rest.trim().to_string(), line: ln };
                if in_bg { background.push(step); } else if let Some(s) = out.last_mut() { if s.steps.len() < 60 { s.steps.push(step); } }
                break;
            }
        }
        if out.len() >= 200 { break; }
    }
    out
}

fn maestro_scenario(path: &str, text: &str) -> Scenario {
    let name = MAESTRO_NAME.captures(text.split("\n---").next().unwrap_or(""))
        .map(|c| c[1].trim().to_string())
        .unwrap_or_else(|| humanize(path.rsplit('/').next().unwrap_or(path).split('.').next().unwrap_or("flow")));
    let body = match text.find("\n---") { Some(i) => &text[i + 4..], None => text };
    let offset = text.len() - body.len();
    let mut steps = Vec::new();
    for c in MAESTRO_STEP.captures_iter(body) {
        let off = offset + c.get(0).unwrap().start();
        let arg = c[2].trim().trim_matches('"').trim_matches('\'');
        steps.push(ScenarioStep { kw: c[1].to_string(), text: if arg.is_empty() { humanize(&c[1]) } else { arg.to_string() }, line: line_at(text, off) });
        if steps.len() >= 80 { break; }
    }
    Scenario { name, group: String::new(), line: 1, steps }
}

pub fn scenarios_for(path: &str, lang: &str, text: &str, maestro: bool) -> Vec<Scenario> {
    if lang == "gherkin" { return gherkin_scenarios(text); }
    if maestro { return vec![maestro_scenario(path, text)]; }
    let p = path.to_lowercase();
    let testy = role_for(path) == Some("Test") || p.contains("test") || p.contains("spec") || p.contains("e2e") || p.contains("cypress");
    if !testy { return vec![]; }
    let mut out: Vec<Scenario> = Vec::new();
    let mut add = |name: String, group: String, off: usize| {
        if out.len() < 120 { out.push(Scenario { name, group, line: line_at(text, off), steps: vec![] }); }
    };
    match lang {
        "javascript" | "typescript" => {
            let groups: Vec<(usize, String)> = T_JS_DESCRIBE.captures_iter(text).map(|c| (c.get(0).unwrap().start(), c[1].to_string())).collect();
            for c in T_JS_IT.captures_iter(text) {
                let off = c.get(0).unwrap().start();
                let g = groups.iter().filter(|(o, _)| *o < off).last().map(|(_, g)| g.clone()).unwrap_or_default();
                add(c[1].to_string(), g, off);
            }
        }
        "python" => {
            let classes: Vec<(usize, String)> = T_PY_CLASS.captures_iter(text).map(|c| (c.get(0).unwrap().start(), c[1].to_string())).collect();
            for c in T_PY_DEF.captures_iter(text) {
                let off = c.get(0).unwrap().start();
                let g = if c[1].is_empty() { String::new() } else { classes.iter().filter(|(o, _)| *o < off).last().map(|(_, g)| humanize(g)).unwrap_or_default() };
                add(humanize(&c[2]), g, off);
            }
        }
        "csharp" | "java" | "kotlin" => {
            let class = T_CLASS.captures(text).map(|c| humanize(c[1].trim_end_matches("Tests").trim_end_matches("Test"))).unwrap_or_default();
            for c in T_CLIKE.captures_iter(text) { add(humanize(&c[1]), class.clone(), c.get(0).unwrap().start()); }
        }
        "php" => {
            let class = T_CLASS.captures(text).map(|c| humanize(c[1].trim_end_matches("Test"))).unwrap_or_default();
            for c in T_PHP.captures_iter(text) {
                let name = c.get(1).map(|m| humanize(m.as_str())).or_else(|| c.get(2).map(|m| m.as_str().to_string())).unwrap_or_default();
                if !name.is_empty() { add(name, class.clone(), c.get(0).unwrap().start()); }
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod behaviour_tests {
    use super::*;

    #[test]
    fn routes_are_found_in_common_frameworks() {
        let r = routes_for("php", "Route::post('/rides', [RideController::class, 'store']);\nRoute::resource('bills', BillController::class);");
        assert_eq!((r[0].method.as_str(), r[0].path.as_str(), r[0].handler.as_str(), r[0].action.as_str()), ("POST", "/rides", "RideController", "store"));
        assert!(r.iter().any(|x| x.path == "/bills/{id}" && x.action == "show"));
        assert_eq!(routes_for("typescript", "router.post('/users/login', auth.optional, async (req, res) => {})")[0].action, "");
        let r = routes_for("javascript", "router.post('/api/rides/:id/assign', auth, rides.assign)");
        assert_eq!((r[0].path.as_str(), r[0].handler.as_str(), r[0].action.as_str()), ("/api/rides/:id/assign", "rides", "assign"));
        let r = routes_for("python", "@app.post(\"/rides\")\nasync def create_ride(body: Ride):\n    pass\n");
        assert_eq!((r[0].method.as_str(), r[0].action.as_str()), ("POST", "create_ride"));
        let r = routes_for("csharp", "[ApiController]\n[Route(\"api/[controller]\")]\npublic class RidesController {\n [HttpPost(\"{id}/assign\")]\n public IActionResult Assign(int id) {} }");
        assert_eq!((r[0].path.as_str(), r[0].action.as_str()), ("/api/rides/{id}/assign", "Assign"));
    }

    #[test]
    fn calls_and_scenarios_are_found() {
        let c = calls_for("const r = await axios.post(`${API}/rides`, body); fetch('/api/login', { method: 'POST' })");
        assert!(c.iter().any(|x| x.method == "POST" && x.url == "${API}/rides"));
        assert!(c.iter().any(|x| x.method == "POST" && x.url == "/api/login"));
        let s = gherkin_scenarios("Feature: Login\nBackground:\n  Given the app is open\nScenario: Valid user\n  When I sign in as \"ana\"\n  And I tap Login\n  Then I see the home screen\n");
        assert_eq!(s[0].name, "Valid user");
        assert_eq!(s[0].group, "Login");
        assert_eq!(s[0].steps.len(), 4);
        assert_eq!(s[0].steps[2].kw, "And");
        assert!(scenarios_for("StepDefinitions/LoginSteps.cs", "csharp", "[Given(@\"x\")] public void Open() {}", false).is_empty());
        let s = scenarios_for("tests/login.spec.ts", "typescript", "describe('Login', () => { it('rejects a bad password', () => {}) })", false);
        assert_eq!((s[0].name.as_str(), s[0].group.as_str()), ("rejects a bad password", "Login"));
        assert!(signals_for("csharp", "var d = new ChromeDriver(); d.FindElement(By.Id(\"u\"));").contains(&"driver"));
        assert!(signals_for("yaml", "appId: com.x\n---\n- launchApp\n- tapOn: \"Login\"").contains(&"maestro"));
    }
}
