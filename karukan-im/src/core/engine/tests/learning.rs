//! Tests for the learning cache and the Tab-skips-learning behavior.
//!
//! Space/Down: include learning candidates (default conversion).
//! Tab: skip learning candidates (lets users escape stale learned entries).

use karukan_engine::LearningCache;

use super::*;

/// Engine seeded with a learning entry `reading → surface`, no kanji model.
/// We bypass `init.rs` (which gates learning on settings + file I/O) and just
/// inject a populated `LearningCache` directly — these tests assert the
/// build_conversion_candidates branching, not the load path.
fn engine_with_learned(reading: &str, surface: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    let mut cache = LearningCache::new(100);
    cache.record(reading, surface);
    engine.learning = Some(cache);
    engine
}

#[test]
fn build_candidates_includes_learning_when_not_skipped() {
    let mut engine = engine_with_learned("あい", "藍");

    let texts: Vec<String> = engine
        .build_conversion_candidates("あい", 9, false)
        .into_iter()
        .map(|c| c.text)
        .collect();

    assert!(
        texts.contains(&"藍".to_string()),
        "Space path (skip_learning=false) should surface learned `藍`, got {:?}",
        texts,
    );
}

#[test]
fn build_candidates_omits_learning_when_skipped() {
    let mut engine = engine_with_learned("あい", "藍");

    let texts: Vec<String> = engine
        .build_conversion_candidates("あい", 9, true)
        .into_iter()
        .map(|c| c.text)
        .collect();

    assert!(
        !texts.contains(&"藍".to_string()),
        "Tab path (skip_learning=true) must drop learned `藍`, got {:?}",
        texts,
    );
}

#[test]
fn tab_key_skips_learning_in_composing() {
    // End-to-end: type the reading, press Tab → learned candidate is gone.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.input_buf.text, "あい");

    let result = engine.process_key(&press_key(Keysym::TAB));
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
        !texts.contains(&"藍".to_string()),
        "Tab must skip the learned `藍` candidate, got {:?}",
        texts,
    );
}

#[test]
fn ctrl_delete_removes_selected_learning_candidate() {
    // Type the reading, Space → conversion with the learned candidate on
    // top (cursor 0), Ctrl+Delete → the learning entry is gone from both
    // the cache and the rebuilt candidate list (Mozc's DeleteSelectedCandidate).
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(
        engine.state().candidates().unwrap().selected_text(),
        Some("藍"),
        "learned candidate must be preselected",
    );

    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    assert!(result.consumed);
    assert_eq!(engine.learning.as_ref().unwrap().entry_count(), 0);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::ShowCandidates(_))),
        "candidate list must be re-shown after deletion",
    );

    let texts: Vec<String> = engine
        .state()
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect();
    assert!(
        !texts.contains(&"藍".to_string()),
        "deleted `藍` must disappear from the rebuilt list, got {:?}",
        texts,
    );
}

#[test]
fn ctrl_delete_on_non_learning_candidate_is_noop() {
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    // Move off the learned candidate onto a non-learning one.
    engine.process_key(&press_key(Keysym::SPACE));
    let selected = engine
        .state()
        .candidates()
        .unwrap()
        .selected()
        .cloned()
        .unwrap();
    assert!(!selected.is_learning);

    let before: Vec<String> = engine
        .state()
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect();

    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    // Consumed as a no-op: an unconsumed Ctrl+Delete would reach the app
    // and edit text behind the open candidate window.
    assert!(result.consumed);
    assert_eq!(engine.learning.as_ref().unwrap().entry_count(), 1);

    let after: Vec<String> = engine
        .state()
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect();
    assert_eq!(before, after, "candidate list must be unchanged");
}

#[test]
fn ctrl_delete_prefix_match_uses_full_reading() {
    // The learned entry is a prefix match (typing `あ` surfaces あした→明日,
    // whose candidate carries the full reading). Ctrl+Delete must remove
    // the cache entry under the full reading, not the typed prefix.
    let mut engine = engine_with_learned("あした", "明日");

    engine.process_key(&press('a'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(
        engine.state().candidates().unwrap().selected_text(),
        Some("明日"),
        "prefix-matched learned candidate must be preselected",
    );

    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    assert!(result.consumed);
    assert_eq!(engine.learning.as_ref().unwrap().entry_count(), 0);
}

#[test]
fn ctrl_delete_keeps_other_learned_entries() {
    let mut engine = engine_with_learned("きょう", "今日");
    engine.learning.as_mut().unwrap().record("きょう", "京");

    for ch in ['k', 'y', 'o', 'u'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    // Cursor 0 is the higher-scored learned candidate; delete whichever is
    // selected and verify the sibling survives in cache and list.
    let deleted = engine
        .state()
        .candidates()
        .unwrap()
        .selected_text()
        .unwrap()
        .to_string();
    let sibling = if deleted == "今日" { "京" } else { "今日" };

    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    assert!(result.consumed);
    assert_eq!(engine.learning.as_ref().unwrap().entry_count(), 1);

    let candidates = engine.state().candidates().unwrap().candidates().to_vec();
    assert!(
        candidates
            .iter()
            .any(|c| c.text == sibling && c.is_learning),
        "sibling learned entry `{}` must survive, got {:?}",
        sibling,
        candidates.iter().map(|c| &c.text).collect::<Vec<_>>(),
    );
    assert!(
        !candidates
            .iter()
            .any(|c| c.text == deleted && c.is_learning),
        "deleted `{}` must not remain as a learning candidate",
        deleted,
    );
}

#[test]
fn plain_backspace_still_cancels_conversion() {
    // Regression guard: the Ctrl+Delete branch must not affect the
    // modifier-less Backspace, which returns to Composing.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let result = engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.learning.as_ref().unwrap().entry_count(), 1);
}

#[test]
fn learning_candidates_carry_is_learning_flag() {
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let candidates = engine.state().candidates().unwrap().candidates().to_vec();
    for c in &candidates {
        assert_eq!(
            c.is_learning,
            c.text == "藍",
            "only the learned `藍` may carry is_learning, got {:?}",
            candidates,
        );
    }
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
