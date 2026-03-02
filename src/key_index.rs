//! Compact binary key index for fast word existence checks.
//!
//! Format (`VRKIDX1`):
//! ```text
//! [0..8]    Magic: b"VRKIDX1\0"
//! [8..12]   Count: u32 little-endian — number of keys
//! [12..12+4*(count+1)]  Offsets: (count+1) × u32 LE — byte offsets into string data
//! [12+4*(count+1)..]    String data: raw UTF-8, keys concatenated in sorted order
//! ```
//!
//! Keys are stored sorted (lexicographic). `contains` does binary search in O(log n).
//! The index can be built from an iterator of strings and saved to / loaded from a file.

use std::io::{Read, Write};
use std::path::Path;

use crate::error::{Result, TentokuError};

const MAGIC: &[u8; 8] = b"VRKIDX1\0";

/// An in-memory key index built from a sorted list of UTF-8 strings.
pub struct KeyIndex {
    /// Number of keys.
    count: usize,
    /// Byte offsets into `data`; `offsets[i]` = start of key i, `offsets[count]` = end.
    offsets: Vec<u32>,
    /// Concatenated UTF-8 key bytes.
    data: Vec<u8>,
}

impl KeyIndex {
    /// Build a `KeyIndex` from an iterator of strings.
    /// Duplicates are removed; keys are stored sorted.
    pub fn build(keys: impl IntoIterator<Item = String>) -> Self {
        let mut sorted: Vec<String> = keys.into_iter().collect();
        sorted.sort_unstable();
        sorted.dedup();

        let mut offsets = Vec::with_capacity(sorted.len() + 1);
        let mut data: Vec<u8> = Vec::new();

        for key in &sorted {
            offsets.push(data.len() as u32);
            data.extend_from_slice(key.as_bytes());
        }
        offsets.push(data.len() as u32);

        Self {
            count: sorted.len(),
            offsets,
            data,
        }
    }

    /// Number of keys in the index.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Returns `true` if the index contains `key` (binary search, O(log n)).
    pub fn contains(&self, key: &str) -> bool {
        let needle = key.as_bytes();
        let mut lo = 0usize;
        let mut hi = self.count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let start = self.offsets[mid] as usize;
            let end = self.offsets[mid + 1] as usize;
            let mid_key = &self.data[start..end];
            match mid_key.cmp(needle) {
                std::cmp::Ordering::Equal => return true,
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        false
    }

    /// Serialise to any `Write` sink.
    pub fn write_to<W: Write>(&self, w: &mut W) -> Result<()> {
        w.write_all(MAGIC).map_err(TentokuError::Io)?;

        let count_bytes = (self.count as u32).to_le_bytes();
        w.write_all(&count_bytes).map_err(TentokuError::Io)?;

        for &off in &self.offsets {
            w.write_all(&off.to_le_bytes()).map_err(TentokuError::Io)?;
        }

        w.write_all(&self.data).map_err(TentokuError::Io)?;
        Ok(())
    }

    /// Deserialise from any `Read` source.
    pub fn read_from<R: Read>(r: &mut R) -> Result<Self> {
        let mut magic = [0u8; 8];
        r.read_exact(&mut magic).map_err(TentokuError::Io)?;
        if &magic != MAGIC {
            return Err(TentokuError::Index("bad magic bytes".into()));
        }

        let mut count_buf = [0u8; 4];
        r.read_exact(&mut count_buf).map_err(TentokuError::Io)?;
        let count = u32::from_le_bytes(count_buf) as usize;

        let offset_count = count + 1;
        let mut offsets = Vec::with_capacity(offset_count);
        for _ in 0..offset_count {
            let mut buf = [0u8; 4];
            r.read_exact(&mut buf).map_err(TentokuError::Io)?;
            offsets.push(u32::from_le_bytes(buf));
        }

        let data_len = *offsets.last().unwrap_or(&0) as usize;
        let mut data = vec![0u8; data_len];
        r.read_exact(&mut data).map_err(TentokuError::Io)?;

        Ok(Self {
            count,
            offsets,
            data,
        })
    }

    /// Save the index to a file.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(TentokuError::Io)?;
        }
        let mut f = std::fs::File::create(path).map_err(TentokuError::Io)?;
        self.write_to(&mut f)
    }

    /// Load the index from a file.
    pub fn load(path: &Path) -> Result<Self> {
        let mut f = std::fs::File::open(path).map_err(TentokuError::Io)?;
        Self::read_from(&mut f)
    }

    /// Build a `KeyIndex` from the readings and kanji spellings in a SQLite dictionary.
    ///
    /// Reads all `reading_text` and `kanji_text` values directly from the open connection.
    pub fn build_from_db(conn: &rusqlite::Connection) -> Result<Self> {
        let mut keys: Vec<String> = Vec::new();

        let mut stmt = conn
            .prepare("SELECT reading_text FROM readings")
            .map_err(TentokuError::Database)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(TentokuError::Database)?;
        for row in rows {
            keys.push(row.map_err(TentokuError::Database)?);
        }

        let mut stmt = conn
            .prepare("SELECT kanji_text FROM kanji")
            .map_err(TentokuError::Database)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(TentokuError::Database)?;
        for row in rows {
            keys.push(row.map_err(TentokuError::Database)?);
        }

        Ok(Self::build(keys.into_iter()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn sample_index() -> KeyIndex {
        KeyIndex::build(
            ["食べる", "読む", "高い", "たべる", "よむ", "たかい"]
                .iter()
                .map(|s| s.to_string()),
        )
    }

    #[test]
    fn test_contains_found() {
        let idx = sample_index();
        assert!(idx.contains("食べる"));
        assert!(idx.contains("読む"));
        assert!(idx.contains("たかい"));
    }

    #[test]
    fn test_contains_not_found() {
        let idx = sample_index();
        assert!(!idx.contains("みる"));
        assert!(!idx.contains(""));
        assert!(!idx.contains("食べた")); // inflected form not in index
    }

    #[test]
    fn test_dedup() {
        let idx = KeyIndex::build(["食べる", "食べる", "読む"].iter().map(|s| s.to_string()));
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn test_round_trip() {
        let original = sample_index();
        let mut buf: Vec<u8> = Vec::new();
        original.write_to(&mut buf).unwrap();

        let mut cursor = Cursor::new(&buf);
        let loaded = KeyIndex::read_from(&mut cursor).unwrap();

        assert_eq!(loaded.len(), original.len());
        assert!(loaded.contains("食べる"));
        assert!(loaded.contains("よむ"));
        assert!(!loaded.contains("みる"));
    }

    #[test]
    fn test_empty_index() {
        let idx = KeyIndex::build(std::iter::empty());
        assert_eq!(idx.len(), 0);
        assert!(!idx.contains("foo"));

        let mut buf: Vec<u8> = Vec::new();
        idx.write_to(&mut buf).unwrap();
        let mut cursor = Cursor::new(&buf);
        let loaded = KeyIndex::read_from(&mut cursor).unwrap();
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_build_from_db() {
        use crate::build_database::build_database_from_xml;
        use tempfile::NamedTempFile;

        const MINI: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<JMdict>
<entry>
<ent_seq>1549240</ent_seq>
<r_ele><reb>たべる</reb></r_ele>
<k_ele><keb>食べる</keb></k_ele>
<sense><pos>v1</pos><gloss>to eat</gloss></sense>
</entry>
</JMdict>"#;

        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_owned();
        std::mem::forget(tmp);
        let conn = rusqlite::Connection::open(&path).unwrap();
        build_database_from_xml(&conn, MINI).unwrap();

        let idx = KeyIndex::build_from_db(&conn).unwrap();
        assert!(idx.contains("たべる"));
        assert!(idx.contains("食べる"));
        assert!(!idx.contains("みる"));
    }
}
