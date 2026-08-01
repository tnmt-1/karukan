use super::*;

// --- Mode toggle key tests (one-way: alphabet → hiragana) ---

#[test]
fn test_mode_toggle_key_switches_alphabet_to_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));
    assert!(engine.input_mode == InputMode::Alphabet);

    // Alt_R press → switch to hiragana mode (mid-composition; the toggle key
    // is the explicit way out, independent of the per-word auto-revert)
    let result = engine.process_key(&press_key(Keysym::ALT_R));
    assert!(result.consumed);
    assert!(engine.input_mode != InputMode::Alphabet);

    // Clear the composed "A", then type 'a' → should be 'あ' (hiragana mode)
    engine.process_key(&press_key(Keysym::RETURN));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_mode_toggle_key_noop_in_hiragana() {
    let mut engine = InputMethodEngine::new();
    assert!(engine.input_mode != InputMode::Alphabet);

    // Alt_R press in hiragana mode → not consumed, no mode change
    let result = engine.process_key(&press_key(Keysym::ALT_R));
    assert!(!result.consumed);
    assert!(engine.input_mode != InputMode::Alphabet);

    // Type 'a' → still hiragana
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_mode_toggle_key_during_alphabet_input() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A and type "b"
    engine.process_key(&press_shift('A'));
    engine.process_key(&press('b'));
    assert_eq!(engine.preedit().unwrap().text(), "Ab");
    assert!(engine.input_mode == InputMode::Alphabet);

    // Alt_R → switch to hiragana
    let result = engine.process_key(&press_key(Keysym::ALT_R));
    assert!(result.consumed);
    assert!(engine.input_mode != InputMode::Alphabet);

    // Continue typing → hiragana
    engine.process_key(&press('k'));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "Abか");
}

#[test]
fn test_super_r_also_switches_alphabet_to_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));
    assert!(engine.input_mode == InputMode::Alphabet);

    // Super_R press → switch to hiragana (one-way)
    let result = engine.process_key(&press_key(Keysym::SUPER_R));
    assert!(result.consumed);
    assert!(engine.input_mode != InputMode::Alphabet);
}

#[test]
fn test_meta_r_also_switches_alphabet_to_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));
    assert!(engine.input_mode == InputMode::Alphabet);

    // Meta_R press → switch to hiragana (one-way)
    let result = engine.process_key(&press_key(Keysym::META_R));
    assert!(result.consumed);
    assert!(engine.input_mode != InputMode::Alphabet);
}

#[test]
fn test_focus_loss_commit_restores_alphabet_mode() {
    // Shift+letter is a per-word alphabet mode: focus-loss commit
    // (engine.commit()) must restore the prior mode so the next word is
    // kana again. Regression: commit() skipped the mode restore.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_shift('A'));
    assert!(engine.input_mode == InputMode::Alphabet);

    assert_eq!(engine.commit(), "A");
    assert!(matches!(engine.state(), InputState::Empty));
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    // Next word starts in kana again
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_focus_loss_commit_restores_katakana_from_alphabet() {
    // Alphabet entered from Katakana mode: focus-loss commit returns to
    // Katakana, matching the Enter path.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_shift('A'));
    assert!(engine.input_mode == InputMode::Alphabet);
    // Simulate the prior mode being Katakana (what the Shift gesture saved)
    engine.pre_alphabet_mode = Some(InputMode::Katakana);

    assert_eq!(engine.commit(), "A");
    assert_eq!(engine.input_mode, InputMode::Katakana);
}

#[test]
fn test_super_modified_keys_pass_through() {
    // Super+letter / Super+Space are OS/DE shortcuts — the engine must not
    // consume them (regression: Super+Space inserted a full-width space and
    // Super+A started composition on Linux).
    let mut engine = InputMethodEngine::new();
    let super_a = KeyEvent::new(
        Keysym(b'a' as u32),
        KeyModifiers {
            shift_key: false,
            control_key: false,
            alt_key: false,
            super_key: true,
        },
        true,
    );
    let result = engine.process_key(&super_a);
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));

    let super_space = KeyEvent::new(
        Keysym::SPACE,
        KeyModifiers {
            shift_key: false,
            control_key: false,
            alt_key: false,
            super_key: true,
        },
        true,
    );
    let result = engine.process_key(&super_space);
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));

    // ... also while composing (Conversion trigger must not fire).
    engine.process_key(&press('a'));
    let result = engine.process_key(&super_space);
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
}

// --- JIS IME keys (Henkan / Hiragana_Katakana) ---

#[test]
fn henkan_key_starts_conversion_from_composing() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    let result = engine.process_key(&press_key(Keysym::HENKAN));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
}

#[test]
fn henkan_key_advances_candidate_in_conversion() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let result = engine.process_key(&press_key(Keysym::HENKAN));
    assert!(result.consumed);
    // Still converting, selection advanced (not committed).
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(_)));
    assert!(!has_commit);
}

#[test]
fn henkan_key_from_empty_is_noop() {
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_key(Keysym::HENKAN));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn hiragana_katakana_key_toggles_kana_mode() {
    let mut engine = InputMethodEngine::new();

    // From Hiragana → Katakana mode.
    let result = engine.process_key(&press_key(Keysym::HIRAGANA_KATAKANA));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Katakana);

    // Back to Hiragana.
    let result = engine.process_key(&press_key(Keysym::HIRAGANA_KATAKANA));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Hiragana);
}

#[test]
fn hiragana_katakana_key_bakes_katakana_on_exit() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::HIRAGANA_KATAKANA));
    assert_eq!(engine.input_mode, InputMode::Katakana);
    assert_eq!(engine.preedit().unwrap().text(), "アイ");

    engine.process_key(&press_key(Keysym::HIRAGANA_KATAKANA));
    assert_eq!(engine.input_mode, InputMode::Hiragana);
    // Preedit stays katakana (baked), subsequent input is hiragana.
    assert_eq!(engine.preedit().unwrap().text(), "アイ");
    engine.process_key(&press('u'));
    assert_eq!(engine.preedit().unwrap().text(), "アイう");
}
