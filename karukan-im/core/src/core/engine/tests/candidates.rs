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
    // num_suggestions bounds how many rows the conversion window shows at
    // once, not how many candidates it holds: the ones past the first page
    // must still be in the list, a keypress away.
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

// --- conversion window height (collapsed until the 4th candidate) tests ---

/// Engine whose conversion list is longer than `num_suggestions`: the dict
/// fixture's four surfaces plus the kana fallbacks.
fn engine_with_collapsed_conversion(num_suggestions: usize) -> InputMethodEngine {
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        num_suggestions,
        ..EngineConfig::default()
    });
    engine.dicts.system = Some(dict_with_many_candidates());
    engine
}

/// The candidate list the conversion window is showing.
fn conversion_list(engine: &InputMethodEngine) -> &CandidateList {
    engine.state().candidates().expect("conversion candidates")
}

#[test]
fn conversion_window_opens_at_num_suggestions() {
    let mut engine = engine_with_collapsed_conversion(3);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let list = conversion_list(&engine);
    assert!(
        list.len() > 3,
        "fixture must have more candidates than num_suggestions, got {}",
        list.len()
    );
    assert_eq!(
        list.page_candidates().len(),
        3,
        "the conversion window must open at num_suggestions rows"
    );
}

#[test]
fn conversion_window_expands_on_fourth_candidate() {
    let mut engine = engine_with_collapsed_conversion(3);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    // Space opens the conversion on the 1st candidate; two more walk to the
    // 3rd, the last one the first page shows.
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_key(Keysym::SPACE));
    let list = conversion_list(&engine);
    assert_eq!(list.cursor(), 2);
    assert_eq!(
        list.page_candidates().len(),
        3,
        "still inside the first page, so the window must not have grown"
    );

    engine.process_key(&press_key(Keysym::SPACE));
    let list = conversion_list(&engine);
    assert_eq!(list.cursor(), 3, "the 4th candidate must be selected");
    assert_eq!(list.page_size(), CandidateList::DEFAULT_PAGE_SIZE);
    assert_eq!(
        list.page_candidates().len(),
        CandidateList::DEFAULT_PAGE_SIZE.min(list.len()),
        "a full page must be shown once expanded"
    );
}

#[test]
fn expanded_conversion_window_stays_expanded() {
    let mut engine = engine_with_collapsed_conversion(3);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    for _ in 0..4 {
        engine.process_key(&press_key(Keysym::SPACE));
    }
    assert_eq!(
        conversion_list(&engine).page_size(),
        CandidateList::DEFAULT_PAGE_SIZE
    );

    // Back to the top: the window must not shrink again.
    for _ in 0..3 {
        engine.process_key(&press_key(Keysym::UP));
    }
    let list = conversion_list(&engine);
    assert_eq!(list.cursor(), 0);
    assert_eq!(
        list.page_size(),
        CandidateList::DEFAULT_PAGE_SIZE,
        "the window must stay expanded for the rest of the conversion"
    );
}

#[test]
fn filtered_view_starts_collapsed_again() {
    let mut engine = engine_with_collapsed_conversion(3);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    for _ in 0..4 {
        engine.process_key(&press_key(Keysym::SPACE));
    }
    assert_eq!(
        conversion_list(&engine).page_size(),
        CandidateList::DEFAULT_PAGE_SIZE
    );

    // Ctrl+T twice: learning (empty), then the 📚 view with the fixture's
    // four surfaces.
    engine.process_key(&press_ctrl(Keysym::KEY_T));
    engine.process_key(&press_ctrl(Keysym::KEY_T));
    let list = conversion_list(&engine);
    assert_eq!(
        list.len(),
        4,
        "the 📚 view must hold the fixture's surfaces"
    );
    assert_eq!(
        list.page_candidates().len(),
        3,
        "a narrowed view is a fresh list, so it starts collapsed again"
    );
}

#[test]
fn composing_suggestions_are_never_paginated() {
    let mut engine = engine_with_collapsed_conversion(3);

    engine.process_key(&press('a'));
    let result = engine.process_key(&press('i'));

    let texts = show_candidates_texts(&result);
    assert_eq!(texts.len(), 3);
    assert_eq!(
        engine.shown_suggestions.total_pages(),
        1,
        "the composing list is trimmed, not paginated — a page indicator \
         here would show up at every keystroke"
    );
}

#[test]
fn conversion_window_never_opens_past_a_full_page() {
    let mut engine = engine_with_collapsed_conversion(10);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let list = conversion_list(&engine);
    assert!(list.len() > CandidateList::DEFAULT_PAGE_SIZE);
    assert_eq!(
        list.page_candidates().len(),
        CandidateList::DEFAULT_PAGE_SIZE,
        "num_suggestions above a full page must cap at one"
    );
}

#[test]
fn restoring_a_selection_keeps_the_window_expanded() {
    let mut engine = engine_with_collapsed_conversion(3);

    // Convert only 「あい」, leaving 「うえお」 as a segment to its right.
    for ch in ['a', 'i', 'u', 'e', 'o'] {
        engine.process_key(&press(ch));
    }
    for _ in 0..3 {
        engine.process_key(&press_key(Keysym::LEFT));
    }
    for _ in 0..4 {
        engine.process_key(&press_key(Keysym::SPACE));
    }
    let list = conversion_list(&engine);
    assert_eq!(list.cursor(), 3);
    assert_eq!(list.page_size(), CandidateList::DEFAULT_PAGE_SIZE);
    let selected = list.selected_text().expect("selection").to_string();

    // Into the next segment and back. The first segment's list is rebuilt
    // from scratch and the selection restored into it, so the window must
    // come back at the height it had, not at the collapsed one.
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::LEFT));

    let list = conversion_list(&engine);
    assert_eq!(
        list.selected_text(),
        Some(selected.as_str()),
        "test setup: the segment must restore its previous selection"
    );
    assert_eq!(
        list.page_size(),
        CandidateList::DEFAULT_PAGE_SIZE,
        "a restored selection past the first page must keep the window expanded"
    );
}

#[test]
fn segment_roundtrip_keeps_the_window_expanded_from_the_first_candidate() {
    let mut engine = engine_with_collapsed_conversion(3);

    // Same setup as above, but walk back to the first candidate before
    // leaving the segment: the cursor no longer says the window was
    // expanded, so the segment has to remember it.
    for ch in ['a', 'i', 'u', 'e', 'o'] {
        engine.process_key(&press(ch));
    }
    for _ in 0..3 {
        engine.process_key(&press_key(Keysym::LEFT));
    }
    for _ in 0..4 {
        engine.process_key(&press_key(Keysym::SPACE));
    }
    for _ in 0..3 {
        engine.process_key(&press_key(Keysym::UP));
    }
    let list = conversion_list(&engine);
    assert_eq!(list.cursor(), 0);
    assert_eq!(list.page_size(), CandidateList::DEFAULT_PAGE_SIZE);
    let selected = list.selected_text().expect("selection").to_string();

    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::LEFT));

    let list = conversion_list(&engine);
    assert_eq!(
        list.selected_text(),
        Some(selected.as_str()),
        "test setup: the segment must restore its previous selection"
    );
    assert_eq!(
        list.page_size(),
        CandidateList::DEFAULT_PAGE_SIZE,
        "the window must stay expanded across the segment round trip"
    );
}

#[test]
fn deleting_a_learning_entry_keeps_the_window_expanded() {
    let mut engine = engine_with_collapsed_conversion(3);
    let mut cache = LearningCache::new(LearningConfig::default());
    cache.record("あい", "藍");
    engine.learning = Some(cache);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    for _ in 0..4 {
        engine.process_key(&press_key(Keysym::SPACE));
    }
    assert_eq!(
        conversion_list(&engine).page_size(),
        CandidateList::DEFAULT_PAGE_SIZE
    );

    // Back to the learning candidate at the top and delete it. The list is
    // rebuilt from scratch, which must not undo the height.
    for _ in 0..3 {
        engine.process_key(&press_key(Keysym::UP));
    }
    assert!(
        conversion_list(&engine)
            .selected()
            .is_some_and(|c| c.is_deletable()),
        "test setup: the learning candidate must be selected"
    );
    engine.process_key(&press_ctrl(Keysym::BACKSPACE));

    assert!(engine.learning.as_ref().unwrap().lookup("あい").is_empty());
    assert_eq!(
        conversion_list(&engine).page_size(),
        CandidateList::DEFAULT_PAGE_SIZE,
        "deleting a learning entry rebuilds the list, but it is the same \
         conversion, so the window stays expanded"
    );
}

#[test]
fn a_new_conversion_starts_collapsed_again() {
    let mut engine = engine_with_collapsed_conversion(3);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    for _ in 0..4 {
        engine.process_key(&press_key(Keysym::SPACE));
    }
    assert_eq!(
        conversion_list(&engine).page_size(),
        CandidateList::DEFAULT_PAGE_SIZE
    );

    engine.process_key(&press_key(Keysym::RETURN));
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let list = conversion_list(&engine);
    assert_eq!(
        list.page_candidates().len(),
        3,
        "the next conversion opens at num_suggestions rows again"
    );
}
