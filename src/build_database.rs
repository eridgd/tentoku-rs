use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::{params, Connection};

use crate::error::{Result, TentokuError};

const JMDICT_URL: &str = "https://www.edrdg.org/pub/Nihongo/JMdict_e.gz";

/// Download JMDict_e.gz, decompress it, and build a SQLite database at `db_path`.
/// If `xml_gz_bytes` is provided the download step is skipped (used in tests).
pub fn build_database(db_path: &str, xml_gz_bytes: Option<Vec<u8>>) -> Result<()> {
    let gz_bytes = match xml_gz_bytes {
        Some(b) => b,
        None => {
            eprintln!("Downloading JMDict from {JMDICT_URL} ...");
            let mut resp = ureq::get(JMDICT_URL)
                .call()
                .map_err(|e| TentokuError::Build(e.to_string()))?;
            resp.body_mut()
                .read_to_vec()
                .map_err(|e| TentokuError::Build(e.to_string()))?
        }
    };

    eprintln!("Decompressing...");
    let mut decoder = GzDecoder::new(gz_bytes.as_slice());
    let mut xml_str = String::new();
    decoder
        .read_to_string(&mut xml_str)
        .map_err(TentokuError::Io)?;

    if let Some(parent) = Path::new(db_path).parent() {
        std::fs::create_dir_all(parent).map_err(TentokuError::Io)?;
    }

    let conn = Connection::open(db_path)?;
    build_database_from_xml(&conn, &xml_str)?;
    optimize_database(&conn)?;
    eprintln!("Database built at {db_path}");
    Ok(())
}

/// Create schema + import XML into an open connection (used in tests too).
pub fn build_database_from_xml(conn: &Connection, xml: &str) -> Result<()> {
    create_schema(conn)?;
    parse_jmdict(conn, xml)?;
    Ok(())
}

fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = OFF;
         PRAGMA cache_size = 10000;
         PRAGMA temp_store = MEMORY;

         CREATE TABLE IF NOT EXISTS entries (
             entry_id INTEGER PRIMARY KEY,
             ent_seq TEXT UNIQUE NOT NULL
         );
         CREATE TABLE IF NOT EXISTS kanji (
             kanji_id INTEGER PRIMARY KEY AUTOINCREMENT,
             entry_id INTEGER NOT NULL,
             kanji_text TEXT NOT NULL,
             priority TEXT,
             info TEXT,
             FOREIGN KEY (entry_id) REFERENCES entries(entry_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS readings (
             reading_id INTEGER PRIMARY KEY AUTOINCREMENT,
             entry_id INTEGER NOT NULL,
             reading_text TEXT NOT NULL,
             no_kanji INTEGER DEFAULT 0,
             priority TEXT,
             info TEXT,
             FOREIGN KEY (entry_id) REFERENCES entries(entry_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS reading_restrictions (
             restriction_id INTEGER PRIMARY KEY AUTOINCREMENT,
             reading_id INTEGER NOT NULL,
             kanji_text TEXT NOT NULL,
             FOREIGN KEY (reading_id) REFERENCES readings(reading_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS senses (
             sense_id INTEGER PRIMARY KEY AUTOINCREMENT,
             entry_id INTEGER NOT NULL,
             sense_index INTEGER NOT NULL,
             info TEXT,
             FOREIGN KEY (entry_id) REFERENCES entries(entry_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS sense_pos (
             sense_pos_id INTEGER PRIMARY KEY AUTOINCREMENT,
             sense_id INTEGER NOT NULL,
             pos TEXT NOT NULL,
             FOREIGN KEY (sense_id) REFERENCES senses(sense_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS sense_field (
             sense_field_id INTEGER PRIMARY KEY AUTOINCREMENT,
             sense_id INTEGER NOT NULL,
             field TEXT NOT NULL,
             FOREIGN KEY (sense_id) REFERENCES senses(sense_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS sense_misc (
             sense_misc_id INTEGER PRIMARY KEY AUTOINCREMENT,
             sense_id INTEGER NOT NULL,
             misc TEXT NOT NULL,
             FOREIGN KEY (sense_id) REFERENCES senses(sense_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS sense_dial (
             sense_dial_id INTEGER PRIMARY KEY AUTOINCREMENT,
             sense_id INTEGER NOT NULL,
             dial TEXT NOT NULL,
             FOREIGN KEY (sense_id) REFERENCES senses(sense_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS sense_stagk (
             sense_stagk_id INTEGER PRIMARY KEY AUTOINCREMENT,
             sense_id INTEGER NOT NULL,
             kanji_text TEXT NOT NULL,
             FOREIGN KEY (sense_id) REFERENCES senses(sense_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS sense_stagr (
             sense_stagr_id INTEGER PRIMARY KEY AUTOINCREMENT,
             sense_id INTEGER NOT NULL,
             reading_text TEXT NOT NULL,
             FOREIGN KEY (sense_id) REFERENCES senses(sense_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS glosses (
             gloss_id INTEGER PRIMARY KEY AUTOINCREMENT,
             sense_id INTEGER NOT NULL,
             gloss_text TEXT NOT NULL,
             lang TEXT DEFAULT 'eng',
             g_type TEXT,
             FOREIGN KEY (sense_id) REFERENCES senses(sense_id) ON DELETE CASCADE
         );
         CREATE VIRTUAL TABLE IF NOT EXISTS glosses_fts USING fts5(
             gloss_text,
             sense_id UNINDEXED,
             content='glosses',
             content_rowid='gloss_id'
         );

         CREATE INDEX IF NOT EXISTS idx_kanji_text ON kanji(kanji_text);
         CREATE INDEX IF NOT EXISTS idx_kanji_entry ON kanji(entry_id);
         CREATE INDEX IF NOT EXISTS idx_reading_text ON readings(reading_text);
         CREATE INDEX IF NOT EXISTS idx_reading_entry ON readings(entry_id);
         CREATE INDEX IF NOT EXISTS idx_gloss_text ON glosses(gloss_text);
         CREATE INDEX IF NOT EXISTS idx_gloss_sense ON glosses(sense_id);
         CREATE INDEX IF NOT EXISTS idx_sense_entry ON senses(entry_id);
         CREATE INDEX IF NOT EXISTS idx_sense_pos_sense ON sense_pos(sense_id);
         CREATE INDEX IF NOT EXISTS idx_sense_field_sense ON sense_field(sense_id);
         CREATE INDEX IF NOT EXISTS idx_sense_misc_sense ON sense_misc(sense_id);
         CREATE INDEX IF NOT EXISTS idx_sense_dial_sense ON sense_dial(sense_id);
         CREATE INDEX IF NOT EXISTS idx_sense_stagk_sense ON sense_stagk(sense_id);
         CREATE INDEX IF NOT EXISTS idx_sense_stagr_sense ON sense_stagr(sense_id);
         CREATE INDEX IF NOT EXISTS idx_reading_restrictions_reading ON reading_restrictions(reading_id);",
    )?;
    Ok(())
}

fn optimize_database(conn: &Connection) -> Result<()> {
    conn.execute_batch("ANALYZE; VACUUM; REINDEX;")?;
    Ok(())
}

fn xml_name(e: &quick_xml::events::BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn xml_end_name(e: &quick_xml::events::BytesEnd<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn xml_empty_name(e: &quick_xml::events::BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn decode_text_event(
    e: &quick_xml::events::BytesText<'_>,
    entities: &HashMap<String, String>,
) -> String {
    // `quick_xml` does not resolve custom DTD entities (e.g. `&v1;`) used by
    // JMdict metadata tags. If unescape fails, use the raw token and resolve
    // named entities from the XML doctype declaration.
    let raw = String::from_utf8_lossy(e.as_ref()).into_owned();
    let text = e.unescape().map(|c| c.into_owned()).unwrap_or(raw);
    decode_jmdict_entity_token(&text, entities)
}

fn decode_jmdict_entity_token(text: &str, entities: &HashMap<String, String>) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('&') && trimmed.ends_with(';') && trimmed.len() > 2 {
        let entity = &trimmed[1..trimmed.len() - 1];
        if entity
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return entities
                .get(entity)
                .cloned()
                .unwrap_or_else(|| entity.to_string());
        }
    }
    text.to_string()
}

fn decode_xml_entities(text: &str) -> String {
    // Decode core predefined XML entities used in DTD replacement strings.
    text.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn extract_jmdict_entities(xml: &str) -> HashMap<String, String> {
    let mut entities = HashMap::new();

    for line in xml.lines() {
        let line = line.trim();
        if !line.starts_with("<!ENTITY ") {
            continue;
        }

        let rest = &line["<!ENTITY ".len()..];
        let Some(name_end) = rest.find(char::is_whitespace) else {
            continue;
        };
        let name = &rest[..name_end];
        let value_part = rest[name_end..].trim_start();

        let Some(quote) = value_part.chars().next() else {
            continue;
        };
        if quote != '"' && quote != '\'' {
            continue;
        }

        let quoted = &value_part[1..];
        let Some(end) = quoted.find(quote) else {
            continue;
        };

        let raw_value = &quoted[..end];
        entities.insert(name.to_string(), decode_xml_entities(raw_value));
    }

    entities
}

fn parse_jmdict(conn: &Connection, xml: &str) -> Result<()> {
    let entities = extract_jmdict_entities(xml);
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    // Current entry state
    let mut entry_id: i64 = 0;
    let mut sense_index: usize = 0;
    let mut prev_pos: Vec<String> = Vec::new();

    // Kanji element
    let mut ke_text = String::new();
    let mut ke_priority: Vec<String> = Vec::new();
    let mut ke_info: Vec<String> = Vec::new();

    // Reading element
    let mut re_text = String::new();
    let mut re_no_kanji = false;
    let mut re_priority: Vec<String> = Vec::new();
    let mut re_info: Vec<String> = Vec::new();
    let mut re_restr: Vec<String> = Vec::new();

    // Sense element
    let mut se_id: i64 = 0;
    let mut se_pos: Vec<String> = Vec::new();
    let mut se_field: Vec<String> = Vec::new();
    let mut se_misc: Vec<String> = Vec::new();
    let mut se_dial: Vec<String> = Vec::new();
    let mut se_stagk: Vec<String> = Vec::new();
    let mut se_stagr: Vec<String> = Vec::new();
    let mut se_info: Vec<String> = Vec::new();

    // Gloss
    let mut ge_text = String::new();
    let mut ge_lang = String::new();
    let mut ge_type: Option<String> = None;

    // Active text-capture tag
    let mut active_tag = String::new();

    conn.execute_batch("BEGIN;")?;
    let mut entry_count = 0u32;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = xml_name(e);
                match name.as_str() {
                    "entry" => {
                        sense_index = 0;
                        prev_pos.clear();
                    }
                    "k_ele" => {
                        ke_text.clear();
                        ke_priority.clear();
                        ke_info.clear();
                    }
                    "r_ele" => {
                        re_text.clear();
                        re_no_kanji = false;
                        re_priority.clear();
                        re_info.clear();
                        re_restr.clear();
                    }
                    "sense" => {
                        conn.execute(
                            "INSERT INTO senses (entry_id, sense_index) VALUES (?1, ?2)",
                            params![entry_id, sense_index],
                        )?;
                        se_id = conn.last_insert_rowid();
                        se_pos.clear();
                        se_field.clear();
                        se_misc.clear();
                        se_dial.clear();
                        se_stagk.clear();
                        se_stagr.clear();
                        se_info.clear();
                        sense_index += 1;
                    }
                    "gloss" => {
                        ge_text.clear();
                        ge_lang = "eng".to_string();
                        ge_type = None;
                        for attr in e.attributes().flatten() {
                            let k = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                            let v = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
                            match k.as_str() {
                                "xml:lang" | "lang" => ge_lang = v,
                                "g_type" => ge_type = Some(v),
                                _ => {}
                            }
                        }
                        active_tag = "gloss".to_string();
                        // Don't fall through to active_tag assignment below
                        buf.clear();
                        continue;
                    }
                    t @ ("ent_seq" | "keb" | "ke_pri" | "ke_inf" | "reb" | "re_pri" | "re_inf"
                    | "re_restr" | "pos" | "field" | "misc" | "dial" | "stagk" | "stagr"
                    | "s_inf") => {
                        active_tag = t.to_string();
                    }
                    _ => {
                        active_tag.clear();
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = xml_empty_name(e);
                if name == "re_nokanji" {
                    re_no_kanji = true;
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = decode_text_event(e, &entities);
                match active_tag.as_str() {
                    "ent_seq" => {
                        conn.execute(
                            "INSERT OR IGNORE INTO entries (ent_seq) VALUES (?1)",
                            params![text],
                        )?;
                        entry_id = conn.last_insert_rowid();
                        if entry_id == 0 {
                            entry_id = conn.query_row(
                                "SELECT entry_id FROM entries WHERE ent_seq = ?1",
                                params![text],
                                |r| r.get(0),
                            )?;
                        }
                        entry_count += 1;
                        if entry_count % 10_000 == 0 {
                            eprintln!("  {entry_count} entries...");
                        }
                    }
                    "keb" => ke_text = text,
                    "ke_pri" => ke_priority.push(text),
                    "ke_inf" => ke_info.push(text),
                    "reb" => re_text = text,
                    "re_pri" => re_priority.push(text),
                    "re_inf" => re_info.push(text),
                    "re_restr" => re_restr.push(text),
                    "pos" => se_pos.push(text),
                    "field" => se_field.push(text),
                    "misc" => se_misc.push(text),
                    "dial" => se_dial.push(text),
                    "stagk" => se_stagk.push(text),
                    "stagr" => se_stagr.push(text),
                    "s_inf" => se_info.push(text),
                    "gloss" => ge_text = text,
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = xml_end_name(e);
                match name.as_str() {
                    "ent_seq" | "keb" | "ke_pri" | "ke_inf" | "reb" | "re_pri" | "re_inf"
                    | "re_restr" | "pos" | "field" | "misc" | "dial" | "stagk" | "stagr"
                    | "s_inf" | "gloss" => {
                        // "gloss" end: flush gloss row
                        if name == "gloss" {
                            conn.execute(
                                "INSERT INTO glosses (sense_id, gloss_text, lang, g_type) \
                                 VALUES (?1, ?2, ?3, ?4)",
                                params![se_id, ge_text, ge_lang, ge_type],
                            )?;
                            let gloss_id = conn.last_insert_rowid();
                            conn.execute(
                                "INSERT INTO glosses_fts (rowid, gloss_text, sense_id) \
                                 VALUES (?1, ?2, ?3)",
                                params![gloss_id, ge_text, se_id],
                            )?;
                        }
                        active_tag.clear();
                    }
                    "k_ele" => {
                        let pri = ke_priority.first().cloned();
                        let inf = ke_info.join(",");
                        conn.execute(
                            "INSERT INTO kanji (entry_id, kanji_text, priority, info) \
                             VALUES (?1, ?2, ?3, ?4)",
                            params![
                                entry_id,
                                ke_text,
                                pri,
                                if inf.is_empty() { None } else { Some(inf) }
                            ],
                        )?;
                    }
                    "r_ele" => {
                        let pri = re_priority.first().cloned();
                        let inf = re_info.join(",");
                        conn.execute(
                            "INSERT INTO readings (entry_id, reading_text, no_kanji, \
                             priority, info) VALUES (?1, ?2, ?3, ?4, ?5)",
                            params![
                                entry_id,
                                re_text,
                                re_no_kanji as i32,
                                pri,
                                if inf.is_empty() { None } else { Some(inf) }
                            ],
                        )?;
                        let rid = conn.last_insert_rowid();
                        for restr in &re_restr {
                            conn.execute(
                                "INSERT INTO reading_restrictions (reading_id, kanji_text) \
                                 VALUES (?1, ?2)",
                                params![rid, restr],
                            )?;
                        }
                    }
                    "sense" => {
                        let sense_info = if se_info.is_empty() {
                            None
                        } else {
                            Some(se_info.join("; "))
                        };
                        conn.execute(
                            "UPDATE senses SET info = ?1 WHERE sense_id = ?2",
                            params![sense_info, se_id],
                        )?;

                        let pos_to_use = if se_pos.is_empty() {
                            prev_pos.clone()
                        } else {
                            prev_pos = se_pos.clone();
                            se_pos.clone()
                        };
                        for p in &pos_to_use {
                            conn.execute(
                                "INSERT INTO sense_pos (sense_id, pos) VALUES (?1, ?2)",
                                params![se_id, p],
                            )?;
                        }
                        for f in &se_field {
                            conn.execute(
                                "INSERT INTO sense_field (sense_id, field) VALUES (?1, ?2)",
                                params![se_id, f],
                            )?;
                        }
                        for m in &se_misc {
                            conn.execute(
                                "INSERT INTO sense_misc (sense_id, misc) VALUES (?1, ?2)",
                                params![se_id, m],
                            )?;
                        }
                        for d in &se_dial {
                            conn.execute(
                                "INSERT INTO sense_dial (sense_id, dial) VALUES (?1, ?2)",
                                params![se_id, d],
                            )?;
                        }
                        for k in &se_stagk {
                            conn.execute(
                                "INSERT INTO sense_stagk (sense_id, kanji_text) VALUES (?1, ?2)",
                                params![se_id, k],
                            )?;
                        }
                        for r in &se_stagr {
                            conn.execute(
                                "INSERT INTO sense_stagr (sense_id, reading_text) VALUES (?1, ?2)",
                                params![se_id, r],
                            )?;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(TentokuError::Build(format!("XML error: {e}"))),
            _ => {}
        }
        buf.clear();
    }

    conn.execute_batch("COMMIT;")?;
    eprintln!("Imported {entry_count} entries.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML_WITH_ENTITY_POS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE JMdict [
<!ENTITY v1 "Ichidan verb">
<!ENTITY sK "search-only kanji form">
]>
<JMdict>
<entry>
<ent_seq>1549240</ent_seq>
<r_ele><reb>たべる</reb><re_pri>ichi1</re_pri><re_pri>news1</re_pri></r_ele>
<k_ele><keb>食べる</keb><ke_inf>&sK;</ke_inf></k_ele>
<sense>
<pos>&v1;</pos>
<stagk>食べる</stagk>
<stagr>たべる</stagr>
<s_inf>first note</s_inf>
<s_inf>second note</s_inf>
<gloss>to eat</gloss>
</sense>
</entry>
</JMdict>"#;

    #[test]
    fn test_build_database_preserves_entity_based_pos_tags() {
        let conn = Connection::open_in_memory().unwrap();
        build_database_from_xml(&conn, XML_WITH_ENTITY_POS).unwrap();

        let pos: String = conn
            .query_row("SELECT pos FROM sense_pos LIMIT 1", [], |r| r.get(0))
            .unwrap();

        assert_eq!(pos, "Ichidan verb");
    }

    #[test]
    fn test_build_database_expands_entity_info_and_persists_sense_metadata() {
        let conn = Connection::open_in_memory().unwrap();
        build_database_from_xml(&conn, XML_WITH_ENTITY_POS).unwrap();

        let kanji_info: String = conn
            .query_row("SELECT info FROM kanji LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let sense_info: String = conn
            .query_row("SELECT info FROM senses LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let stagk: String = conn
            .query_row("SELECT kanji_text FROM sense_stagk LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        let stagr: String = conn
            .query_row("SELECT reading_text FROM sense_stagr LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();

        assert_eq!(kanji_info, "search-only kanji form");
        assert_eq!(sense_info, "first note; second note");
        assert_eq!(stagk, "食べる");
        assert_eq!(stagr, "たべる");
    }

    #[test]
    fn test_build_database_uses_first_priority_value_like_python_builder() {
        let conn = Connection::open_in_memory().unwrap();
        build_database_from_xml(&conn, XML_WITH_ENTITY_POS).unwrap();

        let reading_priority: String = conn
            .query_row("SELECT priority FROM readings LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(reading_priority, "ichi1");
    }
}
