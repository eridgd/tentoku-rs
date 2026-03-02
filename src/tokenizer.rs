use crate::dictionary::Dictionary;
use crate::normalize::normalize_input;
use crate::types::Token;
use crate::word_search::word_search;

/// Tokenize `text` into a sequence of tokens using the provided dictionary.
///
/// Each token covers a span of the original text (by char index). If a
/// dictionary match is found at a position, `dictionary_entry` and
/// `deinflection_reasons` are populated with the best result. Otherwise the
/// token is an "unknown" single-character span with no entry.
pub fn tokenize(text: &str, dictionary: &dyn Dictionary, max_results: usize) -> Vec<Token> {
    if text.is_empty() {
        return Vec::new();
    }

    let (normalized, full_lengths) = normalize_input(text);
    let orig_chars: Vec<char> = text.chars().collect();
    let norm_chars: Vec<char> = normalized.chars().collect();
    let norm_len = norm_chars.len();

    let mut tokens = Vec::new();
    let mut norm_pos: usize = 0;

    while norm_pos < norm_len {
        let orig_pos = full_lengths[norm_pos];

        // Build suffix string and per-position original-char lengths for this offset.
        let suffix: String = norm_chars[norm_pos..].iter().collect();
        let suffix_span = norm_len - norm_pos;
        let suffix_lengths: Vec<usize> = (0..=suffix_span)
            .map(|i| full_lengths[norm_pos + i] - full_lengths[norm_pos])
            .collect();

        match word_search(&suffix, dictionary, max_results, Some(&suffix_lengths)) {
            Some(result) => {
                let match_len_orig = result.match_len;

                // Find how many normalized chars were consumed by scanning forward
                // until full_lengths[norm_pos + k] >= orig_pos + match_len_orig.
                let target = orig_pos + match_len_orig;
                let mut k: usize = 1;
                while norm_pos + k < norm_len && full_lengths[norm_pos + k] < target {
                    k += 1;
                }

                let end = (orig_pos + match_len_orig).min(orig_chars.len());
                let token_text: String = orig_chars[orig_pos..end].iter().collect();

                let (dictionary_entry, deinflection_reasons) = match result.data.into_iter().next()
                {
                    Some(w) => (Some(w.entry), w.reason_chains),
                    None => (None, None),
                };

                tokens.push(Token {
                    text: token_text,
                    start: orig_pos,
                    end,
                    dictionary_entry,
                    deinflection_reasons,
                });

                norm_pos += k;
            }
            None => {
                // No match — emit the single original char at this position.
                let end = (orig_pos + 1).min(orig_chars.len());
                let token_text: String = orig_chars[orig_pos..end].iter().collect();
                tokens.push(Token {
                    text: token_text,
                    start: orig_pos,
                    end,
                    dictionary_entry: None,
                    deinflection_reasons: None,
                });
                norm_pos += 1;
            }
        }
    }

    tokens
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
    fn test_tokenize_empty() {
        let d = make_dict();
        assert!(tokenize("", &d, 5).is_empty());
    }

    #[test]
    fn test_tokenize_plain() {
        let d = make_dict();
        let tokens = tokenize("食べる", &d, 5);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text, "食べる");
        assert_eq!(tokens[0].start, 0);
        assert_eq!(tokens[0].end, 3);
        assert!(tokens[0].dictionary_entry.is_some());
        assert_eq!(
            tokens[0].dictionary_entry.as_ref().unwrap().ent_seq,
            "1549240"
        );
    }

    #[test]
    fn test_tokenize_unknown_chars() {
        let d = make_dict();
        let tokens = tokenize("abc", &d, 5);
        assert_eq!(tokens.len(), 3);
        for t in &tokens {
            assert!(t.dictionary_entry.is_none());
        }
        assert_eq!(tokens[0].text, "a");
        assert_eq!(tokens[1].text, "b");
        assert_eq!(tokens[2].text, "c");
    }

    #[test]
    fn test_tokenize_mixed() {
        let d = make_dict();
        let tokens = tokenize("食べるabc", &d, 5);
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].text, "食べる");
        assert!(tokens[0].dictionary_entry.is_some());
        assert_eq!(tokens[1].text, "a");
        assert!(tokens[1].dictionary_entry.is_none());
    }

    #[test]
    fn test_tokenize_deinflected() {
        let d = make_dict();
        let tokens = tokenize("食べた", &d, 5);
        assert!(!tokens.is_empty());
        assert!(tokens[0].dictionary_entry.is_some());
        assert_eq!(
            tokens[0].dictionary_entry.as_ref().unwrap().ent_seq,
            "1549240"
        );
        assert!(tokens[0].deinflection_reasons.is_some());
    }

    #[test]
    fn test_tokenize_spans_are_contiguous() {
        let d = make_dict();
        let text = "食べるabc";
        let tokens = tokenize(text, &d, 5);
        let chars: Vec<char> = text.chars().collect();
        for t in &tokens {
            let reconstructed: String = chars[t.start..t.end].iter().collect();
            assert_eq!(reconstructed, t.text);
        }
    }
}
