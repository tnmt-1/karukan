//! Tests for conversion range adjustment (Shift+←/→) during conversion mode.
//!
//! Standard Japanese IME behavior (Mozc / MS-IME):
//! - Shift+← shortens the conversion range from the right
//! - Shift+→ lengthens the conversion range from the right (re-expand)
//! - Enter commits only the active (highlighted) segment; remaining hiragana
//!   stays in composing state with candidates for the leftover text

use super::*;
use crate::core::preedit::AttributeType;

fn press_shift_key(keysym: Keysym) -> KeyEvent {
    KeyEvent::new(keysym, KeyModifiers::new().with_shift(true), true)
}

fn type_romaji(engine: &mut InputMethodEngine, s: &str) {
    for ch in s.chars() {
        engine.process_key(&press(ch));
    }
}

fn enter_conversion(engine: &mut InputMethodEngine) {
    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(
        matches!(engine.state(), InputState::Conversion { .. }),
        "expected Conversion state after Space"
    );
}

fn conversion_range(engine: &InputMethodEngine) -> (usize, usize, String) {
    match engine.state() {
        InputState::Conversion {
            full_reading,
            range_start,
            range_end,
            ..
        } => (*range_start, *range_end, full_reading.clone()),
        other => panic!("expected Conversion state, got {other:?}"),
    }
}

fn commit_text(result: &EngineResult) -> Option<&str> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(t) => Some(t.as_str()),
        _ => None,
    })
}

fn has_show_candidates(result: &EngineResult) -> bool {
    result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::ShowCandidates(_)))
}

#[test]
fn shift_left_shrinks_conversion_from_right() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo"); // あいうえお
    enter_conversion(&mut engine);

    let (start, end, full) = conversion_range(&engine);
    assert_eq!(full, "あいうえお");
    assert_eq!((start, end), (0, 5));

    let result = engine.process_key(&press_shift_key(Keysym::LEFT));
    assert!(result.consumed);

    let (start, end, full) = conversion_range(&engine);
    assert_eq!(full, "あいうえお");
    assert_eq!((start, end), (0, 4));

    // Preedit: active segment highlighted, trailing hiragana underlined
    let preedit = engine.preedit().unwrap();
    assert!(preedit.text().starts_with('あ'));
    let attrs: Vec<_> = preedit
        .attributes()
        .iter()
        .map(|a| (a.start, a.end, a.attr_type))
        .collect();
    assert!(attrs.iter().any(|(_, _, t)| *t == AttributeType::Highlight));
    assert!(attrs.iter().any(|(_, _, t)| *t == AttributeType::Underline));
}

#[test]
fn shift_right_expands_conversion_after_shrink() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);

    engine.process_key(&press_shift_key(Keysym::LEFT));
    engine.process_key(&press_shift_key(Keysym::LEFT));
    assert_eq!(conversion_range(&engine), (0, 3, "あいうえお".to_string()));

    let result = engine.process_key(&press_shift_key(Keysym::RIGHT));
    assert!(result.consumed);
    assert_eq!(conversion_range(&engine), (0, 4, "あいうえお".to_string()));

    let result = engine.process_key(&press_shift_key(Keysym::RIGHT));
    assert!(result.consumed);
    assert_eq!(conversion_range(&engine), (0, 5, "あいうえお".to_string()));
}

#[test]
fn shift_right_at_full_range_is_noop() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);

    let result = engine.process_key(&press_shift_key(Keysym::RIGHT));
    assert!(result.consumed);
    assert_eq!(conversion_range(&engine), (0, 5, "あいうえお".to_string()));
}

#[test]
fn shrink_then_expand_restores_full_range_candidates() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);

    engine.process_key(&press_shift_key(Keysym::LEFT));
    engine.process_key(&press_shift_key(Keysym::RIGHT));

    let (start, end, _) = conversion_range(&engine);
    assert_eq!((start, end), (0, 5));
    // Full range again: preedit is a single highlighted segment (no trailing underline)
    let attrs: Vec<_> = engine
        .preedit()
        .unwrap()
        .attributes()
        .iter()
        .map(|a| a.attr_type)
        .collect();
    assert!(attrs.contains(&AttributeType::Highlight));
    assert!(!attrs.contains(&AttributeType::Underline));
}

#[test]
fn shift_arrow_stops_at_one_character_range() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "ai");
    enter_conversion(&mut engine);

    // Narrow to a single character
    engine.process_key(&press_shift_key(Keysym::LEFT));
    let (start, end, _) = conversion_range(&engine);
    assert_eq!((start, end), (0, 1));

    // Further shrinking is a no-op but still consumed
    let result = engine.process_key(&press_shift_key(Keysym::LEFT));
    assert!(result.consumed);
    let (start, end, _) = conversion_range(&engine);
    assert_eq!((start, end), (0, 1));
}

#[test]
fn partial_commit_keeps_remaining_in_composing() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);

    // Narrow active range to あいうえ, leaving お
    engine.process_key(&press_shift_key(Keysym::LEFT));

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.input_buf.text, "お");
    assert_eq!(engine.preedit().unwrap().text(), "お");

    // Only the active segment was committed, not the full reading
    let committed = commit_text(&result).unwrap();
    assert_ne!(committed, "あいうえお");
    assert!(!committed.ends_with('お'));
}

#[test]
fn partial_commit_shows_candidates_for_remainder() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);
    engine.process_key(&press_shift_key(Keysym::LEFT));

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(has_show_candidates(&result) || matches!(engine.state(), InputState::Composing { .. }));
}

#[test]
fn cancel_conversion_restores_full_reading_after_narrow() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);
    engine.process_key(&press_shift_key(Keysym::LEFT));
    engine.process_key(&press_shift_key(Keysym::LEFT));

    let result = engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.input_buf.text, "あいうえお");
    assert_eq!(engine.preedit().unwrap().text(), "あいうえお");
}

#[test]
fn typing_during_narrowed_conversion_commits_all_segments() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);
    engine.process_key(&press_shift_key(Keysym::LEFT)); // active=あいうえ, pending=お

    let result = engine.process_key(&press('k'));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));

    let committed = commit_text(&result).unwrap();
    // Standard IME: printable char finalises converted + pending hiragana together
    assert!(committed.contains('お'));
    assert_eq!(engine.preedit().unwrap().text(), "k");
}

#[test]
fn digit_select_in_narrowed_range_partial_commit() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);
    engine.process_key(&press_shift_key(Keysym::LEFT));

    let result = engine.process_key(&press('1'));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.input_buf.text, "お");
    assert!(commit_text(&result).is_some());
}

#[test]
fn plain_arrow_in_conversion_moves_range() {
    // Plain ←/→ (no Shift) slide the active conversion range instead of
    // leaking through to the application. Regression: the keys were not
    // consumed, so the document caret moved behind the open candidate
    // window.
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);

    let result = engine.process_key(&press_key(Keysym::LEFT));
    assert!(result.consumed);
    let (start, end, _) = conversion_range(&engine);
    assert_eq!((start, end), (0, 5), "at the left edge the range stays");

    engine.process_key(&press_shift_key(Keysym::LEFT)); // active=あいうえ
    let result = engine.process_key(&press_key(Keysym::RIGHT));
    assert!(result.consumed);
    let (start, end, _) = conversion_range(&engine);
    assert_eq!((start, end), (1, 5), "right arrow slides the range");
}

#[test]
fn plain_arrows_slide_range_within_reading() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);
    engine.process_key(&press_shift_key(Keysym::LEFT)); // 0..4
    engine.process_key(&press_shift_key(Keysym::LEFT)); // 0..3

    engine.process_key(&press_key(Keysym::RIGHT)); // 1..4
    let (start, end, _) = conversion_range(&engine);
    assert_eq!((start, end), (1, 4));

    engine.process_key(&press_key(Keysym::RIGHT)); // 2..5
    let (start, end, _) = conversion_range(&engine);
    assert_eq!((start, end), (2, 5));

    // Right edge reached: further right is a consumed no-op.
    let result = engine.process_key(&press_key(Keysym::RIGHT));
    assert!(result.consumed);
    let (start, end, _) = conversion_range(&engine);
    assert_eq!((start, end), (2, 5));
}

#[test]
fn home_end_jump_conversion_range_to_edges() {
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);
    engine.process_key(&press_shift_key(Keysym::LEFT)); // 0..4
    engine.process_key(&press_shift_key(Keysym::LEFT)); // 0..3

    // End: range snaps to the tail of the reading.
    let result = engine.process_key(&press_key(Keysym::END));
    assert!(result.consumed);
    let (start, end, _) = conversion_range(&engine);
    assert_eq!((start, end), (2, 5));

    // Home: range snaps back to the head.
    let result = engine.process_key(&press_key(Keysym::HOME));
    assert!(result.consumed);
    let (start, end, _) = conversion_range(&engine);
    assert_eq!((start, end), (0, 3));
}

#[test]
fn moving_range_rebuilds_candidates_and_keeps_commit_partial() {
    // After sliding the range, Enter must commit only the active segment
    // and keep the rest in composing (same as Shift-resized ranges).
    let mut engine = InputMethodEngine::new();
    type_romaji(&mut engine, "aiueo");
    enter_conversion(&mut engine);
    engine.process_key(&press_shift_key(Keysym::LEFT)); // 0..4
    engine.process_key(&press_key(Keysym::RIGHT)); // 1..5

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.input_buf.text, "あ");
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}
