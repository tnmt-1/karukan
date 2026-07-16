//! Conversion state handling (candidates, commit). The live-conversion
//! chunking lives in the sibling `chunk` module.

use std::collections::HashSet;
use std::time::Instant;

use tracing::debug;

use super::*;

/// Maximum number of learning candidates to show
const MAX_LEARNING_CANDIDATES: usize = 3;

/// Mozc-style width/script annotation for a pure-kana candidate, or `None`
/// if the text mixes scripts or contains kanji/punctuation. Used to label
/// `あ` / `ア` / `ｱ` candidates in the conversion list.
fn width_annotation(text: &str) -> Option<&'static str> {
    if karukan_engine::is_pure_hiragana(text) {
        Some("[全]ひらがな")
    } else if karukan_engine::is_pure_full_katakana(text) {
        Some("[全]カタカナ")
    } else {
        None
    }
}

/// Helper for building a deduplicated list of conversion candidates.
///
/// Two push paths exist: [`push`] dedups by text (skips duplicates), and
/// [`push_force`] always inserts (used for learning candidates that should
/// appear at the top even if a later source re-emits the same text).
struct CandidateBuilder {
    candidates: Vec<AnnotatedCandidate>,
    seen: HashSet<String>,
}

impl CandidateBuilder {
    fn new() -> Self {
        Self {
            candidates: Vec::new(),
            seen: HashSet::new(),
        }
    }

    /// Push a candidate if its text hasn't been seen yet.
    fn push(&mut self, ac: AnnotatedCandidate) {
        if self.seen.insert(ac.text.clone()) {
            self.candidates.push(ac);
        }
    }

    /// Push a candidate unconditionally, marking its text as seen so later
    /// dedup'd inserts skip it. Use only for sources that should win over
    /// duplicates from later steps (e.g. learning cache).
    fn push_force(&mut self, ac: AnnotatedCandidate) {
        self.seen.insert(ac.text.clone());
        self.candidates.push(ac);
    }

    fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    fn into_candidates(self) -> Vec<AnnotatedCandidate> {
        self.candidates
    }
}

impl InputMethodEngine {
    /// Run kana-kanji conversion for a reading via llama.cpp model.
    ///
    /// Determines the conversion strategy (main model, light model, or parallel beam),
    /// dispatches to the appropriate model(s), measures latency, and records which model was used.
    ///
    /// Skips the model entirely when the reading has no hiragana/katakana — the
    /// model is trained on kana → kanji and hallucinates garbage (e.g. `「` → `w`)
    /// for symbol- or alphabet-only inputs. Rule-based variants from
    /// `SymbolRewriter` cover those cases instead.
    ///
    /// `api_context` is the left context (lctx) fed to the model. Callers pass
    /// `truncate_context_for_api()` for a whole-buffer conversion, or — for
    /// chunked live conversion — the converted text of the preceding chunks.
    pub(super) fn run_kana_kanji_conversion(
        &mut self,
        reading: &str,
        api_context: &str,
        num_candidates: usize,
    ) -> Vec<String> {
        if !karukan_engine::contains_kana(reading) {
            return vec![];
        }
        let Some(converter) = self.converters.kanji.as_ref() else {
            return vec![];
        };
        let katakana = karukan_engine::hiragana_to_katakana(reading);
        let main_model_name = converter.model_display_name().to_string();

        let strategy = self.determine_strategy(reading, num_candidates);
        debug!(
            "convert: reading=\"{}\" api_context=\"{}\" candidates={} strategy={:?}",
            reading, api_context, num_candidates, strategy
        );

        let start = Instant::now();

        let candidates = match &strategy {
            ConversionStrategy::ParallelBeam { beam_width } => {
                let Some(light_converter) = self.converters.light_kanji.as_ref() else {
                    return vec![];
                };
                let bw = *beam_width;
                let (default_top1, light_candidates) = std::thread::scope(|s| {
                    let h_default = s.spawn(|| {
                        converter
                            .convert(&katakana, api_context, 1)
                            .unwrap_or_default()
                    });
                    let h_beam = s.spawn(|| {
                        light_converter
                            .convert(&katakana, api_context, bw)
                            .unwrap_or_default()
                    });
                    (
                        h_default.join().unwrap_or_default(),
                        h_beam.join().unwrap_or_default(),
                    )
                });
                Self::merge_candidates_dedup(default_top1, light_candidates, bw)
            }
            ConversionStrategy::LightModelOnly => {
                let Some(light_converter) = self.converters.light_kanji.as_ref() else {
                    return vec![];
                };
                light_converter
                    .convert(&katakana, api_context, 1)
                    .unwrap_or_default()
            }
            ConversionStrategy::MainModelOnly => converter
                .convert(&katakana, api_context, 1)
                .unwrap_or_default(),
            ConversionStrategy::MainModelBeam { beam_width } => converter
                .convert(&katakana, api_context, *beam_width)
                .unwrap_or_default(),
        };

        self.metrics.conversion_ms = start.elapsed().as_millis() as u64;
        self.update_adaptive_model_flag(&strategy);

        self.metrics.model_name = match &strategy {
            ConversionStrategy::ParallelBeam { .. } => {
                let light_name = self
                    .converters
                    .light_kanji
                    .as_ref()
                    .map(|c| c.model_display_name().to_string())
                    .unwrap_or_default();
                format!("{}+{}", main_model_name, light_name)
            }
            ConversionStrategy::LightModelOnly => self
                .converters
                .light_kanji
                .as_ref()
                .map(|c| c.model_display_name().to_string())
                .unwrap_or(main_model_name),
            ConversionStrategy::MainModelOnly | ConversionStrategy::MainModelBeam { .. } => {
                main_model_name
            }
        };

        candidates
    }

    /// Start kanji conversion for the current input buffer.
    ///
    /// Called when DOWN/TAB/SPACE is pressed: flushes any pending romaji,
    /// resolves the reading, runs `build_conversion_candidates`, and
    /// transitions into the Conversion state. The previous live-conversion
    /// result is preserved as the first model candidate so the user sees
    /// the same text they had been looking at during input.
    ///
    /// `skip_learning` is set by the Tab path to omit learning-cache
    /// candidates (Space/Down keep the default learning-included behavior).
    pub(super) fn start_conversion(&mut self, skip_learning: bool) -> EngineResult {
        // Flush any remaining romaji into composed_hiragana
        self.flush_romaji_to_composed();

        let reading = self.input_buf.text.clone();

        // Save auto-suggest/live conversion result before clearing state.
        // This ensures the candidate that was displayed during input is preserved
        // in the conversion candidate list even if the re-inference uses a different strategy.
        let prev_suggest_text = std::mem::take(&mut self.live.text);

        self.converters.romaji.reset();
        self.input_buf.cursor_pos = 0;

        if reading.is_empty() {
            return EngineResult::consumed();
        }

        // Get candidates from kanji converter (use full num_candidates for explicit conversion)
        let mut candidates =
            self.build_conversion_candidates(&reading, self.config.num_candidates, skip_learning);

        // If the previous auto-suggest result is not in the new candidates, insert it at the top
        // so it doesn't disappear when the conversion strategy changes.
        let seen: HashSet<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
        if !prev_suggest_text.is_empty()
            && prev_suggest_text != reading
            && !seen.contains(prev_suggest_text.as_str())
        {
            candidates.insert(
                0,
                AnnotatedCandidate::new(prev_suggest_text, CandidateSource::Model),
            );
        }

        if candidates.is_empty() {
            // No candidates, stay in hiragana mode
            let preedit = Preedit::with_text_underlined(&reading);
            self.state = InputState::Composing {
                preedit: preedit.clone(),
                romaji_buffer: String::new(),
            };
            return EngineResult::consumed().with_action(EngineAction::UpdatePreedit(preedit));
        }

        // Map AnnotatedCandidate → public Candidate. The two annotation
        // slots are kept disjoint so descriptions never duplicate between the
        // aux text and the candidate's right-side comment:
        //   - `source_label` ← source.label() only (e.g. `🤖 AI`, `📚 辞書`)
        //   - `description`  ← the per-candidate description only
        //                      (e.g. `三点リーダ`, `[全]英大文字`)
        let candidate_list = CandidateList::new(
            candidates
                .into_iter()
                .map(|ac| {
                    let cand_reading = ac.reading.unwrap_or_else(|| reading.clone());
                    let label = ac.source.label();
                    Candidate {
                        text: ac.text,
                        reading: Some(cand_reading),
                        source_label: (!label.is_empty()).then(|| label.to_string()),
                        description: ac.description,
                    }
                })
                .collect(),
        );
        self.enter_conversion_state(&reading, candidate_list)
    }

    /// Transition to Conversion state with the given reading and candidate list.
    ///
    /// Sets up the preedit (highlighted selected text), updates the state, and
    /// returns an EngineResult with preedit, candidates, and aux text actions.
    fn enter_conversion_state(&mut self, reading: &str, candidates: CandidateList) -> EngineResult {
        let full_len = reading.chars().count();
        let selected_text = candidates.selected_text().unwrap_or(reading).to_string();

        let preedit = Preedit::from_segments(
            vec![PreeditSegment::highlighted(&selected_text)],
            selected_text.chars().count(),
        );

        self.state = InputState::Conversion {
            preedit: preedit.clone(),
            candidates: candidates.clone(),
            full_reading: reading.to_string(),
            range_start: 0,
            range_end: full_len,
        };

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(candidates.clone()))
            .with_action(EngineAction::UpdateAuxText(
                self.format_aux_conversion_with_page(reading, Some(&candidates)),
            ))
    }

    /// Search user and system dictionaries for candidates matching a reading.
    ///
    /// User dictionary results come first (higher priority), then system dictionary
    /// results sorted by score. Duplicates are removed via HashSet.
    fn search_dictionaries(&self, reading: &str, limit: usize) -> Vec<AnnotatedCandidate> {
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();

        // User dictionary (higher priority)
        if let Some(dict) = &self.dicts.user
            && let Some(result) = dict.exact_match_search(reading)
        {
            for cand in result.candidates {
                if candidates.len() >= limit {
                    break;
                }
                if seen.insert(cand.surface.clone()) {
                    candidates.push(AnnotatedCandidate::new(
                        cand.surface.clone(),
                        CandidateSource::UserDictionary,
                    ));
                }
            }
        }

        // System dictionary (sorted by score)
        if let Some(dict) = &self.dicts.system
            && let Some(result) = dict.exact_match_search(reading)
        {
            let mut dict_candidates: Vec<_> = result.candidates.to_vec();
            dict_candidates.sort_by(|a, b| a.score.total_cmp(&b.score));
            for cand in dict_candidates {
                if candidates.len() >= limit {
                    break;
                }
                if seen.insert(cand.surface.clone()) {
                    candidates.push(AnnotatedCandidate::new(
                        cand.surface,
                        CandidateSource::Dictionary,
                    ));
                }
            }
        }

        candidates
    }

    /// Build conversion candidates for a reading from multiple sources.
    ///
    /// Combines learning cache, dictionaries, and model inference results
    /// with deduplication. Uses dynamic candidate count based on input token
    /// count for performance.
    ///
    /// Priority: Learning → User Dictionary → Model → System Dictionary → Fallback
    ///
    /// `skip_learning` suppresses the learning-cache step (1). Used by the Tab
    /// key path so users can escape a noisy learning history without losing
    /// access to dictionary/model candidates.
    pub(super) fn build_conversion_candidates(
        &mut self,
        reading: &str,
        num_candidates: usize,
        skip_learning: bool,
    ) -> Vec<AnnotatedCandidate> {
        // Try to initialize the kanji converter, but don't bail out if it
        // fails — symbol-only inputs (e.g. `。。。`) don't need the model and
        // we still want to produce dictionary, rewriter, and fallback candidates.
        // run_kana_kanji_conversion handles the converter-missing case.
        if self.converters.kanji.is_none()
            && let Err(e) = self.init_kanji_converter()
        {
            debug!("Failed to initialize kanji converter: {}", e);
        }

        let api_context = self.truncate_context_for_api();
        let candidates = self.run_kana_kanji_conversion(reading, &api_context, num_candidates);

        let hiragana = reading.to_string();
        let katakana = karukan_engine::hiragana_to_katakana(reading);

        // Priority: Learning → User Dictionary → Model → System Dictionary → Fallback
        let mut builder = CandidateBuilder::new();

        // 1. Learning cache candidates (highest priority).
        //    Force-inserted so they win against duplicate text from later sources.
        //    Skipped when the caller asks for a learning-free conversion (Tab key).
        if !skip_learning {
            for c in self.lookup_learning_candidates(reading) {
                // Exact matches have reading == input reading; use None to avoid redundancy
                let cand_reading = c.reading.filter(|r| r != reading);
                builder.push_force(
                    AnnotatedCandidate::new(c.text, CandidateSource::Learning)
                        .with_reading(cand_reading),
                );
            }
        }

        // 2. Dictionary candidates (user dict first, then system dict)
        let dict_results = self.search_dictionaries(reading, usize::MAX);
        // Insert user dictionary entries at the top (after learning)
        for ac in &dict_results {
            if ac.source == CandidateSource::UserDictionary {
                builder.push(ac.clone());
            }
        }

        // 3. Model inference results
        if candidates.is_empty() {
            // In emoji mode, defer the literal-fallback decision until
            // after rewriters have run — otherwise `:smile` would be
            // pinned to the top of the candidate list as a Fallback
            // and outrank the 😄 we surface in step 5/6.
            if builder.is_empty() && self.input_mode != InputMode::Emoji {
                builder.push(AnnotatedCandidate::new(
                    hiragana.clone(),
                    CandidateSource::Fallback,
                ));
            }
        } else {
            for text in candidates {
                builder.push(AnnotatedCandidate::new(text, CandidateSource::Model));
            }
        }

        // 4. System dictionary candidates (from search_dictionaries result)
        for ac in dict_results {
            if ac.source == CandidateSource::Dictionary {
                builder.push(ac);
            }
        }

        // 5/6. Hiragana/katakana fallback + rewriter variants.
        //
        // In emoji mode we surface ONLY the rewriter (i.e. EmojiRewriter)
        // candidates — Slack's emoji picker shows emojis and nothing
        // else, and that's the mental model the user wants here.
        // No literal `:smile` / `:xyz` fallback in the candidate list:
        // if nothing matches, the picker is just empty. (Enter on a
        // no-match query in Composing still commits the buffer
        // literal via `commit_composing`; that's the escape hatch.)
        // Non-emoji modes keep the original order so existing IME
        // behavior is untouched.
        let rewriter_variants = self
            .converters
            .rewriters
            .rewrite_all(&[reading.to_string()]);
        if self.input_mode == InputMode::Emoji {
            for (variant, description) in rewriter_variants {
                builder.push(
                    AnnotatedCandidate::new(variant, CandidateSource::Rewriter)
                        .with_description(description),
                );
            }
        } else {
            builder.push(AnnotatedCandidate::new(hiragana, CandidateSource::Fallback));
            builder.push(AnnotatedCandidate::new(katakana, CandidateSource::Fallback));
            // Rewriters operate on the user's typed input (the reading
            // itself). Running them on dictionary/model/fallback
            // candidates produces unrelated noise (e.g. a dictionary
            // entry of `,` for some reading would generate `、`/`，`
            // variants the user never asked for; a learning entry `アト`
            // pulled by prefix lookup on `あ` would emit `ｱﾄ`).
            for (variant, description) in rewriter_variants {
                builder.push(
                    AnnotatedCandidate::new(variant, CandidateSource::Rewriter)
                        .with_description(description),
                );
            }
        }

        // 7. Enrich Fallback candidates whose text is a known symbol with
        //    its description (mirrors the relevant slice of mozc's
        //    `AddDescForCurrentCandidates`). Restricted to Fallback so the
        //    AI/Dict/Learning paths don't pick up unwanted labels — e.g.
        //    the model returning `金` for `きん` should NOT inherit mozc's
        //    "部首" annotation. Typed-symbol input still gets annotated:
        //    pressing `「` produces a Fallback candidate `「`, which here
        //    picks up "始めかぎ括弧".
        for c in &mut builder.candidates {
            if c.source == CandidateSource::Fallback
                && c.description.is_none()
                && let Some(desc) = karukan_engine::symbol_description(&c.text)
            {
                c.description = Some(desc.to_string());
            }
        }

        // 8. Attach mozc-style width annotations (`[全]ひらがな`,
        //    `[全]カタカナ`, `[半]カタカナ`) to any pure-kana candidate that
        //    still has no description. This catches `あ`/`ア` candidates that
        //    arrived via the Model or Fallback paths and were deduped against
        //    the rewriter's already-labelled variants.
        for c in &mut builder.candidates {
            if c.description.is_none()
                && let Some(desc) = width_annotation(&c.text)
            {
                c.description = Some(desc.to_string());
            }
        }

        builder.into_candidates()
    }

    /// Look up learning cache candidates for a reading (exact + prefix match, max 3).
    ///
    /// Returns candidates from the learning cache suitable for auto-suggest display.
    pub(super) fn lookup_learning_candidates(&self, reading: &str) -> Vec<Candidate> {
        let Some(cache) = &self.learning else {
            return vec![];
        };
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut seen = HashSet::new();
        let label = CandidateSource::Learning.label().to_string();

        // Exact match
        for (surface, _score) in cache.lookup(reading) {
            if candidates.len() >= MAX_LEARNING_CANDIDATES {
                break;
            }
            if seen.insert(surface.clone()) {
                candidates.push(Candidate {
                    text: surface,
                    reading: Some(reading.to_string()),
                    source_label: Some(label.clone()),
                    description: None,
                });
            }
        }

        // Prefix match (predictive)
        for (full_reading, surface, _score) in cache.prefix_lookup(reading) {
            if candidates.len() >= MAX_LEARNING_CANDIDATES {
                break;
            }
            if full_reading == reading {
                continue;
            }
            if seen.insert(surface.clone()) {
                candidates.push(Candidate {
                    text: surface,
                    reading: Some(full_reading),
                    source_label: Some(label.clone()),
                    description: None,
                });
            }
        }

        candidates
    }

    /// Look up dictionary candidates for a reading (1 page, for live conversion display)
    ///
    /// Searches user dictionary first, then system dictionary.
    pub(super) fn lookup_dict_candidates(&self, reading: &str) -> Vec<Candidate> {
        self.search_dictionaries(reading, CandidateList::DEFAULT_PAGE_SIZE)
            .into_iter()
            .map(|ac| Candidate {
                text: ac.text,
                reading: Some(reading.to_string()),
                source_label: Some(ac.source.label().to_string()),
                description: None,
            })
            .collect()
    }

    /// Build rule-based rewriter variants for the reading itself (e.g. for
    /// symbol input `「` → `『`, `【`, `（`, ...). Used in the auto-suggest path
    /// so users see mozc-style symbol variants without pressing Space first.
    pub(super) fn lookup_rewriter_variants(&self, reading: &str) -> Vec<Candidate> {
        let source_label = CandidateSource::Rewriter.label().to_string();
        self.converters
            .rewriters
            .rewrite_all(&[reading.to_string()])
            .into_iter()
            .map(|(text, description)| Candidate {
                text,
                reading: Some(reading.to_string()),
                source_label: Some(source_label.clone()),
                description,
            })
            .collect()
    }

    /// Merge two candidate lists with deduplication
    /// Primary candidates come first, then secondary candidates that aren't duplicates
    pub(super) fn merge_candidates_dedup(
        primary: Vec<String>,
        secondary: Vec<String>,
        max_candidates: usize,
    ) -> Vec<String> {
        let mut seen = HashSet::new();
        primary
            .into_iter()
            .chain(secondary)
            .filter(|c| seen.insert(c.clone()))
            .take(max_candidates)
            .collect()
    }

    /// Build a preedit that shows inactive parts as plain underlined hiragana
    /// and the active conversion range as highlighted.
    fn build_range_preedit(
        &self,
        full_reading: &str,
        selected_text: &str,
        range_start: usize,
        range_end: usize,
    ) -> Preedit {
        let chars: Vec<char> = full_reading.chars().collect();
        let before: String = chars[..range_start].iter().collect();
        let after: String = chars[range_end..].iter().collect();

        if before.is_empty() && after.is_empty() {
            Preedit::from_segments(
                vec![PreeditSegment::highlighted(selected_text)],
                selected_text.chars().count(),
            )
        } else {
            let caret = before.chars().count() + selected_text.chars().count();
            let mut segments = Vec::new();
            if !before.is_empty() {
                segments.push(PreeditSegment::new(&before, AttributeType::Underline));
            }
            segments.push(PreeditSegment::highlighted(selected_text));
            if !after.is_empty() {
                segments.push(PreeditSegment::new(&after, AttributeType::Underline));
            }
            Preedit::from_segments(segments, caret)
        }
    }

    /// Rebuild the candidate list for a (sub-)reading during range adjustment.
    fn rebuild_range_candidates(&mut self, reading: &str) -> CandidateList {
        let annotated =
            self.build_conversion_candidates(reading, self.config.num_candidates, false);
        CandidateList::new(
            annotated
                .into_iter()
                .map(|ac| {
                    let cand_reading = ac.reading.unwrap_or_else(|| reading.to_string());
                    let label = ac.source.label();
                    Candidate {
                        text: ac.text,
                        reading: Some(cand_reading),
                        source_label: (!label.is_empty()).then(|| label.to_string()),
                        description: ac.description,
                    }
                })
                .collect(),
        )
    }

    /// Apply a new conversion range, rebuilding candidates and preedit.
    ///
    /// Mozc / MS-IME style: the left edge (`range_start`) stays fixed and
    /// Shift+←/→ move only the right edge (`range_end`).
    fn apply_conversion_range(
        &mut self,
        full_reading: String,
        range_start: usize,
        range_end: usize,
    ) -> EngineResult {
        let sub_reading: String = full_reading
            .chars()
            .skip(range_start)
            .take(range_end - range_start)
            .collect();
        let candidates = self.rebuild_range_candidates(&sub_reading);
        let selected_text = candidates
            .selected_text()
            .unwrap_or(&sub_reading)
            .to_string();
        let preedit =
            self.build_range_preedit(&full_reading, &selected_text, range_start, range_end);

        self.state = InputState::Conversion {
            preedit: preedit.clone(),
            candidates: candidates.clone(),
            full_reading,
            range_start,
            range_end,
        };

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(candidates.clone()))
            .with_action(EngineAction::UpdateAuxText(
                self.format_aux_conversion_with_page(&sub_reading, Some(&candidates)),
            ))
    }

    /// Shrink the conversion range from the right (Shift+←):
    /// decreases `range_end` by one.
    fn shrink_conversion_range(&mut self) -> EngineResult {
        let (full_reading, range_start, range_end) = match &self.state {
            InputState::Conversion {
                full_reading,
                range_start,
                range_end,
                ..
            } => (full_reading.clone(), *range_start, *range_end),
            _ => return EngineResult::not_consumed(),
        };

        if range_end <= range_start + 1 {
            return EngineResult::consumed();
        }

        self.apply_conversion_range(full_reading, range_start, range_end - 1)
    }

    /// Expand the conversion range to the right (Shift+→):
    /// increases `range_end` by one, up to the end of `full_reading`.
    fn expand_conversion_range(&mut self) -> EngineResult {
        let (full_reading, range_start, range_end) = match &self.state {
            InputState::Conversion {
                full_reading,
                range_start,
                range_end,
                ..
            } => (full_reading.clone(), *range_start, *range_end),
            _ => return EngineResult::not_consumed(),
        };

        let full_len = full_reading.chars().count();
        if range_end >= full_len {
            return EngineResult::consumed();
        }

        self.apply_conversion_range(full_reading, range_start, range_end + 1)
    }

    /// Process key in conversion state
    pub(super) fn process_key_conversion(&mut self, key: &KeyEvent) -> EngineResult {
        // Shift+←/→: resize conversion range (right edge only, Mozc/MS-IME style)
        if key.modifiers.shift_key {
            match key.keysym {
                Keysym::LEFT => return self.shrink_conversion_range(),
                Keysym::RIGHT => return self.expand_conversion_range(),
                _ => {}
            }
        }

        match key.keysym {
            Keysym::RETURN => self.commit_conversion(),
            Keysym::ESCAPE => self.cancel_conversion(),
            Keysym::SPACE | Keysym::DOWN | Keysym::TAB => self.next_candidate(),
            Keysym::UP => self.prev_candidate(),
            Keysym::PAGE_DOWN => self.next_candidate_page(),
            Keysym::PAGE_UP => self.prev_candidate_page(),
            Keysym::BACKSPACE => self.backspace_conversion(),
            _ => {
                // Ctrl+N / Ctrl+P: emacs-style candidate navigation
                if key.modifiers.control_key && !key.modifiers.alt_key {
                    match key.keysym {
                        Keysym::KEY_N | Keysym::KEY_N_UPPER => return self.next_candidate(),
                        Keysym::KEY_P | Keysym::KEY_P_UPPER => return self.prev_candidate(),
                        _ => {}
                    }
                }

                // Check for digit selection (1-9)
                if let Some(digit) = key.keysym.digit_value() {
                    return self.select_candidate_by_digit(digit);
                }

                // Any printable character: commit current conversion and start new input
                if let Some(ch) = key.to_char()
                    && !key.modifiers.control_key
                    && !key.modifiers.alt_key
                {
                    return self.commit_conversion_and_continue(ch);
                }

                EngineResult::not_consumed()
            }
        }
    }

    /// Get selected text and reading from conversion state, or None if not in conversion
    fn selected_conversion_info(&self) -> Option<(String, Option<String>)> {
        match &self.state {
            InputState::Conversion { candidates, .. } => {
                let text = candidates.selected_text().unwrap_or("").to_string();
                let reading = candidates.selected().and_then(|c| c.reading.clone());
                Some((text, reading))
            }
            _ => None,
        }
    }

    /// Record a conversion selection in the learning cache.
    pub(super) fn record_learning(&mut self, reading: &str, surface: &str) {
        if let Some(cache) = &mut self.learning {
            cache.record(reading, surface);
        }
    }

    /// Commit the current conversion (or the active range in range mode).
    ///
    /// When the conversion range has been narrowed (Shift+←/→), only the
    /// active segment is committed. The remaining (non-active) parts of the
    /// reading are put back into the composing buffer so the user sees
    /// auto-suggest candidates for the leftover text immediately.
    fn commit_conversion(&mut self) -> EngineResult {
        let Some((text, reading)) = self.selected_conversion_info() else {
            return EngineResult::not_consumed();
        };

        if text.is_empty() {
            return EngineResult::consumed();
        }

        // Check if we're in narrowed range mode
        let (range_start, range_end, full_reading) = match &self.state {
            InputState::Conversion {
                full_reading,
                range_start,
                range_end,
                ..
            } => (*range_start, *range_end, full_reading.clone()),
            _ => return EngineResult::not_consumed(),
        };

        let full_len = full_reading.chars().count();
        let is_partial = range_start != 0 || range_end != full_len;

        // Skip learning when the buffer is a `:shortcode` query — the
        // reading would be e.g. `:smile`, which isn't a hiragana key
        // and would corrupt the kana-keyed learning cache.
        if self.input_mode != InputMode::Emoji
            && let Some(reading) = &reading
        {
            self.record_learning(reading, &text);
        }

        self.state = InputState::Empty;

        if is_partial && !full_reading.is_empty() {
            // Narrowed range: commit only the active segment, keep the rest
            let chars: Vec<char> = full_reading.chars().collect();
            let before: String = chars[..range_start].iter().collect();
            let after: String = chars[range_end..].iter().collect();
            self.input_buf.text = format!("{}{}", before, after);
            self.input_buf.cursor_pos = before.chars().count();
            self.exit_emoji_mode();
            self.exit_alphabet_mode();

            // Re-enter composing for the remaining text, showing auto-suggest
            if !self.input_buf.text.is_empty() {
                let mut result = EngineResult::consumed()
                    .with_action(EngineAction::Commit(text))
                    .with_action(EngineAction::HideCandidates);
                let refresh = self.refresh_input_state();
                result.actions.extend(refresh.actions);
                result
            } else {
                EngineResult::consumed()
                    .with_action(EngineAction::Commit(text))
                    .with_action(EngineAction::HideCandidates)
                    .with_action(EngineAction::HideAuxText)
            }
        } else {
            // Full range: existing behavior — clear everything
            self.input_buf.text.clear();
            self.exit_emoji_mode();
            self.exit_alphabet_mode();

            EngineResult::consumed()
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::HideAuxText)
                .with_action(EngineAction::Commit(text))
        }
    }

    /// Commit current conversion and then process a new character as fresh input.
    ///
    /// In narrowed range mode, the whole buffer (converted + pending hiragana)
    /// is committed as a single string, matching standard IME behavior where
    /// typing a printable character during conversion finalises everything.
    fn commit_conversion_and_continue(&mut self, ch: char) -> EngineResult {
        let Some((text, _reading)) = self.selected_conversion_info() else {
            return EngineResult::not_consumed();
        };

        // Build the full commit text (range-aware)
        let commit_text = self.build_full_commit_text(&text);

        // Use the selected candidate's reading (sub-range) for learning
        let reading_for_learning = match &self.state {
            InputState::Conversion { candidates, .. } => {
                candidates.selected().and_then(|c| c.reading.clone())
            }
            _ => None,
        };
        if self.input_mode != InputMode::Emoji
            && let Some(r) = &reading_for_learning
        {
            self.record_learning(r, &text);
        }

        self.state = InputState::Empty;
        self.input_buf.text.clear();
        self.exit_emoji_mode();
        self.exit_alphabet_mode();

        // Start new input with the character
        let new_input_result = self.start_input(ch);

        // Combine: commit first, then new input actions
        let mut result = EngineResult::consumed()
            .with_action(EngineAction::Commit(commit_text))
            .with_action(EngineAction::HideCandidates);
        result.actions.extend(new_input_result.actions);
        result
    }

    /// Build the full text to commit, combining converted active range with
    /// pending hiragana segments when in narrowed range mode.
    fn build_full_commit_text(&self, converted_text: &str) -> String {
        match &self.state {
            InputState::Conversion {
                full_reading,
                range_start,
                range_end,
                ..
            } => {
                let full_len = full_reading.chars().count();
                if *range_start == 0 && *range_end == full_len {
                    converted_text.to_string()
                } else {
                    let chars: Vec<char> = full_reading.chars().collect();
                    let before: String = chars[..*range_start].iter().collect();
                    let after: String = chars[*range_end..].iter().collect();
                    format!("{}{}{}", before, converted_text, after)
                }
            }
            _ => converted_text.to_string(),
        }
    }

    /// Cancel conversion and return to hiragana.
    ///
    /// Restores the full reading (including any segments that were outside
    /// the narrowed conversion range) so no input is lost on Escape.
    pub(super) fn cancel_conversion(&mut self) -> EngineResult {
        if !matches!(self.state, InputState::Conversion { .. }) {
            return EngineResult::not_consumed();
        }

        let reading = match &self.state {
            InputState::Conversion { full_reading, .. } if !full_reading.is_empty() => {
                full_reading.clone()
            }
            _ => self.input_buf.text.clone(),
        };

        if reading.is_empty() {
            self.state = InputState::Empty;
            self.input_buf.clear();
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(Preedit::new()))
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::HideAuxText);
        }

        // Set up composed_hiragana with the reading
        self.input_buf.text = reading.clone();
        self.input_buf.cursor_pos = self.input_buf.text.chars().count();

        // Reset romaji converter and set output to reading
        self.converters.romaji.reset();
        // We need to push each character to rebuild the state
        for ch in reading.chars() {
            self.converters.romaji.push(ch);
        }

        let preedit = self.set_composing_state();

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()))
    }

    /// Navigate candidates with the given operation, then update preedit
    fn navigate_candidate(&mut self, op: impl FnOnce(&mut CandidateList) -> bool) -> EngineResult {
        let (selected_text, candidates) = {
            let Some(candidates) = self.state.candidates_mut() else {
                return EngineResult::not_consumed();
            };
            op(candidates);
            let text = candidates.selected_text().unwrap_or("").to_string();
            (text, candidates.clone())
        };
        self.update_conversion_preedit(&selected_text, &candidates)
    }

    /// Select next candidate
    fn next_candidate(&mut self) -> EngineResult {
        self.navigate_candidate(CandidateList::move_next)
    }

    /// Select previous candidate
    fn prev_candidate(&mut self) -> EngineResult {
        self.navigate_candidate(CandidateList::move_prev)
    }

    /// Go to next candidate page
    fn next_candidate_page(&mut self) -> EngineResult {
        self.navigate_candidate(CandidateList::next_page)
    }

    /// Go to previous candidate page
    fn prev_candidate_page(&mut self) -> EngineResult {
        self.navigate_candidate(CandidateList::prev_page)
    }

    /// Select and commit the candidate at `page_index` (0-based) within the
    /// current page, like pressing the digit key `page_index + 1`. Not
    /// consumed unless a candidate list is active (Conversion state).
    pub fn select_candidate_on_page(&mut self, page_index: usize) -> EngineResult {
        let start = std::time::Instant::now();
        self.metrics.conversion_ms = 0;
        let result = self.select_candidate_by_digit(page_index + 1);
        self.metrics.process_key_ms = start.elapsed().as_millis() as u64;
        result
    }

    /// Select candidate by digit (1-9).
    ///
    /// In range mode, commits only the active segment and keeps the
    /// remaining text in composing state (same as `commit_conversion`).
    fn select_candidate_by_digit(&mut self, digit: usize) -> EngineResult {
        let (selected_text, reading) = {
            let candidates = match self.state.candidates_mut() {
                Some(c) => c,
                None => return EngineResult::not_consumed(),
            };

            if candidates.select_on_page(digit).is_none() {
                return EngineResult::consumed();
            }

            let text = candidates.selected_text().unwrap_or("").to_string();
            let reading = candidates.selected().and_then(|c| c.reading.clone());
            (text, reading)
        };

        // Record learning before committing
        if let Some(reading) = &reading {
            self.record_learning(reading, &selected_text);
        }

        // Check if we're in narrowed range mode
        let (range_start, range_end, full_reading) = match &self.state {
            InputState::Conversion {
                full_reading,
                range_start,
                range_end,
                ..
            } => (*range_start, *range_end, full_reading.clone()),
            _ => return EngineResult::not_consumed(),
        };

        let full_len = full_reading.chars().count();
        let is_partial = range_start != 0 || range_end != full_len;

        if is_partial && !full_reading.is_empty() && !selected_text.is_empty() {
            // Narrowed mode: commit active segment, re-enter composing for the rest
            let chars: Vec<char> = full_reading.chars().collect();
            let before: String = chars[..range_start].iter().collect();
            let after: String = chars[range_end..].iter().collect();

            self.state = InputState::Empty;
            self.input_buf.text = format!("{}{}", before, after);
            self.input_buf.cursor_pos = before.chars().count();

            let mut result = EngineResult::consumed()
                .with_action(EngineAction::Commit(selected_text))
                .with_action(EngineAction::HideCandidates);
            if !self.input_buf.text.is_empty() {
                let refresh = self.refresh_input_state();
                result.actions.extend(refresh.actions);
            } else {
                result = result.with_action(EngineAction::HideAuxText);
            }
            result
        } else {
            // Full range: existing immediate-commit behavior
            self.state = InputState::Empty;
            EngineResult::consumed()
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::HideAuxText)
                .with_action(EngineAction::Commit(selected_text))
        }
    }

    /// Update preedit after candidate selection change.
    ///
    /// In range mode, uses `build_range_preedit` to show inactive segments
    /// as plain underlined hiragana alongside the highlighted active segment.
    fn update_conversion_preedit(
        &mut self,
        selected_text: &str,
        candidates: &CandidateList,
    ) -> EngineResult {
        let (full_reading, range_start, range_end) = match &self.state {
            InputState::Conversion {
                full_reading,
                range_start,
                range_end,
                ..
            } => (full_reading.clone(), *range_start, *range_end),
            _ => {
                // Fallback (shouldn't happen): simple highlighted preedit
                let mut preedit = Preedit::with_text(selected_text);
                preedit.set_attributes(vec![PreeditAttribute::new(
                    0,
                    selected_text.chars().count(),
                    AttributeType::Highlight,
                )]);
                if let Some(p) = self.state.preedit_mut() {
                    *p = preedit.clone();
                }
                let reading = candidates
                    .selected()
                    .and_then(|c| c.reading.as_deref())
                    .unwrap_or("");
                return EngineResult::consumed()
                    .with_action(EngineAction::UpdatePreedit(preedit))
                    .with_action(EngineAction::ShowCandidates(candidates.clone()))
                    .with_action(EngineAction::UpdateAuxText(
                        self.format_aux_conversion_with_page(reading, Some(candidates)),
                    ));
            }
        };

        let preedit =
            self.build_range_preedit(&full_reading, selected_text, range_start, range_end);

        if let Some(p) = self.state.preedit_mut() {
            *p = preedit.clone();
        }

        let reading = candidates
            .selected()
            .and_then(|c| c.reading.as_deref())
            .unwrap_or("");

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(candidates.clone()))
            .with_action(EngineAction::UpdateAuxText(
                self.format_aux_conversion_with_page(reading, Some(candidates)),
            ))
    }

    /// Handle backspace in conversion mode
    fn backspace_conversion(&mut self) -> EngineResult {
        // Return to hiragana mode with the reading
        self.cancel_conversion()
    }
}
