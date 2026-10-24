//! Text preparation for speech: strip markup that should not be read aloud
//! and split long text into sentence-sized chunks, so playback can start
//! after the first chunk instead of after the whole selection.

/// Target maximum characters per chunk. Kokoro fails past ~510 tokens, and
/// shorter chunks make the first audio arrive sooner.
pub const MAX_CHUNK_CHARS: usize = 300;

/// Remove Markdown syntax that would otherwise be spoken ("asterisk", URLs).
pub fn clean_for_speech(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_code_block = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            out.push('\n');
            continue;
        }
        if in_code_block {
            continue;
        }
        out.push_str(strip_line_prefix(trimmed));
        out.push('\n');
    }
    let out = strip_links(&out);
    out.chars().filter(|c| !matches!(c, '*' | '_' | '`' | '~' | '|')).collect()
}

/// Drop heading, quote and list markers at the start of a line.
fn strip_line_prefix(line: &str) -> &str {
    let rest = line.trim_start_matches('#');
    if rest.len() != line.len() && rest.starts_with(' ') {
        return rest.trim_start();
    }
    for marker in ["> ", "- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return rest;
        }
    }
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        if let Some(rest) = line[digits..].strip_prefix(". ") {
            return rest;
        }
    }
    line
}

/// `[text](url)` -> `text`, `[[page|alias]]` -> `alias`, `[[page]]` -> `page`,
/// and bare `http(s)://` URLs are dropped.
fn strip_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(inner) = rest.strip_prefix("[[") {
            if let Some(end) = inner.find("]]") {
                let link = &inner[..end];
                out.push_str(link.rsplit('|').next().unwrap_or(link));
                rest = &inner[end + 2..];
                continue;
            }
        }
        let image = rest.starts_with("![");
        if let Some(inner) = rest.strip_prefix(if image { "![" } else { "[" }) {
            if let Some(close) = inner.find("](") {
                if let Some(end) = inner[close + 2..].find(')') {
                    out.push_str(&inner[..close]);
                    rest = &inner[close + 2 + end + 1..];
                    continue;
                }
            }
        }
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            rest = &rest[end..];
            continue;
        }
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

/// Split text into chunks of at most `max_chars`, breaking at paragraph and
/// sentence boundaries, then at commas, then at spaces.
pub fn split_for_speech(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.split_whitespace().collect::<Vec<_>>().join(" ");
        if paragraph.is_empty() {
            continue;
        }
        let mut current = String::new();
        for sentence in split_sentences(&paragraph) {
            for piece in split_long(sentence, max_chars) {
                if !current.is_empty() && current.len() + 1 + piece.len() > max_chars {
                    chunks.push(std::mem::take(&mut current));
                }
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(piece);
            }
        }
        if !current.is_empty() {
            chunks.push(current);
        }
    }
    chunks
}

fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let at_end = matches!(b, b'.' | b'!' | b'?' | b';' | b':')
            && bytes.get(i + 1).is_some_and(|n| *n == b' ');
        if at_end {
            sentences.push(text[start..=i].trim());
            start = i + 1;
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        sentences.push(tail);
    }
    sentences
}

fn split_long(sentence: &str, max_chars: usize) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = sentence.trim();
    while rest.len() > max_chars {
        let mut limit = max_chars;
        while !rest.is_char_boundary(limit) {
            limit -= 1;
        }
        let head = &rest[..limit];
        let cut = head
            .rfind(", ")
            .map(|i| i + 1)
            .or_else(|| head.rfind(' '))
            .filter(|&i| i > 0)
            .unwrap_or(limit);
        pieces.push(rest[..cut].trim());
        rest = rest[cut..].trim_start();
    }
    if !rest.is_empty() {
        pieces.push(rest);
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markdown() {
        let md = "# Title\n\nThis is **bold** and a [link](https://x.com).\n- item one\n> quoted\n1. first";
        let clean = clean_for_speech(md);
        assert!(clean.contains("Title"));
        assert!(clean.contains("This is bold and a link."));
        assert!(clean.contains("item one") && clean.contains("quoted") && clean.contains("first"));
        assert!(!clean.contains('#') && !clean.contains('*') && !clean.contains("https"));
    }

    #[test]
    fn strips_wikilinks_code_and_bare_urls() {
        let clean = clean_for_speech("See [[Page|the alias]] and [[Other]].\n```\ncode()\n```\nVisit https://a.b/c now");
        assert!(clean.contains("See the alias and Other."));
        assert!(!clean.contains("code()"));
        assert!(clean.contains("Visit  now"));
    }

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(clean_for_speech("It's 10:30, isn't it?").trim(), "It's 10:30, isn't it?");
    }

    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(split_for_speech("Hello there. How are you?", 300), vec!["Hello there. How are you?"]);
    }

    #[test]
    fn paragraphs_split_and_whitespace_collapses() {
        let chunks = split_for_speech("One.\n\n\n  Two\nlines.  ", 300);
        assert_eq!(chunks, vec!["One.", "Two lines."]);
    }

    #[test]
    fn long_text_respects_limit_and_keeps_every_word() {
        let text = "This is a sentence that goes on for a while. ".repeat(40);
        let chunks = split_for_speech(&text, 120);
        assert!(chunks.len() > 10);
        assert!(chunks.iter().all(|c| c.len() <= 120), "{chunks:?}");
        let rejoined = chunks.join(" ");
        assert_eq!(rejoined.split_whitespace().count(), text.split_whitespace().count());
    }

    #[test]
    fn run_on_sentence_breaks_at_commas_then_spaces() {
        let text = format!("{}{}", "word, ".repeat(100), "x".repeat(50));
        let chunks = split_for_speech(&text, 80);
        assert!(chunks.iter().all(|c| c.len() <= 80));
        assert!(chunks[0].ends_with(','));
    }

    #[test]
    fn unicode_does_not_panic() {
        let text = "é".repeat(500);
        let chunks = split_for_speech(&text, 101);
        assert_eq!(chunks.concat().chars().count(), 500);
    }

    #[test]
    fn empty_input() {
        assert!(split_for_speech("", 300).is_empty());
        assert!(split_for_speech("  \n\n ", 300).is_empty());
    }
}
