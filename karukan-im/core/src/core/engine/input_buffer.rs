//! InputBuffer: a recorded element array plus a caret, with every view
//! derived by evaluation.
//!
//! **The record** is the single source of truth: one element per display
//! character plus `cursor`, the caret as an element index. Typing `kyo` records
//! `[Romaji(k), Romaji(y), Romaji(o)]`, which evaluation re-records as
//! `[Converted(き), Converted(ょ)]` — elements and displayed characters
//! always correspond one to one, so the record can never disagree with
//! what is shown, and the caret is simply an index into both.
//!
//! - [`Element::Romaji`]: one keystroke not yet consumed by a rule (`y`,
//!   `k`, a lone `n`). Shown verbatim; evaluation may later consume it.
//! - [`Element::Converted`]: one settled character — a fired rule's kana,
//!   a passthrough like `1`, or direct input (alphabet/emoji mode). Opaque
//!   to evaluation; it never reverts. It also carries the keystrokes it
//!   came from, so the alphanumeric function keys (F9 / F10) can turn
//!   `あいう` back into the `aiu` that was typed — see [`InputBuffer::raw_text`].
//!
//! **Evaluation** derives everything else: the display, the conversion
//! reading, and the aux romaji tail. After a romaji keystroke is recorded,
//! the Romaji run ending at the cursor is evaluated through the converter:
//! keystrokes a rule consumed are re-recorded as its output. Elements
//! right of the cursor are never touched, so nothing combines across the
//! caret, and the caret moves without settling anything — `[Romaji(k),
//! Romaji(y), Converted(K)]` plus `o` typed before the `K` evaluates to
//! 「きょK」.
//!
//! Every record edit ends with an evaluation. Typing evaluates the run
//! ending at the caret; backspace/delete remove exactly one element and
//! then evaluate the run the removal joined, so the result always equals
//! typing the remaining keystrokes fresh: removing こ from `ytko`
//! re-exposes the live elements (`o` → 「yと」, again 「よ」), and
//! removing the `1` from `yt1t` evaluates `ytt` → 「yっt」.

use karukan_engine::RomajiConverter;

/// One display character of the composition.
#[derive(Clone)]
enum Element {
    /// A keystroke not yet consumed by a conversion rule
    Romaji(char),
    /// A settled character: fired rule output (`ko` → こ), passthrough
    /// (`1`), or direct input — excluded from romaji evaluation
    Converted {
        ch: char,
        /// Keystrokes this character came from, for the alphanumeric
        /// function keys (F9 / F10) — see [`InputBuffer::raw_text`].
        ///
        /// A rule that emits several kana records its whole key on the
        /// first of them and `Some("")` on the rest, so concatenating the
        /// buffer reproduces the typing: `kya` → `[き(Some("kya")),
        /// ゃ(Some(""))]`. `None` means the keystrokes are unknown (a
        /// reading rebuilt after a cancelled conversion), and the kana
        /// stands in for them.
        raw: Option<String>,
    },
}

impl Element {
    fn converted(ch: char) -> Self {
        Element::Converted { ch, raw: None }
    }

    fn ch(&self) -> char {
        match self {
            Element::Romaji(ch) | Element::Converted { ch, .. } => *ch,
        }
    }

    fn is_romaji(&self) -> bool {
        matches!(self, Element::Romaji(_))
    }

    /// The keystrokes behind this character, falling back to the character
    /// itself when none were recorded.
    fn raw(&self) -> String {
        match self {
            Element::Romaji(ch) => ch.to_string(),
            Element::Converted { ch, raw } => raw.clone().unwrap_or_else(|| ch.to_string()),
        }
    }
}

/// The recorded composition: elements plus the caret index.
pub(super) struct InputBuffer {
    elements: Vec<Element>,
    /// Caret: a boundary index into `elements`, which — with one element
    /// per display character — is also the display position.
    ///
    /// ```text
    /// elements: [Romaji(k), Romaji(y), Converted(1), Converted(K)]
    /// boundary: 0         1          2             3             4
    ///                                ↑ cursor = 2 (between y and 1)
    /// ```
    cursor: usize,
}

impl InputBuffer {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            cursor: 0,
        }
    }

    pub fn clear(&mut self) {
        self.elements.clear();
        self.cursor = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    // --- Record edits -----------------------------------------------------

    /// Record a kana-mode keystroke at the caret, then evaluate the active
    /// run it now ends.
    pub fn push_romaji(&mut self, ch: char, romaji: &RomajiConverter) {
        self.elements
            .insert(self.cursor, Element::Romaji(ch.to_ascii_lowercase()));
        self.cursor += 1;
        self.evaluate_active_run(romaji);
    }

    /// Record a direct-input keystroke (alphabet/emoji mode) at the caret,
    /// settled as-is.
    pub fn push_direct(&mut self, ch: char) {
        self.elements.insert(self.cursor, Element::converted(ch));
        self.cursor += 1;
    }

    /// Record a settled character at the caret whose keystrokes differ from
    /// the character itself (Ctrl+Space types a space and inserts U+3000, so
    /// F10 turns it back into a half-width space).
    pub fn push_direct_raw(&mut self, ch: char, raw: &str) {
        self.elements.insert(
            self.cursor,
            Element::Converted {
                ch,
                raw: Some(raw.to_string()),
            },
        );
        self.cursor += 1;
    }

    /// Replace the whole composition with settled `text`, caret at the end
    /// and no recorded keystrokes.
    ///
    /// This is the raw-less entry point, used where a reading is rebuilt
    /// from scratch and the keystrokes behind it are gone: cancelling a
    /// conversion, resuming an unconverted tail, and segment navigation.
    /// F9 / F10 then show the kana for those characters.
    pub fn set_text(&mut self, text: &str) {
        self.clear();
        self.elements.extend(text.chars().map(Element::converted));
        self.cursor = self.elements.len();
    }

    /// Record settled `text` at the caret as one group produced by the
    /// keystrokes `raw` — what typing `raw` would leave behind. Test setup
    /// only; production code always goes through the typed-key paths.
    #[cfg(test)]
    pub fn insert_raw(&mut self, text: &str, raw: &str) {
        let group = attribute_raw(text, raw);
        let count = group.len();
        self.elements.splice(self.cursor..self.cursor, group);
        self.cursor += count;
    }

    /// Record settled text at the caret. Test setup only — production
    /// code always goes through the typed-key paths.
    #[cfg(test)]
    pub fn insert(&mut self, text: &str) {
        let count = text.chars().count();
        self.elements.splice(
            self.cursor..self.cursor,
            text.chars().map(Element::converted),
        );
        self.cursor += count;
    }

    /// Remove the element before the caret, then evaluate the Romaji run
    /// the removal joined, so the result matches typing the remaining
    /// keystrokes fresh (`yt1t` minus the `1` → 「yっt」). Returns false
    /// when the caret is at the start.
    pub fn backspace(&mut self, romaji: &RomajiConverter) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        self.elements.remove(self.cursor);
        self.evaluate_joined_run(romaji);
        true
    }

    /// Remove the element at the caret (delete key), then evaluate the
    /// Romaji run the removal joined. Returns false when the caret is at
    /// the end.
    pub fn delete_at_cursor(&mut self, romaji: &RomajiConverter) -> bool {
        if self.cursor == self.elements.len() {
            return false;
        }
        self.elements.remove(self.cursor);
        self.evaluate_joined_run(romaji);
        true
    }

    /// Evaluate the active run (the Romaji run ending at the cursor),
    /// re-recording keystrokes a rule consumed as its output. Typing never
    /// combines across the caret, so this stops there.
    fn evaluate_active_run(&mut self, romaji: &RomajiConverter) {
        let range = self.active_run();
        let evaluated_len = self.evaluate_range(range.clone(), romaji);
        self.cursor = range.start + evaluated_len;
    }

    /// Evaluate the Romaji run containing the caret — both sides of a
    /// deletion point. The caret keeps its offset from the run start,
    /// clamped to the evaluated length.
    fn evaluate_joined_run(&mut self, romaji: &RomajiConverter) {
        let start = self.elements[..self.cursor]
            .iter()
            .rposition(|e| !e.is_romaji())
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = self.cursor
            + self.elements[self.cursor..]
                .iter()
                .position(|e| !e.is_romaji())
                .unwrap_or(self.elements.len() - self.cursor);
        let offset = self.cursor - start;
        let evaluated_len = self.evaluate_range(start..end, romaji);
        self.cursor = start + offset.min(evaluated_len);
    }

    /// Replace a Romaji range with its evaluation; returns the new length.
    fn evaluate_range(&mut self, range: std::ops::Range<usize>, romaji: &RomajiConverter) -> usize {
        if range.is_empty() {
            return 0;
        }
        let run: String = self.elements[range.clone()]
            .iter()
            .map(Element::ch)
            .collect();
        let evaluated = evaluate_run(&run, romaji);
        let len = evaluated.len();
        self.elements.splice(range, evaluated);
        len
    }

    /// The reading as it would settle: Romaji runs force-converted in
    /// place, everything else as displayed. The non-destructive
    /// counterpart of [`Self::settle_romaji`] — used when the composition
    /// must stay editable (starting a conversion that Escape can undo).
    pub fn settled_reading(&self, romaji: &RomajiConverter) -> String {
        settle_slice(&self.elements, romaji)
    }

    /// The caret's position within [`Self::settled_reading`]: the settled
    /// length of everything left of it. Romaji runs shrink when they settle
    /// (`kya` → きゃ), so this is not the element index.
    pub fn settled_cursor(&self, romaji: &RomajiConverter) -> usize {
        settle_slice(&self.elements[..self.cursor], romaji)
            .chars()
            .count()
    }

    /// The keystrokes behind the first `pos` display characters, or `None`
    /// when `pos` falls inside a group of characters that share one raw
    /// (splitting きゃ leaves no principled way to split `kya`).
    pub fn raw_prefix(&self, pos: usize) -> Option<String> {
        let pos = pos.min(self.elements.len());
        if let Some(Element::Converted { raw: Some(raw), .. }) = self.elements.get(pos)
            && raw.is_empty()
        {
            return None;
        }
        Some(self.elements[..pos].iter().map(Element::raw).collect())
    }

    /// Settle all Romaji keystrokes in place (`ltu` → っ; unmatched
    /// consonants pass through literally). Called before conversion,
    /// commit, and katakana baking. The caret keeps its distance from the
    /// end, so an end-of-composition caret stays at the end.
    pub fn settle_romaji(&mut self, romaji: &RomajiConverter) {
        if !self.elements.iter().any(Element::is_romaji) {
            return;
        }
        let from_end = self.elements.len() - self.cursor;
        let mut settled: Vec<Element> = Vec::with_capacity(self.elements.len());
        let mut run = String::new();
        for element in self.elements.drain(..) {
            match element {
                Element::Romaji(ch) => run.push(ch),
                other => {
                    flush_run(&mut settled, &mut run, romaji);
                    settled.push(other);
                }
            }
        }
        flush_run(&mut settled, &mut run, romaji);
        self.elements = settled;
        self.cursor = self.elements.len().saturating_sub(from_end);
    }

    /// Convert every settled element to katakana permanently. Called when
    /// leaving katakana mode so the preedit doesn't revert. The mapping is
    /// 1:1, so the recorded keystrokes still describe the same characters
    /// and travel through untouched.
    pub fn bake_katakana(&mut self) {
        for element in &mut self.elements {
            if let Element::Converted { ch, .. } = element {
                let katakana = karukan_engine::hiragana_to_katakana(&ch.to_string());
                *ch = katakana.chars().next().unwrap_or(*ch);
            }
        }
    }

    /// The keystrokes behind the whole composition, for the alphanumeric
    /// function keys: typing `aiu` and pressing F10 yields `aiu`, not a
    /// reverse romanization of `あいう`.
    ///
    /// Characters with no recorded keystrokes contribute themselves, so a
    /// reading rebuilt after a cancelled conversion shows its kana.
    pub fn raw_text(&self) -> String {
        self.elements.iter().map(Element::raw).collect()
    }

    /// Move the caret to a display position (also its element index).
    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.elements.len());
    }

    // --- Evaluation: views derived from the record ------------------------

    /// Display caret position (== the element index of the caret).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Full composition display.
    pub fn display(&self) -> String {
        self.elements.iter().map(Element::ch).collect()
    }

    pub fn char_count(&self) -> usize {
        self.elements.len()
    }

    /// Element indices of the active run: the maximal Romaji run ending at
    /// the cursor — the keystrokes currently being typed. Empty when the
    /// element left of the cursor is settled (a stranded consonant elsewhere
    /// is NOT active; it stays part of the reading at its position).
    fn active_run(&self) -> std::ops::Range<usize> {
        let start = self.elements[..self.cursor]
            .iter()
            .rposition(|e| !e.is_romaji())
            .map(|i| i + 1)
            .unwrap_or(0);
        start..self.cursor
    }

    /// Keystrokes of the active run (shown as the aux romaji tail).
    pub fn pending(&self) -> String {
        self.elements[self.active_run()]
            .iter()
            .map(Element::ch)
            .collect()
    }

    /// Conversion reading: everything except the active run. A Romaji
    /// keystroke stranded away from the caret counts as a literal
    /// character at its position, so `y1` + `ka` reads 「y1か」.
    pub fn reading(&self) -> String {
        let active = self.active_run();
        self.elements
            .iter()
            .enumerate()
            .filter(|(i, _)| !active.contains(i))
            .map(|(_, e)| e.ch())
            .collect()
    }

    /// Caret position within [`Self::reading`]. The active run sits just
    /// before the cursor and is excluded from the reading, so this is the
    /// caret minus the active run's length.
    pub fn reading_cursor(&self) -> usize {
        self.cursor - self.active_run().len()
    }
}

/// Settle `elements` into the text they would commit as: Romaji runs
/// force-converted in place, everything else as displayed.
fn settle_slice(elements: &[Element], romaji: &RomajiConverter) -> String {
    let mut settled = String::new();
    let mut run = String::new();
    for element in elements {
        match element {
            Element::Romaji(ch) => run.push(*ch),
            Element::Converted { ch, .. } => {
                if !run.is_empty() {
                    settled.push_str(&romaji.convert_flush(&run));
                    run.clear();
                }
                settled.push(*ch);
            }
        }
    }
    if !run.is_empty() {
        settled.push_str(&romaji.convert_flush(&run));
    }
    settled
}

/// Settle one Romaji run into `out` and clear it. The whole run is the raw
/// of whatever it settles into (`ltu` → っ), so it is recorded on the first
/// resulting character.
fn flush_run(out: &mut Vec<Element>, run: &mut String, romaji: &RomajiConverter) {
    if run.is_empty() {
        return;
    }
    let settled = romaji.convert_flush(run);
    out.extend(attribute_raw(&settled, run));
    run.clear();
}

/// Record `text` as settled characters that all came from the keystrokes
/// `raw`: the first character carries `raw`, the rest carry `Some("")` so
/// concatenating their raws reproduces `raw` exactly once.
fn attribute_raw(text: &str, raw: &str) -> Vec<Element> {
    text.chars()
        .enumerate()
        .map(|(i, ch)| Element::Converted {
            ch,
            raw: Some(if i == 0 {
                raw.to_string()
            } else {
                String::new()
            }),
        })
        .collect()
}

/// Evaluate a run of romaji keystrokes: convert the whole run and record
/// one element per output character.
///
/// Rule outputs never contain ASCII (see the converter's contract), so an
/// ASCII character in the output is a keystroke that passed through: it
/// stays live (`Romaji`) if it can still begin a rule (`ykt` → BS → `o`
/// → 「yこ」) and settles otherwise (`1`). Everything else is a fired
/// rule's output, settled for good. The trailing pending stays `Romaji`
/// per keystroke.
///
/// Settling is where the configured width applies, after the classification
/// above: a character settles at the width in force when it was typed, so
/// switching to alphabet input mid-word (`（` then Shift+A) leaves what is
/// already settled alone.
fn evaluate_run(run: &str, romaji: &RomajiConverter) -> Vec<Element> {
    let converted = romaji.convert(run);
    let settled: Vec<char> = converted
        .text
        .chars()
        .map(|c| {
            if romaji.starts_rule(c) {
                c
            } else {
                romaji.width().apply(c)
            }
        })
        .collect();

    // Walk the run one keystroke at a time so each output character can be
    // credited to the keystrokes that produced it (`kya` → きゃ). Converting
    // a longer prefix only ever extends the output, so the characters that
    // appear since the last step are exactly what the keystrokes buffered
    // since then produced.
    let mut elements: Vec<Element> = Vec::new();
    let mut prefix = String::new();
    let mut emitted = 0usize;
    let mut buffered_raw = String::new();

    for ch in run.chars() {
        prefix.push(ch);
        buffered_raw.push(ch);
        let step = romaji.convert(&prefix);
        let text: Vec<char> = step.text.chars().collect();
        if text.len() <= emitted {
            continue;
        }
        // A rule can fire while leaving part of its input live (`tt` emits
        // っ and re-buffers the second `t`), so only the keystrokes ahead of
        // the pending tail were actually consumed; the rest carries over to
        // whatever they end up producing.
        let consumed_len = buffered_raw
            .chars()
            .count()
            .saturating_sub(step.pending.chars().count());
        let consumed: String = buffered_raw.chars().take(consumed_len).collect();
        buffered_raw = buffered_raw.chars().skip(consumed_len).collect();

        elements.extend(settle_chars(&text[emitted..], &consumed, romaji));
        emitted = text.len();
    }

    // Defensive: if the incremental walk and the whole-run conversion ever
    // disagree, fall back to the run's own conversion with the keystrokes
    // unattributed rather than displaying something else than upstream would.
    if elements.iter().map(Element::ch).ne(settled.iter().copied()) {
        elements = settle_chars(&settled, run, romaji);
    }

    elements
        .into_iter()
        .chain(converted.pending.chars().map(Element::Romaji))
        .collect()
}

/// Record converted output characters, keeping ASCII that can still begin a
/// rule live (`ykt` → BS → `o` → 「yこ」) and settling everything else with
/// `raw` as the keystrokes behind it.
fn settle_chars(chars: &[char], raw: &str, romaji: &RomajiConverter) -> Vec<Element> {
    let mut out = Vec::with_capacity(chars.len());
    let mut raw_pending = Some(raw);
    for &c in chars {
        if romaji.starts_rule(c) {
            out.push(Element::Romaji(c));
        } else {
            out.push(Element::Converted {
                ch: romaji.width().apply(c),
                raw: Some(raw_pending.take().unwrap_or_default().to_string()),
            });
        }
    }
    out
}

#[cfg(test)]
mod raw_tests {
    use super::*;

    /// Type `keys` in kana mode and return the recorded keystrokes.
    fn typed_raw(keys: &str) -> String {
        let romaji = RomajiConverter::new();
        let mut buf = InputBuffer::new();
        for ch in keys.chars() {
            buf.push_romaji(ch, &romaji);
        }
        buf.raw_text()
    }

    /// Type `keys` and return what is displayed, to pin the raw tests to a
    /// composition the user would actually see.
    fn typed_display(keys: &str) -> String {
        let romaji = RomajiConverter::new();
        let mut buf = InputBuffer::new();
        for ch in keys.chars() {
            buf.push_romaji(ch, &romaji);
        }
        buf.display()
    }

    #[test]
    fn one_keystroke_per_kana() {
        assert_eq!(typed_display("aiu"), "あいう");
        assert_eq!(typed_raw("aiu"), "aiu");
    }

    #[test]
    fn multi_kana_rule_keeps_its_whole_key() {
        // `kya` emits きゃ at once: both kana share the one raw, so the
        // buffer still reads back as `kya` and not `kyakya`.
        assert_eq!(typed_display("kya"), "きゃ");
        assert_eq!(typed_raw("kya"), "kya");
    }

    #[test]
    fn nn_is_recorded_as_the_pair_that_produced_it() {
        assert_eq!(typed_display("nn"), "ん");
        assert_eq!(typed_raw("nn"), "nn");
    }

    #[test]
    fn passthrough_characters_carry_themselves() {
        assert_eq!(typed_display("1"), "1");
        assert_eq!(typed_raw("1"), "1");
    }

    #[test]
    fn pending_keystrokes_are_recorded_verbatim() {
        // `k` has not converted yet; it is shown and recorded as itself.
        assert_eq!(typed_display("k"), "k");
        assert_eq!(typed_raw("k"), "k");
    }

    #[test]
    fn uppercase_is_recorded_lowercased_like_the_display() {
        assert_eq!(typed_display("KA"), "か");
        assert_eq!(typed_raw("KA"), "ka");
    }

    #[test]
    fn mixed_runs_concatenate_in_order() {
        assert_eq!(typed_display("kya1ttu"), "きゃ1っつ");
        assert_eq!(typed_raw("kya1ttu"), "kya1ttu");
    }

    #[test]
    fn backspace_re_records_the_run_it_rejoins() {
        // Deleting ゃ dissolves the きゃ group; what is left must still read
        // back as keystrokes that produce the remaining display.
        let romaji = RomajiConverter::new();
        let mut buf = InputBuffer::new();
        for ch in "kya".chars() {
            buf.push_romaji(ch, &romaji);
        }
        assert!(buf.backspace(&romaji));
        assert_eq!(buf.display(), "き");
        assert_eq!(buf.raw_text(), "kya");
    }

    #[test]
    fn set_text_falls_back_to_the_kana() {
        let mut buf = InputBuffer::new();
        buf.set_text("あい");
        assert_eq!(buf.raw_text(), "あい");
    }

    #[test]
    fn push_direct_raw_records_the_key_that_was_typed() {
        let mut buf = InputBuffer::new();
        buf.push_direct_raw('\u{3000}', " ");
        assert_eq!(buf.display(), "\u{3000}");
        assert_eq!(buf.raw_text(), " ");
    }

    #[test]
    fn raw_prefix_is_none_inside_a_shared_group() {
        let romaji = RomajiConverter::new();
        let mut buf = InputBuffer::new();
        for ch in "kya".chars() {
            buf.push_romaji(ch, &romaji);
        }
        // Splitting きゃ down the middle leaves no way to split `kya`.
        assert_eq!(buf.raw_prefix(1), None);
        assert_eq!(buf.raw_prefix(2), Some("kya".to_string()));
        assert_eq!(buf.raw_prefix(0), Some(String::new()));
    }
}
