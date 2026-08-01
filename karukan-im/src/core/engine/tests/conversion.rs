use super::*;

#[test]
fn test_conversion_char_commits_and_continues() {
    let mut engine = InputMethodEngine::new();

    // Type "あい" and enter conversion
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Type 'k' during conversion → should commit candidate and start new input
    let result = engine.process_key(&press('k'));
    assert!(result.consumed);

    // Should have committed the conversion
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(_)));
    assert!(has_commit, "Should have a commit action");

    // Should now be in Composing with 'k' in preedit
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "k");
}

#[test]
fn test_conversion_char_commits_and_continues_romaji() {
    let mut engine = InputMethodEngine::new();

    // Type "あ" and enter conversion
    engine.process_key(&press('a'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Type 'k', 'a' → commits conversion, then starts "か"
    engine.process_key(&press('k'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "k");

    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "か");
}

#[test]
fn test_alphabet_mode_space_inserts_literal_space() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+N
    engine.process_key(&press_shift('N'));
    assert!(engine.input_mode == InputMode::Alphabet);

    // Type "ew"
    engine.process_key(&press('e'));
    engine.process_key(&press('w'));
    assert_eq!(engine.preedit().unwrap().text(), "New");

    // Space → should insert literal space, NOT start conversion
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "New ");

    // Type "york"
    engine.process_key(&press('y'));
    engine.process_key(&press('o'));
    engine.process_key(&press('r'));
    engine.process_key(&press('k'));
    assert_eq!(engine.preedit().unwrap().text(), "New york");
}

/// Park the engine in a Conversion state with `n` dummy candidates.
fn engine_in_conversion_with(n: usize) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    let candidates = CandidateList::new(
        (0..n)
            .map(|i| Candidate::with_reading(format!("候補{}", i), "あい"))
            .collect(),
    );
    engine.state = InputState::Conversion {
        preedit: Preedit::with_text("候補0"),
        candidates: candidates.clone(),
        full_reading: "あい".to_string(),
        range_start: 0,
        range_end: 2,
    };
    engine
}

#[test]
fn test_zero_selects_tenth_candidate() {
    // Standard IME: `0` selects the 10th candidate instead of committing
    // the current candidate and inserting a literal "0".
    let mut engine = engine_in_conversion_with(12);

    let result = engine.process_key(&press('0'));
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
    assert_eq!(commit_text, "候補9");
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_zero_with_fewer_than_ten_candidates_is_noop() {
    // `0` with a short page must not commit or insert anything.
    let mut engine = engine_in_conversion_with(3);

    let result = engine.process_key(&press('0'));
    assert!(result.consumed);
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(_)));
    assert!(!has_commit);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
}

#[test]
fn keypad_digit_in_conversion_commits_and_continues() {
    // KP_1 must commit the current candidate and start a new composition
    // with the literal "1" — never select the first candidate.
    let mut engine = engine_in_conversion_with(3);
    let result = engine.process_key(&press_key(Keysym::KP_1));
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
    assert_eq!(commit_text, "候補0");
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "1");
}

#[test]
fn keypad_enter_in_conversion_commits() {
    let mut engine = engine_in_conversion_with(3);
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
    assert_eq!(commit_text, "候補0");
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn public_page_navigation_moves_between_pages() {
    // Frontend page buttons (fcitx5) drive the same engine path as PgUp/
    // PgDn via the public select_next/prev_candidate_page API.
    let mut engine = engine_in_conversion_with(12); // 2 pages of 9

    let result = engine.select_next_candidate_page();
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::ShowCandidates(_)))
    );

    // Page 2 is showing: next wraps back to page 1, prev returns to 2.
    let result = engine.select_next_candidate_page();
    assert!(result.consumed);
    let result = engine.select_prev_candidate_page();
    assert!(result.consumed);
    let result = engine.select_prev_candidate_page();
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Not consumed outside conversion.
    let mut idle = InputMethodEngine::new();
    let result = idle.select_next_candidate_page();
    assert!(!result.consumed);
}

/// Selected candidate text of the current conversion, or None.
fn selected_text(engine: &InputMethodEngine) -> Option<String> {
    engine
        .state()
        .candidates()
        .and_then(|c| c.selected_text())
        .map(String::from)
}

fn press_shift_key(keysym: Keysym) -> KeyEvent {
    KeyEvent::new(keysym, KeyModifiers::new().with_shift(true), true)
}

#[test]
fn shift_space_moves_to_previous_candidate() {
    // Standard IME convention: while converting, Shift+Space goes back to
    // the previous candidate (Space moves forward).
    let mut engine = engine_in_conversion_with(3);
    assert_eq!(selected_text(&engine).as_deref(), Some("候補0"));

    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(selected_text(&engine).as_deref(), Some("候補1"));

    let result = engine.process_key(&press_shift_key(Keysym::SPACE));
    assert!(result.consumed);
    assert_eq!(selected_text(&engine).as_deref(), Some("候補0"));

    // Still converting (no commit), and further Shift+Space wraps to the
    // last candidate like Up does.
    let result = engine.process_key(&press_shift_key(Keysym::SPACE));
    assert!(result.consumed);
    assert_eq!(selected_text(&engine).as_deref(), Some("候補2"));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
}

#[test]
fn shift_tab_moves_to_previous_candidate() {
    // Shift+Tab (both the TAB+shift shape macOS sends and the XKB
    // ISO_Left_Tab shape Linux sends) goes back like Shift+Space.
    let mut engine = engine_in_conversion_with(3);
    engine.process_key(&press_key(Keysym::TAB)); // forward
    assert_eq!(selected_text(&engine).as_deref(), Some("候補1"));

    let result = engine.process_key(&press_shift_key(Keysym::TAB));
    assert!(result.consumed);
    assert_eq!(selected_text(&engine).as_deref(), Some("候補0"));

    let result = engine.process_key(&press_key(Keysym::ISO_LEFT_TAB));
    assert!(result.consumed);
    assert_eq!(selected_text(&engine).as_deref(), Some("候補2"));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
}
