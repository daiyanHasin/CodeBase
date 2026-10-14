# CodeBase

**Know how your project works, not just where its files are.** Open a folder or a GitHub link. CodeBase finds the project's architecture, traces every journey end to end (log in → request a ride → assign a driver → pay, or scenario → step definitions → page objects → driver), shows every version in plain words, and hands ChatGPT, Claude, Gemini or any other chat *exactly the right small slice* of your code for the job in front of you.

It is made for people who build apps with AI tools, including people with little or no programming background, on ordinary laptops with free AI chats and small context windows.

- **Offline and free.** One program, no account, no API key, no telemetry. Nothing leaves your computer until you paste it somewhere yourself.
- **Small and light.** A single ~6 MB executable for Windows, macOS and Linux.
- **No AI needed.** Architecture, flows and history come from reading the code and Git. An AI chat is optional, for naming things in your own words.
- **Honest.** It reads code without running it, and says so when that limits what it can see.

Version 0.5.0 · Rust · MIT licence · Hasin Md. Daiyan

---

## Contents

1. [Why CodeBase exists](#why-codebase-exists)
2. [Start in two minutes](#start-in-two-minutes)
3. [Ask AI: context packs](#ask-ai-context-packs)
4. [The rest of the app](#the-rest-of-the-app)
5. [Architecture and flows](#architecture-and-flows)
6. [What CodeBase understands](#what-codebase-understands)
7. [How file picking works](#how-file-picking-works)
8. [Privacy and where data is kept](#privacy-and-where-data-is-kept)
9. [Command line](#command-line)
10. [For developers](#for-developers)
11. [Troubleshooting](#troubleshooting)
12. [Roadmap](#roadmap)
13. [What changed in 0.5.0](#what-changed-in-050)

---

## Why CodeBase exists

Free AI chats can read only a few thousand words at a time. Paste too little and the AI guesses what your other files contain. Paste too much and it forgets the start, or the chat refuses. Either way you get code that fixes one thing and breaks another.

CodeBase solves the "what do I paste?" problem. It knows the whole project, so you don't have to. You say what you want, and it builds one block of text that fits your chat:

```
instructions for the AI      what it may change, how to answer, what not to invent
project brief                what the app is, what it is built with, how to run it
your task or error           exactly as you wrote it
full code                    the few files that matter most
outlines                     just the function names of the files around them
related files                one line each, so the AI knows they exist
```

CodeBase is not an AI and does not need one. It is the librarian; your chat is the expert.

---

## Start in two minutes

### Windows

**Option A: a ready-made program (no installs).**
1. Download `codebase-windows-x64.exe` from the project's Releases page (or from a finished **build** run in the GitHub **Actions** tab).
2. Rename it to `codebase.exe` and put it next to `CodeBase.bat`.
3. Double-click `CodeBase.bat`.

**Option B: build it yourself.**
1. Install Rust from <https://rustup.rs>. When asked, accept the Visual Studio C++ build tools.
2. Double-click `CodeBase.bat`.

The first build takes a few minutes. After that `CodeBase.bat` rebuilds in about a second on every start, so you always run the newest files in the folder.

On first run a folder picker opens: choose your project. CodeBase opens in your browser at `http://127.0.0.1:4321`. Keep the black window open while you work; closing it stops CodeBase.

### macOS and Linux

```
cargo build --release
./target/release/codebase /path/to/your/project
```

### Open a project from GitHub

```
codebase https://github.com/user/repo
```

CodeBase clones it with its full history into `~/.codebase/repos/` (Git must be installed) and opens it. Running the same command again updates it.

---

## Ask AI: context packs

This is the main reason to use CodeBase.

### 1. Say what you want

Type the change in your own words, for example:

> Add a "paid on" date to bills and show it in the bill list

or **paste an error message or stack trace** exactly as you got it. CodeBase recognises errors from Python, JavaScript/TypeScript, PHP, C#/.NET, Java and others, finds the project files they name (even when the paths come from another computer) and notes the line numbers.

Press **Build context pack**, or Ctrl+Enter.

### 2. Pick a size

| Size | Use it for |
| --- | --- |
| **4k** | Small free chats, or a very focused question |
| **8k** | Free ChatGPT, Claude and Gemini (default) |
| **16k** | Most paid chats |
| **32k** | Long-context models |

Sizes are in *tokens*, the units AI chats count text in. CodeBase estimates one token per four characters, which is close enough to stay safely inside the limit.

### 3. Check the list, then copy

CodeBase shows every file it chose and **why**: "Named in the error (line 9)", "Defines BillService", "Tests BillService.cs", "Changed together with Bill.cs (3 commits)". Change how any file is included with its menu:

| Option | What the AI gets | Can the AI change it? |
| --- | --- | --- |
| **Full code** | The whole file | Yes |
| **Part** | Only the lines around an error | Yes |
| **Outline** | Names of classes and functions only | No, it asks first |
| **Summary** | One line saying what the file does | No, it asks first |
| **Off** | Nothing | No |

The bar at the top shows how much of your chosen size is used. **Auto** next to a file hands the choice back to CodeBase. Press **Copy context pack** and paste it as the first message of a new chat.

If the AI needs a file you left out, it is told to reply `NEED FILES: path1, path2` instead of guessing. Add those files (or switch them to **Full code**) and copy again.

### Copy context for this

Every file, folder and feature has a **Copy context for this** button. One click builds a pack around it at your current size and copies it. Then type what you want at the top of the chat.

**Pinning:** Shift+click files on the map, or press **Pin to Ask AI**, to make sure they are always in the pack in full.

### Tips for small chats

- Start a **new chat** for each task. Old messages use up the chat's memory.
- Paste the **whole error**, not a description of it.
- If the answer changes a file you did not expect, open that file in CodeBase and look at **Used by** before you accept the change.

---

## The rest of the app

| View | What it is for |
| --- | --- |
| **Overview** | What the project is, what kind of design it has, the journeys people take through it in order, and anything that needs attention. |
| **Architecture** | The path data takes through the parts of the design, the supporting parts beside it, and design checks. Click a part for its files and what it uses. |
| **Flows** | Every journey, and every flow in it, traced end to end. **Play the flow** steps through it. Scenario flows show each step line next to the step definition that runs it. |
| **Tree** | The project as an m-ary tree: project → parts → groups → files. In 3D the parts stand left to right in data-path order with their files on a ring below; in 2D it reads top to bottom. |
| **History** | Every commit in plain words, the design at any version compared with today, and one file's change in one commit. Save points for folders without Git. |
| **Ask AI** | Context packs, described above. |
| **Memory** | Plain-language descriptions of files and folders. Write them yourself, or copy one prompt into any AI chat and paste the answer back. Your words override the automatic summaries and power every context pack. |
| **Clean up** | Hide generated or noisy files from the tree and from packs. |

Clicking a file shows its part in the design, a plain summary, **what a change to it would reach** (files that use it, flows that pass through it, journeys and tests), its links and tests, files that usually change with it, the last commit message, and the code.

---

## Architecture and flows

Folders say where files are kept. Architecture says what each file does and how data moves between them. Everything below is plain, repeatable code in `src/arch.rs` and `src/flows.rs`; no AI is involved.

**Parts.** Each file gets the part it plays, from the strongest evidence first:
1. *What the code does*: defines web addresses, sends requests, starts a browser or device, finds screen elements, asserts, queries a database, calls an outside service. A component a router shows is a screen, wherever it lives.
2. *What the file is called*: `article.service.ts` is business logic and `RideController.php` is a controller, even in a folder called `routes/`.
3. *Where it sits*: `Pages/`, `StepDefinitions/`, `migrations/`.

**Kind of project.** Test automation framework, full-stack app, server-rendered web app, backend API, front-end or mobile app, machine-learning project, command-line tool, or library. Each kind has a data path, for example:

```
test automation   Scenarios → Step definitions → Page objects → Driver → App under test
                  (beside it: Hooks, Test data, Test settings, Test helpers, Reports)
full-stack app    Screens → UI components → State → API client → Routes → Guards →
                  Controllers → Validation → Business logic → Data access → Models → Database
```

**Flows** start from Gherkin scenarios (Background steps included), Maestro flows, end-to-end tests, server routes and screens. A screen's flow crosses from the front end to the back end by matching the requests it sends (`axios.post('/api/rides')`) to the route that answers them (`router.post('/')` mounted at `/api/rides`).

**Journeys** group flows by the words they share and order them the way people move through an app: set up, sign up, log in, profile, browse, create, plan, assign, track, pay, finish, log out. Paste one prompt into any AI chat to have them named and ordered in your product's words; the answer is saved in `.codebase/memory.json`.

**Design checks**: scenario steps with no step definition, step definitions that bypass page objects, page objects that assert, parts that depend on a later part of the data path, controllers that query the database directly, import loops, unused step definitions and code nothing reaches.

---

## What CodeBase understands

CodeBase reads code with fast pattern matching. No compiler or language server is needed.

| Language | Links between files | Definitions |
| --- | --- | --- |
| JavaScript, TypeScript, Vue, Svelte | `import`, `require`, dynamic `import()`, `@/` and `~/` aliases | functions, classes, arrow functions, types |
| Python | `import`, `from … import`, relative imports | functions, classes |
| PHP, Laravel, Blade | `use` via composer PSR-4, `view()`, `@extends`/`@include`, `<x-component>`, `@vite` | classes, methods, sections |
| C# / .NET | types used from other files (`BillService` links to `BillService.cs`) | classes, methods |
| Java, Kotlin | `import`, plus type references | classes, methods |
| Go, Swift, Dart | type references | functions, types |
| Rust | `mod`, `use crate::` | functions, structs, traits |
| CSS, SCSS, HTML | `@import`, `@use`, `<script src>`, `<link href>` | none |

**Routes** are read from Laravel (including `Route::resource`), Express and Fastify (including `app.use('/prefix', router)`), FastAPI and Flask, Django, ASP.NET (attributes and minimal APIs), Spring, Go (net/http, gin, chi) and Rust (axum, actix). **Requests** from `fetch`, axios and similar clients, HttpClient, Dio and Laravel `Http::`. **Screens** from React Router, Vue and Angular route tables and Flutter route maps. **Data files** named in strings (`"TestData/users.json"`, `read_excel('rides.xlsx')`) are linked to the code that reads them.

**Tests are linked to the code they check:**

- **By name.** `BillServiceTests.cs`, `test_bill.py`, `bill.spec.ts` and `bill_test.go` are linked to `BillService.cs`, `bill.py`, `bill.ts` and `bill.go`.
- **BDD steps.** In SpecFlow, Reqnroll, Cucumber and behave projects, each line of a `.feature` file is matched to the step-definition method that runs it.

**Git history** (when the project is a Git repository): files that keep changing in the same commits are linked, and the last commit message for each file is shown.

**Always skipped:** `node_modules`, `vendor`, `.git`, build output (`dist`, `build`, `target`, `obj`, .NET `bin`), caches, IDE folders (`.vs`, `.idea`), lock files, minified files and binaries, plus everything in your `.gitignore` and your Clean up list.

**What it cannot see.** Wiring that only happens when the program runs: dependency-injection containers, routes built from strings, reflection, dynamic imports with computed names. The AI is told this in every pack, so it can ask about files that might be involved.

---

## How file picking works

Everything below is plain, repeatable code, in `src/context.rs`.

1. **Start files**
   - Files named in a pasted error or stack trace, with their line numbers.
   - Files that define a name you typed, such as `BillService` or `calculateTotal`.
   - A keyword search (BM25) over file names, definitions, folder names, summaries, your Memory notes and commit messages. A file name counts three times as much as a summary.
2. **Neighbours.** For each start file: what it uses, what uses it, its tests, and files Git says change together with it.
3. **Ranking.** Start files first, then neighbours. Ties go to more *important* files: CodeBase runs PageRank over the links, so a model used by twenty screens outranks a one-off helper.
4. **Filling the size you picked.**
   - Full code goes to the top start files.
   - For a huge file named in an error, only the lines around the error are included.
   - Neighbours get outlines, and the rest get one-line summaries.
   - If room is left over, the best neighbours are upgraded to full code.
5. **Your choices win.** Anything you set by hand stays as you set it, even if it goes over the size. CodeBase warns you when it does.

The project brief at the top (about 500 tokens) is built from your Memory description or the README, the detected stack, the main folders, the features, the tests and how to run the project. Run commands are detected for npm/pnpm/yarn, Laravel, Django, Cargo, Go, Flutter and dotnet, or taken from the README.

---

## Privacy and where data is kept

CodeBase never sends your code anywhere. It makes no network requests except `git clone`/`git pull` when you open a repository by link, and it collects no telemetry. Fonts come from your system.

| Path | Contents |
| --- | --- |
| `<project>/.codebase/memory.json` | Your file and folder descriptions, and which files went into past prompts |
| `<project>/.codebase/config.json` | Your Clean up list |
| `<project>/.codebase/journal.json` | Save points, for folders without Git |
| `~/.codebase/recent.json` | Recently opened projects |
| `~/.codebase/repos/` | Repositories opened by link |

Add `.codebase/` to your project's `.gitignore` if you do not want to share your notes. Commit it if your team should share them.

Projects opened with the earlier Canopy builds keep their notes: CodeBase reads `.canopy/` when `.codebase/` does not exist yet.

---

## Command line

```
codebase [folder | git-url] [--pick] [--port 4321] [--host 127.0.0.1] [--no-open]
```

| Option | Effect |
| --- | --- |
| `folder` | Open a local folder |
| `git-url` | Clone into `~/.codebase/repos` (or update it), then open it. Needs Git |
| nothing | Reopen the last project; on first run show a folder picker (Windows) |
| `--pick` | Always show the folder picker |
| `--port` | First port to try; the next free one up to 20 higher is used |
| `--host 0.0.0.0` | Let other devices on your network open it. Only on networks you trust: anyone who can reach it can read your code |
| `--no-open` | Do not open the browser |
| `--version`, `--help` | Print the version or a short help |

---

## For developers

### Layout

```
codebase/
  CodeBase.bat            Windows launcher (builds if Rust is installed, then runs)
  Cargo.toml, Cargo.lock  Rust project and pinned library versions
  LICENSE                 MIT licence
  src/
    main.rs               start-up, project switching, file watching, local web API
    scan.rs               walks the project, skips junk, resolves imports, builds the map
    analyze.rs            per-file reading: language, definitions, imports, layer, summary
    links.rs              type references, test links, BDD steps, Git co-change, importance
    context.rs            context-pack builder: error parsing, search, budget, project brief
    arch.rs               parts, kind of project, data path, the m-ary tree, design checks
    flows.rs              end-to-end flows and journeys
    history.rs            plain-language timeline, design at any version, single-file diffs
    understand.rs         feature detection from shared vocabulary
    memory.rs             notes, prompt history, hidden files, journeys and save points in .codebase/
  web/                    the app, built into the program at compile time
    app.html, app.css, app.js, tokens.css, favicon.svg
    vendor/               3d-force-graph 1.80.0 and highlight.js 11.11.1
  site/                   landing page (/welcome) and documentation (/docs)
  tests/fixtures/         tiny sample projects used by the tests
  .github/workflows/      builds Windows, macOS and Linux programs on GitHub
```

The web files are embedded in the program, so rebuild after editing `web/` or `site/`. `CodeBase.bat` does this for you.

### Build and test

```
cargo build --release
cargo test --release
```

The 34 tests cover:
- junk filtering
- import and reference resolution for JavaScript, Python, PHP/Laravel, C# (with Reqnroll), and Rust
- test and step linking, Git log parsing and importance ranking
- stack-trace parsing for Python, .NET, JavaScript and PHP
- keyword search
- context packs: never over budget in automatic mode, user choices win, packs build from seed files without a task, and the project brief
- routes, requests, scenarios and behaviour signals across frameworks
- architecture: a test-automation project and a ride-sharing app, each traced end to end (scenario to driver with its test data; ride-request screen across the API to the matching service and models), journey order, design checks and import loops
- commit summaries and revision validation

Each fixture in `tests/fixtures/` is a miniature project, including the junk folders that must be skipped. To add a language, add a fixture there and a test in `scan.rs`.

### Dependencies

Six small crates, all pinned in `Cargo.lock`:
- `tiny_http` serves the local web page.
- `serde` and `serde_json` read and write JSON.
- `regex` does the parsing.
- `ignore` respects `.gitignore`.
- `notify` watches for file changes.
- `sha1_smol` hashes file content.

Versions 0.4.0 and 0.5.0 added none.

### API

The browser talks to the program over `http://127.0.0.1:<port>/api/…`:
- `graph`: files with their parts, links, the architecture (`arch`), design checks, flows and journeys.
- `poll`: live changes since the last check.
- `file` and `files`: details about files.
- `note` and `notes`: save descriptions.
- `history`: record which files went into a prompt.
- `hidden`: the Clean up list.
- `rescan`: scan again.
- `timeline`: commits in plain words, uncommitted edits and save points.
- `at?rev=`: the design at a commit or tag, compared with today.
- `diff?rev=&path=`: what one commit changed in one file.
- `journeys` (POST): save journey names and order, or `null` to go back to automatic.
- `context`: build a context pack (POST: `task`, `budget`, `pins`, `seeds`, `seedReason`, `modes`, `plain`, `done`).
- `brief`: the project brief on its own.
- `projects` and `open`: list and open projects.

### Releases

Push a tag such as `v0.5.0`. The **build** workflow attaches Windows, macOS and Linux programs to a GitHub Release. You can also run it by hand from the Actions tab.

---

## Troubleshooting

- **The build fails on Windows with a linker error (`link.exe` not found).** The Visual Studio C++ build tools are missing. Run the Rust installer again and accept them, or use a ready-made `codebase.exe`.
- **Windows blocks the program.** Right-click it, choose Properties, tick Unblock.
- **The page shows an old version.** Close every CodeBase window and start again. With Rust installed, `CodeBase.bat` always rebuilds from the current files.
- **A file shows no links.** Links come from code that can be read without running it (see [What CodeBase understands](#what-codebase-understands)). Pin the files you know belong together, or describe the connection in Memory.
- **The context pack picked the wrong files.**
  - Use the words your app uses: "bill", not "invoice", if the code says Bill.
  - Paste the real error instead of describing it.
  - Turn files on or off in the list.
  - Better Memory descriptions improve every future pack.
- **"Only the first 8,000 files are mapped."** Hide folders you do not work on from Clean up.
- **Co-change links are missing.** They need a Git repository with some history, and Git installed.

The full guide is built in: open CodeBase and choose **Documentation**, or visit `http://127.0.0.1:4321/docs`.

---

## Roadmap

| Phase | Status |
| --- | --- |
| 1. Context-pack builder | Done in 0.4.0 |
| 2. Deeper analysis: junk filtering, C#/Java links, test and BDD links, Git co-change, importance | Done in 0.4.0, except tree-sitter parsing (planned; see below) |
| 3. Architecture, end-to-end flows, journeys, design checks | **Done in 0.5.0** |
| 4. Map redesign: the m-ary tree in data-path order | **Done in 0.5.0** |
| 5. History: plain-language timeline, design at any version | **Done in 0.5.0** |
| 6. "What would this change reach?": files, flows, journeys and tests | **Done in 0.5.0** (in the file details) |
| Function-level flows, so a screen shows only the requests of the functions it imports | Next |
| Background scanning with progress, so large projects never freeze the page | Next |
| Optional AI layer: none, local Ollama, or your own free API key; never required | Planned |
| First-run tour, plain-language polish | Planned |

**Why not tree-sitter yet?** It would give exact definitions and function calls, but every language grammar is C code, so it needs a C compiler to build and adds several MB per language. Type-reference linking already gives C# and Java projects real links without those costs. Tree-sitter will come as an optional, per-language step.

---

## What changed in 0.5.0

**New**
- **Architecture instead of folders.** Every file gets the part it plays in the design, read from what the code does, what it is called and where it sits. CodeBase works out the kind of project (test automation, full-stack, web app, API, front-end, mobile, machine learning, command-line tool) and the path data takes through its parts.
- **Flows**, traced end to end, from Gherkin scenarios, Maestro flows, end-to-end tests, server routes and screens. Screens are joined to the server routes their requests reach. **Play the flow** steps through it.
- **Journeys** in the order people use an app, with optional naming by your AI chat.
- **Tree**: project → parts → groups → files, as a fixed m-ary tree in 3D (parts in a row in data-path order, files on a ring below, data moving along the path) or 2D. Replaces the folder map.
- **History**: every commit in plain words with the parts it touched, a commit-activity strip, filters, the design at any version compared with today, and per-file diffs. Save points for folders without Git. GitHub links are now cloned with their full history.
- **Design checks**: undefined and unused steps, steps bypassing page objects, asserting page objects, backward dependencies, controllers querying the database, import loops, unreachable code.
- **What a change would reach**: files that use a file, flows through it, journeys and tests, in the file details.
- **Copy context for this flow / part**; the project brief in every pack now states the architecture and the journeys.
- **Detection**: routes in Laravel, Express (with mount prefixes), FastAPI, Flask, Django, ASP.NET, Spring, Go and Rust; requests from fetch, axios, HttpClient, Dio and Laravel; screens from React Router, Vue, Angular and Flutter routers; Gherkin scenarios with Background steps; test functions in JavaScript, Python, C#, Java, Kotlin and PHP; data files named in strings.
- **New look**: cool slate neutrals with one blue accent, a light theme, and colours that say where a part sits on the data path.

**Changed**
- Map and Features are replaced by Architecture, Flows and Tree.
- 34 tests (was 23), including two architecture fixtures.

---

## Licence

MIT. See [LICENSE](LICENSE). You may use, change and share CodeBase freely.
