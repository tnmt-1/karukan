//! Behaviors ported from mozkey: committing on a mark (単打確定), no live
//! conversion for a lone kana, holding the suggestion list while a romaji
//! tail is unresolved, and taking back a commit's learning record.

use super::*;

fn commit_text(result: &EngineResult) -> Option<String> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(text) => Some(text.clone()),
        _ => None,
    })
}

fn type_str(engine: &mut InputMethodEngine, keys: &str) {
    for ch in keys.chars() {
        engine.process_key(&press(ch));
    }
}

fn learned(engine: &InputMethodEngine, reading: &str) -> Vec<String> {
    engine
        .learning
        .as_ref()
        .unwrap()
        .lookup(reading)
        .into_iter()
        .map(|(surface, _)| surface)
        .collect()
}

/// Live conversion on, the shipped `commit_on` marks and `live_min_chars`,
/// an empty learning cache.
fn mozkey_engine() -> InputMethodEngine {
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        live_conversion: true,
        live_min_chars: 2,
        commit_on: vec!['。', '、', '？', '！'],
        ..EngineConfig::default()
    });
    engine.learning = Some(LearningCache::new(LearningConfig::default()));
    engine
}

#[test]
fn mark_commits_live_text_with_mark() {
    let mut engine = mozkey_engine();
    seed_model_cache(&mut engine, "カンジ", "", &["漢字"]);
    type_str(&mut engine, "kanji");

    let result = engine.process_key(&press('.'));
    assert_eq!(commit_text(&result).as_deref(), Some("漢字。"));
    assert!(matches!(engine.state(), InputState::Empty));
    // Learned without the mark
    assert_eq!(learned(&engine, "かんじ"), ["漢字"]);
}

#[test]
fn shifted_mark_commits_too() {
    let mut engine = mozkey_engine();
    seed_model_cache(&mut engine, "カンジ", "", &["漢字"]);
    type_str(&mut engine, "kanji");

    let result = engine.process_key(&press_shift('?'));
    assert_eq!(commit_text(&result).as_deref(), Some("漢字？"));
}

#[test]
fn mark_from_empty_commits_alone() {
    let mut engine = mozkey_engine();
    let result = engine.process_key(&press('.'));
    assert_eq!(commit_text(&result).as_deref(), Some("。"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn mark_after_escape_commits_kana() {
    let mut engine = mozkey_engine();
    seed_model_cache(&mut engine, "カンジ", "", &["漢字"]);
    type_str(&mut engine, "kanji");
    engine.process_key(&press_key(Keysym::ESCAPE));

    let result = engine.process_key(&press(','));
    assert_eq!(commit_text(&result).as_deref(), Some("かんじ、"));
}

#[test]
fn mark_in_conversion_commits_selected_candidate() {
    let mut engine = mozkey_engine();
    seed_model_cache(&mut engine, "カンジ", "", &["漢字"]);
    type_str(&mut engine, "kanji");
    engine.process_key(&press_key(Keysym::SPACE));
    let (selected, _) = engine.selected_conversion_info().unwrap();

    let result = engine.process_key(&press('.'));
    assert_eq!(commit_text(&result), Some(format!("{selected}。")));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn mark_stays_in_composition_without_commit_on() {
    let mut engine = InputMethodEngine::new();
    type_str(&mut engine, "ka.");
    assert_eq!(engine.input_buf.reading(), "か。");
}

#[test]
fn lone_kana_is_not_live_converted() {
    let mut engine = mozkey_engine();
    seed_model_cache(&mut engine, "ハ", "", &["葉"]);
    seed_model_cache(&mut engine, "ハナ", "", &["花"]);

    type_str(&mut engine, "ha");
    assert_eq!(engine.live_text(), "");
    assert_eq!(engine.preedit().unwrap().text(), "は");

    type_str(&mut engine, "na");
    assert_eq!(engine.live_text(), "花");
}

#[test]
fn trailing_marks_do_not_count_toward_live_min_chars() {
    let mut engine = mozkey_engine();
    seed_model_cache(&mut engine, "エー", "", &["絵ー"]);
    type_str(&mut engine, "e-");
    assert_eq!(engine.input_buf.reading(), "えー");
    assert_eq!(engine.live_text(), "");
}

#[test]
fn suggestions_hold_while_romaji_tail_is_unresolved() {
    let mut engine = mozkey_engine();
    engine.dicts.system = Some(dict_from_json(
        r#"[
            {"reading":"わせ","candidates":[{"surface":"和瀬","score":1000.0}]},
            {"reading":"わせだ","candidates":[{"surface":"早稲田","score":1000.0}]}
        ]"#,
    ));
    let shown = |engine: &InputMethodEngine| -> Vec<String> {
        engine
            .shown_suggestions
            .candidates()
            .iter()
            .map(|c| c.text.clone())
            .collect()
    };

    type_str(&mut engine, "wase");
    let before = shown(&engine);
    assert!(before.contains(&"和瀬".to_string()));

    // `d` would narrow away the exact match 和瀬; the list holds instead.
    engine.process_key(&press('d'));
    assert_eq!(shown(&engine), before);

    // Resolved: the list follows the reading again.
    engine.process_key(&press('a'));
    assert!(!shown(&engine).contains(&"和瀬".to_string()));
}

#[test]
fn backspace_right_after_commit_takes_back_learning() {
    let mut engine = mozkey_engine();
    type_str(&mut engine, "kana");
    engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(learned(&engine, "かな"), ["かな"]);

    let result = engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(!result.consumed, "Backspace still reaches the application");
    assert!(learned(&engine, "かな").is_empty());
}

#[test]
fn ctrl_z_right_after_commit_takes_back_learning() {
    let mut engine = mozkey_engine();
    type_str(&mut engine, "kana");
    engine.process_key(&press_key(Keysym::RETURN));

    engine.process_key(&press_ctrl(Keysym::KEY_Z));
    assert!(learned(&engine, "かな").is_empty());
}

#[test]
fn other_key_after_commit_keeps_learning() {
    let mut engine = mozkey_engine();
    type_str(&mut engine, "kana");
    engine.process_key(&press_key(Keysym::RETURN));

    engine.process_key(&press('a'));
    engine.process_key(&press_key(Keysym::ESCAPE));
    engine.process_key(&press_key(Keysym::ESCAPE));
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(learned(&engine, "かな"), ["かな"]);
}

#[test]
fn modifier_press_does_not_settle_the_commit() {
    let mut engine = mozkey_engine();
    type_str(&mut engine, "kana");
    engine.process_key(&press_key(Keysym::RETURN));

    engine.process_key(&press_key(Keysym::CONTROL_L));
    engine.process_key(&press_ctrl(Keysym::KEY_Z));
    assert!(learned(&engine, "かな").is_empty());
}
