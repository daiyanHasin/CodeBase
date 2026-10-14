//! Project memory stored inside the repo at `.codebase/`:
//!   memory.json  what files and folders do (your notes + AI descriptions) and prompt history
//!   config.json  files and folders you chose to hide
//!   journal.json save points (what changed, when) so folders without Git still have a history
//! Notes remember the file hash they were written against, so an edited file
//! shows its note as "may be outdated" until you confirm or rewrite it.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct HistoryItem {
    pub task: String,
    pub at: u64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Entry {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source: Option<String>,
    #[serde(rename = "noteHash", skip_serializing_if = "Option::is_none", default)]
    pub note_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub updated: Option<u64>,
    #[serde(default)]
    pub history: Vec<HistoryItem>,
}

#[derive(Serialize, Deserialize, Default)]
struct MemoryFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    files: BTreeMap<String, Entry>,
    /// Journey names and order, from you or your AI chat: {"journeys":[{"name","about","flows":[ids]}]}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    journeys: Option<serde_json::Value>,
}

/// One automatic save point: files that changed while CodeBase was watching.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct JournalEntry {
    pub at: u64,
    #[serde(default)]
    pub added: Vec<String>,
    #[serde(default)]
    pub modified: Vec<String>,
    #[serde(default)]
    pub removed: Vec<String>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Config {
    #[serde(default)]
    pub hidden: Vec<String>,
}

pub struct NoteView {
    pub note: Option<String>,
    pub source: Option<String>,
    pub stale: bool,
    pub history: Vec<HistoryItem>,
}

pub struct Memory {
    dir: PathBuf,
    data: MemoryFile,
    pub config: Config,
    pub hashes: HashMap<String, String>,
    pub journal: Vec<JournalEntry>,
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(p: &Path) -> Option<T> {
    fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok())
}

impl Memory {
    pub fn load(root: &Path) -> Memory {
        let dir = root.join(".codebase");
        // Carry over notes from the earlier Canopy builds if this project has them.
        let legacy = root.join(".canopy");
        let data = read_json::<MemoryFile>(&dir.join("memory.json"))
            .or_else(|| read_json(&legacy.join("memory.json")))
            .unwrap_or(MemoryFile { version: 1, files: BTreeMap::new(), journeys: None });
        let config = read_json::<Config>(&dir.join("config.json"))
            .or_else(|| read_json(&legacy.join("config.json")))
            .unwrap_or_default();
        let journal = read_json::<Vec<JournalEntry>>(&dir.join("journal.json")).unwrap_or_default();
        Memory { dir, data, config, hashes: HashMap::new(), journal }
    }

    fn write<T: Serialize>(&self, name: &str, value: &T) {
        if fs::create_dir_all(&self.dir).is_err() { return; }
        let path = self.dir.join(name);
        let tmp = self.dir.join(format!("{name}.tmp"));
        if let Ok(text) = serde_json::to_string_pretty(value) {
            if fs::write(&tmp, text).is_ok() { let _ = fs::rename(&tmp, &path); }
        }
    }

    fn save(&self) { self.write("memory.json", &self.data); }
    fn save_config(&self) { self.write("config.json", &self.config); }

    pub fn view(&self, path: &str, hash: Option<&str>) -> NoteView {
        match self.data.files.get(path) {
            Some(e) if e.note.is_some() => NoteView {
                note: e.note.clone(),
                source: e.source.clone(),
                stale: matches!(hash, Some(h) if e.note_hash.as_deref() != Some(h)),
                history: e.history.clone(),
            },
            Some(e) => NoteView { note: None, source: None, stale: false, history: e.history.clone() },
            None => NoteView { note: None, source: None, stale: false, history: vec![] },
        }
    }

    pub fn set_note(&mut self, path: &str, note: &str, source: &str) {
        let hash = self.hashes.get(path).cloned();
        let e = self.data.files.entry(path.to_string()).or_default();
        let text = note.trim();
        if text.is_empty() {
            e.note = None; e.source = None; e.note_hash = None;
        } else {
            e.note = Some(text.chars().take(600).collect());
            e.source = Some(source.to_string());
            e.note_hash = hash;
            e.updated = Some(now_ms());
        }
        self.save();
    }

    pub fn confirm_note(&mut self, path: &str) {
        let hash = self.hashes.get(path).cloned();
        if let Some(e) = self.data.files.get_mut(path) {
            if e.note.is_some() { e.note_hash = hash; e.updated = Some(now_ms()); self.save(); }
        }
    }

    pub fn add_history(&mut self, paths: &[String], task: &str) {
        let at = now_ms();
        for p in paths {
            let e = self.data.files.entry(p.clone()).or_default();
            e.history.insert(0, HistoryItem { task: task.chars().take(300).collect(), at });
            e.history.truncate(10);
        }
        self.save();
    }

    pub fn journeys(&self) -> Option<&serde_json::Value> { self.data.journeys.as_ref() }

    pub fn set_journeys(&mut self, v: Option<serde_json::Value>) {
        self.data.journeys = v;
        self.save();
    }

    /// Adds a save point. Edits within two minutes of the last one are merged into it,
    /// so a burst of saves reads as one change.
    pub fn record(&mut self, added: &[String], modified: &[String], removed: &[String]) {
        if added.is_empty() && modified.is_empty() && removed.is_empty() { return; }
        let at = now_ms();
        let merge = self.journal.last().map(|l| at.saturating_sub(l.at) < 120_000).unwrap_or(false);
        if merge {
            let l = self.journal.last_mut().unwrap();
            for p in added { if !l.added.contains(p) { l.added.push(p.clone()); } l.removed.retain(|x| x != p); }
            for p in modified { if !l.added.contains(p) && !l.modified.contains(p) { l.modified.push(p.clone()); } }
            for p in removed {
                let was_new = l.added.contains(p);
                l.added.retain(|x| x != p); l.modified.retain(|x| x != p);
                if !was_new && !l.removed.contains(p) { l.removed.push(p.clone()); }
            }
            l.at = at;
        } else {
            self.journal.push(JournalEntry { at, added: added.to_vec(), modified: modified.to_vec(), removed: removed.to_vec() });
        }
        for l in self.journal.iter_mut() { l.added.truncate(200); l.modified.truncate(200); l.removed.truncate(200); }
        let n = self.journal.len();
        if n > 500 { self.journal.drain(..n - 500); }
        self.write("journal.json", &self.journal);
    }

    pub fn set_hidden(&mut self, patterns: Vec<String>) {
        let mut out: Vec<String> = Vec::new();
        for p in patterns.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()) {
            if !out.contains(&p) { out.push(p); }
        }
        self.config.hidden = out;
        self.save_config();
    }
}
