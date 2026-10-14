//! Turns structure into plain-language understanding.
//! A "feature" is a word from the project's own vocabulary (bill, payment, login,
//! recorder…) that shows up in several files, ideally across different layers
//! (screens, server, logic, data). That vertical slice is how a non-programmer
//! thinks about an app: "where is the payments stuff?".

use crate::analyze::FileInfo;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub fn features(infos: &BTreeMap<String, FileInfo>, imports_of: &HashMap<String, Vec<String>>) -> Vec<Value> {
    let candidates: Vec<(&String, &FileInfo)> = infos
        .iter()
        .filter(|(_, i)| i.kind == "code" && !i.generated && !matches!(i.layer, "setup" | "docs" | "media"))
        .collect();
    let total = candidates.len().max(1);
    let limit = (total as f64 * 0.4).max(6.0) as usize;

    let mut by_word: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
    for (path, info) in &candidates {
        for w in &info.words { by_word.entry(w.as_str()).or_default().push(path); }
    }

    let mut scored: Vec<(usize, &str, Vec<&String>)> = by_word
        .into_iter()
        .filter_map(|(w, files)| {
            let layers: BTreeSet<&str> = files.iter().map(|f| infos[*f].layer).collect();
            let ok = files.len() >= 2 && files.len() <= limit && (layers.len() >= 2 || files.len() >= 3);
            ok.then(|| (files.len() + 3 * layers.len(), w, files))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));

    let mut seen_sets: Vec<BTreeSet<&String>> = Vec::new();
    let mut out = Vec::new();
    for (_, word, files) in scored {
        let set: BTreeSet<&String> = files.iter().copied().collect();
        // Skip a word whose files are (almost) the same as a stronger feature's.
        if seen_sets.iter().any(|s| { let common = s.intersection(&set).count(); common * 10 >= set.len() * 9 }) { continue; }
        let mut layers: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
        for f in &files { layers.entry(infos[*f].layer).or_default().push(f); }
        let internal = files.iter().map(|f| imports_of.get(*f).map(|d| d.iter().filter(|x| set.contains(x)).count()).unwrap_or(0)).sum::<usize>();
        out.push(json!({ "name": word, "files": files, "layers": layers, "links": internal }));
        seen_sets.push(set);
        if out.len() >= 16 { break; }
    }
    out
}
