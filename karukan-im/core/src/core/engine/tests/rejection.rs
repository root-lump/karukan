//! Erasing a commit right away and retyping its reading demotes the
//! rejected surface and undoes its learning record (`rejection.rs`).

use karukan_engine::{LearningCache, LearningConfig};

use super::*;

/// Engine with an empty learning cache and the model pinned to answer
/// `あい` with `藍`.
fn engine() -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    engine.learning = Some(LearningCache::new(LearningConfig::default()));
    seed_model_cache(&mut engine, "アイ", "", &["藍"]);
    engine
}

fn type_str(engine: &mut InputMethodEngine, s: &str) {
    for ch in s.chars() {
        engine.process_key(&press(ch));
    }
}

/// Type `ai`, convert, and return the conversion's candidate texts.
fn convert_ai(engine: &mut InputMethodEngine) -> Vec<String> {
    type_str(engine, "ai");
    engine.process_key(&press_key(Keysym::SPACE));
    engine
        .state()
        .candidates()
        .expect("in conversion")
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect()
}

fn commit_first(engine: &mut InputMethodEngine) -> String {
    let result = engine.process_key(&press_key(Keysym::RETURN));
    result
        .actions
        .iter()
        .find_map(|a| match a {
            EngineAction::Commit(t) => Some(t.clone()),
            _ => None,
        })
        .expect("Enter commits")
}

#[test]
fn erase_and_retype_demotes_the_rejected_surface() {
    let mut engine = engine();
    assert_eq!(convert_ai(&mut engine)[0], "藍");
    assert_eq!(commit_first(&mut engine), "藍");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    let texts = convert_ai(&mut engine);
    assert_eq!(
        texts[1], "藍",
        "rejected surface moves to second: {texts:?}"
    );
    assert_ne!(texts[0], "藍");
    assert_eq!(
        engine.state().candidates().unwrap().selected_text(),
        Some(texts[0].as_str())
    );
}

#[test]
fn erased_commit_is_unlearned() {
    let mut engine = engine();
    convert_ai(&mut engine);
    commit_first(&mut engine);
    assert!(
        engine
            .learning
            .as_ref()
            .unwrap()
            .entry("あい", "藍")
            .is_some()
    );

    engine.process_key(&press_key(Keysym::BACKSPACE));
    type_str(&mut engine, "a");
    assert!(
        engine
            .learning
            .as_ref()
            .unwrap()
            .entry("あい", "藍")
            .is_none(),
        "the record the erased commit made must be undone"
    );
}

#[test]
fn unlearning_restores_an_earlier_record() {
    let mut engine = engine();
    engine.learning.as_mut().unwrap().record("あい", "藍");
    convert_ai(&mut engine);
    commit_first(&mut engine);
    assert_eq!(
        engine
            .learning
            .as_ref()
            .unwrap()
            .entry("あい", "藍")
            .unwrap()
            .frequency,
        2
    );

    engine.process_key(&press_key(Keysym::BACKSPACE));
    type_str(&mut engine, "a");
    assert_eq!(
        engine
            .learning
            .as_ref()
            .unwrap()
            .entry("あい", "藍")
            .unwrap()
            .frequency,
        1,
        "only the erased commit's record is undone"
    );
}

#[test]
fn partial_erase_is_not_a_rejection() {
    let mut engine = engine();
    seed_model_cache(&mut engine, "アイウ", "", &["藍う"]);
    type_str(&mut engine, "aiu");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(commit_first(&mut engine), "藍う");

    // One BackSpace leaves 「藍」 in the editor.
    engine.process_key(&press_key(Keysym::BACKSPACE));
    type_str(&mut engine, "aiu");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(
        engine.state().candidates().unwrap().selected_text(),
        Some("藍う")
    );
}

#[test]
fn another_key_in_between_forgets_the_commit() {
    let mut engine = engine();
    convert_ai(&mut engine);
    commit_first(&mut engine);

    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(convert_ai(&mut engine)[0], "藍");
}

#[test]
fn ctrl_backspace_erases_the_whole_commit() {
    let mut engine = engine();
    seed_model_cache(&mut engine, "アイウ", "", &["藍う"]);
    type_str(&mut engine, "aiu");
    engine.process_key(&press_key(Keysym::SPACE));
    commit_first(&mut engine);

    engine.process_key(&press_ctrl(Keysym::BACKSPACE));
    type_str(&mut engine, "aiu");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_ne!(
        engine.state().candidates().unwrap().selected_text(),
        Some("藍う")
    );
}

#[test]
fn a_different_reading_is_not_demoted() {
    let mut engine = engine();
    seed_model_cache(&mut engine, "アイウ", "", &["藍う"]);
    convert_ai(&mut engine);
    commit_first(&mut engine);

    engine.process_key(&press_key(Keysym::BACKSPACE));
    type_str(&mut engine, "aiu");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(
        engine.state().candidates().unwrap().selected_text(),
        Some("藍う")
    );
}

#[test]
fn the_next_commit_clears_the_rejection() {
    let mut engine = engine();
    convert_ai(&mut engine);
    commit_first(&mut engine);
    engine.process_key(&press_key(Keysym::BACKSPACE));
    convert_ai(&mut engine);
    commit_first(&mut engine); // accepts the new head
    assert!(engine.rejected_commit.is_none());
}
