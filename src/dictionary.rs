use crate::error::Result;
use crate::types::WordEntry;

/// Trait for dictionary backends.
pub trait Dictionary: Send + Sync {
    /// Look up words matching `input_text` exactly (or via hiragana normalization).
    fn get_words(
        &self,
        input_text: &str,
        max_results: usize,
        matching_text: Option<&str>,
    ) -> Result<Vec<WordEntry>>;

    /// Fast existence check — returns `true` if any entry matches `word`.
    fn exists(&self, word: &str) -> bool;
}
