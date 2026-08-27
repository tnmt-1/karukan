//! Tests for F6-F10 function key conversion behavior (fork-ported).
//!
//! Standard Japanese IME behavior:
//! - F6:  Convert composing/conversion text to hiragana and commit
//! - F7:  Convert composing/conversion text to full-width katakana and commit
//! - F8:  Convert composing/conversion text to half-width katakana and commit
//! - F9:  Convert composing/conversion text to full-width alphanumeric and commit
//! - F10: Convert composing/conversion text to half-width alphanumeric and commit
//!
//! All F-keys in Empty state pass through (not consumed). F-keys with Ctrl
//! or Alt modifiers pass through (application shortcuts). Note: the fork's
//! Ctrl+J hiragana chord is NOT ported — upstream binds Ctrl+J to the
//! live-conversion chunk break (issue #87); F6 covers hiragana.

use super::*;
use crate::core::candidate::CandidateList;

/// Helper: prepare an engine that has composed text ready.
/// Characters are inserted as direct elements (no romaji conversion), so
/// katakana/ascii strings can be composed for transformation tests.
fn composed_engine(input: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    for ch in input.chars() {
        engine.input_buf.push_direct(ch);
    }
    engine.state = InputState::Composing {
        preedit: Preedit::with_text_underlined(input),
    };
    engine
}

/// Helper: prepare an engine in conversion state with candidates.
fn conversion_engine(reading: &str, candidates: Vec<&str>) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    for ch in reading.chars() {
        engine.input_buf.push_direct(ch);
    }
    let cands: Vec<_> = candidates
        .into_iter()
        .map(|s| Candidate {
            text: s.to_string(),
            reading: Some(reading.to_string()),
            source: None,
            description: None,
        })
        .collect();
    let candidate_list = CandidateList::new(cands);
    let selected_text = candidate_list
        .selected_text()
        .unwrap_or(reading)
        .to_string();
    engine.state = InputState::Conversion {
        preedit: Preedit::with_text_highlighted(&selected_text),
        candidates: candidate_list,
        reading: reading.to_string(),
        filter: None,
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

fn with_learning_cache(mut engine: InputMethodEngine) -> InputMethodEngine {
    engine.learning = Some(karukan_engine::LearningCache::new(
        karukan_engine::LearningConfig {
            max_entries: 100,
            max_surface_chars: 50,
        },
    ));
    engine
}

fn learning_surfaces(engine: &InputMethodEngine, reading: &str) -> Vec<String> {
    engine
        .learning
        .as_ref()
        .unwrap()
        .lookup(reading)
        .into_iter()
        .map(|(surface, _)| surface)
        .collect()
}

// ---------------------------------------------------------------------------
// Empty state: all F-keys pass through
// ---------------------------------------------------------------------------

#[test]
fn fkeys_in_empty_state_pass_through() {
    for keysym in [Keysym::F6, Keysym::F7, Keysym::F8, Keysym::F9, Keysym::F10] {
        let mut engine = InputMethodEngine::new();
        let result = engine.process_key(&press_key(keysym));
        assert!(!result.consumed, "keysym {keysym:?} must pass through");
        assert!(matches!(engine.state(), InputState::Empty));
    }
}

// ---------------------------------------------------------------------------
// Composing state transforms
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

#[test]
fn f7_converts_hiragana_composing_to_katakana() {
    let mut engine = composed_engine("かきく");
    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("カキク"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn f7_converts_dakuten_and_handakuten() {
    let mut engine = composed_engine("がっこう");
    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ガッコウ"));
}

#[test]
fn f8_converts_hiragana_to_half_katakana() {
    let mut engine = composed_engine("かきく");
    let result = engine.process_key(&press_key(Keysym::F8));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ｶｷｸ"));
}

#[test]
fn f8_converts_voiced_sounds() {
    let mut engine = composed_engine("ヴァイオリン");
    let result = engine.process_key(&press_key(Keysym::F8));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ｳﾞｧｲｵﾘﾝ"));
}

#[test]
fn f9_converts_mixed_ascii_and_kana() {
    let mut engine = composed_engine("abcかきく");
    let result = engine.process_key(&press_key(Keysym::F9));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ａｂｃｋａｋｉｋｕ"));
}

#[test]
fn f10_converts_mixed_full_width_text() {
    let mut engine = composed_engine("ＡＢＣカキク");
    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ABCkakiku"));
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
fn f10_converts_selected_candidate_to_half_width() {
    let mut engine = conversion_engine("ＡＢＣ", vec!["ＡＢＣ"]);
    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ABC"));
}

// ---------------------------------------------------------------------------
// Learning: F6/F7/F8 record the commit, F9/F10 (and Ctrl+L / Ctrl+;) do
// not. Kana-formatting transforms are a real preference signal; the
// recency-bias softening in the learning score keeps a one-off from
// dominating the candidate order. Alphanumeric targets stay excluded to
// keep the kana-keyed cache clean.
// ---------------------------------------------------------------------------

#[test]
fn f6_on_conversion_records_learning() {
    let mut engine = with_learning_cache(conversion_engine("あいう", vec!["あいう"]));
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
    assert_eq!(engine.learning.as_ref().unwrap().entry_count(), 1);
    // Identity pairs are recorded too: "leave this reading as-is" is a
    // preference signal.
    assert_eq!(learning_surfaces(&engine, "あいう"), vec!["あいう"]);
}

#[test]
fn f7_composing_records_learning() {
    let mut engine = with_learning_cache(composed_engine("かきく"));
    let result = engine.process_key(&press_key(Keysym::F7));
    assert_eq!(commit_text(&result), Some("カキク"));
    assert_eq!(learning_surfaces(&engine, "かきく"), vec!["カキク"]);
}

#[test]
fn f9_does_not_record_learning() {
    let mut engine = with_learning_cache(composed_engine("かきく"));
    let result = engine.process_key(&press_key(Keysym::F9));
    assert!(result.consumed);
    assert_eq!(engine.learning.as_ref().unwrap().entry_count(), 0);
}

#[test]
fn f10_does_not_record_learning() {
    let mut engine = with_learning_cache(composed_engine("あいう"));
    let result = engine.process_key(&press_key(Keysym::F10));
    assert!(result.consumed);
    assert_eq!(engine.learning.as_ref().unwrap().entry_count(), 0);
}

// ---------------------------------------------------------------------------
// macOS-standard Ctrl+L / Ctrl+; conversion shortcuts (issue #49). Ctrl+J
// is intentionally NOT mapped (upstream uses it for the chunk break, #87).
// ---------------------------------------------------------------------------

#[test]
fn ctrl_l_converts_to_fullwidth_alpha_like_f9() {
    let mut engine = composed_engine("abc");
    let result = engine.process_key(&press_ctrl(Keysym::KEY_L));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ａｂｃ"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn ctrl_semicolon_converts_to_halfwidth_alpha_like_f10() {
    let mut engine = composed_engine("ＡＢＣ");
    let result = engine.process_key(&press_ctrl(Keysym(0x003b)));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("ABC"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn ctrl_l_in_empty_state_passes_through() {
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_ctrl(Keysym::KEY_L));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn ctrl_semicolon_does_not_record_learning() {
    let mut engine = with_learning_cache(composed_engine("ＡＢＣ"));
    let result = engine.process_key(&press_ctrl(Keysym(0x003b)));
    assert!(result.consumed);
    assert_eq!(engine.learning.as_ref().unwrap().entry_count(), 0);
}

#[test]
fn ctrl_f6_stays_an_app_shortcut() {
    // F-keys with Ctrl/Alt modifiers remain app shortcuts; only
    // Ctrl+L/; are conversion bindings.
    let mut engine = composed_engine("アイウ");
    let result = engine.process_key(&press_ctrl(Keysym::F6));
    assert!(!result.consumed);
}

// ---------------------------------------------------------------------------
// Composing with pending romaji: F-keys settle before transform
// ---------------------------------------------------------------------------

#[test]
fn f6_settles_pending_romaji_before_conversion() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    let result = engine.process_key(&press_key(Keysym::F6));
    assert!(result.consumed);
    assert_eq!(commit_text(&result), Some("あ"));
}

// ---------------------------------------------------------------------------
// Empty composition: F-keys are not consumed
// ---------------------------------------------------------------------------

#[test]
fn fkey_on_empty_composition_not_consumed() {
    for keysym in [Keysym::F6, Keysym::F7, Keysym::F8, Keysym::F9, Keysym::F10] {
        let mut engine = InputMethodEngine::new();
        engine.state = InputState::Composing {
            preedit: Preedit::new(),
        };
        let result = engine.process_key(&press_key(keysym));
        assert!(!result.consumed, "keysym {keysym:?} must pass through");
    }
}
