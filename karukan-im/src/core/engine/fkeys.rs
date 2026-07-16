//! F6-F10 function key conversion handlers.
//!
//! Standard Japanese IME function key behavior:
//!
//! | Key | Action                                  | Transform                         |
//! |-----|-----------------------------------------|-----------------------------------|
//! | F6  | ひらがなに変換 (convert to hiragana)       | `katakana_to_hiragana`            |
//! | F7  | 全角カタカナに変換 (convert to full katakana) | `hiragana_to_katakana`            |
//! | F8  | 半角カタカナに変換 (convert to half katakana) | `hiragana_to_half_katakana`       |
//! | F9  | 全角英数に変換 (convert to full-width alpha) | ASCII → full-width ASCII           |
//! | F10 | 半角英数に変換 (convert to half-width alpha) | Full-width → half-width + kana    |
//!
//! In Empty state all F-keys pass through (not consumed) so the application
//! sees them. In Composing or Conversion state the current text is transformed
//! and committed immediately.

use super::*;

impl InputMethodEngine {
    /// Try to handle an F6-F10 function key press.
    ///
    /// Returns `Some(result)` if the key was handled, `None` if the key is
    /// not an F6-F10 key.
    ///
    /// Behavior by state:
    /// - Empty: not consumed (pass through to application)
    /// - Composing: transform input buffer and commit
    /// - Conversion: transform selected candidate and commit
    /// - Emoji mode: not consumed (f-key semantics don't apply)
    ///
    /// F-keys with Ctrl or Alt modifiers are not consumed (may be app shortcuts).
    pub(super) fn handle_fkey(&mut self, key: &KeyEvent) -> Option<EngineResult> {
        let transform = match key.keysym {
            Keysym::F6 => f6_transform,
            Keysym::F7 => f7_transform,
            Keysym::F8 => f8_transform,
            Keysym::F9 => f9_transform,
            Keysym::F10 => f10_transform,
            _ => return None,
        };

        // Don't consume function keys with Ctrl or Alt modifiers — they may
        // be application shortcuts (e.g. Alt+F7 in IDEs, Ctrl+F6 in terminals).
        if key.modifiers.control_key || key.modifiers.alt_key {
            return Some(EngineResult::not_consumed());
        }

        // Empty state: pass through to application
        if matches!(self.state, InputState::Empty) {
            return Some(EngineResult::not_consumed());
        }

        // Emoji mode: f-key semantics don't make sense for emoji queries
        if self.input_mode == InputMode::Emoji {
            return Some(EngineResult::not_consumed());
        }

        // Get text to transform, depending on state
        let text = match &self.state {
            InputState::Composing { .. } => {
                // Flush any pending romaji into the input buffer first
                self.flush_romaji_to_composed();
                self.input_buf.text.clone()
            }
            InputState::Conversion {
                candidates,
                full_reading,
                range_start,
                range_end,
                ..
            } => {
                let selected = candidates
                    .selected_text()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                let full_len = full_reading.chars().count();
                if *range_start == 0 && *range_end == full_len {
                    selected
                } else {
                    let chars: Vec<char> = full_reading.chars().collect();
                    let before: String = chars[..*range_start].iter().collect();
                    let after: String = chars[*range_end..].iter().collect();
                    format!("{}{}{}", before, selected, after)
                }
            }
            _ => return Some(EngineResult::not_consumed()),
        };

        if text.is_empty() {
            return Some(EngineResult::not_consumed());
        }

        let transformed = transform(&text);

        // Record learning
        let reading = self.input_buf.text.clone();
        if !reading.is_empty() {
            self.record_learning(&reading, &transformed);
        }

        // Clear all state
        self.converters.romaji.reset();
        self.input_buf.clear();
        self.live.text.clear();
        self.chunks.clear();
        self.state = InputState::Empty;

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
    fn f9_ascii_to_fullwidth() {
        assert_eq!(f9_transform("abc"), "ａｂｃ");
        assert_eq!(f9_transform("ABC"), "ＡＢＣ");
        assert_eq!(f9_transform("123"), "１２３");
        assert_eq!(f9_transform(""), "");
    }

    #[test]
    fn f9_kana_becomes_fullwidth_romaji() {
        assert_eq!(f9_transform("あいう"), "ａｉｕ");
        assert_eq!(f9_transform("カタカナ"), "ｋａｔａｋａｎａ");
    }

    #[test]
    fn f10_fullwidth_to_halfwidth() {
        assert_eq!(f10_transform("ＡＢＣ"), "ABC");
        assert_eq!(f10_transform("１２３"), "123");
        assert_eq!(f10_transform(""), "");
    }

    #[test]
    fn f10_katakana_to_half_width_romaji() {
        assert_eq!(f10_transform("カキク"), "kakiku");
        assert_eq!(f10_transform("ガッコウ"), "gakkou");
    }

    #[test]
    fn f10_mixed_input() {
        assert_eq!(f10_transform("ＡＢＣカキク"), "ABCkakiku");
        assert_eq!(f10_transform("あいう"), "aiu");
    }
}
