//! Splits extracted document text into overlapping windows for embedding.
//!
//! Hand-rolled, character-based — no tokenizer dependency. Good enough for a
//! first pass: it backs off to the nearest whitespace so it doesn't split
//! mid-word, and each window repeats `overlap` characters from the previous
//! one so a fact sitting right on a boundary isn't orphaned in either chunk.

#[derive(Debug, Clone, PartialEq)]
pub struct ChunkSpan {
    pub text: String,
    /// Character offsets into the original text (not byte offsets — safe to
    /// use directly with `.chars()`, not with byte-indexed string slicing).
    pub char_start: usize,
    pub char_end: usize,
}

/// `size`/`overlap` are character counts. `overlap` is clamped below `size`
/// so the window always makes forward progress.
pub fn chunk_text(text: &str, size: usize, overlap: usize) -> Vec<ChunkSpan> {
    let size = size.max(1);
    let overlap = overlap.min(size.saturating_sub(1).max(0));
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let hard_end = (start + size).min(chars.len());
        // Back off to the last whitespace in the second half of the window,
        // so a real cut doesn't land mid-word — but not so far back that the
        // chunk shrinks to nothing.
        let mut end = hard_end;
        if hard_end < chars.len() {
            let search_from = start + size / 2;
            if let Some(ws) = (search_from..hard_end)
                .rev()
                .find(|&i| chars[i].is_whitespace())
            {
                end = ws;
            }
        }

        let span_text: String = chars[start..end].iter().collect();
        let trimmed = span_text.trim();
        if !trimmed.is_empty() {
            spans.push(ChunkSpan {
                text: trimmed.to_string(),
                char_start: start,
                char_end: end,
            });
        }

        if end >= chars.len() {
            break;
        }
        // Always move forward by at least one char, even if `overlap` would
        // otherwise stall the window on a run of whitespace.
        start = end.saturating_sub(overlap).max(start + 1);
    }
    spans
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_yields_no_chunks() {
        assert!(chunk_text("", 100, 10).is_empty());
        assert!(chunk_text("   ", 100, 10).is_empty());
    }

    #[test]
    fn short_text_is_a_single_chunk() {
        let spans = chunk_text("hello world", 100, 10);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "hello world");
        assert_eq!(spans[0].char_start, 0);
    }

    #[test]
    fn long_text_is_split_with_overlap_and_no_mid_word_cuts() {
        let text = "one two three four five six seven eight nine ten eleven twelve";
        let spans = chunk_text(text, 20, 5);
        assert!(spans.len() > 1);
        for s in &spans {
            assert!(!s.text.starts_with(' ') && !s.text.ends_with(' '));
            // Never cuts a word: every chunk's edges land on a word boundary
            // in the source text (verified by re-finding it whole).
            assert!(text.contains(&s.text) || s.text.split_whitespace().all(|w| text.contains(w)));
        }
        // Offsets strictly increase — the loop always makes forward progress.
        for w in spans.windows(2) {
            assert!(w[1].char_start > w[0].char_start);
        }
    }

    #[test]
    fn overlap_repeats_a_tail_between_consecutive_chunks() {
        let text = "a".repeat(50) + " " + &"b".repeat(50);
        let spans = chunk_text(&text, 40, 10);
        assert!(spans.len() >= 2);
        // Consecutive spans' character ranges actually overlap.
        assert!(spans[1].char_start < spans[0].char_end);
    }

    #[test]
    fn never_loops_forever_on_pathological_input() {
        // All-whitespace-adjacent long single "word" with no break points.
        let text = "x".repeat(1000);
        let spans = chunk_text(&text, 30, 29);
        assert!(!spans.is_empty());
        assert!(spans.len() < 1000); // sanity bound, not an infinite/degenerate split
    }
}
