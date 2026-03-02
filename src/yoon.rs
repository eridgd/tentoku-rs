/// Yoon start characters: きしちにひみりぎじびぴ
const YOON_START: &[u32] = &[
    0x304d, 0x3057, 0x3061, 0x306b, 0x3072, 0x307f, 0x308a, 0x304e, 0x3058, 0x3073, 0x3074,
];

/// Small y characters: ゃゅょ
const SMALL_Y: &[u32] = &[0x3083, 0x3085, 0x3087];

/// Returns `true` if `text` ends in a yoon (拗音), e.g. きゃ, しゅ, ちょ.
pub fn ends_in_yoon(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 2 {
        return false;
    }
    let last = chars[chars.len() - 1] as u32;
    let second_last = chars[chars.len() - 2] as u32;
    SMALL_Y.contains(&last) && YOON_START.contains(&second_last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ends_in_yoon_true() {
        assert!(ends_in_yoon("きゃ"));
        assert!(ends_in_yoon("しゅ"));
        assert!(ends_in_yoon("ちょ"));
        assert!(ends_in_yoon("abcきゃ"));
    }

    #[test]
    fn test_ends_in_yoon_false() {
        assert!(!ends_in_yoon("き"));
        assert!(!ends_in_yoon(""));
        assert!(!ends_in_yoon("a"));
        assert!(!ends_in_yoon("かな"));
    }
}
