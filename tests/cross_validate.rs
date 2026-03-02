/// Cross-validation: verify Rust and Python tokenizers produce identical
/// token boundaries on a set of representative Japanese sentences.
///
/// Skips gracefully if:
/// - No JMDict database is found (run `tentoku build-db` first)
/// - The Python reference tokenizer is unavailable or errors
///
/// On Unix, if `reference/` is missing, the integration test runs
/// `scripts/setup_python_reference.sh` to clone tentoku and checkout
/// cython-version so the user does not have to do it manually.
use std::path::{Path, PathBuf};
use std::process::Command;

use tentoku::database_path::find_database_path;
use tentoku::sqlite_dict::SqliteDictionary;
use tentoku::tokenizer::tokenize;

fn python_comparison_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("TENTOKU_PYTHON_DB") {
        return PathBuf::from(path);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("reference")
        .join("tentoku")
        .join("data")
        .join("jmdict.python.db")
}

fn shared_comparison_db_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("TENTOKU_COMPARE_DB") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    let python_db = python_comparison_db_path();
    if python_db.exists() {
        return Some(python_db);
    }

    find_database_path()
}

/// If reference/tentoku is missing, run the setup script to clone tentoku and checkout
/// cython-version. Returns true if reference is now available (or was already there).
#[cfg(unix)]
fn ensure_python_reference() -> bool {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let reference = Path::new(manifest).join("reference");
    let python_db = python_comparison_db_path();
    if reference.join("tentoku").exists() && python_db.exists() {
        return true;
    }
    let script = Path::new(manifest)
        .join("scripts")
        .join("setup_python_reference.sh");
    if !script.exists() {
        return false;
    }
    let ok = Command::new("sh")
        .arg(&script)
        .arg(manifest)
        .current_dir(manifest)
        .env("TENTOKU_PYTHON_DB", &python_db)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    ok && reference.join("tentoku").exists() && python_db.exists()
}

#[cfg(not(unix))]
fn ensure_python_reference() -> bool {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("reference")
        .join("tentoku")
        .exists()
        && python_comparison_db_path().exists()
}

#[derive(Debug, PartialEq)]
struct SimpleToken {
    text: String,
    start: usize,
    end: usize,
}

const SENTENCES: &[&str] = &[
    "私は学生です",
    "食べました",
    "食べています",
    "食べない",
    "食べた",
    "日本語を勉強しています",
    "高かった",
    "読みます",
    "見てください",
    "行かなかった",
    "東京へ行く",
    "本を読んでいます",
    "食べさせられた",
    "こんにちは",
    "走っている",
];

/// ~200-char paragraph; same as bench MEDIUM_PARAGRAPH for cross-validate coverage.
const MEDIUM_PARAGRAPH: &str = concat!(
    "私は毎日日本語を勉強しています。図書館で本を読んだり、音楽を聞いたりします。",
    "友達と話すことが好きです。先生に質問することができます。",
    "日本の文化について学んでいます。食べ物の中でお寿司が一番好きです。",
    "東京に行きたいと思っています。日本語がだんだん上手になってきました。",
    "毎朝六時に起きて、学校まで電車で通っています。",
);

fn python_tokenize(text: &str, python_db_path: &std::path::Path) -> Option<Vec<SimpleToken>> {
    let py_bin = std::env::var("PYTHON_BIN").unwrap_or_else(|_| "python3".into());
    let manifest = env!("CARGO_MANIFEST_DIR");
    let reference = format!("{manifest}/reference");
    let db_str = python_db_path.to_str()?;

    // Use Rust Debug format ({:?}) to emit properly-quoted Python string literals.
    let script = format!(
        "import sys, json\n\
         sys.path.insert(0, {reference:?})\n\
         from tentoku.sqlite_dict_optimized import OptimizedSQLiteDictionary\n\
         from tentoku.tokenizer import tokenize as py_tok\n\
         d = OptimizedSQLiteDictionary({db_str:?}, auto_build=False)\n\
         tokens = py_tok({text:?}, d)\n\
         print(json.dumps([{{\"text\": t.text, \"start\": t.start, \"end\": t.end}} for t in tokens]))\n",
    );

    let out = Command::new(&py_bin).arg("-c").arg(&script).output().ok()?;
    if !out.status.success() {
        eprintln!("python stderr: {}", String::from_utf8_lossy(&out.stderr));
        return None;
    }

    let parsed: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).ok()?;
    let tokens = parsed
        .into_iter()
        .map(|v| SimpleToken {
            text: v["text"].as_str().unwrap_or("").to_owned(),
            start: v["start"].as_u64().unwrap_or(0) as usize,
            end: v["end"].as_u64().unwrap_or(0) as usize,
        })
        .collect();
    Some(tokens)
}

fn rust_tokenize(text: &str, dict: &SqliteDictionary) -> Vec<SimpleToken> {
    tokenize(text, dict, 10)
        .into_iter()
        .map(|t| SimpleToken {
            text: t.text,
            start: t.start,
            end: t.end,
        })
        .collect()
}

#[test]
fn cross_validate_tokenization() {
    // Auto-setup Python reference (clone tentoku, checkout cython-version) if missing.
    if !ensure_python_reference() {
        eprintln!(
            "SKIP cross_validate: Python reference not ready \
             (needs reference/tentoku and dedicated Python DB; \
             on Unix, scripts/setup_python_reference.sh prepares both)"
        );
        return;
    }

    // Rust and Python must use the same DB for a meaningful parity check.
    let db_path = match shared_comparison_db_path() {
        Some(p) => p,
        None => {
            eprintln!(
                "SKIP cross_validate: no shared JMDict database found \
                 (set TENTOKU_COMPARE_DB, or prepare reference/tentoku/data/jmdict.python.db, \
                 or set TENTOKU_DB)"
            );
            return;
        }
    };

    let dict = match SqliteDictionary::open(&db_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "SKIP cross_validate: failed to open shared DB at {}: {e}",
                db_path.display()
            );
            return;
        }
    };

    let mut mismatches = 0;
    for &sentence in SENTENCES {
        let rust = rust_tokenize(sentence, &dict);
        let python = match python_tokenize(sentence, &db_path) {
            Some(p) => p,
            None => {
                eprintln!(
                    "SKIP cross_validate: Python tokenizer unavailable or errored \
                     (set PYTHON_BIN if needed)"
                );
                return;
            }
        };

        if rust != python {
            eprintln!("MISMATCH for {:?}", sentence);
            eprintln!("  Rust:   {:?}", rust);
            eprintln!("  Python: {:?}", python);
            mismatches += 1;
        }
    }

    // One medium-paragraph test (same input as vs_python bench) for longer-text parity.
    let rust_para = rust_tokenize(MEDIUM_PARAGRAPH, &dict);
    let python_para = match python_tokenize(MEDIUM_PARAGRAPH, &db_path) {
        Some(p) => p,
        None => {
            eprintln!(
                "SKIP cross_validate: Python tokenizer unavailable or errored \
                 (set PYTHON_BIN if needed)"
            );
            return;
        }
    };
    if rust_para != python_para {
        eprintln!("MISMATCH for MEDIUM_PARAGRAPH");
        eprintln!("  Rust:   {} tokens", rust_para.len());
        eprintln!("  Python: {} tokens", python_para.len());
        mismatches += 1;
    }

    assert_eq!(
        mismatches, 0,
        "{mismatches} sentence(s) had token boundary mismatches between Rust and Python"
    );
}
