//! Single-kanji candidates: the whole reading as one kanji, among the
//! rewriter variants. No kanji model is loaded.

use super::*;
use crate::config::settings::PredictionMode;

fn select_by_arrow(engine: &mut InputMethodEngine, text: &str) {
    let list = engine.candidates().unwrap();
    assert_eq!(list.cursor(), 0);
    let index = list
        .candidates()
        .iter()
        .position(|c| c.text == text)
        .expect(text);
    for _ in 0..index {
        engine.process_key(&press_key(Keysym::DOWN));
    }
    assert_eq!(engine.candidates().unwrap().selected_text(), Some(text));
}

/// Mixed list for `reading` as `(text, source, description)`.
fn mixed_list(
    engine: &mut InputMethodEngine,
    reading: &str,
) -> Vec<(String, CandidateSource, Option<String>)> {
    let lookup = engine.primary_lookup();
    engine
        .build_conversion_candidates(reading, reading, "", 9, lookup)
        .into_iter()
        .map(|c| (c.text, c.source, c.description))
        .collect()
}

fn position(list: &[(String, CandidateSource, Option<String>)], text: &str) -> usize {
    list.iter()
        .position(|(t, _, _)| t == text)
        .unwrap_or_else(|| panic!("{text} missing from {list:?}"))
}

#[test]
fn single_kanji_sit_behind_the_kana_pair_before_the_width_variants() {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    let list = mixed_list(&mut engine, "たか");

    // たか・タカ, then every single kanji in one run, then ﾀｶ.
    let first = position(&list, "考");
    let last = position(&list, "髙");
    assert!(position(&list, "タカ") < first, "list: {list:?}");
    assert!(last < position(&list, "ﾀｶ"), "list: {list:?}");
    assert!(
        list[first..=last].iter().all(|(text, source, _)| {
            *source == CandidateSource::Rewriter && text.chars().count() == 1
        }),
        "list: {list:?}"
    );
    assert!(
        list.iter().any(|(text, _, _)| text == "高"),
        "list: {list:?}"
    );
}

#[test]
fn single_kanji_dedup_against_higher_sources() {
    let mut engine = engine_with_learned("たか", "高");
    let list = mixed_list(&mut engine, "たか");
    let rows: Vec<_> = list.iter().filter(|(text, _, _)| text == "高").collect();
    assert_eq!(rows.len(), 1, "list: {list:?}");
    assert_eq!(rows[0].1, CandidateSource::Learning);
}

#[test]
fn kanji_used_under_another_reading_moves_up() {
    // 高 committed as こう: for たか it is not a learning row (different
    // reading) but heads the single kanji, ahead of the data order; the
    // unlearned keep that order.
    let mut engine = engine_with_learned("こう", "高");
    let list = mixed_list(&mut engine, "たか");
    let takai = position(&list, "高");
    assert_eq!(list[takai].1, CandidateSource::Rewriter);
    assert!(takai < position(&list, "考"), "list: {list:?}");
    assert!(
        position(&list, "考") < position(&list, "嵩"),
        "list: {list:?}"
    );
}

#[test]
fn ranking_stays_inside_the_single_kanji() {
    // ﾀｶ committed under another reading scores too, but it is not a
    // single kanji: it keeps its slot behind them.
    let mut engine = engine_with_learned("はんかく", "ﾀｶ");
    let list = mixed_list(&mut engine, "たか");
    assert!(
        position(&list, "髙") < position(&list, "ﾀｶ"),
        "list: {list:?}"
    );
}

#[test]
fn variant_note_decorates_the_same_kanji_from_any_source() {
    // The learning row for 髙 wins the dedup; the note the single-kanji
    // row carried still shows on it, in every list.
    let mut engine = engine_with_learned("たか", "髙");
    for ch in "taka".chars() {
        engine.process_key(&press(ch));
    }
    let note_on = |list: &CandidateList| {
        let row = list
            .candidates()
            .iter()
            .find(|c| c.text == "髙")
            .expect("髙 shown");
        assert_eq!(row.source, Some(CandidateSource::Learning));
        row.description.clone()
    };
    // Composing suggestions.
    assert_eq!(
        note_on(&engine.shown_suggestions).as_deref(),
        Some("高の異体字")
    );
    // Mixed list.
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(
        note_on(engine.candidates().unwrap()).as_deref(),
        Some("高の異体字")
    );
    let plain = engine
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .find(|c| c.text == "高");
    assert_eq!(plain.and_then(|c| c.description.clone()), None);
    // 📝 learning view.
    engine.process_key(&press_ctrl(Keysym::KEY_T));
    assert_eq!(
        note_on(engine.candidates().unwrap()).as_deref(),
        Some("高の異体字")
    );
}

#[test]
fn rewriter_view_lists_the_single_kanji() {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    for ch in "taka".chars() {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));

    let result = engine.process_key(&press_ctrl(Keysym::KEY_R));
    let aux = last_aux_text(&result).expect("aux");
    assert!(aux.contains("[変換:🔄]"), "aux: {aux}");
    let list = engine.candidates().expect("candidates");
    let kasa = list
        .candidates()
        .iter()
        .find(|c| c.text == "嵩")
        .expect("嵩 in the rewriter view");
    assert_eq!(kasa.source, Some(CandidateSource::Rewriter));
    assert_eq!(kasa.reading.as_deref(), Some("たか"));
}

#[test]
fn single_kanji_preserve_the_configured_space_tab_roles() {
    for prediction in [
        PredictionMode::Separate,
        PredictionMode::Merged,
        PredictionMode::Off,
    ] {
        for swap in [false, true] {
            for key in [Keysym::SPACE, Keysym::TAB] {
                let mut engine = composing_with("たか");
                engine.config.prediction = prediction;
                engine.config.swap_space_tab = swap;
                let mut cache = LearningCache::new(LearningConfig::default());
                cache.record("たか", "高");
                cache.record("たかだい", "高台");
                engine.learning = Some(cache);
                engine.process_key(&press_key(key));
                let rows = engine.candidates().unwrap().candidates();
                let primary = (key == Keysym::SPACE) != swap;
                let predictive = match prediction {
                    PredictionMode::Separate => !primary,
                    PredictionMode::Merged => primary,
                    PredictionMode::Off => false,
                };
                let skip = !primary && prediction != PredictionMode::Separate;
                let context = format!("{prediction:?}, swap={swap}, key={key:?}");
                assert_eq!(
                    rows.iter().any(|c| c.text == "高台"),
                    predictive,
                    "{context}"
                );
                let high: Vec<_> = rows.iter().filter(|c| c.text == "高").collect();
                assert_eq!(high.len(), 1, "{context}");
                assert_eq!(
                    high[0].source,
                    Some(if skip {
                        CandidateSource::Rewriter
                    } else {
                        CandidateSource::Learning
                    }),
                    "{context}"
                );
                let variant = rows.iter().find(|c| c.text == "髙").expect("catalog 髙");
                assert_eq!(variant.source, Some(CandidateSource::Rewriter));
                assert_eq!(variant.description.as_deref(), Some("高の異体字"));
            }
        }
    }
}

#[test]
fn single_kanji_survive_paging_and_segment_round_trip() {
    let mut engine = composing_with("たかさ");
    engine.config.num_suggestions = 1;
    engine.learning = Some(LearningCache::new(LearningConfig::default()));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(engine.candidates().unwrap().page_size(), 1);
    select_by_arrow(&mut engine, "髙");
    assert_eq!(
        engine.candidates().unwrap().page_size(),
        CandidateList::DEFAULT_PAGE_SIZE
    );
    assert!(engine.candidates().unwrap().cursor() >= CandidateList::DEFAULT_PAGE_SIZE);
    engine.process_key(&press_key(Keysym::RIGHT));
    select_by_arrow(&mut engine, "佐");
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.candidates().unwrap().selected_text(), Some("髙"));
    assert_eq!(engine.preedit().unwrap().text(), "髙佐");
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(engine.candidates().unwrap().selected_text(), Some("佐"));
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(t) if t == "髙佐"))
    );
    let learning = engine.learning.as_ref().unwrap();
    assert!(learning.lookup("たか").iter().any(|(text, _)| text == "髙"));
    assert!(learning.lookup("さ").iter().any(|(text, _)| text == "佐"));
    assert!(learning.lookup("たかさ").is_empty());
}

#[test]
fn typing_with_single_kanji_respects_live_conversion() {
    for live in [false, true] {
        let mut engine = composing_with("たか");
        engine.live.enabled = live;
        engine.process_key(&press_ctrl(Keysym::KEY_R));
        select_by_arrow(&mut engine, "髙");
        let result = engine.process_key(&press('s'));
        let commits: Vec<_> = result
            .actions
            .iter()
            .filter_map(|a| match a {
                EngineAction::Commit(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        if live {
            assert!(commits.is_empty());
            assert_eq!(engine.input_buf.display(), "たかs");
            assert!(matches!(engine.state(), InputState::Conversion { .. }));
            assert!(last_aux_text(&result).unwrap().contains("[変換:🔄]"));
        } else {
            assert_eq!(commits, ["髙"]);
            assert_eq!(engine.input_buf.display(), "s");
            assert!(matches!(engine.state(), InputState::Composing { .. }));
        }
    }
}

#[test]
fn collapsed_lists_keep_variant_notes_on_dictionary_and_model_rows() {
    for source in [
        CandidateSource::UserDictionary,
        CandidateSource::Dictionary,
        CandidateSource::Model,
    ] {
        let mut engine = composing_with("たか");
        engine.config.num_suggestions = 1;
        let json = r#"[{"reading":"たか","candidates":[{"surface":"髙","score":0.0}]}]"#;
        match source {
            CandidateSource::UserDictionary => engine.dicts.user = Some(dict_from_json(json)),
            CandidateSource::Dictionary => engine.dicts.system = Some(dict_from_json(json)),
            CandidateSource::Model => seed_model_cache(&mut engine, "タカ", "", &["髙"]),
            _ => unreachable!(),
        }
        engine.process_key(&press_key(Keysym::SPACE));
        let list = engine.candidates().unwrap();
        assert_eq!(list.page_size(), 1);
        let variants: Vec<_> = list
            .candidates()
            .iter()
            .filter(|c| c.text == "髙")
            .collect();
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].source, Some(source));
        assert_eq!(variants[0].description.as_deref(), Some("高の異体字"));
    }
}

#[test]
fn deleting_a_learned_variant_keeps_its_catalog_fallback() {
    let mut engine = engine_with_learned("たか", "髙");
    engine.input_buf.insert("たか");
    engine.state = InputState::Composing {
        preedit: engine.build_composing_preedit(),
    };
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(engine.candidates().unwrap().selected_text(), Some("髙"));
    engine.process_key(&press_ctrl(Keysym::DELETE));
    assert!(engine.learning.as_ref().unwrap().lookup("たか").is_empty());
    let list = engine.candidates().unwrap();
    let variant = list
        .candidates()
        .iter()
        .find(|c| c.text == "髙")
        .expect("catalog 髙");
    assert_eq!(variant.source, Some(CandidateSource::Rewriter));
    assert_eq!(variant.description.as_deref(), Some("高の異体字"));
    assert!(!variant.is_deletable());
}
