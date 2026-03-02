use std::collections::HashMap;

use crate::deinflect_rules::get_rules_by_ending;
use crate::normalize::kana_to_hiragana;
use crate::types::{word_type::*, CandidateWord, Reason};

/// Returns all possible de-inflected forms of `word`.
///
/// Ported directly from the Python `_deinflect_py` function.
pub fn deinflect(word: &str) -> Vec<CandidateWord> {
    let mut result: Vec<CandidateWord> = Vec::new();
    let mut result_index: HashMap<String, usize> = HashMap::new();

    let rules_by_ending = get_rules_by_ending();

    // Seed with original word; type mask allows everything except the three
    // intermediate stem types that don't make sense on their own.
    let initial_type = 0xffffu16 ^ (TA_TE_STEM | DA_DE_STEM | IRREALIS_STEM);
    result.push(CandidateWord {
        word: word.to_string(),
        word_type: initial_type,
        reason_chains: vec![],
    });
    result_index.insert(word.to_string(), 0);

    let mut i = 0;
    while i < result.len() {
        // Clone everything we need before mutating `result`.
        let word_text = result[i].word.clone();
        let word_type = result[i].word_type;
        let reason_chains = result[i].reason_chains.clone();

        // Skip Ichidan masu-stem results — they are already the plain form.
        if word_type & ICHIDAN_VERB != 0
            && reason_chains.len() == 1
            && reason_chains[0].len() == 1
            && reason_chains[0][0] == Reason::MasuStem
        {
            i += 1;
            continue;
        }

        // Forward intermediate stems to the plain Ichidan/Kuru form by appending る.
        if word_type & (MASU_STEM | TA_TE_STEM | IRREALIS_STEM) != 0 {
            let inapplicable = word_type & IRREALIS_STEM != 0
                && !reason_chains.is_empty()
                && !reason_chains[0].is_empty()
                && matches!(
                    reason_chains[0][0],
                    Reason::Passive | Reason::Causative | Reason::CausativePassive
                );

            if !inapplicable {
                let mut extra_reason: Vec<Vec<Reason>> = Vec::new();
                if word_type & MASU_STEM != 0 && reason_chains.is_empty() {
                    extra_reason.push(vec![Reason::MasuStem]);
                }

                let new_word = format!("{}る", word_text);
                let mut new_chains = reason_chains.clone();
                new_chains.extend(extra_reason);

                if !result_index.contains_key(&new_word) {
                    result_index.insert(new_word.clone(), result.len());
                    result.push(CandidateWord {
                        word: new_word,
                        word_type: ICHIDAN_VERB | KURU_VERB,
                        reason_chains: new_chains,
                    });
                }
            }
        }

        // Try deinflection rules for each suffix length 1..=7.
        let chars: Vec<char> = word_text.chars().collect();
        let max_len = chars.len().min(7);

        for from_len in (1..=max_len).rev() {
            let ending: String = chars[chars.len() - from_len..].iter().collect();
            let hiragana_ending = kana_to_hiragana(&ending);

            // Collect matching rules (original ending + hiragana variant).
            let mut matching_rules = Vec::new();
            if let Some(rules) = rules_by_ending.get(ending.as_str()) {
                matching_rules.extend_from_slice(rules);
            }
            if hiragana_ending != ending {
                if let Some(rules) = rules_by_ending.get(hiragana_ending.as_str()) {
                    matching_rules.extend_from_slice(rules);
                }
            }

            for rule in matching_rules {
                if word_type & rule.from_type == 0 {
                    continue;
                }

                // Build the deinflected word.
                let stem: String = chars[..chars.len() - from_len].iter().collect();
                let new_word = format!("{}{}", stem, rule.to);
                if new_word.is_empty() {
                    continue;
                }

                // Skip if any reason in this rule already appears in the chain.
                let flat_reasons: Vec<Reason> = reason_chains
                    .iter()
                    .flat_map(|c| c.iter().copied())
                    .collect();
                if rule.reasons.iter().any(|r| flat_reasons.contains(r)) {
                    continue;
                }

                // If a candidate with this word and type already exists, prepend a
                // new reason chain (don't create a duplicate entry).
                if let Some(&existing_idx) = result_index.get(&new_word) {
                    if result[existing_idx].word_type == rule.to_type {
                        if !rule.reasons.is_empty() {
                            result[existing_idx]
                                .reason_chains
                                .insert(0, rule.reasons.to_vec());
                        }
                        continue;
                    }
                }

                // Deep-clone reason chains from current candidate.
                let mut new_chains: Vec<Vec<Reason>> =
                    reason_chains.iter().map(|c| c.clone()).collect();

                if !rule.reasons.is_empty() {
                    if !new_chains.is_empty() {
                        let first = &mut new_chains[0];
                        // Combine causative + potential-or-passive → causative passive.
                        if rule.reasons[0] == Reason::Causative
                            && !first.is_empty()
                            && first[0] == Reason::PotentialOrPassive
                        {
                            first[0] = Reason::CausativePassive;
                        } else if rule.reasons[0] == Reason::MasuStem {
                            // Don't prepend MasuStem when chain already has content.
                        } else {
                            // Prepend rule reasons to first chain.
                            let mut prepended = rule.reasons.to_vec();
                            prepended.extend_from_slice(first);
                            *first = prepended;
                        }
                    } else {
                        new_chains.push(rule.reasons.to_vec());
                    }
                }

                result_index.insert(new_word.clone(), result.len());
                result.push(CandidateWord {
                    word: new_word,
                    word_type: rule.to_type,
                    reason_chains: new_chains,
                });
            }
        }

        i += 1;
    }

    // Keep only candidates that are valid final dictionary-entry types.
    result.retain(|c| c.word_type & ALL_FINAL != 0);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_result(candidates: &[CandidateWord], word: &str) -> bool {
        candidates.iter().any(|c| c.word == word)
    }

    fn has_result_with_reason(candidates: &[CandidateWord], word: &str, reason: Reason) -> bool {
        candidates
            .iter()
            .any(|c| c.word == word && c.reason_chains.iter().any(|chain| chain.contains(&reason)))
    }

    #[test]
    fn test_original_word_preserved() {
        let r = deinflect("食べる");
        assert!(has_result(&r, "食べる"));
    }

    #[test]
    fn test_past_tense() {
        let r = deinflect("読んだ");
        assert!(
            has_result(&r, "読む"),
            "読む not found in {:?}",
            r.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_te_form() {
        // 食べて: て→'' (TA_TE_STEM), then stem forwarding appends る → 食べる
        let r = deinflect("食べて");
        assert!(has_result_with_reason(&r, "食べる", Reason::Te));
    }

    #[test]
    fn test_negative() {
        let r = deinflect("読まない");
        assert!(has_result(&r, "読む"), "読む not found");
    }

    #[test]
    fn test_polite() {
        let r = deinflect("走ります");
        assert!(has_result_with_reason(&r, "走る", Reason::Polite));
    }

    #[test]
    fn test_i_adj_past() {
        let r = deinflect("高かった");
        assert!(has_result_with_reason(&r, "高い", Reason::Past));
    }

    #[test]
    fn test_i_adj_negative() {
        let r = deinflect("高くない");
        assert!(has_result(&r, "高い"));
    }

    #[test]
    fn test_i_adj_adv() {
        let r = deinflect("高く");
        assert!(has_result_with_reason(&r, "高い", Reason::Adv));
    }

    #[test]
    fn test_suru_volitional() {
        let r = deinflect("しよう");
        assert!(has_result_with_reason(&r, "する", Reason::Volitional));
    }

    #[test]
    fn test_suru_negative() {
        let r = deinflect("しない");
        assert!(has_result(&r, "する"));
    }

    #[test]
    fn test_suru_past() {
        let r = deinflect("した");
        assert!(has_result(&r, "する"));
    }

    #[test]
    fn test_suru_te() {
        let r = deinflect("して");
        assert!(has_result_with_reason(&r, "する", Reason::Te));
    }

    #[test]
    fn test_iku_past() {
        // 行った → 行く (irregular te-form)
        let r = deinflect("行った");
        assert!(
            has_result(&r, "行く"),
            "行く not found in {:?}",
            r.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_chain_reason() {
        // 踊りたくなかった → 踊る via Tai, Negative, Past
        let r = deinflect("踊りたくなかった");
        assert!(has_result(&r, "踊る"), "踊る not found");
    }

    #[test]
    fn test_no_duplicate_reasons() {
        // 見させさせる should NOT produce 見る (duplicate Causative)
        let r = deinflect("見させさせる");
        let found = r.iter().any(|c| c.word == "見る");
        assert!(!found, "見る should NOT appear (duplicate causative)");
    }
}
