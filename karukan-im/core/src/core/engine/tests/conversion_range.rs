//! Shift+←/→ resize what the conversion covers (fork extension).

use super::*;
use crate::core::preedit::{AttributeType, PreeditAttribute};

/// かんじにしたい converted, with 漢字 learned for かんじ.
fn converted_kanjinishitai() -> InputMethodEngine {
    let mut engine = engine_with_learned("かんじ", "漢字");
    for ch in "kanjinishitai".chars() {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(engine.state.reading(), Some("かんじにしたい"));
    engine
}

fn narrow(engine: &mut InputMethodEngine, times: usize) -> EngineResult {
    let mut result = EngineResult::consumed();
    for _ in 0..times {
        result = engine.process_key(&press_shift_key(Keysym::LEFT));
    }
    result
}

fn commit_text(result: &EngineResult) -> Option<&str> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(text) => Some(text.as_str()),
        _ => None,
    })
}

#[test]
fn test_shift_left_narrows_the_head_and_shows_the_rest_as_kana() {
    let mut engine = converted_kanjinishitai();
    narrow(&mut engine, 4);

    assert_eq!(engine.state.range(), Some(3));
    assert_eq!(engine.state.reading(), Some("かんじ"));
    let preedit = engine.preedit().unwrap();
    assert_eq!(preedit.text(), "漢字|にしたい");
    assert_eq!(
        preedit.attributes(),
        [
            PreeditAttribute::new(0, 2, AttributeType::Highlight),
            PreeditAttribute::new(2, 3, AttributeType::Underline),
            PreeditAttribute::new(3, 7, AttributeType::Underline),
        ]
    );
    assert_eq!(preedit.caret(), 2);
}

#[test]
fn test_enter_commits_the_head_and_keeps_the_rest_composing() {
    let mut engine = converted_kanjinishitai();
    narrow(&mut engine, 4);

    let result = engine.process_key(&press_key(Keysym::RETURN));

    assert_eq!(commit_text(&result), Some("漢字"));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.input_buf.display(), "にしたい");

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text(&result), Some("にしたい"));
    assert!(engine.state().is_empty());
}

#[test]
fn test_digit_commits_the_head_only() {
    let mut engine = converted_kanjinishitai();
    narrow(&mut engine, 4);

    let result = engine.process_key(&press('1'));

    assert_eq!(commit_text(&result), Some("漢字"));
    assert_eq!(engine.input_buf.display(), "にしたい");
}

#[test]
fn test_shift_right_back_to_the_end_converts_the_whole_reading() {
    let mut engine = converted_kanjinishitai();
    narrow(&mut engine, 2);
    assert_eq!(engine.state.range(), Some(5));

    engine.process_key(&press_shift_key(Keysym::RIGHT));
    engine.process_key(&press_shift_key(Keysym::RIGHT));

    assert_eq!(engine.state.range(), None);
    assert_eq!(engine.state.reading(), Some("かんじにしたい"));
}

#[test]
fn test_head_stops_at_one_char() {
    let mut engine = converted_kanjinishitai();
    narrow(&mut engine, 10);

    assert_eq!(engine.state.range(), Some(1));
    assert_eq!(engine.state.reading(), Some("か"));
}

#[test]
fn test_escape_restores_the_whole_composition() {
    let mut engine = converted_kanjinishitai();
    narrow(&mut engine, 4);

    engine.process_key(&press_key(Keysym::ESCAPE));

    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.input_buf.display(), "かんじにしたい");
}

#[test]
fn test_focus_out_commits_the_head_with_the_rest() {
    let mut engine = converted_kanjinishitai();
    narrow(&mut engine, 4);

    assert_eq!(engine.commit(), "漢字にしたい");
    assert!(engine.state().is_empty());
}
