//! Text cleanup shared by every speech-to-text engine. Engines return raw
//! text; `finalize` turns it into what gets shown and pasted.

/// Engine-agnostic cleanup, in order: drop non-speech tags, remove filler
/// words (if enabled), then apply the personal dictionary.
pub fn finalize(raw: &str) -> String {
    let dictionary = crate::dictionary::load();
    finalize_with(raw, &dictionary)
}

pub fn finalize_with(raw: &str, dictionary: &[crate::dictionary::DictionaryEntry]) -> String {
    // Default on; if settings can't be read, behave like the default.
    let fillers = crate::commands::settings::get_settings().map(|s| s.remove_fillers).unwrap_or(true);
    finalize_opts(raw, dictionary, fillers)
}

/// `finalize_with` with the filler switch explicit (no settings access).
pub fn finalize_opts(raw: &str, dictionary: &[crate::dictionary::DictionaryEntry], remove_fillers: bool) -> String {
    let text = crate::engines::whisper::strip_non_speech(raw);
    let text = if remove_fillers { strip_fillers(&text) } else { text };
    crate::dictionary::apply(&text, dictionary)
}

/// Is this (punctuation-stripped, lowercased) word a hesitation sound?
/// Exact list: um, uh, erm, er, ah, hmm, mm, uhm - plus drawn-out spellings
/// (umm, uhh, hmmm, mmm). Real words ("err", "ahh-choo", "umbrella") are not
/// matched because the whole word must be one of these shapes.
fn is_filler(w: &str) -> bool {
    // base plus extra copies of its last letter: "um" -> "umm", "ummm"
    let drawn_out = |base: &str| {
        let last = base.chars().last().unwrap_or(' ');
        w.len() > base.len() && w.starts_with(base) && w[base.len()..].chars().all(|c| c == last)
    };
    matches!(w, "um" | "uh" | "erm" | "er" | "ah" | "hmm" | "mm" | "uhm")
        || ["um", "uh", "uhm", "hmm", "mm"].iter().any(|b| drawn_out(b))
}

const OPENERS: &[char] = &['"', '\'', '\u{201C}', '\u{2018}', '(', '['];
const CLOSERS: &[char] = &['"', '\'', '\u{201D}', '\u{2019}', ')', ']'];

fn ends_sentence(s: &str) -> bool {
    s.trim_end_matches(CLOSERS).chars().last().is_some_and(|c| matches!(c, '.' | '?' | '!' | '\u{2026}'))
}

fn capitalize_first(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut done = false;
    for c in s.chars() {
        if !done && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            done = true;
        } else {
            out.push(c);
            if !done && !OPENERS.contains(&c) {
                done = true; // starts with a digit/symbol: leave it alone
            }
        }
    }
    out
}

struct Parts<'a> {
    lead: &'a str,
    core: &'a str,
    trail: &'a str,
}

/// Split a token into leading punctuation, the word itself, trailing punctuation.
fn split(tok: &str) -> Parts<'_> {
    let Some(start) = tok.find(|c: char| c.is_alphanumeric()) else {
        return Parts { lead: tok, core: "", trail: "" };
    };
    let end = tok
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_alphanumeric())
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(tok.len());
    Parts { lead: &tok[..start], core: &tok[start..end], trail: &tok[end..] }
}

/// Remove standalone hesitation words ("um", "uh", "erm", ...) and tidy what
/// is left: dangling commas, doubled punctuation, and the capital letter a
/// sentence-initial filler used to carry. Text without fillers is returned
/// byte-for-byte unchanged.
pub fn strip_fillers(text: &str) -> String {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if !tokens.iter().any(|t| is_filler(&split(t).core.to_lowercase())) {
        return text.to_string();
    }

    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut prefix = String::new(); // opening quote/paren owned by a removed filler
    let mut capitalize_next = false;
    let mut last_was_filler = false;
    for tok in &tokens {
        let p = split(tok);
        if !is_filler(&p.core.to_lowercase()) {
            let mut word = format!("{prefix}{tok}");
            prefix.clear();
            if capitalize_next {
                word = capitalize_first(&word);
                capitalize_next = false;
            }
            out.push(word);
            last_was_filler = false;
            continue;
        }
        last_was_filler = true;
        let capitalized = p.core.chars().next().is_some_and(|c| c.is_uppercase());
        let sentence_start = out.last().is_none_or(|prev| ends_sentence(prev));
        prefix.extend(p.lead.chars().filter(|c| OPENERS.contains(c)));
        let mut closers: String = p.trail.chars().filter(|c| CLOSERS.contains(c)).collect();
        if !closers.is_empty() && !p.lead.is_empty() && prefix.chars().any(|c| OPENERS.contains(&c)) && p.lead.chars().any(|c| OPENERS.contains(&c)) {
            // "(uh)" - the filler owned both brackets: drop the pair.
            closers.clear();
            prefix.clear();
        }
        let ellipsis = p.trail.contains("...") || p.trail.contains('\u{2026}');
        let terminal = if ellipsis { None } else { p.trail.chars().find(|c| matches!(c, '.' | '?' | '!')) };
        let comma = p.trail.contains([',', ';', ':', '\u{2014}', '\u{2013}', '-']);
        if let Some(prev) = out.last_mut() {
            match terminal {
                Some(t) if !ends_sentence(prev) => {
                    let trimmed = prev.trim_end_matches([',', ';', ':']).to_string();
                    *prev = format!("{trimmed}{t}");
                    capitalize_next = true;
                }
                Some(_) => capitalize_next = true,
                // "I, uh, think" -> "I think": both neighbours had a comma.
                None if comma && prev.ends_with(',') => {
                    prev.pop();
                }
                None => {}
            }
            prev.push_str(&closers);
        }
        if sentence_start && capitalized {
            capitalize_next = true;
        }
    }
    if last_was_filler {
        if let Some(prev) = out.last_mut() {
            let closers: String = prev.chars().rev().take_while(|c| CLOSERS.contains(c)).collect::<Vec<_>>().into_iter().rev().collect();
            let mut body = prev[..prev.len() - closers.len()].to_string();
            while body.ends_with([',', ';', ':']) {
                body.pop();
            }
            *prev = body + &closers;
        }
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictionary::DictionaryEntry;

    #[test]
    fn strips_tags_then_applies_dictionary() {
        let d = vec![DictionaryEntry { id: "1".into(), triggers: vec!["bto".into()], replacement: "BTO".into() }];
        assert_eq!(finalize_opts("[BLANK_AUDIO] I love bto (music)", &d, true), "I love BTO");
        assert_eq!(finalize_opts("[BLANK_AUDIO]", &d, true), "");
    }

    #[test]
    fn fillers_come_out_before_the_dictionary_runs() {
        let d = vec![DictionaryEntry { id: "1".into(), triggers: vec!["mass lab".into()], replacement: "MASS Lab".into() }];
        assert_eq!(finalize_opts("Um, the mass lab, uh, ships", &d, true), "The MASS Lab ships");
        // Switch off: text untouched by the filler pass.
        assert_eq!(finalize_opts("Um, the mass lab", &d, false), "Um, the MASS Lab");
    }

    fn f(s: &str) -> String {
        strip_fillers(s)
    }

    #[test]
    fn leading_filler_drops_and_recapitalises() {
        assert_eq!(f("Um, so we go"), "So we go");
        assert_eq!(f("Uh so we go"), "So we go");
        assert_eq!(f("um, so we go"), "so we go"); // lowercase stays lowercase
        assert_eq!(f("Um... so we go"), "So we go");
        assert_eq!(f("Hmm. Let me think"), "Let me think");
    }

    #[test]
    fn mid_sentence_filler_with_commas() {
        assert_eq!(f("I, uh, think so"), "I think so");
        assert_eq!(f("I think, um, we should go"), "I think we should go");
        assert_eq!(f("I think uh we should go"), "I think we should go");
        assert_eq!(f("We went, erm, to the shop"), "We went to the shop");
        assert_eq!(f("It was... um... fine"), "It was... fine");
    }

    #[test]
    fn trailing_filler() {
        assert_eq!(f("That's it, um"), "That's it");
        assert_eq!(f("That's it, um."), "That's it.");
        assert_eq!(f("Is that it, uh?"), "Is that it?");
        assert_eq!(f("Done. Um."), "Done.");
    }

    #[test]
    fn filler_ending_a_sentence_mid_text() {
        assert_eq!(f("We left early, um. Then it rained"), "We left early. Then it rained");
        assert_eq!(f("We left early um. then it rained"), "We left early. Then it rained");
    }

    #[test]
    fn consecutive_and_only_fillers() {
        assert_eq!(f("Um, uh, so yes"), "So yes");
        assert_eq!(f("um um um"), "");
        assert_eq!(f("Hmm."), "");
        assert_eq!(f("Uh, um, ah, hmm, mm, erm, er, uhm"), "");
    }

    #[test]
    fn each_listed_filler_is_removed_case_insensitively() {
        for w in ["um", "uh", "erm", "er", "ah", "hmm", "mm", "uhm", "UM", "Uh", "HMM", "Erm"] {
            assert_eq!(f(&format!("well {w} fine")), "well fine", "{w}");
            assert_eq!(f(&format!("well {w}, fine")), "well fine", "{w},");
            assert_eq!(f(&format!("well {w}. fine")), "well. Fine", "{w}.");
            assert_eq!(f(&format!("well {w}... fine")), "well fine", "{w}...");
        }
    }

    #[test]
    fn drawn_out_spellings() {
        assert_eq!(f("well umm fine"), "well fine");
        assert_eq!(f("well uhhh fine"), "well fine");
        assert_eq!(f("well hmmm fine"), "well fine");
        assert_eq!(f("well mmm fine"), "well fine");
    }

    #[test]
    fn real_words_are_untouched() {
        for s in [
            "umbrella", "Uhura", "her", "summer", "hummus", "error", "err on the side", "ahead", "alarm",
            "Hmmm-hmm", "uh-huh", "mm-hmm", "Erma", "Ahmed", "ermine", "humming", "chum",
            "The summer umbrella is on her desk",
            "Lieutenant Uhura said ahead",
        ] {
            assert_eq!(f(s), s, "{s}");
        }
    }

    #[test]
    fn untouched_text_keeps_its_exact_spacing() {
        assert_eq!(f("hello  world\nnext line"), "hello  world\nnext line");
        assert_eq!(f(""), "");
    }

    #[test]
    fn quotes_around_a_filler_move_to_neighbours() {
        assert_eq!(f("He said \"um, hello\""), "He said \"hello\"");
        assert_eq!(f("He said \"hello, um\""), "He said \"hello\"");
        assert_eq!(f("Say (uh) hello"), "Say hello");
    }

    #[test]
    fn numbers_and_other_punctuation_survive() {
        assert_eq!(f("Um, 3 apples, uh, and 4 pears."), "3 apples and 4 pears.");
        assert_eq!(f("So, um, what's up?"), "So what's up?");
    }
}
