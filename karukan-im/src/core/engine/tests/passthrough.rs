use super::*;

#[test]
fn test_passthrough_no_double_counting() {
    // Regression test: typing '7' twice should produce "77" in the preedit,
    // not "777" or "7777". The converter adds PassThrough chars to output()
    // AND returns them as PassThrough events; without proper handling, both
    // paths would insert the char.
    //
    // Digits are the PassThrough representative here: every ASCII symbol now
    // has a romaji rule (they convert to their full-width form), so symbols no
    // longer exercise this path.
    let mut engine = InputMethodEngine::new();

    // Type '7' from empty state → enters Composing with preedit "7"
    engine.process_key(&press('7'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "7");

    // Type '7' again → preedit becomes "77", not "777"
    engine.process_key(&press('7'));
    assert_eq!(
        engine.preedit().unwrap().text(),
        "77",
        "Second '7' should produce '77', not over-count chars"
    );
}

#[test]
fn test_apostrophe_starts_input_mode() {
    // Regression for: typing `'` in empty state should enter Composing,
    // not auto-commit. This lets users type `’word’` or get symbol variants.
    // The apostrophe itself is full-width `’` (mozc's width table), and the
    // ASCII form stays available from the candidate list.
    let mut engine = InputMethodEngine::new();

    let result = engine.process_key(&press('\''));
    assert!(result.consumed);
    assert!(
        matches!(engine.state(), InputState::Composing { .. }),
        "Apostrophe should enter Composing, not auto-commit"
    );
    assert_eq!(engine.preedit().unwrap().text(), "’");

    // No Commit action should have fired.
    assert!(
        !result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(_))),
        "First apostrophe should not commit"
    );
}

#[test]
fn test_thx_chars_not_lost() {
    // Regression test: typing "thx" should show "thx" in preedit, not lose chars.
    // The converter recursively passes through 't' and 'h', keeps 'x' in buffer.
    // The engine must pick up ALL chars from output delta, not just the last PassThrough.
    let mut engine = InputMethodEngine::new();

    // Type 't'
    engine.process_key(&press('t'));
    assert_eq!(engine.preedit().unwrap().text(), "t");

    // Type 'h'
    engine.process_key(&press('h'));
    assert_eq!(engine.preedit().unwrap().text(), "th");

    // Type 'x' → converter breaks "thx" into output="th" + buffer="x"
    engine.process_key(&press('x'));
    let preedit = engine.preedit().unwrap().text().to_string();
    assert_eq!(preedit, "thx", "Should show 'thx', not lose characters");

    // Commit should produce "thx"
    let result = engine.process_key(&press_key(Keysym::RETURN));
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(text) if text == "thx"));
    assert!(has_commit, "Should commit 'thx'");
}

#[test]
fn test_passthrough_after_hiragana_no_double() {
    // Typing hiragana then '7' should append exactly one '7', not two.
    // See `test_passthrough_no_double_counting` for why this uses a digit.
    let mut engine = InputMethodEngine::new();

    // Type "あ" (a)
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    // Type '7' while in hiragana input state
    engine.process_key(&press('7'));
    let preedit = engine.preedit().unwrap().text().to_string();
    assert_eq!(preedit, "あ7", "Should be 'あ7', not 'あ77'");

    // Type another '7'
    engine.process_key(&press('7'));
    let preedit = engine.preedit().unwrap().text().to_string();
    assert_eq!(preedit, "あ77", "Should be 'あ77', not 'あ777'");
}

#[test]
fn test_digit_starts_input_mode() {
    // Typing a digit from Empty state should enter Composing,
    // not commit immediately. This allows typing "20世紀" etc.
    let mut engine = InputMethodEngine::new();

    // Type '2' from Empty state
    let result = engine.process_key(&press('2'));
    assert!(result.consumed);
    assert!(
        matches!(engine.state(), InputState::Composing { .. }),
        "Digit should enter Composing, not stay Empty"
    );
    assert_eq!(engine.preedit().unwrap().text(), "2");

    // Type '0'
    engine.process_key(&press('0'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "20");

    // Type "seiki" -> "20せいき"
    engine.process_key(&press('s'));
    engine.process_key(&press('e'));
    engine.process_key(&press('i'));
    engine.process_key(&press('k'));
    engine.process_key(&press('i'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "20せいき");

    // Commit should produce "20せいき"
    let result = engine.process_key(&press_key(Keysym::RETURN));
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(text) if text == "20せいき"));
    assert!(has_commit, "Should commit '20せいき'");
}

#[test]
fn test_digit_in_middle_of_hiragana() {
    // Typing a digit while in Composing should keep the preedit
    let mut engine = InputMethodEngine::new();

    // Type "あ" then "2"
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    engine.process_key(&press('2'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あ2");
}

#[test]
fn test_ascii_symbols_are_full_width_in_kana_mode() {
    // Kana mode is a full-width context (mozc's default preedit character
    // form), so a typed ASCII symbol lands in the preedit full-width.
    // Regression: these used to fall through the PassThrough path and stay
    // half-width, unlike `?` → `？` and `[` → `「`.
    for (typed, expected) in [
        ('(', '（'),
        (')', '）'),
        ('{', '｛'),
        ('@', '＠'),
        ('&', '＆'),
        ('=', '＝'),
        ('<', '＜'),
        (';', '；'),
        ('"', '”'),
    ] {
        let mut engine = InputMethodEngine::new();
        engine.process_key(&press(typed));
        assert!(matches!(engine.state(), InputState::Composing { .. }));
        assert_eq!(
            engine.preedit().unwrap().text(),
            expected.to_string(),
            "`{typed}` should convert to `{expected}`"
        );
    }
}

#[test]
fn test_symbols_compose_and_commit_with_kana() {
    // Symbols accumulate in the preedit alongside kana and commit together.
    let mut engine = InputMethodEngine::new();

    engine.process_key(&press('('));
    engine.process_key(&press('a'));
    engine.process_key(&press(')'));
    assert_eq!(engine.preedit().unwrap().text(), "（あ）");

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(text) if text == "（あ）")),
        "Should commit '（あ）'"
    );
}

#[test]
fn test_half_width_symbol_stays_reachable_as_candidate() {
    // The full-width mapping must not strip access to the ASCII form:
    // converting the symbol still surfaces it via the rewriter's chain.
    for (typed, half) in [('(', "("), ('@', "@"), (';', ";")] {
        let mut engine = InputMethodEngine::new();
        engine.process_key(&press(typed));
        engine.process_key(&press_key(Keysym::SPACE));

        let texts: Vec<String> = engine
            .state()
            .candidates()
            .map(|cl| cl.candidates().iter().map(|c| c.text.clone()).collect())
            .unwrap_or_default();

        assert!(
            texts.iter().any(|t| t == half),
            "half-width `{half}` should be offered as a candidate, got: {texts:?}"
        );
    }
}

#[test]
fn test_symbols_stay_half_width_in_alphabet_mode() {
    // Alphabet mode bypasses the romaji table entirely, so ASCII stays ASCII.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_shift('F'));
    assert_eq!(engine.input_mode, InputMode::Alphabet);

    engine.process_key(&press('n'));
    engine.process_key(&press('('));
    engine.process_key(&press(')'));
    assert_eq!(engine.preedit().unwrap().text(), "Fn()");
}

#[test]
fn test_colon_from_empty_still_starts_emoji_mode() {
    // `:` maps to `：` in the romaji table, but the emoji shortcode trigger
    // is checked before romaji conversion, so `:smile` still works.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press(':'));
    assert_eq!(engine.input_mode, InputMode::Emoji);
    assert_eq!(engine.preedit().unwrap().text(), ":");
}

#[test]
fn test_colon_mid_composition_is_full_width() {
    // Mid-word `:` is not an emoji trigger (that is Empty-state only), so it
    // takes the full-width form like any other symbol.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press(':'));
    assert_eq!(engine.input_mode, InputMode::Hiragana);
    assert_eq!(engine.preedit().unwrap().text(), "あ：");
}
