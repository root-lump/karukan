use karukan_engine::Dictionary;

use super::*;

// --- Candidate preservation tests ---

#[test]
fn test_live_text_preserved_in_conversion_via_down() {
    // When DOWN is pressed during live conversion, the AI inference result
    // (live_conversion_text) should appear in the candidate list.
    let mut engine = make_live_conversion_engine();

    // Simulate typing "あい" with live conversion active
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    set_live_text(&mut engine, "愛");

    // Press DOWN → start_conversion()
    let result = engine.process_key(&press_key(Keysym::DOWN));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // The candidate list should contain "愛"
    let candidates = engine.state().candidates().unwrap();
    assert!(
        candidates.candidates().iter().any(|c| c.text == "愛"),
        "AI inference result '愛' should be in the candidate list"
    );
}

#[test]
fn test_live_text_not_duplicated_in_conversion() {
    // If the live_text matches the reading, it should not be duplicated
    let mut engine = make_live_conversion_engine();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    // live_conversion_text same as hiragana reading → should not be added
    set_live_text(&mut engine, "あい");

    let result = engine.process_key(&press_key(Keysym::DOWN));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // "あい" should not appear twice (it's same as reading, so live_text is skipped)
    let candidates = engine.state().candidates().unwrap();
    let count = candidates
        .candidates()
        .iter()
        .filter(|c| c.text == "あい")
        .count();
    assert_eq!(count, 1, "Reading should appear exactly once");
}

#[test]
fn test_suggest_result_preserved_in_start_conversion() {
    // When Space is pressed, the previous auto-suggest/live conversion result
    // should be preserved in the candidate list even if re-inference doesn't produce it.
    // (Without a kanji converter, build_conversion_candidates returns fallback only,
    // so the live_conversion_text would be lost without the preservation logic.)
    let mut engine = InputMethodEngine::new();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    set_live_text(&mut engine, "愛");

    // Press Space → start_conversion()
    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // "愛" should be preserved in the candidate list
    let candidates = engine.state().candidates().unwrap();
    assert!(
        candidates.candidates().iter().any(|c| c.text == "愛"),
        "Previous suggest result '愛' should be preserved in candidates"
    );
}

// --- num_suggestions (composing suggestion window size) tests ---

/// Dict fixture with several exact-match candidates for a single reading,
/// so the composing suggestion list has more entries than a small
/// `num_suggestions` can hold.
fn dict_with_many_candidates() -> Dictionary {
    dict_from_json(
        r#"[{"reading":"あい","candidates":[
            {"surface":"愛","score":1000.0},
            {"surface":"藍","score":900.0},
            {"surface":"哀","score":800.0},
            {"surface":"合","score":700.0}
        ]}]"#,
    )
}

/// Texts from the most recent `ShowCandidates` action in `result`.
fn show_candidates_texts(result: &EngineResult) -> Vec<String> {
    result
        .actions
        .iter()
        .find_map(|a| match a {
            EngineAction::ShowCandidates(list) => {
                Some(list.candidates().iter().map(|c| c.text.clone()).collect())
            }
            _ => None,
        })
        .expect("ShowCandidates action")
}

#[test]
fn composing_suggestions_truncated_to_num_suggestions() {
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        num_suggestions: 2,
        ..EngineConfig::default()
    });
    engine.dicts.system = Some(dict_with_many_candidates());

    engine.process_key(&press('a'));
    let result = engine.process_key(&press('i'));

    let texts = show_candidates_texts(&result);
    assert_eq!(
        texts.len(),
        2,
        "num_suggestions=2 must cap the composing list at 2, got {texts:?}"
    );
    // The stored list (what Ctrl+digit indexes) must match the display,
    // not just the rendered `ShowCandidates` action.
    assert_eq!(engine.shown_suggestions.len(), 2);
}

#[test]
fn composing_suggestions_not_truncated_below_num_suggestions() {
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        num_suggestions: 10,
        ..EngineConfig::default()
    });
    engine.dicts.system = Some(dict_with_many_candidates());

    engine.process_key(&press('a'));
    let result = engine.process_key(&press('i'));

    let texts = show_candidates_texts(&result);
    for surface in ["愛", "藍", "哀", "合"] {
        assert!(
            texts.iter().any(|t| t == surface),
            "num_suggestions=10 must not drop `{surface}`, got {texts:?}"
        );
    }
}

#[test]
fn composing_suggestions_zero_clamped_to_one() {
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        num_suggestions: 0,
        ..EngineConfig::default()
    });
    engine.dicts.system = Some(dict_with_many_candidates());

    engine.process_key(&press('a'));
    let result = engine.process_key(&press('i'));

    let texts = show_candidates_texts(&result);
    assert_eq!(
        texts.len(),
        1,
        "num_suggestions=0 must clamp to 1, got {texts:?}"
    );
}

#[test]
fn conversion_candidates_ignore_num_suggestions() {
    // Space's conversion candidate list is a separate path from the
    // composing suggestion window, so a small num_suggestions must not
    // shrink it.
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        num_suggestions: 1,
        ..EngineConfig::default()
    });
    engine.dicts.system = Some(dict_with_many_candidates());

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let candidates = engine.state().candidates().expect("conversion candidates");
    let texts: Vec<&str> = candidates
        .candidates()
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    assert!(
        texts.len() > 1,
        "num_suggestions must not affect the Space conversion list, got {texts:?}"
    );
    for surface in ["愛", "藍", "哀", "合"] {
        assert!(
            texts.contains(&surface),
            "missing `{surface}`, got {texts:?}"
        );
    }
}

#[test]
fn test_empty_live_text_not_added_to_candidates() {
    // When live_conversion_text is empty, no extra candidate should be added
    let mut engine = make_live_conversion_engine();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    // Force empty to test the "no live text" scenario
    engine.live.shown = false;

    // DOWN → start_conversion()
    let result = engine.process_key(&press_key(Keysym::DOWN));
    assert!(result.consumed);

    // Should have candidates but no empty-string candidate
    if let Some(candidates) = engine.state().candidates() {
        assert!(
            !candidates.candidates().iter().any(|c| c.text.is_empty()),
            "Empty candidate should not be in the list"
        );
    }
}
