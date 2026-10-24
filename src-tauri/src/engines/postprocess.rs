//! Text cleanup shared by every speech-to-text engine. Engines return raw
//! text; `finalize` turns it into what gets shown and pasted.

/// Engine-agnostic cleanup, in order: drop non-speech tags, then apply the
/// personal dictionary.
pub fn finalize(raw: &str) -> String {
    let dictionary = crate::dictionary::load();
    finalize_with(raw, &dictionary)
}

pub fn finalize_with(raw: &str, dictionary: &[crate::dictionary::DictionaryEntry]) -> String {
    let text = crate::engines::whisper::strip_non_speech(raw);
    crate::dictionary::apply(&text, dictionary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictionary::DictionaryEntry;

    #[test]
    fn strips_tags_then_applies_dictionary() {
        let d = vec![DictionaryEntry { id: "1".into(), triggers: vec!["bto".into()], replacement: "BTO".into() }];
        assert_eq!(finalize_with("[BLANK_AUDIO] I love bto (music)", &d), "I love BTO");
        assert_eq!(finalize_with("[BLANK_AUDIO]", &d), "");
    }
}
