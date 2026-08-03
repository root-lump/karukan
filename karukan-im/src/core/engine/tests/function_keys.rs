//! Function-key character-form conversion (F6–F10).
//!
//! No kanji model is involved anywhere in this file: the composing state is
//! built by seeding the input buffer directly rather than by typing, so
//! nothing here loads a model and no assertion depends on model output. The
//! romaji → keystroke derivation that real typing performs is covered by the
//! `consumed_raw` unit tests in `engine::input`, and the span bookkeeping by
//! the `InputBuffer` unit tests.
//!
//! The form building itself is unit-tested in `engine::form`; these cases
//! exercise the state-machine integration — entering conversion, cycling on
//! repeat, committing, and the fallbacks when keystrokes are unavailable.

use super::*;

/// Engine in Composing state whose buffer holds `kana`, recorded as having
/// been produced by the keystrokes `raw` (one span, as one insertion would).
fn composing(kana: &str, raw: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.input_buf.insert(kana, raw);
    engine.state = InputState::Composing {
        preedit: Preedit::with_text_underlined(kana),
        romaji_buffer: String::new(),
    };
    engine
}

/// Engine in Composing state with no tracked keystrokes — the state the
/// buffer is left in after a cancelled conversion or an edit inside a
/// multi-kana span.
fn composing_without_raw(kana: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.input_buf.set_text_kana_raw(kana);
    engine.state = InputState::Composing {
        preedit: Preedit::with_text_underlined(kana),
        romaji_buffer: String::new(),
    };
    engine
}

/// The preedit text currently displayed.
fn preedit_text(engine: &InputMethodEngine) -> String {
    engine
        .preedit()
        .map(|p| p.text().to_string())
        .unwrap_or_default()
}

/// The text of the `Commit` action in `result`, if any.
fn commit_text(result: &EngineResult) -> Option<String> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(text) => Some(text.clone()),
        _ => None,
    })
}

#[test]
fn f7_converts_to_full_katakana_and_enter_commits_it() {
    let mut engine = composing("あいうえお", "aiueo");

    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(result.consumed);
    assert_eq!(preedit_text(&engine), "アイウエオ");
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert!(
        commit_text(&result).is_none(),
        "F7 must not commit by itself"
    );

    let enter = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text(&enter).as_deref(), Some("アイウエオ"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f8_converts_to_half_katakana() {
    let mut engine = composing("あいうえお", "aiueo");
    engine.process_key(&press_key(Keysym::F8));
    assert_eq!(preedit_text(&engine), "ｱｲｳｴｵ");
}

#[test]
fn f6_returns_to_hiragana_after_f7() {
    let mut engine = composing("あいうえお", "aiueo");
    engine.process_key(&press_key(Keysym::F7));
    assert_eq!(preedit_text(&engine), "アイウエオ");

    engine.process_key(&press_key(Keysym::F6));
    assert_eq!(preedit_text(&engine), "あいうえお");
}

#[test]
fn f10_transliterates_the_typed_romaji_and_cycles_case() {
    let mut engine = composing("あいう", "aiu");

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "aiu");

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "AIU");

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "Aiu");

    // Wraps back around to the start of the cycle.
    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "aiu");
}

#[test]
fn f9_produces_full_width_alphanumerics() {
    let mut engine = composing("あいう", "aiu");

    engine.process_key(&press_key(Keysym::F9));
    assert_eq!(preedit_text(&engine), "ａｉｕ");

    engine.process_key(&press_key(Keysym::F9));
    assert_eq!(preedit_text(&engine), "ＡＩＵ");
}

#[test]
fn switching_function_keys_restarts_the_cycle() {
    let mut engine = composing("あいう", "aiu");

    engine.process_key(&press_key(Keysym::F10));
    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "AIU");

    // A different form key starts its own list...
    engine.process_key(&press_key(Keysym::F9));
    assert_eq!(preedit_text(&engine), "ａｉｕ");

    // ...and coming back to F10 starts from its first form again.
    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "aiu");
}

#[test]
fn multi_kana_keystrokes_are_tracked_as_one_unit() {
    // `kya` produces two kana from three keystrokes, recorded as one span.
    let mut engine = composing("きゃ", "kya");
    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "kya");
}

#[test]
fn alphanumeric_keys_fall_back_to_kana_without_keystrokes() {
    // After an edit inside a keystroke group (or a cancelled conversion) the
    // keystrokes are gone, so F10 can only show the kana.
    let mut engine = composing_without_raw("き");
    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "き");
}

#[test]
fn function_keys_work_during_conversion() {
    // Enter Conversion via F6, then switch the form — the path a user takes
    // when they convert first and change their mind about the character form.
    let mut engine = composing("あいうえお", "aiueo");
    engine.process_key(&press_key(Keysym::F6));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    engine.process_key(&press_key(Keysym::F7));
    assert_eq!(preedit_text(&engine), "アイウエオ");

    let enter = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text(&enter).as_deref(), Some("アイウエオ"));
}

#[test]
fn function_keys_pass_through_from_the_empty_state() {
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn modified_function_key_chords_pass_through() {
    let mut engine = composing("あいうえお", "aiueo");
    let result = engine.process_key(&press_ctrl(Keysym::F7));
    assert!(
        !result.consumed,
        "Ctrl+F7 may be an application shortcut and must not be swallowed"
    );
    assert_eq!(preedit_text(&engine), "あいうえお");
}

#[test]
fn committing_a_form_conversion_is_recorded_in_learning() {
    let mut engine = composing("あいうえお", "aiueo");
    engine.learning = Some(karukan_engine::LearningCache::new(
        karukan_engine::LearningConfig::default(),
    ));

    engine.process_key(&press_key(Keysym::F7));
    engine.process_key(&press_key(Keysym::RETURN));

    let entries = engine.learning.as_ref().unwrap().lookup("あいうえお");
    assert!(
        entries.iter().any(|(surface, _)| surface == "アイウエオ"),
        "expected the katakana form in the learning cache, got {:?}",
        entries
    );
}

#[test]
fn alphabet_mode_input_gets_width_variants() {
    let mut engine = InputMethodEngine::new();
    // Shift+letter enters alphabet mode and types uppercase directly. Latin
    // input skips the converter entirely, so no model is involved.
    engine.process_key(&press_shift('a'));
    engine.process_key(&press_shift('b'));
    engine.process_key(&press_shift('c'));
    assert_eq!(preedit_text(&engine), "ABC");

    engine.process_key(&press_key(Keysym::F9));
    assert_eq!(preedit_text(&engine), "ａｂｃ");

    engine.process_key(&press_key(Keysym::F9));
    assert_eq!(preedit_text(&engine), "ＡＢＣ");
}

#[test]
fn escape_after_a_form_conversion_returns_to_the_reading() {
    let mut engine = composing("あいう", "aiu");
    engine.process_key(&press_key(Keysym::F7));
    assert_eq!(preedit_text(&engine), "アイウ");

    engine.process_key(&press_key(Keysym::ESCAPE));
    assert_eq!(preedit_text(&engine), "あいう");
    assert!(matches!(engine.state(), InputState::Composing { .. }));
}

#[test]
fn form_conversion_of_a_katakana_reading_normalizes_to_hiragana() {
    // Katakana mode bakes the buffer to katakana; F6 must still reach kana.
    let mut engine = composing("アイウ", "aiu");
    engine.process_key(&press_key(Keysym::F6));
    assert_eq!(preedit_text(&engine), "あいう");
}
