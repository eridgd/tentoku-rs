use std::fs;
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use tentoku::build_database::build_database;
use tentoku::database_path::{find_database_path, get_default_database_path};
use tentoku::error::TentokuError;
use tentoku::normalize::normalize_input;
use tentoku::sqlite_dict::SqliteDictionary;
use tentoku::tokenizer::tokenize;
use tentoku::word_search::word_search;

#[derive(Parser)]
#[command(
    name = "tentoku",
    about = "Japanese text tokenizer with deinflection",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download JMdict_e.gz and build the SQLite dictionary database.
    BuildDb {
        /// Output path for the database file.
        /// Defaults to the platform user-data directory.
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Tokenize Japanese text and print each token as JSON.
    Tokenize {
        /// Text to tokenize (use --file to read from a file instead).
        text: Option<String>,

        /// Read text from file (e.g. for cross-validation with Python output).
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Maximum dictionary results kept per token position.
        #[arg(short, long, default_value = "5")]
        max: usize,

        /// Path to the dictionary database.
        /// Falls back to TENTOKU_DB env var, then the platform default.
        #[arg(short, long)]
        db: Option<String>,

        /// Do not auto-build the database if it does not exist.
        #[arg(long, default_value_t = false)]
        no_build_db: bool,
    },

    /// Look up a single word in the dictionary and print all senses as JSON.
    Lookup {
        /// Word to look up (kana or kanji).
        word: String,

        /// Maximum number of results.
        #[arg(short, long, default_value = "10")]
        max: usize,

        /// Path to the dictionary database.
        #[arg(short, long)]
        db: Option<String>,

        /// Do not auto-build the database if it does not exist.
        #[arg(long, default_value_t = false)]
        no_build_db: bool,
    },
}

fn resolve_db(flag: Option<String>) -> PathBuf {
    if let Some(p) = flag {
        return PathBuf::from(p);
    }
    find_database_path().unwrap_or_else(get_default_database_path)
}

fn open_dict(db_path: &PathBuf) -> SqliteDictionary {
    SqliteDictionary::open(db_path).unwrap_or_else(|e| match e {
        TentokuError::DatabaseNotFound { .. } => {
            eprintln!(
                "Error: database not found at {}\n\
                 Run `tentoku build-db` to download and build the dictionary.",
                db_path.display()
            );
            process::exit(1);
        }
        other => {
            eprintln!(
                "Error: could not open database at {}: {other}",
                db_path.display()
            );
            process::exit(1);
        }
    })
}

fn open_dict_with_autobuild(db_path: &PathBuf, no_build_db: bool) -> SqliteDictionary {
    match SqliteDictionary::open(db_path) {
        Ok(dict) => dict,
        Err(TentokuError::DatabaseNotFound { .. }) if !no_build_db => {
            eprintln!(
                "Database not found at {}.\nBuilding dictionary database (first run may take a while)...",
                db_path.display()
            );
            let output = db_path.to_string_lossy().into_owned();
            if let Err(e) = build_database(&output, None) {
                eprintln!("Build failed: {e}");
                process::exit(1);
            }
            open_dict(db_path)
        }
        Err(e) => {
            eprintln!(
                "Error: could not open database at {}: {e}",
                db_path.display()
            );
            if no_build_db {
                eprintln!("Tip: run `tentoku build-db` first, or remove `--no-build-db`.");
            }
            process::exit(1);
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::BuildDb { output } => {
            let db_path = output
                .unwrap_or_else(|| get_default_database_path().to_string_lossy().into_owned());
            if let Err(e) = build_database(&db_path, None) {
                eprintln!("Build failed: {e}");
                process::exit(1);
            }
        }

        Command::Tokenize {
            text,
            file,
            max,
            db,
            no_build_db,
        } => {
            let input = match (&text, &file) {
                (Some(t), None) => t.clone(),
                (None, Some(f)) => fs::read_to_string(f).unwrap_or_else(|e| {
                    eprintln!("Error reading file {}: {e}", f.display());
                    process::exit(1);
                }),
                (None, None) => {
                    eprintln!("Error: provide text to tokenize or --file <path>");
                    process::exit(1);
                }
                (Some(_), Some(_)) => {
                    eprintln!("Error: provide either text or --file, not both");
                    process::exit(1);
                }
            };
            let db_path = resolve_db(db);
            let dict = open_dict_with_autobuild(&db_path, no_build_db);
            let tokens = tokenize(&input, &dict, max);
            let json = serde_json::to_string_pretty(&tokens).unwrap_or_else(|e| {
                eprintln!("JSON error: {e}");
                process::exit(1);
            });
            println!("{json}");
        }

        Command::Lookup {
            word,
            max,
            db,
            no_build_db,
        } => {
            let db_path = resolve_db(db);
            let dict = open_dict_with_autobuild(&db_path, no_build_db);
            let (normalized, lengths) = normalize_input(&word);
            let result = word_search(&normalized, &dict, max, Some(&lengths));
            match result {
                None => {
                    eprintln!("No results for {:?}", word);
                    process::exit(1);
                }
                Some(r) => {
                    let json = serde_json::to_string_pretty(&r.data).unwrap_or_else(|e| {
                        eprintln!("JSON error: {e}");
                        process::exit(1);
                    });
                    println!("{json}");
                }
            }
        }
    }
}
