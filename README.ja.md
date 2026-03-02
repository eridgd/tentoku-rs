<div align="center">
  <img src="https://raw.githubusercontent.com/eridgd/tentoku/main/images/tentoku_icon.svg" alt="Tentoku Logo" width="128">
</div>

# tentoku-rs（天読）— Rust製日本語トークナイザ

**辞書ベースの日本語トークナイザ（活用形復元機能付き、Rust実装）**

[![License](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

[English README](README.md)

> **⚠️ 注意（自動翻訳）**  
> この README（日本語版）は自動翻訳（機械翻訳）です。  
> 内容の正確性および最新情報については、英語版 README を正としてください。

---

tentoku-rsは[tentoku](https://github.com/eridgd/tentoku)のRust移植版です。tentokuは[10ten Japanese Reader](https://github.com/birchill/10ten-ja-reader)で使用されている高精度トークナイゼーションエンジンのPython移植版です。

MeCabやSudachiなどの統計的分かち書きツールとは異なり、tentoku-rsは貪欲最長一致アルゴリズムと、活用形を辞書形に戻すルールベースシステムを組み合わせています。速度よりも検索精度を優先するため、リーディング支援ツール、辞書ツール、アノテーション作業フローに適しています。

## 機能

- **貪欲最長一致トークナイゼーション**: テキスト内で可能な限り長い単語を見つけます
- **活用形復元サポート**: 241の活用規則を処理し、動詞と形容詞を辞書形に戻します
- **時制と形式の検出**: 「丁寧過去形」「継続形」「否定形」などの動詞形式を識別します
- **自動データベースセットアップ**: CLIがJMDictデータベースを自動的にダウンロードして構築します
- **辞書検索**: JMDict SQLiteデータベースを使用して単語を検索します
- **テキストのバリエーション**: 長音（ー）の展開と旧字体→新字体変換を処理します
- **型検証**: 復元した活用形を品詞タグとの整合性で検証します
- **C FFI / JNI レイヤー**: `cdylib` / `staticlib` を出力し、C互換APIを提供します
- **Pythonリファレンスのtentokuとのクロスバリデーションでトークン境界が一致することを確認済み**

## インストール

### 前提条件

- Rust 1.70以上（`std::sync::OnceLock`を使用）
- ネットワーク接続はJMDictデータベースの構築時のみ必要です

### クイックスタート

```bash
cargo install tentoku
tentoku tokenize "私は学生です"
```

初回実行時の動作：
- `tentoku tokenize ...`および`tentoku lookup ...`は、DBが存在しない場合に自動的に構築します。
- `--no-build-db`を使用すると自動構築を無効にし、即座に失敗させることができます。
- `tentoku build-db`は事前にDBを構築する場合に引き続き使用できます。

### ソースからビルド

PATHにインストールせずにビルドするには：

```bash
git clone https://github.com/eridgd/tentoku-rs.git
cd tentoku-rs
cargo build --release
```

これにより`target/release/tentoku`（CLI）、`target/release/libtentoku.so`/`.dylib`/`.dll`（FFI）、および`target/release/libtentoku.a`（スタティック）が生成されます。CLIは`./target/release/tentoku`（Windowsの場合は`target\release\tentoku.exe`）として実行できます。

## CLIの使用方法

以下の例では`tentoku`がPATHに含まれていることを前提としています。含まれていない場合は`./target/release/tentoku`（Windowsの場合は`target\release\tentoku.exe`）を使用してください。

### 辞書データベースの構築

EDRDGから`JMdict_e.gz`をダウンロードし、ローカルのSQLiteデータベースを構築します：

```bash
tentoku build-db
# またはカスタムパスを指定：
tentoku build-db --output /path/to/jmdict.db
```

デフォルトのデータベース保存場所はプラットフォームによって異なります：
- **macOS**: `~/Library/Application Support/tentoku/jmdict.db`
- **Linux**: `~/.local/share/tentoku/jmdict.db`（`$XDG_DATA_HOME`に従います）
- **Windows**: `%APPDATA%\tentoku\jmdict.db`

環境変数`TENTOKU_DB`で上書きすることもできます。

### テキストのトークナイズ

```bash
tentoku tokenize "私は学生です"
tentoku tokenize --max 10 "食べました"
tentoku tokenize --db /path/to/jmdict.db "高かった"
```

`--max`はトークン位置ごとに保持する辞書結果の数を制限します（デフォルト: 5）。出力は`Token`オブジェクトのJSON配列です（[データ型](#データ型)を参照）。

### 単語の検索

```bash
tentoku lookup "食べる"
tentoku lookup --max 5 "たかい"
```

`--max`は返す結果の数を制限します（デフォルト: 10）。出力はすべての語義、読み方、活用形復元チェーンを含む`WordResult`オブジェクトのJSON配列です。

## ライブラリの使用方法

`Cargo.toml`に追加：

```toml
[dependencies]
tentoku = { path = "/path/to/tentoku-rs" }
```

### 基本的なトークナイゼーション

```rust
use std::error::Error;

use tentoku::database_path::find_database_path;
use tentoku::sqlite_dict::SqliteDictionary;
use tentoku::tokenizer::tokenize;

fn main() -> Result<(), Box<dyn Error>> {
    let db_path = find_database_path()
        .ok_or("データベースが見つかりません。先に`tentoku build-db`を実行してください。")?;
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

この例では`?`を使用しているため、`Result`を返す関数（例：`Result<(), Box<dyn Error>>`）の中で使用する必要があります。

### 活用形の復元

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

利用可能な`Reason`バリアントには以下が含まれます：
`PolitePast`、`Polite`、`Past`、`Negative`、`Continuous`、`Potential`、`Causative`、
`Passive`、`Tai`、`Volitional`、`Te`、`Zu`、`Imperative`、`MasuStem`、`Adv`、`Noun`、
`CausativePassive`、`EruUru`、`Sou`、`Tara`、`Tari`、`Ki`、`SuruNoun`、`ZaruWoEnai`、
`NegativeTe`、`Irregular` など（詳細は`src/types.rs`を参照）。

### 単語検索（上級者向け）

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

### プログラムによるデータベース構築

```rust
use tentoku::build_database::build_database;

// EDRDGからダウンロードして構築：
build_database("/path/to/jmdict.db", None)?;

// または生のgzipバイト列を提供（例：埋め込みアセットから）：
let gz_bytes: Vec<u8> = std::fs::read("JMdict_e.gz")?;
build_database("/path/to/jmdict.db", Some(gz_bytes))?;
```

## C FFI / JNI

このクレートは`cdylib`としてビルドされ、JNI（Android）や Swift、Kotlin/Native などから利用できる安定した C API を公開します。

```c
#include "tentoku.h"

// 辞書を開く
TentokuHandle* h = tentoku_open("/path/to/jmdict.db");

// トークナイズ — ヒープに確保されたJSONを返します。呼び出し元が解放する必要があります
char* json = tentoku_tokenize_json(h, "私は学生です", 5);
// ... jsonを使用 ...
tentoku_free_string(json);

// 単語を検索
char* result = tentoku_lookup_json(h, "食べる", 10);
tentoku_free_string(result);

// 閉じる
tentoku_free(h);
```

関数一覧：
| 関数 | 説明 |
|---|---|
| `tentoku_open(path)` | 辞書を開く。ハンドルまたはnullを返す |
| `tentoku_free(handle)` | ハンドルを解放する |
| `tentoku_tokenize_json(handle, text, max)` | トークナイズ。JSON文字列を返す |
| `tentoku_lookup_json(handle, word, max)` | 単語を検索。JSON文字列を返す |
| `tentoku_free_string(s)` | 上記の関数が返したJSON文字列を解放する |

返される文字列はすべてUTF-8 JSONです。エラー時は null が返されます。

## データ型

すべての型は`Serialize`/`Deserialize`（serde）を実装しています。

### `Token`
```
text              — 元のテキストスパン
start, end        — 元の入力への文字インデックス
dictionary_entry  — Option<WordEntry> — 最良の辞書マッチ
deinflection_reasons — Option<Vec<Vec<Reason>>> — 活用形復元チェーン
```

### `WordEntry`
```
ent_seq           — JMDictシーケンス番号
kanji_readings    — Vec<KanjiReading>
kana_readings     — Vec<KanaReading>
senses            — Vec<Sense>
```

### `Sense`
```
index             — エントリ内の語義インデックス
pos_tags          — Vec<String> — 品詞コード（例："v1"、"adj-i"）
glosses           — Vec<Gloss> — 定義
field             — Option<Vec<String>> — 分野（例："comp"、"med"）
misc              — Option<Vec<String>> — 用法情報（例："uk"、"pol"）
dial              — Option<Vec<String>> — 方言
```

## アルゴリズム

1. **入力の正規化** — 半角数字を全角に変換、Unicode NFC適用、ZWNJの削除、文字インデックスマッピングの構築
2. **貪欲最長一致** — 位置0から開始し、最長の一致する単語を見つける
3. **単語検索** — 各候補部分文字列について：
   - テキストのバリエーションを生成（長音展開、旧字体変換）
   - BFS活用形復元を実行して候補辞書形を取得
   - 候補をSQLite辞書で検索し、品詞タイプを検証
   - 最長の成功した一致を追跡
   - 一致がない場合、入力を2文字（拗音終わり）または1文字短縮して再試行
4. **進行** — 一致した長さだけ進む、または一致がない場合は1文字進む

## アーキテクチャ

| モジュール | 説明 |
|---|---|
| `src/types.rs` | コア型：`Token`、`WordEntry`、`WordResult`、`WordType`、`Reason` |
| `src/normalize.rs` | Unicode NFC、全角数字、ZWNJ削除、文字インデックスマッピング |
| `src/variations.rs` | 長音（ー）展開、旧字体→新字体変換 |
| `src/yoon.rs` | 拗音終わりの検出 |
| `src/deinflect_rules.rs` | 241の静的活用形復元規則 |
| `src/deinflect.rs` | 一段動詞語幹フォワーディングを備えたBFS活用形復元エンジン |
| `src/type_matching.rs` | 活用形復元候補の品詞タグ検証 |
| `src/sorting.rs` | 優先度ベースの結果ソート（ichi1、nf##頻度帯など） |
| `src/dictionary.rs` | `Dictionary`トレイト |
| `src/sqlite_dict.rs` | SQLiteバックエンド（WAL、バッチクエリ、ネガティブキャッシュ） |
| `src/database_path.rs` | プラットフォーム固有のデータベースパス解決 |
| `src/build_database.rs` | JMDictのダウンロード、gzip解凍、XMLパース、SQLiteインポート |
| `src/word_search.rs` | 貪欲バックトラッキング単語検索 |
| `src/tokenizer.rs` | 文全体のトークナイザ |
| `src/ffi.rs` | C互換FFIレイヤー |
| `src/bin/tentoku.rs` | CLI（`build-db`、`tokenize`、`lookup`） |

## データベース

トークナイザーはJMDict SQLiteデータベースを使用します。`tentoku build-db`を実行して作成してください。EDRDGの公式ソースから`JMdict_e.gz`（約10MB）をダウンロードし、解凍してXMLをSQLiteにインポートします（結果は約105MB）。

データベーステーブル：`entries`、`kanji`、`readings`、`reading_restrictions`、`senses`、`sense_pos`、`sense_field`、`sense_misc`、`sense_dial`、`glosses`。

これは一度きりの処理です。以降の使用では構築済みのデータベースを即座に読み込みます。

## テスト

```bash
cargo test
```

テストスイートには57のテストがあります：56のユニットテスト（正規化、活用形復元、型マッチング、ソート、SQLite辞書、単語検索、トークナイゼーション、キーインデックス）と1つの統合テストです。ユニットテストはインメモリのミニJMDictを使用します（ネットワーク不要）。統合テスト`tests/cross_validate.rs`は代表的な文をRustとPython両方のトークナイザーで処理し、トークン境界が一致することを検証します。**Pythonリファレンスは自動化されています：** 初回実行時（`reference/`が存在しない場合）、テストは`scripts/setup_python_reference.sh`を実行し、[tentoku](https://github.com/eridgd/tentoku)をクローンして`cython-version`ブランチをチェックアウトしてインストールします。手動のクローンやインストールは不要です。テストは、JMDictデータベースが存在しないかセットアップスクリプトが失敗した場合（例：git/ネットワークなし）にのみスキップされます。

## ベンチマーク

Rust実装は、長文トークナイゼーションベンチマークでPythonリファレンスを大幅に上回ります（`cargo bench`を参照）。

```bash
cargo bench
```

ベンチマーク（[Criterion](https://github.com/bheisler/criterion.rs)経由）には以下が含まれます：
- **ミニ辞書（実DB不要）：** `tokenize/plain_verb`、`tokenize/deinflected_verb`、`tokenize/short_sentence`、`word_search/plain`、`normalize_input/hiragana`
- **実JMDict：** `tokenize/medium_paragraph`、`tokenize/long_text`、`tokenize/kokoro_passage`（`tentoku build-db`または`TENTOKU_DB`が必要）
- **Rust vs Python：** グループ`vs_python` — `rust/medium_paragraph`、`python/medium_paragraph`；グループ`vs_python_long` — `rust/kokoro_passage`、`python/kokoro_passage`（3サンプル）。実DBとPythonが必要です。**Pythonリファレンスは自動セットアップされます：** これらのベンチマークを実行し`reference/`が存在しない場合、ベンチハーネスが`scripts/setup_python_reference.sh`を実行してtentokuをクローン、`cython-version`をチェックアウトしてインストールします。手動の手順は不要です。Pythonバイナリを上書きするには`PYTHON_BIN`を使用してください。

比較ベンチマークのみ実行：`cargo bench -- vs_python`または`cargo bench -- vs_python_long`。リファレンスを手動で準備する場合（例：オフライン時）：リポジトリルートから`./scripts/setup_python_reference.sh`を実行してください。

## クレジット

トークナイゼーションロジック、活用形復元規則、マッチング戦略は[10ten Japanese Reader](https://github.com/birchill/10ten-ja-reader)で使用されている元のTypeScript実装から派生しています。Python移植版[tentoku](https://github.com/eridgd/tentoku)がこのRust移植版の直接のリファレンスとなっています。

### 辞書データ

このクレートは[JMDict](https://www.edrdg.org/wiki/index.php/JMdict-EDICT_Dictionary_Project)辞書データを使用しています。このデータは[電子辞書研究開発グループ](https://www.edrdg.org/)（EDRDG）の所有物です。辞書データは[Creative Commons Attribution-ShareAlike 4.0 International License](https://creativecommons.org/licenses/by-sa/4.0/)（CC BY-SA 4.0）の下でライセンスされています。

著作権はJames William BREENと電子辞書研究開発グループが保持しています。

JMDictデータは実行時に公式EDRDGソースからダウンロードされます。データはこのリポジトリには含まれていません。詳細については：
- **JMDictプロジェクト**: https://www.edrdg.org/wiki/index.php/JMdict-EDICT_Dictionary_Project
- **EDRDGライセンス声明**: https://www.edrdg.org/edrdg/licence.html

完全な帰属の詳細については[JMDICT_ATTRIBUTION.md](JMDICT_ATTRIBUTION.md)を参照してください。

## ライセンス

このプロジェクトはGNU General Public License v3.0以降（GPL-3.0-or-later）の下でライセンスされています。

完全なライセンステキストについては[LICENSE](LICENSE)ファイルを参照してください。

**辞書データに関する注意**: このソフトウェアはGPL-3.0-or-laterの下でライセンスされていますが、JMDict辞書データはCC BY-SA 4.0の下で別途ライセンスされています。データベースと共にこのソフトウェアを配布する場合、両方のライセンスがそれぞれのコンポーネントに適用されます。