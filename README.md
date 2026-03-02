<div align="center">
  <img src="https://raw.githubusercontent.com/eridgd/tentoku/main/images/tentoku_icon.svg" alt="Tentoku Logo" width="128">
</div>

# tentoku-rs (天読) — Japanese Tokenizer in Rust

**A dictionary-driven Japanese tokenizer with built-in deinflection, ported to Rust.**

[![Crates.io](https://img.shields.io/crates/v/tentoku)](https://crates.io/crates/tentoku)
[![docs.rs](https://img.shields.io/docsrs/tentoku)](https://docs.rs/tentoku)
[![CI](https://github.com/eridgd/tentoku-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/eridgd/tentoku-rs/actions)
[![MSRV](https://img.shields.io/badge/rustc-1.70+-blue)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

[日本語版 README](README.ja.md)

tentoku-rs is a Rust port of [tentoku](https://github.com/eridgd/tentoku), which is itself a port of the high-accuracy tokenization engine used in [10ten Japanese Reader](https://github.com/birchill/10ten-ja-reader).

Unlike statistical segmenters (such as MeCab or Sudachi), tentoku-rs uses a greedy longest-match algorithm paired with a rule-based system that resolves conjugated words back to their dictionary forms. It prioritizes lookup accuracy over speed, making it well suited for reading aids, dictionary tools, and annotation workflows. 

## Features

- **Greedy longest-match tokenization**: Finds the longest possible words in text
- **Deinflection support**: 241 conjugation rules to resolve verbs and adjectives back to dictionary forms
- **Tense and form detection**: Identifies verb forms like "polite past", "continuous", "negative", etc.
- **Automatic database setup**: CLI downloads and builds the JMDict database
- **Dictionary lookup**: Uses JMDict SQLite database for word lookups
- **Text variations**: Handles choon (ー) expansion and kyuujitai (旧字体) → shinjitai (新字体) conversion
- **Type validation**: Validates deinflected forms against part-of-speech tags
- **C FFI / JNI layer**: `cdylib` and `staticlib` outputs with a C-compatible API
- **Cross-validated against the Python reference tentoku for identical token boundaries**

## Installation

### Prerequisites

- Rust 1.70+ (uses `std::sync::OnceLock`)
- Network is only required when building the JMDict database

### Quickstart

```bash
cargo install tentoku
tentoku tokenize "私は学生です"
```

First-run behavior:
- `tentoku tokenize ...` and `tentoku lookup ...` will auto-build the DB if it is missing.
- Use `--no-build-db` to disable auto-build and fail fast.
- `tentoku build-db` is still useful when you want to prebuild once up front.

### Build from source

To build without installing on your PATH:

```bash
git clone https://github.com/eridgd/tentoku-rs.git
cd tentoku-rs
cargo build --release
```

This produces `target/release/tentoku` (CLI), `target/release/libtentoku.so`/`.dylib`/`.dll` (FFI), and `target/release/libtentoku.a` (static). Run the CLI as `./target/release/tentoku` (or `target\release\tentoku.exe` on Windows).

## CLI Usage

The examples below assume `tentoku` is on your PATH. Otherwise use `./target/release/tentoku` (or `target\release\tentoku.exe` on Windows).

### Build the dictionary database

Downloads `JMdict_e.gz` from EDRDG and builds a local SQLite database:

```bash
tentoku build-db
# or specify a custom path:
tentoku build-db --output /path/to/jmdict.db
```

The default database location is platform-specific:
- **macOS**: `~/Library/Application Support/tentoku/jmdict.db`
- **Linux**: `~/.local/share/tentoku/jmdict.db` (respects `$XDG_DATA_HOME`)
- **Windows**: `%APPDATA%\tentoku\jmdict.db`

You can also override with the `TENTOKU_DB` environment variable.

### Tokenize text

```bash
tentoku tokenize "私は学生です"
tentoku tokenize --max 10 "食べました"
tentoku tokenize --db /path/to/jmdict.db "高かった"
```

`--max` limits how many dictionary results are kept per token position (default 5). Output is a JSON array of `Token` objects (see [Data Types](#data-types)).

### Look up a word

```bash
tentoku lookup "食べる"
tentoku lookup --max 5 "たかい"
```

`--max` limits how many results are returned (default 10). Output is a JSON array of `WordResult` objects including all senses, readings, and deinflection chains.

## Library Usage

Add to `Cargo.toml`:

```toml
[dependencies]
tentoku = { path = "/path/to/tentoku-rs" }
```

### Basic tokenization

```rust
use std::error::Error;

use tentoku::database_path::find_database_path;
use tentoku::sqlite_dict::SqliteDictionary;
use tentoku::tokenizer::tokenize;

fn main() -> Result<(), Box<dyn Error>> {
    let db_path = find_database_path()
        .ok_or("No database found. Run `tentoku build-db` first.")?;
    let dict = SqliteDictionary::open(&db_path)?;

    let tokens = tokenize("私は学生です", &dict, 5);
    for token in &tokens {
        println!("{} ({}-{})", token.text, token.start, token.end);
        if let Some(entry) = &token.dictionary_entry {
            let meaning = entry.senses.first()
                .and_then(|s| s.glosses.first())
                .map(|g| g.text.as_str())
                .unwrap_or("?");
            println!("  {}", meaning);
        }
    }

    Ok(())
}
// 私 (0-1)
//   I; me
// は (1-2)
//   ...
// 学生 (2-4)
//   student (esp. a university student)
// です (4-6)
//   be; is
```

This example uses `?`, so it must be inside a function returning `Result` (e.g. `Result<(), Box<dyn Error>>`).

### Deinflection

```rust
use tentoku::tokenizer::tokenize;

let tokens = tokenize("食べました", &dict, 5);
for token in &tokens {
    if let Some(chains) = &token.deinflection_reasons {
        for chain in chains {
            let reasons: Vec<_> = chain.iter().map(|r| format!("{:?}", r)).collect();
            println!("{} → {}", token.text, reasons.join(", "));
        }
    }
}
// 食べました → PolitePast
```

Available `Reason` variants include:
`PolitePast`, `Polite`, `Past`, `Negative`, `Continuous`, `Potential`, `Causative`,
`Passive`, `Tai`, `Volitional`, `Te`, `Zu`, `Imperative`, `MasuStem`, `Adv`, `Noun`,
`CausativePassive`, `EruUru`, `Sou`, `Tara`, `Tari`, `Ki`, `SuruNoun`, `ZaruWoEnai`,
`NegativeTe`, `Irregular`, and more (see `src/types.rs`).

### Word search (advanced)

```rust
use tentoku::normalize::normalize_input;
use tentoku::word_search::word_search;

let text = "食べています";
let (normalized, lengths) = normalize_input(text);
let result = word_search(&normalized, &dict, 7, Some(&lengths));

if let Some(r) = result {
    for word_result in &r.data {
        let reading = word_result.entry.kana_readings.first()
            .map(|r| r.text.as_str()).unwrap_or("?");
        println!("→ {} ({})", reading, word_result.entry.ent_seq);
        if let Some(chains) = &word_result.reason_chains {
            for chain in chains {
                let names: Vec<_> = chain.iter().map(|r| format!("{:?}", r)).collect();
                println!("  via: {}", names.join(" → "));
            }
        }
    }
}
// → たべる (1549240)
//   via: Continuous → Polite
```

### Building the database programmatically

```rust
use tentoku::build_database::build_database;

// Download from EDRDG and build:
build_database("/path/to/jmdict.db", None)?;

// Or supply raw gzip bytes (e.g. from an embedded asset):
let gz_bytes: Vec<u8> = std::fs::read("JMdict_e.gz")?;
build_database("/path/to/jmdict.db", Some(gz_bytes))?;
```

## C FFI / JNI

The crate builds as a `cdylib`, exposing a stable C API for use from JNI (Android), Swift, Kotlin/Native, etc.

```c
#include "tentoku.h"

// Open dictionary
TentokuHandle* h = tentoku_open("/path/to/jmdict.db");

// Tokenize — returns heap-allocated JSON, caller must free
char* json = tentoku_tokenize_json(h, "私は学生です", 5);
// ... use json ...
tentoku_free_string(json);

// Look up a word
char* result = tentoku_lookup_json(h, "食べる", 10);
tentoku_free_string(result);

// Close
tentoku_free(h);
```

Functions:
| Function | Description |
|---|---|
| `tentoku_open(path)` | Open a dictionary; returns handle or null |
| `tentoku_free(handle)` | Free a handle |
| `tentoku_tokenize_json(handle, text, max)` | Tokenize; returns JSON string |
| `tentoku_lookup_json(handle, word, max)` | Look up word; returns JSON string |
| `tentoku_free_string(s)` | Free a JSON string returned by the above |

All returned strings are UTF-8 JSON. Null is returned on error.

## Data Types

All types implement `Serialize`/`Deserialize` (serde).

### `Token`
```
text              — original text span
start, end        — char indices into the original input
dictionary_entry  — Option<WordEntry> — best dictionary match
deinflection_reasons — Option<Vec<Vec<Reason>>> — conjugation chain(s)
```

### `WordEntry`
```
ent_seq           — JMDict sequence number
kanji_readings    — Vec<KanjiReading>
kana_readings     — Vec<KanaReading>
senses            — Vec<Sense>
```

### `Sense`
```
index             — sense index within the entry
pos_tags          — Vec<String> — part-of-speech codes (e.g. "v1", "adj-i")
glosses           — Vec<Gloss> — definitions
field             — Option<Vec<String>> — domain (e.g. "comp", "med")
misc              — Option<Vec<String>> — usage notes (e.g. "uk", "pol")
dial              — Option<Vec<String>> — dialect
```

## Algorithm

1. **Normalize input** — convert half-width digits to full-width, apply Unicode NFC, strip ZWNJ, build char-index mapping
2. **Greedy longest-match** — start at position 0, search for the longest matching word
3. **Word search** — for each candidate substring:
   - Generate text variations (choon expansion, kyuujitai conversion)
   - Run BFS deinflection to get candidate dictionary forms
   - Look up candidates in the SQLite dictionary and validate POS types
   - Track the longest successful match
   - If no match, shorten input by 2 chars (yoon ending) or 1 char and retry
4. **Advance** — move forward by the match length, or 1 char if no match found

## Architecture

| Module | Description |
|---|---|
| `src/types.rs` | Core types: `Token`, `WordEntry`, `WordResult`, `WordType`, `Reason` |
| `src/normalize.rs` | Unicode NFC, full-width numbers, ZWNJ stripping, char-index mapping |
| `src/variations.rs` | Choon (ー) expansion, kyuujitai → shinjitai conversion |
| `src/yoon.rs` | Yoon (拗音) ending detection |
| `src/deinflect_rules.rs` | 241 static deinflection rules |
| `src/deinflect.rs` | BFS deinflection engine with Ichidan stem forwarding |
| `src/type_matching.rs` | POS-tag validation for deinflected candidates |
| `src/sorting.rs` | Priority-based result sorting (ichi1, nf## frequency bands, etc.) |
| `src/dictionary.rs` | `Dictionary` trait |
| `src/sqlite_dict.rs` | SQLite backend (WAL, batched queries, negative cache) |
| `src/database_path.rs` | Platform-specific database path resolution |
| `src/build_database.rs` | JMDict download, gzip decompress, XML parse, SQLite import |
| `src/word_search.rs` | Greedy backtracking word search |
| `src/tokenizer.rs` | Full sentence tokenizer |
| `src/ffi.rs` | C-compatible FFI layer |
| `src/bin/tentoku.rs` | CLI (`build-db`, `tokenize`, `lookup`) |

## Database

The tokenizer uses a JMDict SQLite database. Run `tentoku build-db` to create it. It downloads `JMdict_e.gz` (~10 MB) from the official EDRDG source, decompresses it, and imports the XML into SQLite (~105 MB result).

Database tables: `entries`, `kanji`, `readings`, `reading_restrictions`, `senses`, `sense_pos`, `sense_field`, `sense_misc`, `sense_dial`, `glosses`.

This is a one-time operation. Subsequent uses load the pre-built database instantly.

## Testing

```bash
cargo test
```

The test suite has 57 tests: 56 unit tests (normalization, deinflection, type matching, sorting, SQLite dictionary, word search, tokenization, key index) plus one integration test. Unit tests use an in-memory mini JMDict (no network). The integration test `tests/cross_validate.rs` runs representative sentences through both the Rust and Python tokenizers and asserts identical token boundaries. **Python reference is automated:** on first run (when `reference/` is missing), the test runs `scripts/setup_python_reference.sh`, which clones [tentoku](https://github.com/eridgd/tentoku), checks out the `cython-version` branch, and installs it; no manual clone or install is required. The test skips only if the JMDict database is missing or the setup script fails (e.g. no git/network).

## Benchmarking

Rust implementation significantly outperforms the Python reference in long-text tokenization benchmarks (see `cargo bench`).

```bash
cargo bench
```

Benchmarks (via [Criterion](https://github.com/bheisler/criterion.rs)) include:
- **Mini-dict (no real DB):** `tokenize/plain_verb`, `tokenize/deinflected_verb`, `tokenize/short_sentence`, `word_search/plain`, `normalize_input/hiragana`
- **Real JMDict:** `tokenize/medium_paragraph`, `tokenize/long_text`, `tokenize/kokoro_passage` (require `tentoku build-db` or `TENTOKU_DB`)
- **Rust vs Python:** group `vs_python` — `rust/medium_paragraph`, `python/medium_paragraph`; group `vs_python_long` — `rust/kokoro_passage`, `python/kokoro_passage` (3 samples). Require real DB and Python. **The Python reference is set up automatically:** when you run these benchmarks and `reference/` is missing, the bench harness runs `scripts/setup_python_reference.sh` to clone tentoku, checkout `cython-version`, and install it—no manual steps. Use `PYTHON_BIN` to override the Python binary.

Run only the comparison benchmarks: `cargo bench -- vs_python` or `cargo bench -- vs_python_long`. To prepare the reference manually (e.g. offline): `./scripts/setup_python_reference.sh` from the repo root.

## Credits

The tokenization logic, deinflection rules, and matching strategy are derived from the original TypeScript implementation in [10ten Japanese Reader](https://github.com/birchill/10ten-ja-reader). The Python port [tentoku](https://github.com/eridgd/tentoku) served as the immediate reference for this Rust port.

### Dictionary Data

This crate uses the [JMDict](https://www.edrdg.org/wiki/index.php/JMdict-EDICT_Dictionary_Project) dictionary data, which is the property of the [Electronic Dictionary Research and Development Group](https://www.edrdg.org/) (EDRDG). The dictionary data is licensed under the [Creative Commons Attribution-ShareAlike 4.0 International License](https://creativecommons.org/licenses/by-sa/4.0/) (CC BY-SA 4.0).

Copyright is held by James William BREEN and The Electronic Dictionary Research and Development Group.

The JMDict data is downloaded from the official EDRDG source at runtime. The data is **not** bundled in this repository. For more information:
- **JMDict Project**: https://www.edrdg.org/wiki/index.php/JMdict-EDICT_Dictionary_Project
- **EDRDG License Statement**: https://www.edrdg.org/edrdg/licence.html

See [JMDICT_ATTRIBUTION.md](JMDICT_ATTRIBUTION.md) for complete attribution details.

## License

This project is licensed under the GNU General Public License v3.0 or later (GPL-3.0-or-later).

See the [LICENSE](LICENSE) file for the full license text.

**Note on dictionary data**: While this software is GPL-3.0-or-later, the JMDict dictionary data is separately licensed under CC BY-SA 4.0. When distributing this software together with the database, both licenses apply to their respective components.
