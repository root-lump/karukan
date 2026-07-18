//! Tests for the learning cache and the Space/Tab conversion split.
//!
//! Space/Down: convert exactly the typed reading — exact-match learning
//! candidates only, never predictions beyond what was typed.
//! Tab at end of buffer: predictive conversion — learned readings that
//! extend beyond the typed prefix are offered as selectable candidates.
//! Tab mid-buffer: degrades to the Space behavior (convert up to cursor).
//! Ctrl+Delete: delete the selected learning candidate from the history.

use karukan_engine::{LearningCache, LearningConfig};

use super::*;
use crate::core::engine::conversion::LearningLookup;
use crate::core::engine::display::LEARNING_DELETE_HINT;

/// Engine seeded with a learning entry `reading → surface`, no kanji model.
/// We bypass `init.rs` (which gates learning on settings + file I/O) and just
/// inject a populated `LearningCache` directly — these tests assert the
/// build_conversion_candidates branching, not the load path.
fn engine_with_learned(reading: &str, surface: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    let mut cache = LearningCache::new(LearningConfig::default());
    cache.record(reading, surface);
    engine.learning = Some(cache);
    engine
}

/// Candidate texts currently shown in the Conversion state.
fn conversion_texts(engine: &InputMethodEngine) -> Vec<String> {
    engine
        .state()
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect()
}

#[test]
fn build_candidates_includes_exact_learning() {
    let mut engine = engine_with_learned("あい", "藍");

    let texts: Vec<String> = engine
        .build_conversion_candidates("あい", 9, LearningLookup::Exact)
        .into_iter()
        .map(|c| c.text)
        .collect();

    assert!(
        texts.contains(&"藍".to_string()),
        "exact-match learned `藍` must appear on the Space path, got {:?}",
        texts,
    );
}

#[test]
fn build_candidates_excludes_predictions_when_not_predictive() {
    let mut engine = engine_with_learned("あいさつ", "挨拶");

    let texts: Vec<String> = engine
        .build_conversion_candidates("あい", 9, LearningLookup::Exact)
        .into_iter()
        .map(|c| c.text)
        .collect();

    assert!(
        !texts.contains(&"挨拶".to_string()),
        "Space path must not surface the prediction `挨拶` for `あい`, got {:?}",
        texts,
    );
}

#[test]
fn build_candidates_includes_predictions_when_predictive() {
    let mut engine = engine_with_learned("あいさつ", "挨拶");

    let candidates = engine.build_conversion_candidates("あい", 9, LearningLookup::Predictive);
    let predicted = candidates
        .iter()
        .find(|c| c.text == "挨拶")
        .unwrap_or_else(|| panic!("Tab path must surface the prediction `挨拶`"));

    // The prediction carries its full reading so committing it records
    // learning under `あいさつ`, not the typed prefix `あい`.
    assert_eq!(predicted.reading.as_deref(), Some("あいさつ"));
}

#[test]
fn tab_at_end_offers_predictions() {
    // End-to-end: type a prefix, press Tab at the end of the buffer →
    // the learned longer reading's surface is selectable.
    let mut engine = engine_with_learned("あいさつ", "挨拶");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.input_buf.text, "あい");

    let result = engine.process_key(&press_key(Keysym::TAB));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let texts = conversion_texts(&engine);
    assert!(
        texts.contains(&"挨拶".to_string()),
        "Tab at end must offer the prediction `挨拶`, got {:?}",
        texts,
    );
}

#[test]
fn tab_keeps_learning_candidates() {
    // Tab does not skip the learning cache: an exact-match learned
    // candidate stays visible on the Tab path.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    let result = engine.process_key(&press_key(Keysym::TAB));
    assert!(result.consumed);

    let texts = conversion_texts(&engine);
    assert!(
        texts.contains(&"藍".to_string()),
        "Tab must keep the learned `藍` candidate, got {:?}",
        texts,
    );
}

#[test]
fn tab_mid_buffer_behaves_like_space() {
    // Cursor moved off the end: Tab converts only up to the cursor and
    // must not surface predictions.
    let mut engine = engine_with_learned("あいさつ", "挨拶");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('s'));
    engine.process_key(&press('a'));
    assert_eq!(engine.input_buf.text, "あいさ");

    // Move cursor left once: あい|さ
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.input_buf.cursor_pos, 2);

    let result = engine.process_key(&press_key(Keysym::TAB));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Converted reading is bounded by the cursor; the rest is the tail.
    assert_eq!(engine.input_buf.text, "あい");
    assert_eq!(engine.conversion_tail.as_deref(), Some("さ"));

    let texts = conversion_texts(&engine);
    assert!(
        !texts.contains(&"挨拶".to_string()),
        "Tab mid-buffer must not offer the prediction `挨拶`, got {:?}",
        texts,
    );
}

#[test]
fn space_key_excludes_predictions_in_composing() {
    // End-to-end: Space on a typed prefix must not show the learned longer
    // reading's surface.
    let mut engine = engine_with_learned("あいさつ", "挨拶");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let texts = conversion_texts(&engine);
    assert!(
        !texts.contains(&"挨拶".to_string()),
        "Space must not offer the prediction `挨拶`, got {:?}",
        texts,
    );
}

#[test]
fn tab_commit_records_learning() {
    // Selecting a prediction via Tab and committing must record learning
    // under the prediction's full reading.
    let mut engine = engine_with_learned("あいさつ", "挨拶");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::TAB));

    // Select the `挨拶` candidate.
    let idx = {
        let candidates = engine.state().candidates().unwrap();
        candidates
            .candidates()
            .iter()
            .position(|c| c.text == "挨拶")
            .expect("prediction `挨拶` must be present")
    };
    for _ in 0..idx {
        engine.process_key(&press_key(Keysym::TAB));
    }
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(result.consumed);

    // Learning must be recorded under the prediction's full reading, not
    // under the typed prefix — a record for `あい → 挨拶` would make the
    // 2-char reading auto-commit 4 chars' worth of text later.
    let cache = engine.learning.as_ref().unwrap();
    assert!(
        cache.lookup("あいさつ").iter().any(|(s, _)| s == "挨拶"),
        "committing via Tab must record `あいさつ → 挨拶` in the learning cache",
    );
    assert!(
        !cache.lookup("あい").iter().any(|(s, _)| s == "挨拶"),
        "the typed prefix `あい` must not learn the prediction's surface",
    );
}

#[test]
fn ctrl_delete_removes_selected_learning_entry() {
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Learning candidates are force-pushed first, so the learned entry is
    // the initial selection.
    let selected = engine
        .state()
        .candidates()
        .unwrap()
        .selected()
        .unwrap()
        .clone();
    assert_eq!(selected.text, "藍");
    assert!(
        selected.is_deletable(),
        "learning candidate must be flagged"
    );

    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    assert!(result.consumed);
    // The entry is gone from the cache...
    assert!(engine.learning.as_ref().unwrap().lookup("あい").is_empty());
    // ...and the window stays up: the conversion is rebuilt in place,
    // staying in Conversion.
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::ShowCandidates(_))),
        "deletion must refresh the candidate window, not close it"
    );
    assert!(
        !result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::HideCandidates)),
        "deletion must not hide the candidate window"
    );

    // `藍` is no longer a *learning* candidate. It may still return from the
    // model or dictionary (deleting history doesn't blacklist a surface — the
    // whole point of rebuilding instead of dropping the row), but never again
    // flagged as user history.
    let candidates = engine.state().candidates().unwrap();
    assert!(
        !candidates
            .candidates()
            .iter()
            .any(|c| c.text == "藍" && c.is_deletable()),
        "`藍` must no longer be a learning candidate after deletion",
    );
    // The rebuilt list reopens at the top.
    assert_eq!(candidates.cursor(), 0);
}

#[test]
fn ctrl_delete_removes_prefix_twins_so_surface_does_not_resurface() {
    // The same surface learned under two prefix-related readings is shown as a
    // single deduped row; deleting it must clear both, or the twin under the
    // longer reading pops back on the next conversion of the same input.
    let mut engine = engine_with_learned("あい", "藍");
    engine.learning.as_mut().unwrap().record("あいさ", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let selected = engine
        .state()
        .candidates()
        .unwrap()
        .selected()
        .unwrap()
        .clone();
    assert_eq!(selected.text, "藍");
    assert!(selected.is_deletable());

    engine.process_key(&press_ctrl(Keysym::DELETE));
    // Both the exact and the prefix entry are gone.
    assert!(engine.learning.as_ref().unwrap().lookup("あい").is_empty());
    assert!(
        engine
            .learning
            .as_ref()
            .unwrap()
            .lookup("あいさ")
            .is_empty(),
        "the prefix twin (あいさ→藍) must be cleared too, not just the exact entry",
    );
}

#[test]
fn ctrl_delete_keeps_surface_that_another_source_also_produces() {
    // #2 regression: the learned surface equals the hiragana reading, which
    // the fallback ALWAYS produces. That fallback copy is deduped away under
    // the learning entry; deleting the entry must bring it back (now
    // non-learning) rather than remove the only row — which is why deletion
    // rebuilds the conversion instead of dropping the candidate in place.
    let mut engine = engine_with_learned("あい", "あい");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let selected = engine
        .state()
        .candidates()
        .unwrap()
        .selected()
        .unwrap()
        .clone();
    assert_eq!(selected.text, "あい");
    assert!(selected.is_deletable());

    engine.process_key(&press_ctrl(Keysym::DELETE));
    assert!(engine.learning.as_ref().unwrap().lookup("あい").is_empty());

    // `あい` survives as an ordinary fallback candidate.
    let candidates = engine.state().candidates().unwrap();
    let ai = candidates.candidates().iter().find(|c| c.text == "あい");
    assert!(
        ai.is_some(),
        "the fallback `あい` must survive the deletion, not vanish with the \
         learning entry",
    );
    assert!(
        !ai.unwrap().is_deletable(),
        "the surviving `あい` must no longer be flagged as learning",
    );
}

#[test]
fn init_learning_cache_applies_configured_surface_cap() {
    // Guards the config→cache seam: if init_learning_cache stops applying
    // max_surface_chars, the 6-char surface (over the configured 5, under
    // the default 50) gets recorded and this fails.
    let mut engine = InputMethodEngine::new();
    engine.init_learning_cache(
        true,
        LearningConfig {
            max_entries: 10_000,
            max_surface_chars: 5,
        },
    );
    let cache = engine.learning.as_mut().expect("learning enabled");

    let before = cache.entry_count();
    cache.record("__karukan_seam_test__", &"漢".repeat(6));
    assert_eq!(
        cache.entry_count(),
        before,
        "a surface over the configured cap must be skipped; the configured \
         value is not reaching the cache",
    );
}

#[test]
fn ctrl_backspace_deletes_learning_entry_like_ctrl_delete() {
    // Mac keyboards label the Backspace key "delete", so the natural macOS
    // chord is Ctrl+delete = Ctrl+Backspace; it must behave like Ctrl+Delete
    // (forward delete), not like the plain-Backspace cancel.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let result = engine.process_key(&press_ctrl(Keysym::BACKSPACE));
    assert!(result.consumed);
    assert!(engine.learning.as_ref().unwrap().lookup("あい").is_empty());
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
}

#[test]
fn ctrl_backspace_does_nothing_for_non_learning_candidate() {
    // When the selection isn't a learning candidate, Ctrl+Backspace (like
    // Ctrl+Delete) is consumed so it can't leak to the app mid-conversion,
    // but the conversion is left intact. Cancelling stays on plain
    // Backspace / Escape.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    // Move the selection off the learning candidate.
    engine.process_key(&press_key(Keysym::SPACE));
    let before = engine.state().candidates().unwrap().clone();
    assert!(!before.selected().unwrap().is_deletable());

    let result = engine.process_key(&press_ctrl(Keysym::BACKSPACE));
    assert!(
        result.consumed,
        "the chord must be consumed, not leak to the app"
    );
    assert!(
        matches!(engine.state(), InputState::Conversion { .. }),
        "Ctrl+Backspace must not cancel when the selection isn't deletable"
    );
    // Nothing changed: same selection, same list, history intact.
    let after = engine.state().candidates().unwrap();
    assert_eq!(after.cursor(), before.cursor());
    assert_eq!(after.selected_text(), before.selected_text());
    assert!(!engine.learning.as_ref().unwrap().lookup("あい").is_empty());
}

#[test]
fn ctrl_backspace_in_composing_deletes_char_not_history() {
    // History deletion is a Conversion-state-only chord. During Composing,
    // Ctrl+Backspace edits text like plain Backspace, even while a learning
    // suggestion is on screen.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));

    let result = engine.process_key(&press_ctrl(Keysym::BACKSPACE));
    assert!(result.consumed);
    assert_eq!(engine.input_buf.text, "あ");
    assert!(
        !engine.learning.as_ref().unwrap().lookup("あい").is_empty(),
        "the learning entry must survive — deletion only works in Conversion",
    );
}

#[test]
fn ctrl_alt_delete_leaves_history_alone() {
    // Ctrl+Alt+Delete is a desktop chord, not ours. The delete arm guards on
    // `!alt_key` like its siblings, so the key passes through untouched
    // instead of irreversibly purging a history entry.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(
        engine
            .state()
            .candidates()
            .unwrap()
            .selected()
            .unwrap()
            .is_deletable()
    );

    let result = engine.process_key(&press_ctrl_alt(Keysym::DELETE));
    assert!(!result.consumed, "Ctrl+Alt+Delete must reach the desktop");
    assert!(
        !engine.learning.as_ref().unwrap().lookup("あい").is_empty(),
        "Ctrl+Alt+Delete must not delete the learning entry"
    );
}

#[test]
fn plain_backspace_still_cancels_conversion() {
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let result = engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(result.consumed);
    // Backspace without Ctrl keeps its cancel-to-composing behavior and
    // deletes nothing from the history.
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert!(!engine.learning.as_ref().unwrap().lookup("あい").is_empty());
}

#[test]
fn ctrl_delete_ignores_non_learning_candidate() {
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    // Move the selection off the learning candidate onto a fallback one.
    engine.process_key(&press_key(Keysym::SPACE));
    let selected = engine
        .state()
        .candidates()
        .unwrap()
        .selected()
        .unwrap()
        .clone();
    assert!(!selected.is_deletable());

    let before_len = engine.state().candidates().unwrap().len();
    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    // The key is consumed (it must not leak to the app mid-conversion) but
    // nothing is deleted and the conversion continues.
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(engine.state().candidates().unwrap().len(), before_len);
    assert!(!engine.learning.as_ref().unwrap().lookup("あい").is_empty());
}

#[test]
fn ctrl_delete_removes_prefix_matched_entry_by_full_reading() {
    // A prefix-matched learning candidate carries its own (longer) reading;
    // deletion must remove the cache entry under that full reading. On this
    // fork predictions surface via Tab (Space is exact-match only).
    let mut engine = engine_with_learned("あいさつ", "挨拶");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::TAB));

    let selected = engine
        .state()
        .candidates()
        .unwrap()
        .selected()
        .unwrap()
        .clone();
    assert_eq!(selected.text, "挨拶");
    assert_eq!(selected.reading.as_deref(), Some("あいさつ"));
    assert!(selected.is_deletable());

    engine.process_key(&press_ctrl(Keysym::DELETE));
    assert!(
        engine
            .learning
            .as_ref()
            .unwrap()
            .lookup("あいさつ")
            .is_empty()
    );
}

#[test]
fn aux_shows_delete_hint_only_for_learning_candidate() {
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    // Learning candidate selected → aux carries the deletion hint.
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let aux = last_aux_text(&result).expect("conversion must update aux text");
    assert!(
        aux.contains(LEARNING_DELETE_HINT),
        "aux should show the deletion hint for a learning candidate, got {:?}",
        aux,
    );

    // Moving to a non-learning candidate drops the hint.
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let aux = last_aux_text(&result).expect("navigation must update aux text");
    assert!(
        !aux.contains(LEARNING_DELETE_HINT),
        "aux must not show the deletion hint for non-learning candidates, got {:?}",
        aux,
    );
}

#[test]
fn space_key_keeps_learning_in_composing() {
    // Counterpart to tab_key_skips_learning_in_composing: Space stays on the
    // learning-included path so the default UX is unchanged.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let texts: Vec<String> = engine
        .state()
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect();
    assert!(
        texts.contains(&"藍".to_string()),
        "Space must surface learned `藍`, got {:?}",
        texts,
    );
}

// --- prediction / swap_space_tab settings ---

use crate::config::settings::PredictionMode;

/// Engine with the given prediction/swap settings and a learned entry.
fn engine_with_learned_config(
    reading: &str,
    surface: &str,
    prediction: PredictionMode,
    swap_space_tab: bool,
) -> InputMethodEngine {
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        prediction,
        swap_space_tab,
        ..EngineConfig::default()
    });
    engine.converters.kanji = None;
    let mut cache = LearningCache::new(LearningConfig::default());
    cache.record(reading, surface);
    engine.learning = Some(cache);
    engine
}

#[test]
fn merged_space_includes_predictions() {
    // prediction = "merged": the primary conversion (Space) mixes in
    // predictive learning candidates, upstream-style.
    let mut engine = engine_with_learned_config("あいさつ", "挨拶", PredictionMode::Merged, false);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    assert_eq!(
        engine.candidates().and_then(|c| c.selected_text()),
        Some("挨拶"),
        "merged mode must surface the prediction on Space"
    );
}

#[test]
fn merged_tab_skips_learning() {
    // prediction = "merged": the secondary key becomes the learning-free
    // conversion (escape hatch), like the historical Tab behavior.
    let mut engine = engine_with_learned_config("あい", "藍", PredictionMode::Merged, false);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::TAB));

    let texts = conversion_texts(&engine);
    assert!(
        !texts.contains(&"藍".to_string()),
        "merged mode's Tab must skip learned candidates, got {:?}",
        texts,
    );
}

#[test]
fn off_hides_predictions_everywhere() {
    // prediction = "off": no predictions in the composing suggestion window
    // nor in either conversion.
    let mut engine = engine_with_learned_config("あいさつ", "挨拶", PredictionMode::Off, false);

    engine.process_key(&press('a'));
    let result = engine.process_key(&press('i'));

    // Composing suggestion window must not show the prediction.
    for action in &result.actions {
        if let EngineAction::ShowCandidates(list) = action {
            assert!(
                !list.candidates().iter().any(|c| c.text == "挨拶"),
                "off mode must hide predictions from the suggestion window"
            );
        }
    }

    // Neither Space nor Tab surfaces it.
    engine.process_key(&press_key(Keysym::SPACE));
    let texts = conversion_texts(&engine);
    assert!(!texts.contains(&"挨拶".to_string()));
}

#[test]
fn swap_space_tab_swaps_conversion_roles() {
    // swap_space_tab = true with the default "separate" prediction:
    // Space becomes the predictive conversion, Tab the exact one.
    let mut engine = engine_with_learned_config("あいさつ", "挨拶", PredictionMode::Separate, true);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    let texts = conversion_texts(&engine);
    assert!(
        texts.contains(&"挨拶".to_string()),
        "swapped Space must offer predictions, got {:?}",
        texts,
    );
    engine.process_key(&press_key(Keysym::ESCAPE));

    engine.process_key(&press_key(Keysym::TAB));
    let texts = conversion_texts(&engine);
    assert!(
        !texts.contains(&"挨拶".to_string()),
        "swapped Tab must convert the typed reading only, got {:?}",
        texts,
    );
}
