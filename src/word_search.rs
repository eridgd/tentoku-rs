use std::collections::HashSet;

use crate::deinflect::deinflect;
use crate::dictionary::Dictionary;
use crate::normalize::normalize_input;
use crate::sorting::sort_word_results;
use crate::type_matching::entry_matches_type;
use crate::types::WordResult;
use crate::variations::{expand_choon, kyuujitai_to_shinjitai};
use crate::yoon::ends_in_yoon;

/// Result of a word search at a single position in text.
pub struct WordSearchResult {
    pub data: Vec<WordResult>,
    /// Length (in `input_lengths` units) of the longest match found.
    pub match_len: usize,
    pub more: bool,
}

/// Returns `true` if `text` consists only of digits (half/full-width) and
/// common numeric punctuation.
pub fn is_only_digits(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    text.chars().all(|c| {
        let code = c as u32;
        (0x0030..=0x0039).contains(&code)   // half-width 0-9
            || (0xFF10..=0xFF19).contains(&code) // full-width ０-９
            || code == 0x002C || code == 0xFF0C || code == 0x3001 // commas
            || code == 0x002E || code == 0xFF0E || code == 0x3002 // periods
    })
}

/// Look up candidates for `input_text`, deinflecting as needed.
///
/// `existing_entries` prevents duplicates across iterations.
/// `input_length` is the value to store in `WordResult::match_len`.
/// `original_search_text` is used for `matching_text` (sets `match_range` on readings).
fn lookup_candidates(
    input_text: &str,
    dictionary: &dyn Dictionary,
    existing_entries: &HashSet<i64>,
    max_results: usize,
    input_length: usize,
    original_search_text: Option<&str>,
) -> Vec<WordResult> {
    let matching_text = original_search_text.unwrap_or(input_text);
    let mut candidate_results: Vec<WordResult> = Vec::new();

    let candidates = deinflect(input_text);

    for (candidate_index, candidate) in candidates.iter().enumerate() {
        if !dictionary.exists(&candidate.word) {
            continue;
        }

        let lookup_max = (max_results * 3).max(20);
        let word_entries =
            match dictionary.get_words(&candidate.word, lookup_max, Some(matching_text)) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

        // Filter by word type for deinflected candidates (index > 0).
        let is_deinflection = candidate_index != 0;
        let word_entries: Vec<_> = if is_deinflection {
            word_entries
                .into_iter()
                .filter(|e| entry_matches_type(e, candidate.word_type))
                .collect()
        } else {
            word_entries
        };

        // Drop entries already found in a previous iteration.
        let word_entries: Vec<_> = word_entries
            .into_iter()
            .filter(|e| !existing_entries.contains(&e.entry_id))
            .collect();

        for entry in word_entries {
            candidate_results.push(WordResult {
                entry,
                match_len: input_length,
                reason_chains: if candidate.reason_chains.is_empty() {
                    None
                } else {
                    Some(candidate.reason_chains.clone())
                },
            });
        }
    }

    sort_word_results(&mut candidate_results);
    candidate_results.truncate(max_results);
    candidate_results
}

/// Search for words starting at the beginning of `input_text`.
///
/// `input_lengths` maps char positions in `input_text` to char positions in the
/// original (pre-normalization) text. Pass `None` to compute it internally.
pub fn word_search(
    input_text: &str,
    dictionary: &dyn Dictionary,
    max_results: usize,
    input_lengths: Option<&[usize]>,
) -> Option<WordSearchResult> {
    let (normalized, computed_lengths);
    let input_lengths: &[usize] = match input_lengths {
        Some(l) => l,
        None => {
            let (n, l) = normalize_input(input_text);
            normalized = n;
            computed_lengths = l;
            let _ = &normalized; // used below via current_input
            &computed_lengths
        }
    };

    let base_text = match input_lengths {
        _ if input_lengths.is_empty() => input_text,
        _ => input_text,
    };
    // When caller already normalized, input_text IS the normalized form.
    let _ = base_text;

    let mut current_input = input_text.to_string();
    let mut longest_match: usize = 0;
    let mut have: HashSet<i64> = HashSet::new();
    let mut results: Vec<WordResult> = Vec::new();
    let mut include_variants = true;

    while !current_input.is_empty() {
        if is_only_digits(&current_input) {
            break;
        }

        // Build variation list.
        let mut variations = vec![current_input.clone()];
        if include_variants {
            variations.extend(expand_choon(&current_input));
            let new_form = kyuujitai_to_shinjitai(&current_input);
            if new_form != current_input {
                variations.push(new_form);
            }
        }

        // `current_input_length` = value from input_lengths for this char count.
        let char_count = current_input.chars().count();
        let current_input_length = if char_count < input_lengths.len() {
            input_lengths[char_count]
        } else {
            *input_lengths.last().unwrap_or(&char_count)
        };

        let mut found_match = false;
        for variant in &variations {
            let word_results = lookup_candidates(
                variant,
                dictionary,
                &have,
                max_results,
                current_input_length,
                Some(&current_input),
            );

            if word_results.is_empty() {
                continue;
            }

            found_match = true;
            have.extend(word_results.iter().map(|r| r.entry.entry_id));
            longest_match = longest_match.max(current_input_length);
            results.extend(word_results);
            current_input = variant.clone();
            include_variants = false;
            break;
        }

        let _ = found_match;

        if results.len() >= max_results * 10 {
            break;
        }

        // Shorten by 2 chars for yoon endings, otherwise 1.
        let shorten = if ends_in_yoon(&current_input) { 2 } else { 1 };
        let chars: Vec<char> = current_input.chars().collect();
        if chars.len() <= shorten {
            break;
        }
        current_input = chars[..chars.len() - shorten].iter().collect();
    }

    if results.is_empty() {
        return None;
    }

    sort_word_results(&mut results);
    let more = results.len() >= max_results;
    results.truncate(max_results);

    Some(WordSearchResult {
        data: results,
        match_len: longest_match,
        more,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_database::build_database_from_xml;
    use crate::sqlite_dict::SqliteDictionary;
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    const MINI_JMDICT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<JMdict>
<entry>
<ent_seq>1549240</ent_seq>
<r_ele><reb>たべる</reb><re_pri>ichi1</re_pri></r_ele>
<k_ele><keb>食べる</keb><ke_pri>ichi1</ke_pri></k_ele>
<sense><pos>v1</pos><gloss>to eat</gloss></sense>
</entry>
<entry>
<ent_seq>1166770</ent_seq>
<r_ele><reb>よむ</reb></r_ele>
<k_ele><keb>読む</keb></k_ele>
<sense><pos>v5m</pos><gloss>to read</gloss></sense>
</entry>
<entry>
<ent_seq>1002830</ent_seq>
<r_ele><reb>たかい</reb></r_ele>
<k_ele><keb>高い</keb></k_ele>
<sense><pos>adj-i</pos><gloss>high; tall</gloss></sense>
</entry>
</JMdict>"#;

    fn make_dict() -> SqliteDictionary {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_owned();
        std::mem::forget(tmp);
        let conn = Connection::open(&path).unwrap();
        build_database_from_xml(&conn, MINI_JMDICT).unwrap();
        SqliteDictionary::open(&path).unwrap()
    }

    #[test]
    fn test_is_only_digits() {
        assert!(is_only_digits("123"));
        assert!(is_only_digits("１２３"));
        assert!(is_only_digits("1,2.3"));
        assert!(!is_only_digits("123a"));
        assert!(!is_only_digits(""));
        assert!(!is_only_digits("食べ"));
    }

    #[test]
    fn test_word_search_plain() {
        let d = make_dict();
        let (normalized, lengths) = normalize_input("食べる");
        let result = word_search(&normalized, &d, 5, Some(&lengths));
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(!r.data.is_empty());
        assert_eq!(r.data[0].entry.ent_seq, "1549240");
    }

    #[test]
    fn test_word_search_deinflected() {
        let d = make_dict();
        // 食べた → 食べる (past)
        let (normalized, lengths) = normalize_input("食べた");
        let result = word_search(&normalized, &d, 5, Some(&lengths));
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(
            r.data.iter().any(|w| w.entry.ent_seq == "1549240"),
            "expected 食べる in results"
        );
    }

    #[test]
    fn test_word_search_adj_deinflected() {
        let d = make_dict();
        // 高かった → 高い (past adj)
        let (normalized, lengths) = normalize_input("高かった");
        let result = word_search(&normalized, &d, 5, Some(&lengths));
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.data.iter().any(|w| w.entry.ent_seq == "1002830"));
    }

    #[test]
    fn test_word_search_no_match() {
        let d = make_dict();
        let (normalized, lengths) = normalize_input("zzz");
        let result = word_search(&normalized, &d, 5, Some(&lengths));
        assert!(result.is_none());
    }
}
