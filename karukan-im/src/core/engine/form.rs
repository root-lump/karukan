//! Character-form conversion driven by the function keys (F6–F10).
//!
//! These are the keybindings every Japanese IME ships (mozc / MS-IME):
//!
//! | Key | Form |
//! |-----|------|
//! | F6  | ひらがな |
//! | F7  | 全角カタカナ |
//! | F8  | 半角カタカナ |
//! | F9  | 全角英数（連打で 小文字 → 大文字 → 先頭大文字） |
//! | F10 | 半角英数（同上） |
//!
//! F9/F10 transliterate the *keystrokes* rather than the kana — typing `aiu`
//! and pressing F10 yields `aiu`, not a reverse romanization of `あいう`. The
//! keystrokes come from the raw spans kept in
//! [`InputBuffer`](super::input_buffer::InputBuffer); where they were lost to
//! an edit, the kana stands in.
//!
//! A press builds a candidate list of that key's forms and enters the
//! Conversion state, so Enter/Escape/digit selection behave exactly as they do
//! after a normal conversion. Pressing the same key again advances within the
//! list, which is what produces the mozc-style case cycle.

use karukan_engine::{
    ascii_to_fullwidth, ascii_to_halfwidth, hiragana_to_katakana, katakana_to_half_width,
    katakana_to_hiragana,
};

use super::*;

/// The character form a function key converts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::core) enum ConversionForm {
    /// F6 — ひらがな
    Hiragana,
    /// F7 — 全角カタカナ
    FullKatakana,
    /// F8 — 半角カタカナ
    HalfKatakana,
    /// F9 — 全角英数
    FullAlphanumeric,
    /// F10 — 半角英数
    HalfAlphanumeric,
}

/// Lower-case, upper-case, and capitalized forms of `raw`, in the order the
/// alphanumeric keys cycle through them (mozc's F9/F10 cycle). Duplicates are
/// dropped, so digit-only input yields a single entry.
fn case_cycle(raw: &str) -> Vec<(String, &'static str)> {
    let lower = raw.to_lowercase();
    let upper = raw.to_uppercase();
    let mut capitalized = String::new();
    for (i, c) in lower.chars().enumerate() {
        if i == 0 {
            capitalized.extend(c.to_uppercase());
        } else {
            capitalized.push(c);
        }
    }

    let mut out: Vec<(String, &'static str)> = Vec::new();
    for (text, desc) in [
        (lower, "英小文字"),
        (upper, "英大文字"),
        (capitalized, "英先頭大文字"),
    ] {
        if !out.iter().any(|(t, _)| *t == text) {
            out.push((text, desc));
        }
    }
    out
}

impl ConversionForm {
    /// The form a keysym selects, or `None` for keys that aren't bound.
    pub(in crate::core) fn from_keysym(keysym: Keysym) -> Option<Self> {
        match keysym {
            Keysym::F6 => Some(Self::Hiragana),
            Keysym::F7 => Some(Self::FullKatakana),
            Keysym::F8 => Some(Self::HalfKatakana),
            Keysym::F9 => Some(Self::FullAlphanumeric),
            Keysym::F10 => Some(Self::HalfAlphanumeric),
            _ => None,
        }
    }

    /// The candidate texts for this form, each with its mozc-style width
    /// annotation, in the order repeated presses cycle through them.
    ///
    /// `reading` is the kana being composed; `raw` the keystrokes behind it
    /// (equal to `reading` when they could not be tracked).
    pub(in crate::core) fn variants(&self, reading: &str, raw: &str) -> Vec<(String, String)> {
        let single = |text: String, desc: &str| vec![(text, desc.to_string())];
        match self {
            Self::Hiragana => single(katakana_to_hiragana(reading), "[全]ひらがな"),
            Self::FullKatakana => single(
                ascii_to_fullwidth(&hiragana_to_katakana(reading)),
                "[全]カタカナ",
            ),
            Self::HalfKatakana => single(
                ascii_to_halfwidth(&katakana_to_half_width(&hiragana_to_katakana(reading))),
                "[半]カタカナ",
            ),
            Self::FullAlphanumeric => case_cycle(raw)
                .into_iter()
                .map(|(text, desc)| (ascii_to_fullwidth(&text), format!("[全]{}", desc)))
                .collect(),
            Self::HalfAlphanumeric => case_cycle(raw)
                .into_iter()
                .map(|(text, desc)| (ascii_to_halfwidth(&text), format!("[半]{}", desc)))
                .collect(),
        }
    }
}

impl InputMethodEngine {
    /// Apply a function-key character form (F6–F10).
    ///
    /// From Empty the key is passed through — there is nothing to convert and
    /// the application may have its own binding for it. Otherwise the current
    /// reading is converted into that form's candidate list and the engine
    /// enters (or stays in) the Conversion state. Repeating the same key
    /// advances within the list, which is how F9/F10 cycle through
    /// lower/upper/capitalized.
    pub(super) fn convert_to_form(&mut self, form: ConversionForm) -> EngineResult {
        match self.state {
            InputState::Empty => return EngineResult::not_consumed(),
            InputState::Conversion { .. } if self.form_conversion == Some(form) => {
                // Same key again: step through this form's cycle.
                return self.next_candidate();
            }
            InputState::Conversion { .. } => {}
            InputState::Composing { .. } => {
                // Take the pending romaji into the buffer so the whole
                // composition (and its keystrokes) is converted, then leave
                // composing state behind. The cursor position is ignored:
                // like mozc, a function key converts the entire composition.
                self.flush_romaji_to_composed();
                self.conversion_raw = Some(self.input_buf.raw_text());
                self.live.text.clear();
                self.chunks.clear();
                self.conversion_tail = None;
                self.converters.romaji.reset();
                self.input_buf.cursor_pos = 0;
            }
        }

        let reading = self.input_buf.text.clone();
        if reading.is_empty() {
            return EngineResult::consumed();
        }
        let raw = self
            .conversion_raw
            .clone()
            .unwrap_or_else(|| reading.clone());

        let variants = form.variants(&reading, &raw);
        if variants.is_empty() {
            return EngineResult::consumed();
        }

        let candidates = CandidateList::new(
            variants
                .into_iter()
                .map(|(text, description)| Candidate {
                    text,
                    reading: Some(reading.clone()),
                    source: Some(CandidateSource::Rewriter),
                    description: Some(description),
                })
                .collect(),
        );

        self.form_conversion = Some(form);
        self.enter_conversion_state(&reading, candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(form: ConversionForm, reading: &str, raw: &str) -> Vec<String> {
        form.variants(reading, raw)
            .into_iter()
            .map(|(t, _)| t)
            .collect()
    }

    #[test]
    fn from_keysym_maps_f6_to_f10() {
        assert_eq!(
            ConversionForm::from_keysym(Keysym::F6),
            Some(ConversionForm::Hiragana)
        );
        assert_eq!(
            ConversionForm::from_keysym(Keysym::F10),
            Some(ConversionForm::HalfAlphanumeric)
        );
        assert_eq!(ConversionForm::from_keysym(Keysym::SPACE), None);
    }

    #[test]
    fn kana_forms_are_single_variants() {
        assert_eq!(
            texts(ConversionForm::Hiragana, "アイウ", "aiu"),
            vec!["あいう"]
        );
        assert_eq!(
            texts(ConversionForm::FullKatakana, "あいう", "aiu"),
            vec!["アイウ"]
        );
        assert_eq!(
            texts(ConversionForm::HalfKatakana, "あいう", "aiu"),
            vec!["ｱｲｳ"]
        );
    }

    #[test]
    fn half_katakana_expands_voiced_kana() {
        assert_eq!(
            texts(ConversionForm::HalfKatakana, "がっこう", "gakkou"),
            vec!["ｶﾞｯｺｳ"]
        );
    }

    #[test]
    fn alphanumeric_forms_cycle_case() {
        assert_eq!(
            texts(ConversionForm::HalfAlphanumeric, "あいう", "aiu"),
            vec!["aiu", "AIU", "Aiu"]
        );
        assert_eq!(
            texts(ConversionForm::FullAlphanumeric, "あいう", "aiu"),
            vec!["ａｉｕ", "ＡＩＵ", "Ａｉｕ"]
        );
    }

    #[test]
    fn digits_have_a_single_case_form() {
        assert_eq!(
            texts(ConversionForm::HalfAlphanumeric, "123", "123"),
            ["123"]
        );
        assert_eq!(
            texts(ConversionForm::FullAlphanumeric, "123", "123"),
            ["１２３"]
        );
    }

    #[test]
    fn alphanumeric_falls_back_to_kana_when_raw_is_lost() {
        // With no tracked keystrokes the reading stands in as the raw, so the
        // kana simply survives the width/case pass unchanged.
        assert_eq!(
            texts(ConversionForm::HalfAlphanumeric, "あいう", "あいう"),
            vec!["あいう"]
        );
    }

    #[test]
    fn variant_descriptions_are_mozc_style() {
        let out = ConversionForm::HalfAlphanumeric.variants("あいう", "aiu");
        assert_eq!(out[0].1, "[半]英小文字");
        assert_eq!(out[1].1, "[半]英大文字");
        assert_eq!(out[2].1, "[半]英先頭大文字");
    }
}
