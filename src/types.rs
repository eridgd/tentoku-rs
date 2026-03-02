use serde::{Deserialize, Serialize};

/// Word type flags for deinflection matching.
/// Stored as a u16 bitmask — values can be OR'd together.
pub mod word_type {
    pub const ICHIDAN_VERB: u16 = 1 << 0; // ru-verbs
    pub const GODAN_VERB: u16 = 1 << 1; // u-verbs
    pub const I_ADJ: u16 = 1 << 2;
    pub const KURU_VERB: u16 = 1 << 3;
    pub const SURU_VERB: u16 = 1 << 4;
    pub const SPECIAL_SURU_VERB: u16 = 1 << 5;
    pub const NOUN_VS: u16 = 1 << 6;

    /// All final word types (valid dictionary entries)
    pub const ALL_FINAL: u16 =
        ICHIDAN_VERB | GODAN_VERB | I_ADJ | KURU_VERB | SURU_VERB | SPECIAL_SURU_VERB | NOUN_VS;

    // Intermediate types (not valid dictionary entries)
    pub const INITIAL: u16 = 1 << 7;
    pub const TA_TE_STEM: u16 = 1 << 8;
    pub const DA_DE_STEM: u16 = 1 << 9;
    pub const MASU_STEM: u16 = 1 << 10;
    pub const IRREALIS_STEM: u16 = 1 << 11;
}

/// Reasons for deinflection transformations.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Reason {
    PolitePastNegative = 0,
    PoliteNegative = 1,
    PoliteVolitional = 2,
    Chau = 3,
    Sugiru = 4,
    PolitePast = 5,
    Tara = 6,
    Tari = 7,
    Causative = 8,
    PotentialOrPassive = 9,
    Toku = 10,
    Sou = 11,
    Tai = 12,
    Polite = 13,
    Respectful = 14,
    Humble = 15,
    HumbleOrKansaiDialect = 16,
    Past = 17,
    Negative = 18,
    Passive = 19,
    Ba = 20,
    Volitional = 21,
    Potential = 22,
    EruUru = 23,
    CausativePassive = 24,
    Te = 25,
    Zu = 26,
    Imperative = 27,
    MasuStem = 28,
    Adv = 29,
    Noun = 30,
    ImperativeNegative = 31,
    Continuous = 32,
    Ki = 33,
    SuruNoun = 34,
    ZaruWoEnai = 35,
    NegativeTe = 36,
    Irregular = 37,
}

/// A candidate word from deinflection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateWord {
    pub word: String,
    pub word_type: u16,
    pub reason_chains: Vec<Vec<Reason>>,
}

/// Kanji reading (written form) of a dictionary entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KanjiReading {
    pub text: String,
    pub priority: Option<String>,
    pub info: Option<String>,
    /// `Some((0, len))` if this reading matched the input, `None` otherwise.
    pub match_range: Option<(usize, usize)>,
    pub matched: bool,
}

/// Kana reading (pronunciation) of a dictionary entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KanaReading {
    pub text: String,
    pub no_kanji: bool,
    pub priority: Option<String>,
    pub info: Option<String>,
    /// `Some((0, len))` if this reading matched the input, `None` otherwise.
    pub match_range: Option<(usize, usize)>,
    pub matched: bool,
}

/// Definition/gloss for a sense.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gloss {
    pub text: String,
    pub lang: String,
    pub g_type: Option<String>,
}

/// A sense (meaning) of a dictionary entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sense {
    pub index: usize,
    pub pos_tags: Vec<String>,
    pub glosses: Vec<Gloss>,
    pub info: Option<String>,
    pub field: Option<Vec<String>>,
    pub misc: Option<Vec<String>>,
    pub dial: Option<Vec<String>>,
}

/// A dictionary word entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WordEntry {
    pub entry_id: i64,
    pub ent_seq: String,
    pub kanji_readings: Vec<KanjiReading>,
    pub kana_readings: Vec<KanaReading>,
    pub senses: Vec<Sense>,
}

/// Result from word search.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WordResult {
    pub entry: WordEntry,
    pub match_len: usize,
    pub reason_chains: Option<Vec<Vec<Reason>>>,
}

/// A token from tokenization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Token {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub dictionary_entry: Option<WordEntry>,
    pub deinflection_reasons: Option<Vec<Vec<Reason>>>,
}
