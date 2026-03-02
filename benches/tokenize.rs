use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use tempfile::NamedTempFile;

use tentoku::build_database::build_database_from_xml;
use tentoku::database_path::find_database_path;
use tentoku::normalize::normalize_input;
use tentoku::sqlite_dict::SqliteDictionary;
use tentoku::tokenizer::tokenize;
use tentoku::word_search::word_search;

/// Minimal JMDict used for existing benchmarks (kept in memory, no network needed).
const BENCH_JMDICT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<JMdict>
<entry>
<ent_seq>1549240</ent_seq>
<r_ele><reb>たべる</reb><re_pri>ichi1</re_pri></r_ele>
<k_ele><keb>食べる</keb><ke_pri>ichi1</ke_pri></k_ele>
<sense><pos>v1</pos><gloss>to eat</gloss></sense>
</entry>
<entry>
<ent_seq>1166770</ent_seq>
<r_ele><reb>よむ</reb></r_ele>
<k_ele><keb>読む</keb></k_ele>
<sense><pos>v5m</pos><gloss>to read</gloss></sense>
</entry>
<entry>
<ent_seq>1002830</ent_seq>
<r_ele><reb>たかい</reb></r_ele>
<k_ele><keb>高い</keb></k_ele>
<sense><pos>adj-i</pos><gloss>high; tall</gloss></sense>
</entry>
<entry>
<ent_seq>1467640</ent_seq>
<r_ele><reb>みる</reb><re_pri>ichi1</re_pri></r_ele>
<k_ele><keb>見る</keb><ke_pri>ichi1</ke_pri></k_ele>
<sense><pos>v1</pos><gloss>to see</gloss></sense>
</entry>
<entry>
<ent_seq>1316670</ent_seq>
<r_ele><reb>いく</reb><re_pri>ichi1</re_pri></r_ele>
<k_ele><keb>行く</keb><ke_pri>ichi1</ke_pri></k_ele>
<sense><pos>v5k-s</pos><gloss>to go</gloss></sense>
</entry>
</JMdict>"#;

/// Max dictionary results per token position. Matches reference (Python) tentoku tokenizer
/// so benchmark outputs and work are comparable (no skew).
const BENCH_MAX_RESULTS: usize = 12;

/// ~200-char paragraph of natural Japanese.
const MEDIUM_PARAGRAPH: &str = concat!(
    "私は毎日日本語を勉強しています。図書館で本を読んだり、音楽を聞いたりします。",
    "友達と話すことが好きです。先生に質問することができます。",
    "日本の文化について学んでいます。食べ物の中でお寿司が一番好きです。",
    "東京に行きたいと思っています。日本語がだんだん上手になってきました。",
    "毎朝六時に起きて、学校まで電車で通っています。",
);

/// Long natural passage from Kokoro (Natsume Soseki) for realistic Rust vs Python comparison.
const KOKORO_PASSAGE: &str = concat!(
    "私はその人を常に先生と呼んでいた。だからここでもただ先生と書くだけで本名は打ち明けない。",
    "これは世間を憚る遠慮というよりも、その方が私にとって自然だからである。",
    "私はその人の記憶を呼び起すごとに、すぐ「先生」といいたくなる。筆を執っても心持は同じ事である。",
    "よそよそしい頭文字などはとても使う気にならない。\n\n",
    "私が先生と知り合いになったのは鎌倉である。その時私はまだ若々しい書生であった。",
    "暑中休暇を利用して海水浴に行った友達からぜひ来いという端書を受け取ったので、",
    "私は多少の金を工面して、出掛ける事にした。私は金の工面に二、三日を費やした。",
    "ところが私が鎌倉に着いて三日と経たないうちに、私を呼び寄せた友達は、急に国元から帰れという電報を受け取った。",
    "電報には母が病気だからと断ってあったけれども友達はそれを信じなかった。",
    "友達はかねてから国元にいる親たちに勧まない結婚を強いられていた。",
    "彼は現代の習慣からいうと結婚するにはあまり年が若過ぎた。それに肝心の当人が気に入らなかった。",
    "それで夏休みに当然帰るべきところを、わざと避けて東京の近くで遊んでいたのである。",
    "彼は電報を私に見せてどうしようと相談をした。私にはどうしていいか分らなかった。",
    "けれども実際彼の母が病気であるとすれば彼は元より帰るべきはずであった。それで彼はとうとう帰る事になった。",
    "せっかく来た私は一人取り残された。\n\n",
    "学校の授業が始まるにはまだ大分日数があるので鎌倉におってもよし、帰ってもよいという境遇にいた私は、",
    "当分元の宿に留まる覚悟をした。友達は中国のある資産家の息子で金に不自由のない男であったけれども、",
    "学校が学校なのと年が年なので、生活の程度は私とそう変りもしなかった。",
    "したがって一人ぼっちになった私は別に恰好な宿を探す面倒ももたなかったのである。\n\n",
    "宿は鎌倉でも辺鄙な方角にあった。玉突だのアイスクリームだのというハイカラなものには長い畷を一つ越さなければ手が届かなかった。",
    "車で行っても二十銭は取られた。けれども個人の別荘はそこここにいくつでも建てられていた。",
    "それに海へはごく近いので海水浴をやるには至極便利な地位を占めていた。\n\n",
    "私は毎日海へはいりに出掛けた。古い燻り返った藁葺の間を通り抜けて磯へ下りると、",
    "この辺にこれほどの都会人種が住んでいるかと思うほど、避暑に来た男や女で砂の上が動いていた。",
    "ある時は海の中が銭湯のように黒い頭でごちゃごちゃしている事もあった。",
    "その中に知った人を一人ももたない私も、こういう賑やかな景色の中に包まれて、",
    "砂の上に寝そべってみたり、膝頭を波に打たしてそこいらを跳ね廻るのは愉快であった。\n\n",
    "私は実に先生をこの雑沓の間に見付け出したのである。その時海岸には掛茶屋が二軒あった。",
    "私はふとした機会からその一軒の方に行き慣れていた。",
    "長谷辺に大きな別荘を構えている人と違って、各自に専有の着換場を拵えていないここいらの避暑客には、",
    "ぜひともこうした共同着換所といった風なものが必要なのであった。",
    "彼らはここで茶を飲み、ここで休息する外に、ここで海水着を洗濯させたり、ここで塩はゆい身体を清めたり、",
    "ここへ帽子や傘を預けたりするのである。海水着を持たない私にも持物を盗まれる恐れはあったので、",
    "私は海へはいるたびにその茶屋へ一切を脱ぎ棄てる事にしていた。",
);

static LONG_TEXT: OnceLock<String> = OnceLock::new();

fn long_text() -> &'static str {
    LONG_TEXT
        .get_or_init(|| MEDIUM_PARAGRAPH.repeat(12))
        .as_str()
}

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

// ---------------------------------------------------------------------------
// Mini-dict (for existing fast benchmarks)
// ---------------------------------------------------------------------------

fn make_bench_dict() -> SqliteDictionary {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_owned();
    std::mem::forget(tmp);
    let conn = Connection::open(&path).unwrap();
    build_database_from_xml(&conn, BENCH_JMDICT).unwrap();
    SqliteDictionary::open(&path).unwrap()
}

static DICT: OnceLock<SqliteDictionary> = OnceLock::new();

fn get_dict() -> &'static SqliteDictionary {
    DICT.get_or_init(make_bench_dict)
}

// ---------------------------------------------------------------------------
// Real dict (for medium/long-text and Python-comparison benchmarks)
// ---------------------------------------------------------------------------

fn make_real_dict() -> Option<SqliteDictionary> {
    let path = find_database_path()?;
    SqliteDictionary::open(&path).ok()
}

static REAL_DICT: OnceLock<Option<SqliteDictionary>> = OnceLock::new();

fn get_real_dict_cached() -> Option<&'static SqliteDictionary> {
    REAL_DICT.get_or_init(make_real_dict).as_ref()
}

// ---------------------------------------------------------------------------
// Existing fast benchmarks (mini-dict, no network)
// ---------------------------------------------------------------------------

fn bench_tokenize_plain(c: &mut Criterion) {
    let dict = get_dict();
    c.bench_function("tokenize/plain_verb", |b| {
        b.iter(|| tokenize(black_box("食べる"), dict, BENCH_MAX_RESULTS))
    });
}

fn bench_tokenize_deinflected(c: &mut Criterion) {
    let dict = get_dict();
    c.bench_function("tokenize/deinflected_verb", |b| {
        b.iter(|| tokenize(black_box("食べた"), dict, BENCH_MAX_RESULTS))
    });
}

fn bench_tokenize_sentence(c: &mut Criterion) {
    let dict = get_dict();
    c.bench_function("tokenize/short_sentence", |b| {
        b.iter(|| tokenize(black_box("食べるものを見る"), dict, BENCH_MAX_RESULTS))
    });
}

fn bench_word_search(c: &mut Criterion) {
    let dict = get_dict();
    c.bench_function("word_search/plain", |b| {
        b.iter(|| {
            let (normalized, lengths) = normalize_input(black_box("食べる"));
            word_search(&normalized, dict, 5, Some(&lengths))
        })
    });
}

fn bench_normalize(c: &mut Criterion) {
    c.bench_function("normalize_input/hiragana", |b| {
        b.iter(|| normalize_input(black_box("たべるものをみる")))
    });
}

// ---------------------------------------------------------------------------
// Realistic benchmarks against the real JMDict database
// ---------------------------------------------------------------------------

fn bench_tokenize_medium_paragraph(c: &mut Criterion) {
    let Some(dict) = get_real_dict_cached() else {
        eprintln!("SKIP tokenize/medium_paragraph: no real database (set TENTOKU_DB)");
        return;
    };
    c.bench_function("tokenize/medium_paragraph", |b| {
        b.iter(|| tokenize(black_box(MEDIUM_PARAGRAPH), dict, BENCH_MAX_RESULTS))
    });
}

fn bench_tokenize_long_text(c: &mut Criterion) {
    let Some(dict) = get_real_dict_cached() else {
        eprintln!("SKIP tokenize/long_text: no real database (set TENTOKU_DB)");
        return;
    };
    c.bench_function("tokenize/long_text", |b| {
        b.iter(|| tokenize(black_box(long_text()), dict, BENCH_MAX_RESULTS))
    });
}

/// Rust-only benchmark for the Kokoro passage (no Python comparison).
fn bench_tokenize_kokoro(c: &mut Criterion) {
    let Some(dict) = get_real_dict_cached() else {
        eprintln!("SKIP tokenize/kokoro_passage: no real database (set TENTOKU_DB)");
        return;
    };
    c.bench_function("tokenize/kokoro_passage", |b| {
        b.iter(|| tokenize(black_box(KOKORO_PASSAGE), dict, BENCH_MAX_RESULTS))
    });
}

// ---------------------------------------------------------------------------
// Python comparison benchmarks
// ---------------------------------------------------------------------------

/// Run the Python tokenizer for `iters` iterations inside a single subprocess
/// and return the **total** elapsed duration.  Returns `None` if Python is
/// unavailable or the script fails.
fn python_tokenize_timed(text: &str, iters: u64) -> Option<std::time::Duration> {
    let py_bin = std::env::var("PYTHON_BIN").unwrap_or_else(|_| "python3".to_string());
    let manifest = env!("CARGO_MANIFEST_DIR");
    let reference_path = format!("{manifest}/reference");
    let db_path = python_comparison_db_path();
    if !db_path.exists() {
        eprintln!(
            "SKIP vs_python: dedicated Python DB missing at {}",
            db_path.display()
        );
        return None;
    }
    let db_str = db_path.to_str()?;

    // Use Rust's Debug format ({:?}) to produce properly-quoted Python string literals.
    // Avoid backslash-continuation after the `for` line to preserve indentation.
    let script = format!(
        "import sys, time\n\
         sys.path.insert(0, {reference_path:?})\n\
         from tentoku.sqlite_dict_optimized import OptimizedSQLiteDictionary\n\
         from tentoku.tokenizer import tokenize as py_tokenize\n\
         d = OptimizedSQLiteDictionary({db_str:?}, auto_build=False)\n\
         text = {text:?}\n\
         n = {iters}\n\
         t0 = time.perf_counter()\n\
         for _ in range(n):\n    py_tokenize(text, d)\nprint(time.perf_counter() - t0)\n",
    );

    let out = Command::new(&py_bin).arg("-c").arg(&script).output().ok()?;
    if !out.status.success() {
        eprintln!("python stderr: {}", String::from_utf8_lossy(&out.stderr));
        return None;
    }
    let secs: f64 = std::str::from_utf8(&out.stdout).ok()?.trim().parse().ok()?;
    Some(std::time::Duration::from_secs_f64(secs))
}

/// If reference/tentoku is missing, run setup script to clone tentoku and checkout
/// cython-version so vs_python benchmarks work without manual setup.
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
    Command::new("sh")
        .arg(&script)
        .arg(manifest)
        .current_dir(manifest)
        .env("TENTOKU_PYTHON_DB", &python_db)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        && reference.join("tentoku").exists()
        && python_db.exists()
}

#[cfg(not(unix))]
fn ensure_python_reference() -> bool {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("reference")
        .join("tentoku")
        .exists()
        && python_comparison_db_path().exists()
}

fn bench_python_comparison(c: &mut Criterion) {
    let Some(_dict) = get_real_dict_cached() else {
        eprintln!("SKIP vs_python: no real database (set TENTOKU_DB)");
        return;
    };

    let py_bin = std::env::var("PYTHON_BIN").unwrap_or_else(|_| "python3".to_string());
    if Command::new(&py_bin).arg("--version").output().is_err() {
        eprintln!("SKIP vs_python: python3 not found");
        return;
    }

    // Auto-setup Python reference (clone tentoku, checkout cython-version) if missing.
    if !ensure_python_reference() {
        eprintln!("SKIP vs_python: reference/ not found and setup failed or unavailable");
        return;
    }

    // Verify the Python tokenizer actually works before committing to the group.
    if python_tokenize_timed(MEDIUM_PARAGRAPH, 1).is_none() {
        eprintln!("SKIP vs_python: Python tokenizer unavailable or errored");
        return;
    }

    // Medium paragraph: 20 samples for stable comparison.
    let mut group = c.benchmark_group("vs_python");
    group.sample_size(20);

    group.bench_function("rust/medium_paragraph", |b| {
        let dict = get_real_dict_cached().unwrap();
        b.iter(|| tokenize(black_box(MEDIUM_PARAGRAPH), dict, BENCH_MAX_RESULTS))
    });

    group.bench_function("python/medium_paragraph", |b| {
        b.iter_custom(|iters| {
            python_tokenize_timed(MEDIUM_PARAGRAPH, iters).unwrap_or(std::time::Duration::ZERO)
        })
    });

    group.finish();

    // Kokoro passage: fewer samples so the run finishes in minutes (each iteration ~30s with full JMDict).
    let mut group_long = c.benchmark_group("vs_python_long");
    group_long.sample_size(3);

    group_long.bench_function("rust/kokoro_passage", |b| {
        let dict = get_real_dict_cached().unwrap();
        b.iter(|| tokenize(black_box(KOKORO_PASSAGE), dict, BENCH_MAX_RESULTS))
    });

    group_long.bench_function("python/kokoro_passage", |b| {
        b.iter_custom(|iters| {
            python_tokenize_timed(KOKORO_PASSAGE, iters).unwrap_or(std::time::Duration::ZERO)
        })
    });

    group_long.finish();
}

criterion_group!(
    benches,
    bench_tokenize_plain,
    bench_tokenize_deinflected,
    bench_tokenize_sentence,
    bench_word_search,
    bench_normalize,
    bench_tokenize_medium_paragraph,
    bench_tokenize_long_text,
    bench_tokenize_kokoro,
    bench_python_comparison,
);
criterion_main!(benches);
