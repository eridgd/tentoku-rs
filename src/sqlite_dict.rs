use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::dictionary::Dictionary;
use crate::error::{Result, TentokuError};
use crate::normalize::kana_to_hiragana;
use crate::types::{Gloss, KanaReading, KanjiReading, Sense, WordEntry};

pub struct SqliteDictionary {
    conn: Mutex<Connection>,
    pub max_lookup_length: usize,
    negative_cache: Mutex<HashSet<String>>,
}

impl SqliteDictionary {
    /// Open an existing database at `path`.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(TentokuError::DatabaseNotFound {
                path: path.display().to_string(),
            });
        }

        let conn = Connection::open(path)?;
        Self::apply_pragmas(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            max_lookup_length: 15,
            negative_cache: Mutex::new(HashSet::new()),
        })
    }

    fn apply_pragmas(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "PRAGMA synchronous = OFF;
             PRAGMA journal_mode = WAL;
             PRAGMA temp_store = MEMORY;
             PRAGMA mmap_size = 268435456;
             PRAGMA cache_size = -128000;",
        )?;
        Ok(())
    }

    fn build_entries_batched(
        conn: &Connection,
        entry_ids: &[i64],
        entry_seqs: &HashMap<i64, String>,
        normalized_matching: &str,
    ) -> Result<Vec<WordEntry>> {
        if entry_ids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders = entry_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

        // Batch-fetch kanji
        let sql = format!(
            "SELECT kanji_id, entry_id, kanji_text, priority, info \
             FROM kanji WHERE entry_id IN ({}) ORDER BY entry_id, kanji_id",
            placeholders
        );
        let mut kanji_by_entry: HashMap<i64, Vec<(String, Option<String>, Option<String>)>> =
            HashMap::new();
        {
            let mut stmt = conn.prepare(&sql)?;
            let params_iter: Vec<&dyn rusqlite::ToSql> = entry_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            let mut rows = stmt.query(params_iter.as_slice())?;
            while let Some(row) = rows.next()? {
                let eid: i64 = row.get(1)?;
                kanji_by_entry.entry(eid).or_default().push((
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ));
            }
        }

        // Batch-fetch kana
        let sql = format!(
            "SELECT reading_id, entry_id, reading_text, no_kanji, priority, info \
             FROM readings WHERE entry_id IN ({}) ORDER BY entry_id, reading_id",
            placeholders
        );
        let mut kana_by_entry: HashMap<i64, Vec<(String, bool, Option<String>, Option<String>)>> =
            HashMap::new();
        {
            let mut stmt = conn.prepare(&sql)?;
            let params_iter: Vec<&dyn rusqlite::ToSql> = entry_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            let mut rows = stmt.query(params_iter.as_slice())?;
            while let Some(row) = rows.next()? {
                let eid: i64 = row.get(1)?;
                let no_kanji: i64 = row.get(3)?;
                kana_by_entry.entry(eid).or_default().push((
                    row.get(2)?,
                    no_kanji != 0,
                    row.get(4)?,
                    row.get(5)?,
                ));
            }
        }

        // Batch-fetch senses
        let sql = format!(
            "SELECT sense_id, entry_id, sense_index, info \
             FROM senses WHERE entry_id IN ({}) ORDER BY entry_id, sense_index",
            placeholders
        );
        let mut senses_by_entry: HashMap<i64, Vec<(i64, usize, Option<String>)>> = HashMap::new();
        let mut all_sense_ids: Vec<i64> = Vec::new();
        {
            let mut stmt = conn.prepare(&sql)?;
            let params_iter: Vec<&dyn rusqlite::ToSql> = entry_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            let mut rows = stmt.query(params_iter.as_slice())?;
            while let Some(row) = rows.next()? {
                let eid: i64 = row.get(1)?;
                let sid: i64 = row.get(0)?;
                let idx: i64 = row.get(2)?;
                all_sense_ids.push(sid);
                senses_by_entry
                    .entry(eid)
                    .or_default()
                    .push((sid, idx as usize, row.get(3)?));
            }
        }

        // Batch-fetch sense data
        let mut pos_map: HashMap<i64, Vec<String>> = HashMap::new();
        let mut gloss_map: HashMap<i64, Vec<Gloss>> = HashMap::new();
        let mut field_map: HashMap<i64, Vec<String>> = HashMap::new();
        let mut misc_map: HashMap<i64, Vec<String>> = HashMap::new();
        let mut dial_map: HashMap<i64, Vec<String>> = HashMap::new();

        if !all_sense_ids.is_empty() {
            let sp = all_sense_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");

            macro_rules! batch_fetch_single {
                ($table:expr, $col:expr, $map:expr) => {{
                    let sql = format!(
                        "SELECT sense_id, {} FROM {} WHERE sense_id IN ({})",
                        $col, $table, sp
                    );
                    let mut stmt = conn.prepare(&sql)?;
                    let params_iter: Vec<&dyn rusqlite::ToSql> = all_sense_ids
                        .iter()
                        .map(|id| id as &dyn rusqlite::ToSql)
                        .collect();
                    let mut rows = stmt.query(params_iter.as_slice())?;
                    while let Some(row) = rows.next()? {
                        let sid: i64 = row.get(0)?;
                        let val: String = row.get(1)?;
                        $map.entry(sid).or_insert_with(Vec::new).push(val);
                    }
                }};
            }

            batch_fetch_single!("sense_pos", "pos", pos_map);

            // Glosses need lang and g_type
            {
                let sql = format!(
                    "SELECT sense_id, gloss_text, lang, g_type FROM glosses \
                     WHERE sense_id IN ({}) ORDER BY sense_id, gloss_id",
                    sp
                );
                let mut stmt = conn.prepare(&sql)?;
                let params_iter: Vec<&dyn rusqlite::ToSql> = all_sense_ids
                    .iter()
                    .map(|id| id as &dyn rusqlite::ToSql)
                    .collect();
                let mut rows = stmt.query(params_iter.as_slice())?;
                while let Some(row) = rows.next()? {
                    let sid: i64 = row.get(0)?;
                    let lang: Option<String> = row.get(2)?;
                    gloss_map.entry(sid).or_default().push(Gloss {
                        text: row.get(1)?,
                        lang: lang.unwrap_or_else(|| "eng".to_string()),
                        g_type: row.get(3)?,
                    });
                }
            }

            batch_fetch_single!("sense_field", "field", field_map);
            batch_fetch_single!("sense_misc", "misc", misc_map);
            batch_fetch_single!("sense_dial", "dial", dial_map);
        }

        // Build entries
        let mut entries = Vec::with_capacity(entry_ids.len());
        for &eid in entry_ids {
            let ent_seq = entry_seqs.get(&eid).cloned().unwrap_or_default();

            // Kanji readings
            let kanji_rows = kanji_by_entry
                .get(&eid)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let kanji_match_found = kanji_rows
                .iter()
                .any(|(text, _, _)| kana_to_hiragana(text) == normalized_matching);

            let kanji_readings: Vec<KanjiReading> = kanji_rows
                .iter()
                .map(|(text, priority, info)| {
                    let matches = kana_to_hiragana(text) == normalized_matching;
                    KanjiReading {
                        match_range: if matches {
                            Some((0, text.chars().count()))
                        } else {
                            None
                        },
                        matched: (kanji_match_found && matches) || !kanji_match_found,
                        text: text.clone(),
                        priority: priority.clone(),
                        info: info.clone(),
                    }
                })
                .collect();

            // Kana readings
            let kana_rows = kana_by_entry.get(&eid).map(|v| v.as_slice()).unwrap_or(&[]);
            let kana_readings: Vec<KanaReading> = if !kanji_match_found {
                let mut kana_match_found = false;
                kana_rows
                    .iter()
                    .map(|(text, no_kanji, priority, info)| {
                        let matches = kana_to_hiragana(text) == normalized_matching;
                        if matches {
                            kana_match_found = true;
                        }
                        KanaReading {
                            match_range: if matches {
                                Some((0, text.chars().count()))
                            } else {
                                None
                            },
                            matched: (kana_match_found && matches) || !kana_match_found,
                            text: text.clone(),
                            no_kanji: *no_kanji,
                            priority: priority.clone(),
                            info: info.clone(),
                        }
                    })
                    .collect()
            } else {
                kana_rows
                    .iter()
                    .map(|(text, no_kanji, priority, info)| {
                        let matches = kana_to_hiragana(text) == normalized_matching;
                        KanaReading {
                            match_range: if matches {
                                Some((0, text.chars().count()))
                            } else {
                                None
                            },
                            matched: false,
                            text: text.clone(),
                            no_kanji: *no_kanji,
                            priority: priority.clone(),
                            info: info.clone(),
                        }
                    })
                    .collect()
            };

            // Senses
            let sense_rows = senses_by_entry
                .get(&eid)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let senses: Vec<Sense> = sense_rows
                .iter()
                .map(|(sid, idx, info)| Sense {
                    index: *idx,
                    pos_tags: pos_map.get(sid).cloned().unwrap_or_default(),
                    glosses: gloss_map.get(sid).cloned().unwrap_or_default(),
                    info: info.clone(),
                    field: field_map.get(sid).cloned(),
                    misc: misc_map.get(sid).cloned(),
                    dial: dial_map.get(sid).cloned(),
                })
                .collect();

            entries.push(WordEntry {
                entry_id: eid,
                ent_seq,
                kanji_readings,
                kana_readings,
                senses,
            });
        }

        Ok(entries)
    }

    /// Fetch `(entry_id, ent_seq)` rows in SQLite's returned row order.
    ///
    /// This order is used as a stable tie-breaker by the tokenizer's stable sort.
    /// Converting to `HashMap` too early introduces randomized ordering and causes
    /// non-deterministic entry selection across runs.
    fn fetch_entry_rows(
        conn: &Connection,
        sql: &str,
        input_text: &str,
        normalized_input: &str,
        max_results: usize,
    ) -> Result<Vec<(i64, String)>> {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(
            params![input_text, normalized_input, max_results as i64],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

impl Dictionary for SqliteDictionary {
    fn get_words(
        &self,
        input_text: &str,
        max_results: usize,
        matching_text: Option<&str>,
    ) -> Result<Vec<WordEntry>> {
        if input_text.chars().count() > self.max_lookup_length {
            return Ok(vec![]);
        }

        // Fast negative-cache check.
        {
            let cache = self.negative_cache.lock().unwrap();
            if cache.contains(input_text) {
                return Ok(vec![]);
            }
        }

        let conn = self.conn.lock().unwrap();
        let normalized_input = kana_to_hiragana(input_text);
        let text_for_match = matching_text.unwrap_or(input_text);
        let normalized_matching = kana_to_hiragana(text_for_match);

        // Try readings first, then kanji.
        let mut entry_rows = Self::fetch_entry_rows(
            &conn,
            "SELECT DISTINCT e.entry_id, e.ent_seq \
             FROM entries e \
             JOIN readings r ON e.entry_id = r.entry_id \
             WHERE r.reading_text = ?1 OR r.reading_text = ?2 \
             LIMIT ?3",
            input_text,
            &normalized_input,
            max_results,
        )?;

        if entry_rows.is_empty() {
            entry_rows = Self::fetch_entry_rows(
                &conn,
                "SELECT DISTINCT e.entry_id, e.ent_seq \
                 FROM entries e \
                 JOIN kanji k ON e.entry_id = k.entry_id \
                 WHERE k.kanji_text = ?1 OR k.kanji_text = ?2 \
                 LIMIT ?3",
                input_text,
                &normalized_input,
                max_results,
            )?;
        }

        if entry_rows.is_empty() {
            let mut cache = self.negative_cache.lock().unwrap();
            cache.insert(input_text.to_string());
            if cache.len() > 100_000 {
                // Trim to 80k entries
                let to_remove: Vec<String> = cache.iter().take(20_000).cloned().collect();
                for k in to_remove {
                    cache.remove(&k);
                }
            }
            return Ok(vec![]);
        }

        let ids: Vec<i64> = entry_rows.iter().map(|(id, _)| *id).collect();
        let entry_data: HashMap<i64, String> = entry_rows.into_iter().collect();
        Self::build_entries_batched(&conn, &ids, &entry_data, &normalized_matching)
    }

    fn exists(&self, word: &str) -> bool {
        if word.chars().count() > self.max_lookup_length {
            return false;
        }

        {
            let cache = self.negative_cache.lock().unwrap();
            if cache.contains(word) {
                return false;
            }
        }

        let conn = self.conn.lock().unwrap();
        let hiragana = kana_to_hiragana(word);

        let found: bool = conn
            .query_row(
                "SELECT EXISTS(\
                  SELECT 1 FROM readings WHERE reading_text = ?1 OR reading_text = ?2 \
                  UNION ALL \
                  SELECT 1 FROM kanji WHERE kanji_text = ?1 OR kanji_text = ?2 \
                  LIMIT 1)",
                params![word, hiragana],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !found {
            let mut cache = self.negative_cache.lock().unwrap();
            cache.insert(word.to_string());
        }

        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_database::build_database_from_xml;
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    /// Minimal JMDict XML with one entry: 食べる (v1, to eat).
    const MINI_JMDICT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<JMdict>
<entry>
<ent_seq>1549240</ent_seq>
<r_ele><reb>たべる</reb><re_pri>ichi1</re_pri></r_ele>
<k_ele><keb>食べる</keb><ke_pri>ichi1</ke_pri></k_ele>
<sense>
<pos>v1</pos>
<gloss>to eat</gloss>
</sense>
</entry>
<entry>
<ent_seq>1166770</ent_seq>
<r_ele><reb>よむ</reb></r_ele>
<k_ele><keb>読む</keb></k_ele>
<sense>
<pos>v5m</pos>
<gloss>to read</gloss>
</sense>
</entry>
</JMdict>"#;

    const TIE_JMDICT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<JMdict>
<entry>
<ent_seq>2000001</ent_seq>
<r_ele><reb>たく</reb></r_ele>
<k_ele><keb>焚く</keb></k_ele>
<sense>
<pos>v5k</pos>
<gloss>to burn</gloss>
</sense>
</entry>
<entry>
<ent_seq>2000002</ent_seq>
<r_ele><reb>たく</reb></r_ele>
<k_ele><keb>炊く</keb></k_ele>
<sense>
<pos>v5k</pos>
<gloss>to cook rice</gloss>
</sense>
</entry>
</JMdict>"#;

    fn make_test_dict() -> SqliteDictionary {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_owned();
        // Keep the file alive by leaking the tempfile handle for test duration
        std::mem::forget(tmp);
        let conn = Connection::open(&path).unwrap();
        build_database_from_xml(&conn, MINI_JMDICT).unwrap();
        SqliteDictionary::open(&path).unwrap()
    }

    fn make_tie_dict() -> SqliteDictionary {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_owned();
        std::mem::forget(tmp);
        let conn = Connection::open(&path).unwrap();
        build_database_from_xml(&conn, TIE_JMDICT).unwrap();
        SqliteDictionary::open(&path).unwrap()
    }

    #[test]
    fn test_exists_kana() {
        let d = make_test_dict();
        assert!(d.exists("たべる"));
        assert!(d.exists("よむ"));
        assert!(!d.exists("みる"));
    }

    #[test]
    fn test_exists_kanji() {
        let d = make_test_dict();
        assert!(d.exists("食べる"));
        assert!(d.exists("読む"));
    }

    #[test]
    fn test_get_words_by_kana() {
        let d = make_test_dict();
        let results = d.get_words("たべる", 10, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ent_seq, "1549240");
    }

    #[test]
    fn test_get_words_by_kanji() {
        let d = make_test_dict();
        let results = d.get_words("食べる", 10, None).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_get_words_pos_tags() {
        let d = make_test_dict();
        let results = d.get_words("たべる", 10, None).unwrap();
        let pos: Vec<&str> = results[0].senses[0]
            .pos_tags
            .iter()
            .map(|s| s.as_str())
            .collect();
        assert!(pos.contains(&"v1"), "expected v1 in {:?}", pos);
    }

    #[test]
    fn test_negative_cache() {
        let d = make_test_dict();
        assert!(!d.exists("nonexistent"));
        // Second call should hit cache
        assert!(!d.exists("nonexistent"));
    }

    #[test]
    fn test_get_words_preserves_sqlite_row_order_for_ties() {
        let d = make_tie_dict();

        let expected_order: Vec<String> = {
            let conn = d.conn.lock().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT e.ent_seq \
                     FROM entries e \
                     JOIN readings r ON e.entry_id = r.entry_id \
                     WHERE r.reading_text = ?1 \
                     LIMIT ?2",
                )
                .unwrap();
            let rows = stmt
                .query_map(params!["たく", 10_i64], |row| row.get::<_, String>(0))
                .unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        assert_eq!(
            expected_order.len(),
            2,
            "expected tie fixture to return 2 rows"
        );

        let first: Vec<String> = d
            .get_words("たく", 10, None)
            .unwrap()
            .into_iter()
            .map(|w| w.ent_seq)
            .collect();
        let second: Vec<String> = d
            .get_words("たく", 10, None)
            .unwrap()
            .into_iter()
            .map(|w| w.ent_seq)
            .collect();

        assert_eq!(first, expected_order);
        assert_eq!(second, expected_order);
    }
}
