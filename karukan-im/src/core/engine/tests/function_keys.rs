//! Function-key character-form conversion (F6–F10).
//!
//! The form-building itself is unit-tested in `engine::form`; these cases
//! exercise the state-machine integration — entering conversion, cycling on
//! repeat, committing, and how the raw keystrokes survive editing.

use super::*;

/// Type each character of `input` into a fresh engine.
fn typing(input: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    for ch in input.chars() {
        engine.process_key(&press(ch));
    }
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
    let mut engine = typing("aiueo");
    assert_eq!(preedit_text(&engine), "あいうえお");

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
    let mut engine = typing("aiueo");
    engine.process_key(&press_key(Keysym::F8));
    assert_eq!(preedit_text(&engine), "ｱｲｳｴｵ");
}

#[test]
fn f6_returns_to_hiragana_after_f7() {
    let mut engine = typing("aiueo");
    engine.process_key(&press_key(Keysym::F7));
    assert_eq!(preedit_text(&engine), "アイウエオ");

    engine.process_key(&press_key(Keysym::F6));
    assert_eq!(preedit_text(&engine), "あいうえお");
}

#[test]
fn f10_transliterates_the_typed_romaji_and_cycles_case() {
    let mut engine = typing("aiu");

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
    let mut engine = typing("aiu");

    engine.process_key(&press_key(Keysym::F9));
    assert_eq!(preedit_text(&engine), "ａｉｕ");

    engine.process_key(&press_key(Keysym::F9));
    assert_eq!(preedit_text(&engine), "ＡＩＵ");
}

#[test]
fn switching_function_keys_restarts_the_cycle() {
    let mut engine = typing("aiu");

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
    // `kya` produces two kana from three keystrokes.
    let mut engine = typing("kya");
    assert_eq!(preedit_text(&engine), "きゃ");

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "kya");
}

#[test]
fn editing_inside_a_keystroke_group_falls_back_to_kana() {
    let mut engine = typing("kya");
    // Backspace removes `ゃ`, which splits the `kya` group — the keystrokes
    // behind the surviving `き` can no longer be attributed, so F10 shows the
    // kana itself.
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(preedit_text(&engine), "き");

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "き");
}

#[test]
fn backspace_over_a_whole_group_keeps_the_remaining_raw() {
    let mut engine = typing("kyaku");
    assert_eq!(preedit_text(&engine), "きゃく");

    // `く` is its own group, so removing it leaves `kya` intact.
    engine.process_key(&press_key(Keysym::BACKSPACE));
    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "kya");
}

#[test]
fn f10_after_a_mid_buffer_insert_keeps_the_untouched_raw() {
    let mut engine = typing("aiu");
    // Move the caret between `あ` and `い`, then type `k`+`a`.
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press('k'));
    engine.process_key(&press('a'));
    assert_eq!(preedit_text(&engine), "あかいう");

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(preedit_text(&engine), "akaiu");
}

#[test]
fn function_keys_work_during_conversion() {
    let mut engine = typing("aiueo");
    // No model is loaded, so Space falls back to kana candidates — enough to
    // put the engine in the Conversion state.
    engine.process_key(&press_key(Keysym::SPACE));
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
    let mut engine = typing("aiueo");
    let result = engine.process_key(&press_ctrl(Keysym::F7));
    assert!(
        !result.consumed,
        "Ctrl+F7 may be an application shortcut and must not be swallowed"
    );
    assert_eq!(preedit_text(&engine), "あいうえお");
}

#[test]
fn committing_a_form_conversion_is_recorded_in_learning() {
    let mut engine = InputMethodEngine::new();
    engine.learning = Some(karukan_engine::LearningCache::new(
        karukan_engine::LearningConfig::default(),
    ));
    for ch in "aiueo".chars() {
        engine.process_key(&press(ch));
    }
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
    // Shift+letter enters alphabet mode and types uppercase directly.
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
    let mut engine = typing("aiu");
    engine.process_key(&press_key(Keysym::F7));
    assert_eq!(preedit_text(&engine), "アイウ");

    engine.process_key(&press_key(Keysym::ESCAPE));
    assert_eq!(preedit_text(&engine), "あいう");
    assert!(matches!(engine.state(), InputState::Composing { .. }));
}
