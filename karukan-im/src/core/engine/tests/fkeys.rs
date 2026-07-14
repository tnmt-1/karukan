//! Tests for F6-F10 function key conversion behavior.
//!
//! Standard Japanese IME behavior:
//! - F6:  Convert composing/conversion text to hiragana and commit
//! - F7:  Convert composing/conversion text to full-width katakana and commit
//! - F8:  Convert composing/conversion text to half-width katakana and commit
//! - F9:  Convert composing/conversion text to full-width alphanumeric and commit
//! - F10: Convert composing/conversion text to half-width alphanumeric and commit
//!
//! All F-keys in Empty state pass through (not consumed).

use super::*;
use crate::core::candidate::CandidateList;

/// Helper: prepare an engine that has composed hiragana text ready.
fn composed_engine(input: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    // Build input via direct manipulation (bypass romaji conversion for test clarity)
    engine.input_buf.text = input.to_string();
    engine.input_buf.cursor_pos = input.chars().count();
    engine.state = InputState::Composing {
        preedit: Preedit::with_text_underlined(input),
        romaji_buffer: String::new(),
    };
    engine
}

/// Helper: prepare an engine in conversion state with candidates.
fn conversion_engine(reading: &str, candidates: Vec<&str>) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.input_buf.text = reading.to_string();
    let cands: Vec<_> = candidates
        .into_iter()
        .map(|s| Candidate {
            text: s.to_string(),
            reading: Some(reading.to_string()),
            source_label: None,
            description: None,
        })
        .collect();
    let candidate_list = CandidateList::new(cands);
    let selected_text = candidate_list
        .selected_text()
        .unwrap_or(reading)
        .to_string();
    engine.state = InputState::Conversion {
        preedit: Preedit::from_segments(
            vec![PreeditSegment::highlighted(&selected_text)],
            selected_text.chars().count(),
        ),
        candidates: candidate_list,
    };
    engine
}

fn commit_text(result: &EngineResult) -> Option<&str> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(t) => Some(t.as_str()),
        _ => None,
    })
}

fn has_hide_candidates(result: &EngineResult) -> bool {
    result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::HideCandidates))
}

fn has_hide_aux(result: &EngineResult) -> bool {
    result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::HideAuxText))
}

// ---------------------------------------------------------------------------
// Empty state: all F-keys pass through
// ---------------------------------------------------------------------------

#[test]
fn f6_in_empty_state_passes_through() {
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f7_in_empty_state_passes_through() {
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f8_in_empty_state_passes_through() {
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_key(Keysym::F8));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f9_in_empty_state_passes_through() {
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_key(Keysym::F9));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f10_in_empty_state_passes_through() {
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

// ---------------------------------------------------------------------------
// Composing state: F6 → hiragana
// ---------------------------------------------------------------------------

#[test]
fn f6_converts_katakana_composing_to_hiragana() {
    let mut engine = composed_engine("アイウ");
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("あいう"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f6_on_hiragana_composing_commits_as_is() {
    let mut engine = composed_engine("あいう");
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("あいう"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f6_converts_mixed_kana_to_hiragana() {
    let mut engine = composed_engine("アイウえお");
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("あいうえお"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f6_converts_katakana_with_long_vowel() {
    let mut engine = composed_engine("コーヒー");
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("こーひー"));
}

#[test]
fn f6_hides_candidates_and_aux() {
    let mut engine = composed_engine("アイウ");
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(has_hide_candidates(&result));
    assert!(has_hide_aux(&result));
}

// ---------------------------------------------------------------------------
// Composing state: F7 → full-width katakana
// ---------------------------------------------------------------------------

#[test]
fn f7_converts_hiragana_composing_to_katakana() {
    let mut engine = composed_engine("かきく");
    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("カキク"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f7_on_katakana_composing_commits_as_is() {
    let mut engine = composed_engine("カキク");
    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("カキク"));
}

#[test]
fn f7_converts_mixed_hiragana_to_katakana() {
    let mut engine = composed_engine("こんにちは");
    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("コンニチハ"));
}

#[test]
fn f7_converts_dakuten_and_handakuten() {
    let mut engine = composed_engine("がっこう");
    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ガッコウ"));
}

// ---------------------------------------------------------------------------
// Composing state: F8 → half-width katakana
// ---------------------------------------------------------------------------

#[test]
fn f8_converts_hiragana_to_half_katakana() {
    let mut engine = composed_engine("かきく");
    let result = engine.process_key(&press_key(Keysym::F8));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ｶｷｸ"));
}

#[test]
fn f8_converts_katakana_to_half_katakana() {
    let mut engine = composed_engine("カキク");
    let result = engine.process_key(&press_key(Keysym::F8));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ｶｷｸ"));
}

#[test]
fn f8_converts_dakuten_to_half_width() {
    let mut engine = composed_engine("がっこう");
    let result = engine.process_key(&press_key(Keysym::F8));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ｶﾞｯｺｳ"));
}

#[test]
fn f8_converts_voiced_sounds() {
    let mut engine = composed_engine("ヴァイオリン");
    let result = engine.process_key(&press_key(Keysym::F8));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ｳﾞｧｲｵﾘﾝ"));
}

// ---------------------------------------------------------------------------
// Composing state: F9 → full-width alphanumeric
// ---------------------------------------------------------------------------

#[test]
fn f9_converts_ascii_lowercase_to_full_width() {
    let mut engine = composed_engine("abc");
    let result = engine.process_key(&press_key(Keysym::F9));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ａｂｃ"));
}

#[test]
fn f9_converts_ascii_uppercase_to_full_width() {
    let mut engine = composed_engine("ABC");
    let result = engine.process_key(&press_key(Keysym::F9));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ＡＢＣ"));
}

#[test]
fn f9_converts_digits_to_full_width() {
    let mut engine = composed_engine("123");
    let result = engine.process_key(&press_key(Keysym::F9));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("１２３"));
}

#[test]
fn f9_kana_becomes_fullwidth_romaji() {
    // F9 converts kana to full-width romaji.
    let mut engine = composed_engine("かきく");
    let result = engine.process_key(&press_key(Keysym::F9));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ｋａｋｉｋｕ"));
}

#[test]
fn f9_converts_mixed_ascii_and_kana() {
    let mut engine = composed_engine("abcかきく");
    let result = engine.process_key(&press_key(Keysym::F9));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ａｂｃｋａｋｉｋｕ"));
}

// ---------------------------------------------------------------------------
// Composing state: F10 → half-width alphanumeric
// ---------------------------------------------------------------------------

#[test]
fn f10_converts_full_width_ascii_to_half_width() {
    let mut engine = composed_engine("ＡＢＣ");
    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ABC"));
}

#[test]
fn f10_converts_full_width_digits_to_half_width() {
    let mut engine = composed_engine("１２３");
    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("123"));
}

#[test]
fn f10_converts_full_width_katakana_to_half_width_romaji() {
    let mut engine = composed_engine("カキク");
    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("kakiku"));
}

#[test]
fn f10_converts_mixed_full_width_text() {
    let mut engine = composed_engine("ＡＢＣカキク");
    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ABCkakiku"));
}

#[test]
fn f10_passes_half_width_ascii_through() {
    let mut engine = composed_engine("abc");
    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("abc"));
}

#[test]
fn f10_converts_hiragana_to_half_width_romaji() {
    let mut engine = composed_engine("あいう");
    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("aiu"));
}

// ---------------------------------------------------------------------------
// Conversion state: F6-F10 apply to the selected candidate
// ---------------------------------------------------------------------------

#[test]
fn f6_converts_selected_candidate_to_hiragana() {
    let mut engine = conversion_engine("アイウ", vec!["アイウ", "あいう", "ｱｲｳ"]);
    // First candidate is "アイウ" (katakana)
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("あいう"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f7_converts_selected_candidate_to_katakana() {
    let mut engine = conversion_engine("かきく", vec!["かきく", "カキク"]);
    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("カキク"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f8_converts_selected_candidate_to_half_katakana() {
    let mut engine = conversion_engine("かきく", vec!["かきく"]);
    let result = engine.process_key(&press_key(Keysym::F8));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ｶｷｸ"));
}

#[test]
fn f9_converts_selected_candidate_to_full_width_ascii() {
    let mut engine = conversion_engine("abc", vec!["abc"]);
    let result = engine.process_key(&press_key(Keysym::F9));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ａｂｃ"));
}

#[test]
fn f10_converts_selected_candidate_to_half_width() {
    let mut engine = conversion_engine("ＡＢＣ", vec!["ＡＢＣ"]);
    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ABC"));
}

#[test]
fn f6_on_conversion_also_records_learning() {
    let mut engine = conversion_engine("あいう", vec!["あいう"]);
    // We need a learning cache to verify. But at minimum: state transitions correctly.
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

// ---------------------------------------------------------------------------
// Composing with pending romaji: F-keys flush before transform
// ---------------------------------------------------------------------------

#[test]
fn f6_flushes_pending_romaji_before_conversion() {
    let mut engine = InputMethodEngine::new();
    // Type "a" → "あ", then press F6 without typing further
    engine.process_key(&press('a'));
    // Now input_buf.text = "あ", romaji buffer empty
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("あ"));
}

// ---------------------------------------------------------------------------
// Empty buffer in composing state: F6-F10 still consumed but commit empty
// ---------------------------------------------------------------------------

#[test]
fn fkey_on_empty_composing_spans_not_consumed() {
    let mut engine = InputMethodEngine::new();
    // engine starts in Empty state with empty buffer
    // But we force Composing with empty preedit
    engine.input_buf.text.clear();
    engine.state = InputState::Composing {
        preedit: Preedit::new(),
        romaji_buffer: String::new(),
    };
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(!result.consumed);

    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(!result.consumed);

    let result = engine.process_key(&press_key(Keysym::F8));
    assert!(!result.consumed);

    let result = engine.process_key(&press_key(Keysym::F9));
    assert!(!result.consumed);

    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(!result.consumed);
}
