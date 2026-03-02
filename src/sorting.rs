use std::collections::HashMap;

use crate::types::{WordEntry, WordResult};

/// Priority score assignments for known JMDict priority codes.
fn priority_assignments() -> &'static HashMap<&'static str, i32> {
    use std::sync::OnceLock;
    static MAP: OnceLock<HashMap<&'static str, i32>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("i1", 50);
        m.insert("i2", 20);
        m.insert("n1", 40);
        m.insert("n2", 20);
        m.insert("s1", 32);
        m.insert("s2", 20);
        m.insert("g1", 30);
        m.insert("g2", 15);
        m
    })
}

/// Normalize full JMDict priority names to short codes.
fn normalize_priority(priority: &str) -> &str {
    match priority {
        "ichi1" => "i1",
        "ichi2" => "i2",
        "news1" => "n1",
        "news2" => "n2",
        "spec1" => "s1",
        "spec2" => "s2",
        "gai1" => "g1",
        "gai2" => "g2",
        other => other,
    }
}

/// Get priority score for a single priority string.
fn get_priority_score(priority: &str) -> f64 {
    let normalized = normalize_priority(priority);

    if let Some(&score) = priority_assignments().get(normalized) {
        return score as f64;
    }

    if normalized.starts_with("nf") {
        if let Ok(wordfreq) = normalized[2..].parse::<i32>() {
            if wordfreq > 0 && wordfreq < 48 {
                return (48 - wordfreq / 2) as f64;
            }
        }
    }

    0.0
}

/// Combine multiple priority scores: highest + 10% of second + 1% of third, etc.
fn get_priority_sum(priorities: &[&str]) -> f64 {
    if priorities.is_empty() {
        return 0.0;
    }

    let mut scores: Vec<f64> = priorities.iter().map(|&p| get_priority_score(p)).collect();
    scores.sort_by(|a, b| b.partial_cmp(a).unwrap());

    let mut result = scores[0];
    for (i, &score) in scores[1..].iter().enumerate() {
        result += score / 10f64.powi((i + 1) as i32);
    }
    result
}

/// Get the priority score for an entry (based on matched readings only).
pub fn get_priority(entry: &WordEntry) -> f64 {
    let mut scores = vec![0.0f64];

    for kanji in &entry.kanji_readings {
        if kanji.match_range.is_some() {
            if let Some(ref p) = kanji.priority {
                let parts: Vec<&str> = p.split(',').map(str::trim).collect();
                if !parts.is_empty() {
                    scores.push(get_priority_sum(&parts));
                }
            }
        }
    }

    for kana in &entry.kana_readings {
        if kana.match_range.is_some() {
            if let Some(ref p) = kana.priority {
                let parts: Vec<&str> = p.split(',').map(str::trim).collect();
                if !parts.is_empty() {
                    scores.push(get_priority_sum(&parts));
                }
            }
        }
    }

    scores.into_iter().fold(0.0f64, f64::max)
}

/// Returns 1 if this entry's matched reading is a primary headword, 2 if it's
/// just a reading for a kanji headword.
pub fn get_kana_headword_type(entry: &WordEntry) -> u8 {
    // Find the kana reading that matched.
    let matching_kana = entry.kana_readings.iter().find(|k| k.match_range.is_some());

    let Some(kana) = matching_kana else {
        return 1; // Matched on kanji or no match
    };

    // Obscure reading marker → type 2
    if let Some(ref info) = kana.info {
        let obscure = info
            .split(',')
            .any(|p| matches!(p.trim(), "ok" | "rk" | "sk" | "ik"));
        if obscure {
            return 2;
        }
    }

    // No kanji headwords → type 1
    if entry.kanji_readings.is_empty() {
        return 1;
    }

    // All kanji headwords are obscure → type 1
    let all_kanji_obscure = entry.kanji_readings.iter().all(|k| {
        k.info.as_deref().map_or(false, |info| {
            info.split(',')
                .any(|p| matches!(p.trim(), "rK" | "sK" | "iK"))
        })
    });
    if all_kanji_obscure {
        return 1;
    }

    // ≥50% of English senses have 'uk' misc → type 1
    let matched_en: Vec<_> = entry
        .senses
        .iter()
        .filter(|s| {
            s.glosses.is_empty()
                || s.glosses
                    .iter()
                    .any(|g| matches!(g.lang.as_str(), "eng" | "en"))
        })
        .collect();
    if !matched_en.is_empty() {
        let uk_count = matched_en
            .iter()
            .filter(|s| {
                s.misc
                    .as_deref()
                    .map_or(false, |misc| misc.iter().any(|m| m.contains("uk")))
            })
            .count();
        if uk_count * 2 >= matched_en.len() {
            return 1;
        }
    }

    // no_kanji flag → type 1
    if kana.no_kanji {
        return 1;
    }

    2
}

/// Sort word results: longer match → fewer deinflect steps → headword type 1 → higher priority.
pub fn sort_word_results(results: &mut Vec<WordResult>) {
    results.sort_by(|a, b| {
        // 1. Longer match is better (negate for ascending sort)
        let cmp = b.match_len.cmp(&a.match_len);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }

        // 2. Fewer deinflection steps is better
        let a_reasons = a.reason_chains.as_deref().map_or(0, |chains| {
            chains.iter().map(|c| c.len()).max().unwrap_or(0)
        });
        let b_reasons = b.reason_chains.as_deref().map_or(0, |chains| {
            chains.iter().map(|c| c.len()).max().unwrap_or(0)
        });
        let cmp = a_reasons.cmp(&b_reasons);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }

        // 3. Headword type 1 before type 2
        let a_type = get_kana_headword_type(&a.entry);
        let b_type = get_kana_headword_type(&b.entry);
        let cmp = a_type.cmp(&b_type);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }

        // 4. Higher priority is better (negate)
        let a_prio = get_priority(&a.entry);
        let b_prio = get_priority(&b.entry);
        b_prio
            .partial_cmp(&a_prio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
