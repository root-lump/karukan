//! InputBuffer: composed hiragana text with cursor.
//!
//! This struct bundles `text` and `cursor_pos`
//! which are always operated on together.
//!
//! Alongside the kana it keeps the *raw* keystrokes that produced each run of
//! kana ([`RawSpan`]), so the alphanumeric function keys (F9 / F10) can turn
//! `あいう` back into the `aiu` the user actually typed. Modelled after mozc's
//! `CharChunk`, which likewise pairs a raw string with its conversion.

/// One insertion's worth of kana together with the keystrokes that produced
/// it: `kya` → `きゃ` is a single span of `kana_len` 2 and `raw` `"kya"`.
///
/// Spans are only ever created whole. When an edit lands *inside* a span
/// there is no principled way to split the raw across the kana, so the span
/// is dissolved into one-kana spans whose raw is the kana itself
/// ([`InputBuffer::split_to_singles`]) — the raw is lost, and F9/F10 fall
/// back to showing kana for that character.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawSpan {
    /// Number of kana characters this span covers (always ≥ 1).
    kana_len: usize,
    /// The keystrokes that produced them (e.g. `"kya"`).
    raw: String,
}

/// Composed input buffer with cursor.
pub(super) struct InputBuffer {
    /// Composed hiragana text (source of truth)
    pub text: String,
    /// Cursor position (in characters, not bytes)
    pub cursor_pos: usize,
    /// Raw keystrokes behind `text`, in order. The `kana_len` values always
    /// sum to `text.chars().count()`.
    spans: Vec<RawSpan>,
}

impl InputBuffer {
    /// Create a new empty buffer.
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
            spans: Vec::new(),
        }
    }

    /// Clear the buffer (text, cursor, raw spans).
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor_pos = 0;
        self.spans.clear();
    }

    /// Insert `text` at the current cursor position, recording `raw` as the
    /// keystrokes that produced it.
    ///
    /// Callers that have no meaningful raw (e.g. rebuilding the buffer from a
    /// reading after a cancelled conversion) pass the kana itself.
    pub fn insert(&mut self, text: &str, raw: &str) {
        if text.is_empty() {
            return;
        }
        // Resolve the span boundary before touching `self.text`:
        // `split_to_singles` reads the kana out of the buffer, so it must see
        // the text as it was before the insertion.
        let span_index = self.span_index_at_boundary(self.cursor_pos);

        let byte_pos = self
            .text
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        self.text.insert_str(byte_pos, text);
        let char_count = text.chars().count();

        self.spans.insert(
            span_index,
            RawSpan {
                kana_len: char_count,
                raw: raw.to_string(),
            },
        );

        self.cursor_pos += char_count;
        self.debug_assert_spans();
    }

    /// Replace the whole buffer with `text`, giving every character itself as
    /// its raw (one single-kana span each) and leaving the cursor at the end.
    ///
    /// This is the raw-less entry point, used where a reading is rebuilt from
    /// scratch and the keystrokes behind it are gone: entering conversion,
    /// cancelling back to composing, resuming an unconverted tail, and segment
    /// navigation. F9/F10 then show kana for those characters.
    pub fn set_text_kana_raw(&mut self, text: &str) {
        self.clear();
        for c in text.chars() {
            self.spans.push(RawSpan {
                kana_len: 1,
                raw: c.to_string(),
            });
        }
        self.text = text.to_string();
        self.cursor_pos = self.text.chars().count();
        self.debug_assert_spans();
    }

    /// Index in `spans` where a span starting at kana position `char_pos`
    /// belongs, dissolving the span that `char_pos` falls inside (if any) so
    /// the position becomes a span boundary.
    fn span_index_at_boundary(&mut self, char_pos: usize) -> usize {
        let mut offset = 0;
        for i in 0..self.spans.len() {
            if offset == char_pos {
                return i;
            }
            let span_len = self.spans[i].kana_len;
            if char_pos < offset + span_len {
                // Inside this span: dissolve it so `char_pos` lands on a
                // boundary of the resulting one-kana spans.
                self.split_to_singles(i);
                return i + (char_pos - offset);
            }
            offset += span_len;
        }
        self.spans.len()
    }

    /// Replace the span at `index` with one span per kana, each carrying the
    /// kana itself as its raw. Used when an edit lands inside a multi-kana
    /// span, where the original keystrokes can no longer be attributed to
    /// individual characters.
    fn split_to_singles(&mut self, index: usize) {
        let kana_len = self.spans[index].kana_len;
        if kana_len <= 1 {
            return;
        }
        let span_start = self.spans[..index]
            .iter()
            .map(|s| s.kana_len)
            .sum::<usize>();
        let singles: Vec<RawSpan> = self
            .text
            .chars()
            .skip(span_start)
            .take(kana_len)
            .map(|c| RawSpan {
                kana_len: 1,
                raw: c.to_string(),
            })
            .collect();
        self.spans.splice(index..=index, singles);
    }

    /// Drop the kana at `char_pos` from the raw spans, dissolving a
    /// multi-kana span first so exactly one character is removed.
    fn remove_span_at(&mut self, char_pos: usize) {
        let mut offset = 0;
        for i in 0..self.spans.len() {
            let span_len = self.spans[i].kana_len;
            if char_pos < offset + span_len {
                if span_len > 1 {
                    self.split_to_singles(i);
                    self.spans.remove(i + (char_pos - offset));
                } else {
                    self.spans.remove(i);
                }
                return;
            }
            offset += span_len;
        }
    }

    /// The raw keystrokes behind the whole buffer (e.g. `"kya"` for `きゃ`).
    pub fn raw_text(&self) -> String {
        self.spans.iter().map(|s| s.raw.as_str()).collect()
    }

    /// The raw keystrokes behind the first `kana_len` characters, or `None`
    /// when that boundary falls inside a span (the raw cannot be split).
    pub fn raw_prefix(&self, kana_len: usize) -> Option<String> {
        let mut offset = 0;
        let mut raw = String::new();
        for span in &self.spans {
            if offset == kana_len {
                return Some(raw);
            }
            if offset + span.kana_len > kana_len {
                return None;
            }
            raw.push_str(&span.raw);
            offset += span.kana_len;
        }
        (offset == kana_len).then_some(raw)
    }

    /// Remove the character at the given character position.
    pub fn remove_char_at(&mut self, char_pos: usize) -> Option<char> {
        let (byte_start, removed) = self.text.char_indices().nth(char_pos)?;
        let byte_end = self
            .text
            .char_indices()
            .nth(char_pos + 1)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        // Span bookkeeping first: `split_to_singles` reads the kana out of
        // `self.text`, so it must run before the character is spliced out.
        self.remove_span_at(char_pos);
        self.text.replace_range(byte_start..byte_end, "");
        self.debug_assert_spans();
        Some(removed)
    }

    /// Remove the character before the cursor.
    pub fn remove_char_before_cursor(&mut self) -> Option<char> {
        if self.cursor_pos == 0 {
            return None;
        }
        self.cursor_pos -= 1;
        self.remove_char_at(self.cursor_pos)
    }

    /// Remove the character at the cursor position (delete key).
    pub fn remove_char_at_cursor(&mut self) -> Option<char> {
        self.remove_char_at(self.cursor_pos)
    }

    /// The span lengths must always account for exactly the buffered text.
    fn debug_assert_spans(&self) {
        debug_assert_eq!(
            self.spans.iter().map(|s| s.kana_len).sum::<usize>(),
            self.text.chars().count(),
            "raw span lengths out of sync with buffer text"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(pairs: &[(&str, &str)]) -> InputBuffer {
        let mut b = InputBuffer::new();
        for (text, raw) in pairs {
            b.insert(text, raw);
        }
        b
    }

    #[test]
    fn raw_text_concatenates_spans() {
        let b = buf(&[("きゃ", "kya"), ("く", "ku")]);
        assert_eq!(b.text, "きゃく");
        assert_eq!(b.raw_text(), "kyaku");
    }

    #[test]
    fn removing_inside_a_span_falls_back_to_kana() {
        let mut b = buf(&[("きゃ", "kya")]);
        // Backspace over `ゃ` leaves `き`, whose keystrokes can no longer be
        // attributed — the kana itself becomes the raw.
        b.remove_char_before_cursor();
        assert_eq!(b.text, "き");
        assert_eq!(b.raw_text(), "き");
    }

    #[test]
    fn removing_a_whole_span_keeps_neighbours_intact() {
        let mut b = buf(&[("きゃ", "kya"), ("く", "ku")]);
        b.remove_char_before_cursor();
        assert_eq!(b.text, "きゃ");
        assert_eq!(b.raw_text(), "kya");
    }

    #[test]
    fn inserting_inside_a_span_dissolves_it() {
        let mut b = buf(&[("きゃ", "kya")]);
        b.cursor_pos = 1;
        b.insert("あ", "a");
        assert_eq!(b.text, "きあゃ");
        assert_eq!(b.raw_text(), "きaゃ");
    }

    #[test]
    fn inserting_at_a_span_boundary_keeps_raw() {
        let mut b = buf(&[("きゃ", "kya"), ("く", "ku")]);
        b.cursor_pos = 2;
        b.insert("あ", "a");
        assert_eq!(b.text, "きゃあく");
        assert_eq!(b.raw_text(), "kyaaku");
    }

    #[test]
    fn raw_prefix_only_splits_on_span_boundaries() {
        let b = buf(&[("きゃ", "kya"), ("く", "ku")]);
        assert_eq!(b.raw_prefix(0), Some(String::new()));
        assert_eq!(b.raw_prefix(1), None); // inside `kya`
        assert_eq!(b.raw_prefix(2), Some("kya".to_string()));
        assert_eq!(b.raw_prefix(3), Some("kyaku".to_string()));
    }

    #[test]
    fn clear_resets_spans() {
        let mut b = buf(&[("きゃ", "kya")]);
        b.clear();
        assert_eq!(b.raw_text(), "");
        b.insert("あ", "a");
        assert_eq!(b.raw_text(), "a");
    }
}
