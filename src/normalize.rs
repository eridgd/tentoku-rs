const ZWNJ: char = '\u{200C}';

/// Convert katakana to hiragana.
pub fn kana_to_hiragana(text: &str) -> String {
    text.chars()
        .map(|c| {
            let code = c as u32;
            if (0x30A1..=0x30F6).contains(&code) {
                // Regular katakana → hiragana: subtract 0x60
                char::from_u32(code - 0x60).unwrap_or(c)
            } else {
                match c {
                    '\u{30F7}' => 'わ', // ヷ
                    '\u{30F8}' => 'ゐ', // ヸ
                    '\u{30F9}' => 'ゑ', // ヹ
                    '\u{30FA}' => 'を', // ヺ
                    _ => c,
                }
            }
        })
        .collect()
}

/// Convert half-width digits (0-9) to full-width (０-９).
pub fn half_to_full_width_num(text: &str) -> String {
    text.chars()
        .map(|c| {
            let code = c as u32;
            if (0x0030..=0x0039).contains(&code) {
                char::from_u32(code - 0x0030 + 0xFF10).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

/// Normalize input text and return `(normalized_text, input_lengths)`.
///
/// `input_lengths[i]` gives the character index in the *original* text corresponding
/// to position `i` in the normalized text. The array has `char_count + 1` entries.
///
/// Steps:
/// 1. Convert half-width digits to full-width
/// 2. Apply Unicode NFC normalization
/// 3. Strip ZWNJ (U+200C) characters
/// 4. Build the char-index mapping array
pub fn normalize_input(text: &str) -> (String, Vec<usize>) {
    if text.is_empty() {
        return (String::new(), vec![0]);
    }

    // Step 1: full-width numbers
    let after_num = half_to_full_width_num(text);

    // Step 2: NFC normalization
    let nfc: String = after_num.nfc().collect();

    // Step 3 & 4: strip ZWNJ and build input_lengths simultaneously.
    // We iterate over NFC chars; original_pos tracks the char index in `after_num`
    // (which has the same char count as `text` since half_to_full_width_num is 1:1).
    let mut result_chars = Vec::with_capacity(nfc.chars().count());
    let mut input_lengths = Vec::with_capacity(nfc.chars().count() + 1);
    let mut original_pos = 0usize;

    // NFC may fuse/split chars relative to the pre-NFC string. We track positions
    // through the NFC string by mapping each NFC char back to a position in
    // `after_num` using pre-counted codepoints. Since we only do number widening
    // (1:1 char mapping) before NFC, and NFC only composes chars (reducing count
    // or keeping it equal), we advance original_pos by 1 for each *pre-NFC* char
    // that was consumed. A simple correct approach: build a parallel mapping by
    // iterating before and after NFC and aligning via Unicode normalization rounds.
    //
    // Practical approach: NFC composition only affects certain combining sequences.
    // For Japanese text (the primary use case), NFC is essentially a no-op.
    // We use the following strategy: walk both the pre-NFC and post-NFC strings
    // simultaneously. When a char in NFC matches a char in pre-NFC, advance both.
    // When NFC has composed multiple pre-NFC chars into one, advance pre-NFC by
    // the number of chars consumed.
    //
    // Simpler correct approach used here: since `half_to_full_width_num` is 1:1
    // and NFC only ever combines characters (never splits), we can decompose back
    // to NFD to count the pre-NFC chars consumed per NFC char.
    use unicode_normalization::UnicodeNormalization as _;
    let pre_nfc_chars: Vec<char> = after_num.chars().collect();
    let mut pre_nfc_idx = 0usize;

    for nfc_char in nfc.chars() {
        if nfc_char == ZWNJ {
            // Skip ZWNJ, advance original position
            original_pos += 1;
            pre_nfc_idx += 1;
            continue;
        }

        input_lengths.push(original_pos);
        result_chars.push(nfc_char);

        // How many pre-NFC chars were consumed to produce this NFC char?
        // Scan forward: try k = 1, 2, 3, … until NFC(pre_nfc_chars[i..i+k]) is exactly
        // nfc_char. This handles both precomposed input (k=1, e.g. U+3079 べ) and
        // decomposed input (k=2, e.g. U+3078 + U+3099 → U+3079).
        let remaining = pre_nfc_chars.len() - pre_nfc_idx;
        let consumed = {
            let mut k = 1usize;
            loop {
                if k > remaining {
                    k = remaining.max(1);
                    break;
                }
                let seg: String = pre_nfc_chars[pre_nfc_idx..pre_nfc_idx + k].iter().collect();
                let seg_nfc: String = seg.chars().nfc().collect();
                let mut sc = seg_nfc.chars();
                let first = sc.next();
                let second = sc.next();
                if first == Some(nfc_char) && second.is_none() {
                    break; // k pre-NFC chars compose to exactly this NFC char
                }
                if k >= 4 {
                    k = 1;
                    break;
                } // safety cap; fall back to 1
                k += 1;
            }
            k
        };
        pre_nfc_idx += consumed;
        original_pos += consumed;
    }

    // Push final sentinel
    input_lengths.push(original_pos);

    let normalized: String = result_chars.into_iter().collect();

    if input_lengths.is_empty() {
        input_lengths.push(0);
    }

    (normalized, input_lengths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kana_to_hiragana_katakana() {
        assert_eq!(kana_to_hiragana("カタカナ"), "かたかな");
    }

    #[test]
    fn test_kana_to_hiragana_mixed() {
        assert_eq!(kana_to_hiragana("カタカナとひらがな"), "かたかなとひらがな");
    }

    #[test]
    fn test_kana_to_hiragana_special() {
        assert_eq!(kana_to_hiragana("\u{30F7}"), "わ");
        assert_eq!(kana_to_hiragana("\u{30F8}"), "ゐ");
        assert_eq!(kana_to_hiragana("\u{30F9}"), "ゑ");
        assert_eq!(kana_to_hiragana("\u{30FA}"), "を");
    }

    #[test]
    fn test_kana_to_hiragana_passthrough() {
        assert_eq!(kana_to_hiragana("hello"), "hello");
        assert_eq!(kana_to_hiragana("日本語"), "日本語");
    }

    #[test]
    fn test_half_to_full_width_num() {
        assert_eq!(half_to_full_width_num("123"), "１２３");
        assert_eq!(half_to_full_width_num("abc"), "abc");
        assert_eq!(half_to_full_width_num("1a2b"), "１a２b");
    }

    #[test]
    fn test_normalize_input_empty() {
        let (text, lengths) = normalize_input("");
        assert_eq!(text, "");
        assert_eq!(lengths, vec![0]);
    }

    #[test]
    fn test_normalize_input_simple() {
        let (text, lengths) = normalize_input("こんにちは");
        assert_eq!(text, "こんにちは");
        // lengths should have 6 elements (5 chars + sentinel)
        assert_eq!(lengths.len(), 6);
        assert_eq!(lengths[0], 0);
        assert_eq!(lengths[5], 5);
    }

    #[test]
    fn test_normalize_input_zwnj() {
        // ZWNJ between chars should be stripped
        let input = "こ\u{200C}に";
        let (text, lengths) = normalize_input(input);
        assert_eq!(text, "こに");
        // 2 chars in result → 3 entries in lengths
        assert_eq!(lengths.len(), 3);
    }

    #[test]
    fn test_normalize_input_numbers() {
        let (text, _lengths) = normalize_input("123");
        assert_eq!(text, "１２３");
    }
}
