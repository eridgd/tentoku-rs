use crate::types::{word_type::*, WordEntry};

/// Tests if a dictionary entry's POS tags match the expected word type from deinflection.
pub fn entry_matches_type(entry: &WordEntry, word_type: u16) -> bool {
    let all_pos: Vec<&str> = entry
        .senses
        .iter()
        .flat_map(|s| s.pos_tags.iter().map(|t| t.as_str()))
        .collect();

    if all_pos.is_empty() {
        return false;
    }

    let has = |f: &dyn Fn(&str) -> bool| all_pos.iter().any(|&p| f(p));

    // Expressions match any verb/adj type.
    if has(&|p| p == "exp" || p.to_ascii_lowercase().contains("expression")) {
        let verb_adj =
            ICHIDAN_VERB | GODAN_VERB | I_ADJ | KURU_VERB | SURU_VERB | SPECIAL_SURU_VERB;
        if word_type & verb_adj != 0 {
            return true;
        }
    }

    if word_type & ICHIDAN_VERB != 0
        && has(&|p| p.starts_with("v1") || p.contains("Ichidan verb") || p == "v1")
    {
        return true;
    }

    if word_type & GODAN_VERB != 0
        && has(&|p| p.starts_with("v5") || p.starts_with("v4") || p.contains("Godan verb"))
    {
        return true;
    }

    if word_type & I_ADJ != 0
        && has(&|p| p.starts_with("adj-i") || p.to_ascii_lowercase().contains("adjective"))
    {
        return true;
    }

    if word_type & KURU_VERB != 0
        && has(&|p| p == "vk" || p.to_ascii_lowercase().contains("kuru verb"))
    {
        return true;
    }

    if word_type & SURU_VERB != 0
        && has(&|p| p == "vs-i" || p == "vs-s" || p.to_ascii_lowercase().contains("suru verb"))
    {
        return true;
    }

    if word_type & SPECIAL_SURU_VERB != 0
        && has(&|p| p == "vs-s" || p == "vz" || p.to_ascii_lowercase().contains("suru verb"))
    {
        return true;
    }

    if word_type & NOUN_VS != 0
        && has(&|p| {
            p == "vs"
                || (p.to_ascii_lowercase().contains("noun or participle")
                    && p.to_ascii_lowercase().contains("suru"))
        })
    {
        return true;
    }

    false
}
