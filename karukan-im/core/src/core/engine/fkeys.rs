//! F6-F10 function key conversion handlers (fork-ported onto the upstream
//! engine).
//!
//! Standard Japanese IME function key behavior:
//!
//! | Key | Action                                          | Transform                    |
//! |-----|-------------------------------------------------|------------------------------|
//! | F6  | ひらがなに変換 (convert to hiragana)             | `katakana_to_hiragana`       |
//! | F7  | 全角カタカナに変換 (convert to full katakana)     | `hiragana_to_katakana`       |
//! | F8  | 半角カタカナに変換 (convert to half katakana)     | `hiragana_to_half_katakana`  |
//! | F9  | 全角英数に変換 (convert to full-width alpha)     | ASCII → full-width ASCII     |
//! | F10 | 半角英数に変換 (convert to half-width alpha)     | Full-width → half-width + kana |
//!
//! The macOS conversion-convention chords Ctrl+L (全角英数) and Ctrl+;
//! (半角英数) are accepted as equivalents of F9/F10 (issue #49). Ctrl+J is
//! deliberately NOT mapped here: upstream binds it to the live-conversion
//! chunk break / display anchor (issue #87), which supersedes the fork's
//! hiragana chord on that key — F6 covers hiragana.
//!
//! In Empty state all F-keys pass through (not consumed) so the application
//! sees them. In Composing or Conversion state the current text is
//! transformed and committed immediately. F-keys with Ctrl or Alt modifiers
//! pass through (application shortcuts). Emoji mode lets them pass through.
//!
//! F6/F7/F8 record the committed reading→surface pair in the learning
//! cache; F9/F10 (and Ctrl+L / Ctrl+;) do not, because their
//! romaji/alphanumeric surfaces would pollute the kana-keyed cache.

use super::*;

impl InputMethodEngine {
    /// Try to handle an F6-F10 function key (or Ctrl+L / Ctrl+;) press.
    ///
    /// Returns `Some(result)` if the key was handled, `None` if the key is
    /// not one of these.
    pub(super) fn handle_fkey(&mut self, key: &KeyEvent) -> Option<EngineResult> {
        let is_fkey = matches!(
            key.keysym,
            Keysym::F6 | Keysym::F7 | Keysym::F8 | Keysym::F9 | Keysym::F10
        );
        // Ctrl+L (全角英数) and Ctrl+; (半角英数) mirror the macOS
        // conversion shortcuts. Not Ctrl+J — see the module docs.
        let is_ctrl_convert = !is_fkey
            && key.modifiers.control_key
            && !key.modifiers.alt_key
            && !key.modifiers.super_key
            && matches!(
                key.keysym,
                Keysym::KEY_L | Keysym::KEY_L_UPPER | Keysym(0x003b)
            );

        let (transform, learn): (fn(&str) -> String, bool) = match key.keysym {
            Keysym::F6 => (f6_transform, true),
            Keysym::F7 => (f7_transform, true),
            Keysym::F8 => (f8_transform, true),
            Keysym::F9 => (f9_transform, false),
            Keysym::F10 => (f10_transform, false),
            Keysym::KEY_L | Keysym::KEY_L_UPPER if is_ctrl_convert => (f9_transform, false),
            Keysym(0x003b) if is_ctrl_convert => (f10_transform, false),
            _ => return None,
        };

        // Don't consume F-keys with Ctrl or Alt modifiers — they may be
        // application shortcuts (e.g. Alt+F7 in IDEs, Ctrl+F6 in
        // terminals). The Ctrl+L/; conversion shortcuts above are the
        // deliberate exception.
        if is_fkey && (key.modifiers.control_key || key.modifiers.alt_key) {
            return Some(EngineResult::not_consumed());
        }

        // Empty state: pass through to application
        if matches!(self.state, InputState::Empty) {
            return Some(EngineResult::not_consumed());
        }

        // Emoji mode: f-key semantics don't make sense for emoji queries
        if self.mode.current() == InputMode::Emoji {
            return Some(EngineResult::not_consumed());
        }

        // Get text to transform and the (reading, target) pair for learning,
        // depending on state.
        let (text, learn_source) = match &self.state {
            InputState::Composing { .. } => {
                // Transform the *reading* (settled kana incl. any pending
                // romaji tail), not the live-converted display: standard IME
                // behavior (F7 on こんにちは shows コンニチハ regardless of
                // whether live conversion is currently rendering a kanji
                // surface like 今日は). Mirrors the fork implementation,
                // which transformed input_buf.text.
                let reading = self.input_buf.settled_reading(&self.converters.romaji);
                let learn_source = Some((reading.clone(), reading.clone()));
                (reading, learn_source)
            }
            InputState::Conversion {
                candidates,
                reading,
                ..
            } => {
                // Also reading-based: the selected candidate's own reading
                // (standard IME: F7 on a segment displayed as 私 must give
                // ワタシ, not pass the kanji through). Falls back to the
                // conversion reading when the candidate carries none.
                let seg_reading = candidates
                    .selected()
                    .and_then(|c| c.reading.clone())
                    .unwrap_or_else(|| reading.clone());
                let learn_source = Some((seg_reading.clone(), seg_reading.clone()));
                (seg_reading, learn_source)
            }
            _ => return Some(EngineResult::not_consumed()),
        };

        if text.is_empty() {
            return Some(EngineResult::not_consumed());
        }

        let transformed = transform(&text);

        // Learning: F6/F7/F8 kana-formatting commits ARE recorded —
        // repeatedly forcing e.g. こーひー → コーヒー is a real preference
        // signal (the recency-bias softening in the learning score keeps a
        // one-off from dominating). F9/F10 (and Ctrl+L / Ctrl+;) are NOT:
        // romaji/alphanumeric surfaces would pollute the kana-keyed
        // learning cache.
        if learn && let Some((reading, target)) = learn_source {
            let surface = transform(&target);
            if !reading.is_empty() && !surface.is_empty() {
                self.record_learning(&reading, &surface);
            }
        }

        // Clear all state and commit the transformed text.
        self.end_composition();

        Some(
            EngineResult::consumed()
                .with_action(EngineAction::Commit(transformed))
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::HideAuxText),
        )
    }
}

/// F6: Convert to hiragana (katakana → hiragana).
/// Uses `katakana_to_hiragana` which only affects katakana chars;
/// hiragana and other chars pass through unchanged.
fn f6_transform(text: &str) -> String {
    karukan_engine::katakana_to_hiragana(text)
}

/// F7: Convert to full-width katakana (hiragana → katakana).
/// Uses `hiragana_to_katakana` which only affects hiragana chars;
/// katakana and other chars pass through unchanged.
fn f7_transform(text: &str) -> String {
    karukan_engine::hiragana_to_katakana(text)
}

/// F8: Convert to half-width katakana.
/// First converts hiragana to full-width katakana, then to half-width.
/// Already half-width katakana passes through.
fn f8_transform(text: &str) -> String {
    karukan_engine::hiragana_to_half_katakana(text)
}

/// F9: Convert to full-width alphanumeric.
///
/// Kana (hiragana/katakana) is converted to Hepburn romaji first, then
/// all ASCII characters are made full-width. Non-kana, non-ASCII characters
/// (kanji, symbols) pass through unchanged.
fn f9_transform(text: &str) -> String {
    let romaji = karukan_engine::kana_to_romaji(text);
    romaji
        .chars()
        .map(karukan_engine::ascii_to_fullwidth_char)
        .collect()
}

/// F10: Convert to half-width alphanumeric.
///
/// Three-step conversion:
/// 1. Kana (hiragana/katakana) → Hepburn romaji (half-width ASCII)
/// 2. Full-width ASCII (０-９, Ａ-Ｚ, ａ-ｚ) → half-width ASCII
/// 3. Full-width katakana → half-width katakana (handles voiced marks)
///
/// Steps 2-3 catch any characters not handled in step 1.
fn f10_transform(text: &str) -> String {
    // Step 1: kana → half-width romaji
    let romaji = karukan_engine::kana_to_romaji(text);
    // Step 2: full-width ASCII → half-width ASCII
    let step2: String = romaji
        .chars()
        .map(karukan_engine::fullwidth_to_ascii_char)
        .collect();
    // Step 3: full-width katakana → half-width katakana
    karukan_engine::katakana_to_half_width(&step2)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn f6_katakana_to_hiragana() {
        assert_eq!(f6_transform("アイウ"), "あいう");
        assert_eq!(f6_transform("コーヒー"), "こーひー");
        assert_eq!(f6_transform(""), "");
    }

    #[test]
    fn f7_hiragana_to_katakana() {
        assert_eq!(f7_transform("あいう"), "アイウ");
        assert_eq!(f7_transform("がっこう"), "ガッコウ");
        assert_eq!(f7_transform(""), "");
    }

    #[test]
    fn f8_hiragana_to_half_katakana() {
        assert_eq!(f8_transform("かきく"), "ｶｷｸ");
        assert_eq!(f8_transform("がっこう"), "ｶﾞｯｺｳ");
        assert_eq!(f8_transform(""), "");
    }

    #[test]
    fn f9_kana_to_fullwidth_alpha() {
        assert_eq!(f9_transform("あいう"), "ａｉｕ");
        assert_eq!(f9_transform("がっこう"), "ｇａｋｋｏｕ");
        assert_eq!(f9_transform("abc"), "ａｂｃ");
    }

    #[test]
    fn f10_kana_to_halfwidth_alpha() {
        assert_eq!(f10_transform("あいう"), "aiu");
        assert_eq!(f10_transform("がっこう"), "gakkou");
        assert_eq!(f10_transform("日本"), "日本"); // kanji passes through
        assert_eq!(f10_transform("ｶﾞｯｺｳ"), "ｶﾞｯｺｳ"); // already half-width
    }

    #[test]
    fn f10_fullwidth_to_halfwidth() {
        assert_eq!(f10_transform("ＡＢＣ１２３"), "ABC123");
    }

    #[test]
    fn f9_romaji_hepburn_rules() {
        // し → shi, ふ → fu, ち → chi; digraphs follow Hepburn
        assert_eq!(f9_transform("しゅうしょく"), "ｓｈｕｕｓｈｏｋｕ");
        assert_eq!(f9_transform("きょう"), "ｋｙｏｕ");
        assert_eq!(f9_transform("ふじ"), "ｆｕｊｉ");
    }
}
