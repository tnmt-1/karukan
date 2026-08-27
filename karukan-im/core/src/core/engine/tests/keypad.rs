//! Keypad handling tests (fork-ported, issue #51).
//!
//! Standard IMEs treat the numeric keypad as direct input: keypad digits
//! never select conversion candidates and keypad symbols bypass the romaji
//! table (keypad '/' must stay '/', not become ・).

use super::*;

#[test]
fn keypad_digit_from_empty_starts_composition_literally() {
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_key(Keysym::KP_1));
    assert!(result.consumed);
    assert_eq!(engine.preedit().unwrap().text(), "1");
    assert!(matches!(engine.state(), InputState::Composing { .. }));
}

#[test]
fn keypad_slash_from_empty_is_literal_not_katakana_dot() {
    // Main-row '/' → ・ via romaji rules; keypad '/' stays '/'.
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_key(Keysym::KP_DIVIDE));
    assert!(result.consumed);
    assert_eq!(engine.preedit().unwrap().text(), "/");
}

#[test]
fn keypad_decimal_and_minus_are_literal() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_key(Keysym::KP_1));
    engine.process_key(&press_key(Keysym::KP_DECIMAL));
    engine.process_key(&press_key(Keysym::KP_5));
    engine.process_key(&press_key(Keysym::KP_SUBTRACT));
    engine.process_key(&press_key(Keysym::KP_0));
    assert_eq!(engine.preedit().unwrap().text(), "1.5-0");
}

#[test]
fn keypad_digit_during_composing_inserts_literally() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let result = engine.process_key(&press_key(Keysym::KP_2));
    assert!(result.consumed);
    assert_eq!(engine.preedit().unwrap().text(), "あい2");
}

#[test]
fn keypad_enter_commits_composition() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    let result = engine.process_key(&press_key(Keysym::KP_ENTER));
    assert!(result.consumed);
    let commit_text = result
        .actions
        .iter()
        .find_map(|a| {
            if let EngineAction::Commit(t) = a {
                Some(t.clone())
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(commit_text, "あ");
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn keypad_enter_in_empty_passes_through() {
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_key(Keysym::KP_ENTER));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn keypad_enter_commits_conversion() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let result = engine.process_key(&press_key(Keysym::KP_ENTER));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}
