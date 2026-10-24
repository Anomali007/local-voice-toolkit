//! Personal dictionary: words and phrases Whisper gets wrong or spells in a
//! standard way, mapped to how the user wants them written. Each entry has
//! one or more spoken triggers and the replacement text, e.g. triggers
//! ["mass lab", "the mass lab"] -> "MASS Lab".
//!
//! The replacements are also passed to Whisper as vocabulary hints, which
//! makes it more likely to hear unusual words right in the first place.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DictionaryEntry {
    pub id: String,
    pub triggers: Vec<String>,
    pub replacement: String,
}

fn dictionary_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.blahcubed.app")
        .join("dictionary.json")
}

pub fn load() -> Vec<DictionaryEntry> {
    std::fs::read_to_string(dictionary_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(entries: &[DictionaryEntry]) -> Result<(), String> {
    let path = dictionary_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Drop empty triggers/entries and trim whitespace.
fn clean(entries: Vec<DictionaryEntry>) -> Vec<DictionaryEntry> {
    entries
        .into_iter()
        .filter_map(|mut e| {
            e.replacement = e.replacement.trim().to_string();
            e.triggers = e
                .triggers
                .iter()
                .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|t| !t.is_empty())
                .collect();
            (!e.replacement.is_empty()).then_some(e)
        })
        .collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '’'
}

/// Case-insensitive match of `needle` at `hay[at..]`, treating any run of
/// whitespace in the haystack as matching a single space in the needle.
/// Returns the matched length in chars.
fn match_at(hay: &[char], at: usize, needle: &[char]) -> Option<usize> {
    let mut i = at;
    let mut j = 0;
    while j < needle.len() {
        let n = needle[j];
        if n == ' ' {
            if i >= hay.len() || !hay[i].is_whitespace() {
                return None;
            }
            while i < hay.len() && hay[i].is_whitespace() {
                i += 1;
            }
            j += 1;
            continue;
        }
        let h = *hay.get(i)?;
        if !h.to_lowercase().eq(n.to_lowercase()) {
            return None;
        }
        i += 1;
        j += 1;
    }
    Some(i - at)
}

/// Replace every whole-word occurrence of each trigger with its replacement.
/// Longer triggers win, so "the mass lab" is matched before "mass lab".
pub fn apply(text: &str, entries: &[DictionaryEntry]) -> String {
    let mut rules: Vec<(Vec<char>, &str)> = entries
        .iter()
        .flat_map(|e| {
            e.triggers
                .iter()
                .filter(|t| !t.trim().is_empty())
                .map(move |t| (t.trim().chars().collect::<Vec<_>>(), e.replacement.as_str()))
        })
        .collect();
    if rules.is_empty() {
        return text.to_string();
    }
    rules.sort_by_key(|rule| std::cmp::Reverse(rule.0.len()));

    let hay: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    'outer: while i < hay.len() {
        let at_word_start = i == 0 || !is_word_char(hay[i - 1]);
        if at_word_start {
            for (needle, replacement) in &rules {
                if let Some(len) = match_at(&hay, i, needle) {
                    let end = i + len;
                    let at_word_end = end >= hay.len() || !is_word_char(hay[end]);
                    if at_word_end && len > 0 {
                        out.push_str(replacement);
                        i = end;
                        continue 'outer;
                    }
                }
            }
        }
        out.push(hay[i]);
        i += 1;
    }
    out
}

/// Words to bias Whisper toward, as an initial prompt. Empty when there is
/// no dictionary.
pub fn vocabulary_prompt(entries: &[DictionaryEntry]) -> Option<String> {
    let words: Vec<&str> = entries.iter().map(|e| e.replacement.as_str()).filter(|r| !r.is_empty()).collect();
    if words.is_empty() {
        return None;
    }
    // Whisper reads the prompt as preceding text; keep it short.
    let mut prompt = String::from("Vocabulary: ");
    for (i, w) in words.iter().enumerate() {
        if prompt.len() + w.len() > 600 {
            break;
        }
        if i > 0 {
            prompt.push_str(", ");
        }
        prompt.push_str(w);
    }
    prompt.push('.');
    Some(prompt)
}

/// Read FluidVoice's dictionary from its preferences, if it is installed.
/// (`defaults read` truncates long data, so export the plist and extract.)
fn read_fluidvoice() -> Result<Vec<DictionaryEntry>, String> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("defaults export com.FluidApp.app - | plutil -extract CustomDictionaryEntries raw -o - - | base64 -D")
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err("FluidVoice dictionary not found".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("Unexpected FluidVoice dictionary format: {}", e))
}

/// Add entries not already present (same replacement, case-insensitive),
/// merging triggers into existing entries.
fn merge(mut current: Vec<DictionaryEntry>, incoming: Vec<DictionaryEntry>) -> (Vec<DictionaryEntry>, usize) {
    let mut added = 0;
    for entry in clean(incoming) {
        if let Some(existing) = current
            .iter_mut()
            .find(|e| e.replacement.eq_ignore_ascii_case(&entry.replacement))
        {
            for t in entry.triggers {
                if !existing.triggers.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
                    existing.triggers.push(t);
                }
            }
        } else {
            current.push(entry);
            added += 1;
        }
    }
    (current, added)
}

#[tauri::command]
pub fn get_dictionary() -> Vec<DictionaryEntry> {
    load()
}

#[tauri::command]
pub fn save_dictionary(entries: Vec<DictionaryEntry>) -> Result<Vec<DictionaryEntry>, String> {
    let entries = clean(entries);
    save(&entries)?;
    Ok(entries)
}

/// Preview what the dictionary does to a sentence.
#[tauri::command]
pub fn preview_dictionary(text: String, entries: Vec<DictionaryEntry>) -> String {
    apply(&text, &clean(entries))
}

#[tauri::command]
pub fn fluidvoice_dictionary_available() -> bool {
    read_fluidvoice().map(|e| !e.is_empty()).unwrap_or(false)
}

/// Import FluidVoice's dictionary. Returns the merged list and how many
/// entries were new.
#[tauri::command]
pub fn import_fluidvoice_dictionary() -> Result<(Vec<DictionaryEntry>, usize), String> {
    let (merged, added) = merge(load(), read_fluidvoice()?);
    save(&merged)?;
    Ok((merged, added))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(triggers: &[&str], replacement: &str) -> DictionaryEntry {
        DictionaryEntry {
            id: replacement.into(),
            triggers: triggers.iter().map(|s| s.to_string()).collect(),
            replacement: replacement.into(),
        }
    }

    #[test]
    fn replaces_whole_words_case_insensitively() {
        let d = vec![entry(&["mass lab"], "MASS Lab"), entry(&["bto"], "BTO")];
        assert_eq!(apply("I run the Mass Lab and bto.", &d), "I run the MASS Lab and BTO.");
        // Not inside other words.
        assert_eq!(apply("massive labs and btoken", &d), "massive labs and btoken");
    }

    #[test]
    fn longest_trigger_wins_and_whitespace_is_flexible() {
        let d = vec![entry(&["blah"], "Blah"), entry(&["blah blah blah"], "Blah³")];
        assert_eq!(apply("open blah  blah\nblah now", &d), "open Blah³ now");
        assert_eq!(apply("just blah", &d), "just Blah");
    }

    #[test]
    fn punctuation_and_apostrophes_are_boundaries_correctly() {
        let d = vec![entry(&["mali"], "Mali")];
        assert_eq!(apply("mali, mali's, (mali)", &d), "Mali, mali's, (Mali)");
    }

    #[test]
    fn multiple_triggers_and_unicode() {
        let d = vec![entry(&["k m", "km"], "KnowMessenger"), entry(&["café"], "Café Rio")];
        assert_eq!(apply("KM and k m at CAFÉ", &d), "KnowMessenger and KnowMessenger at Café Rio");
    }

    #[test]
    fn empty_dictionary_is_identity() {
        assert_eq!(apply("unchanged text", &[]), "unchanged text");
        assert_eq!(apply("", &[entry(&["x"], "y")]), "");
    }

    #[test]
    fn clean_drops_blank_entries_and_triggers() {
        let cleaned = clean(vec![entry(&["  a  b ", ""], " AB "), entry(&["x"], "  ")]);
        assert_eq!(cleaned, vec![DictionaryEntry { id: " AB ".into(), triggers: vec!["a b".into()], replacement: "AB".into() }]);
    }

    #[test]
    fn merge_dedupes_by_replacement() {
        let (merged, added) = merge(
            vec![entry(&["mass lab"], "MASS Lab")],
            vec![entry(&["the mass lab", "MASS LAB"], "mass lab"), entry(&["bto"], "BTO")],
        );
        assert_eq!(added, 1);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].triggers, vec!["mass lab", "the mass lab"]);
    }

    #[test]
    fn vocabulary_prompt_lists_replacements() {
        assert_eq!(vocabulary_prompt(&[]), None);
        let p = vocabulary_prompt(&[entry(&["a"], "MASS Lab"), entry(&["b"], "BTO")]).unwrap();
        assert_eq!(p, "Vocabulary: MASS Lab, BTO.");
    }

    #[test]
    fn fluidvoice_format_parses() {
        let json = br#"[{"id":"1","triggers":["mass lab"],"replacement":"MASS Lab"}]"#;
        let parsed: Vec<DictionaryEntry> = serde_json::from_slice(json).unwrap();
        assert_eq!(parsed[0].replacement, "MASS Lab");
    }
}
