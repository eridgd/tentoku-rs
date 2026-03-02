use std::collections::HashMap;
use std::sync::OnceLock;

const CHOON: char = 'ー';

/// Expand the first ー in `text` to its 5 possible vowel replacements.
/// Returns an empty vec if there is no ー.
pub fn expand_choon(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let Some(pos) = chars.iter().position(|&c| c == CHOON) else {
        return vec![];
    };

    let vowels = ['あ', 'い', 'う', 'え', 'お'];
    vowels
        .iter()
        .map(|&v| {
            let mut variant = chars.clone();
            variant[pos] = v;
            variant.into_iter().collect()
        })
        .collect()
}

static KYUU_MAP: OnceLock<HashMap<char, char>> = OnceLock::new();

fn kyuu_map() -> &'static HashMap<char, char> {
    KYUU_MAP.get_or_init(|| {
        let mut m = HashMap::new();
        let pairs = [
            ('舊', '旧'),
            ('體', '体'),
            ('國', '国'),
            ('學', '学'),
            ('會', '会'),
            ('實', '実'),
            ('寫', '写'),
            ('讀', '読'),
            ('賣', '売'),
            ('來', '来'),
            ('歸', '帰'),
            ('變', '変'),
            ('傳', '伝'),
            ('轉', '転'),
            ('廣', '広'),
            ('應', '応'),
            ('當', '当'),
            ('擔', '担'),
            ('戰', '戦'),
            ('殘', '残'),
            ('歲', '歳'),
            ('圖', '図'),
            ('團', '団'),
            ('圓', '円'),
            ('壓', '圧'),
            ('圍', '囲'),
            ('醫', '医'),
            ('鹽', '塩'),
            ('處', '処'),
            ('廳', '庁'),
            ('與', '与'),
            ('餘', '余'),
            ('價', '価'),
            ('兒', '児'),
            ('產', '産'),
            ('縣', '県'),
            ('顯', '顕'),
            ('驗', '験'),
            ('險', '険'),
            ('獻', '献'),
            ('嚴', '厳'),
            ('靈', '霊'),
            ('齡', '齢'),
            ('勞', '労'),
            ('營', '営'),
            ('榮', '栄'),
            ('櫻', '桜'),
            ('驛', '駅'),
        ];
        for (old, new) in pairs {
            m.insert(old, new);
        }
        m
    })
}

/// Convert kyuujitai (旧字体) characters to shinjitai (新字体).
/// Returns the original string unchanged if no substitutions are needed.
pub fn kyuujitai_to_shinjitai(text: &str) -> String {
    let map = kyuu_map();
    let mut changed = false;
    let result: String = text
        .chars()
        .map(|c| {
            if let Some(&new) = map.get(&c) {
                changed = true;
                new
            } else {
                c
            }
        })
        .collect();
    if changed {
        result
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_choon_present() {
        let variants = expand_choon("カー");
        assert_eq!(variants.len(), 5);
        assert!(variants.contains(&"カあ".to_string()));
        assert!(variants.contains(&"カい".to_string()));
        assert!(variants.contains(&"カう".to_string()));
        assert!(variants.contains(&"カえ".to_string()));
        assert!(variants.contains(&"カお".to_string()));
    }

    #[test]
    fn test_expand_choon_absent() {
        assert!(expand_choon("カタカナ").is_empty());
        assert!(expand_choon("").is_empty());
    }

    #[test]
    fn test_expand_choon_first_only() {
        // Only the first ー is expanded
        let variants = expand_choon("カーター");
        assert_eq!(variants.len(), 5);
        // First ー replaced, second remains
        assert!(variants.iter().all(|v| v.contains('ー')));
    }

    #[test]
    fn test_kyuujitai_to_shinjitai() {
        assert_eq!(kyuujitai_to_shinjitai("體"), "体");
        assert_eq!(kyuujitai_to_shinjitai("國語"), "国語");
    }

    #[test]
    fn test_kyuujitai_no_change() {
        let s = "日本語";
        assert_eq!(kyuujitai_to_shinjitai(s), s);
    }
}
