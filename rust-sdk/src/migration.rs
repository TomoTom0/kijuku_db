use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableColumnInfo {
    pub cid: i64,
    pub name: String,
    pub type_name: String,
    pub notnull: bool,
    pub dflt_value: Option<String>,
    pub pk: bool,
}

/// スキーマSQLを取得
fn get_schema_sql() -> &'static str {
    include_str!("../schema.sql")
}

/// マイグレーションを実行
pub fn migrate(conn: &Connection) -> Result<()> {
    let current_version = get_current_version(conn)?;
    let target_version = 4;

    if current_version == 0 {
        // 初回マイグレーション: schema.sqlを実行
        let schema_sql = get_schema_sql();
        conn.execute_batch(schema_sql)?;
    } else {
        // 段階的マイグレーション
        for version in (current_version + 1)..=target_version {
            apply_migration(conn, version)?;
        }
    }
    Ok(())
}

/// 現在のスキーマバージョンを取得（テーブルが存在しない場合は0）
fn get_current_version(conn: &Connection) -> Result<i64> {
    let version: std::result::Result<i64, _> = conn.query_row(
        "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
        [],
        |row| row.get(0),
    );

    match version {
        Ok(v) => Ok(v),
        Err(_) => Ok(0), // テーブルが存在しない場合は0
    }
}

/// 特定バージョンへのマイグレーションを適用
fn apply_migration(conn: &Connection, version: i64) -> Result<()> {
    match version {
        2 => {
            // volume_numberカラムを削除（バージョン2では削除していた）
            conn.execute_batch("ALTER TABLE media DROP COLUMN volume_number;")?;
            conn.execute("INSERT OR IGNORE INTO schema_version (version) VALUES (?)", [version])?;
            Ok(())
        }
        3 => {
            // volume_numberカラムを再追加（保存時自動計算方式）
            conn.execute_batch("ALTER TABLE media ADD COLUMN volume_number INTEGER;")?;

            // 既存データのvolume_numberを計算して設定
            conn.execute(
                "UPDATE media
                 SET volume_number = CAST(volume_text AS INTEGER)
                 WHERE volume_text IS NOT NULL
                   AND volume_text <> ''
                   AND CAST(volume_text AS INTEGER) IS NOT NULL
                   AND TRIM(volume_text) = CAST(CAST(volume_text AS INTEGER) AS TEXT)",
                [],
            )?;

            conn.execute("INSERT OR IGNORE INTO schema_version (version) VALUES (?)", [version])?;
            Ok(())
        }
        4 => {
            // uuid列をNOT NULL制約付きで追加するため、テーブルを再作成する

            // 1. uuid列が存在しない場合のみ追加（冪等性のため）
            let uuid_exists: bool = {
                let mut stmt = conn.prepare("PRAGMA table_info(media)")?;
                let exists = stmt
                    .query_map([], |row| row.get::<_, String>(1))?
                    .any(|name| name.map(|n| n == "uuid").unwrap_or(false));
                exists
            };
            if !uuid_exists {
                conn.execute_batch("ALTER TABLE media ADD COLUMN uuid TEXT;")?;
            }

            // 2. 既存データのうちuuidがNULLのレコードにUUID v4を生成して設定
            let ids: Vec<i64> = {
                let mut stmt = conn.prepare("SELECT id FROM media WHERE uuid IS NULL")?;
                let result = stmt.query_map([], |row| row.get(0))?
                    .collect::<std::result::Result<Vec<i64>, _>>()?;
                result
            };
            for id in ids {
                let uuid = uuid::Uuid::new_v4().to_string();
                conn.execute("UPDATE media SET uuid = ?1 WHERE id = ?2", params![uuid, id])?;
            }

            // 3. テーブルを再作成してNOT NULL制約を付与（SQLiteではALTER TABLEでNOT NULL追加不可）
            conn.execute_batch("
                PRAGMA foreign_keys = OFF;

                CREATE TABLE media_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    uuid TEXT NOT NULL UNIQUE,
                    title TEXT NOT NULL,
                    title_id TEXT,
                    path TEXT UNIQUE,
                    media_type TEXT NOT NULL CHECK(media_type IN ('comic', 'video', 'music')),
                    thumbnail_path TEXT,
                    artist TEXT,
                    artist_id TEXT,
                    description TEXT,
                    file_size INTEGER,
                    duration_sec INTEGER,
                    page_count INTEGER,
                    series TEXT,
                    volume_number INTEGER,
                    volume_text TEXT,
                    volume_title TEXT,
                    magazine TEXT,
                    magazine_id TEXT,
                    language TEXT,
                    source TEXT,
                    external_id TEXT,
                    artist_en TEXT,
                    title_en TEXT,
                    chapters TEXT,
                    extension TEXT,
                    flag_exist INTEGER DEFAULT 0,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                    title_pron TEXT,
                    artist_pron TEXT,
                    series_pron TEXT
                );

                INSERT INTO media_new SELECT
                    id, uuid, title, title_id, path, media_type, thumbnail_path,
                    artist, artist_id, description, file_size, duration_sec,
                    page_count, series, volume_number, volume_text, volume_title,
                    magazine, magazine_id, language, source, external_id,
                    artist_en, title_en, chapters, extension, flag_exist,
                    created_at, updated_at, title_pron, artist_pron, series_pron
                FROM media;

                DROP TABLE media;
                ALTER TABLE media_new RENAME TO media;

                CREATE INDEX IF NOT EXISTS idx_media_title_id ON media(title_id);
                CREATE INDEX IF NOT EXISTS idx_media_artist_id ON media(artist_id);
                CREATE INDEX IF NOT EXISTS idx_media_media_type ON media(media_type);
                CREATE INDEX IF NOT EXISTS idx_media_series ON media(series);
                CREATE INDEX IF NOT EXISTS idx_media_source ON media(source);
                CREATE INDEX IF NOT EXISTS idx_media_type_created ON media(media_type, created_at DESC);

                CREATE TRIGGER IF NOT EXISTS update_media_timestamp
                AFTER UPDATE ON media
                FOR EACH ROW
                BEGIN
                    UPDATE media SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
                END;

                PRAGMA foreign_keys = ON;
            ")?;

            conn.execute("INSERT OR IGNORE INTO schema_version (version) VALUES (?)", [version])?;
            Ok(())
        }
        _ => Err(crate::error::KijukuError::Other(format!(
            "Unknown migration version: {}",
            version
        ))),
    }
}

/// 現在のスキーマバージョンを取得
pub fn get_schema_version(conn: &Connection) -> Result<i64> {
    let version: i64 = conn.query_row(
        "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    Ok(version)
}

/// テーブル一覧を取得
pub fn get_tables(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;

    let tables = stmt
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<Vec<String>, _>>()?;

    Ok(tables)
}

/// 外部キー制約が有効かチェック
pub fn is_foreign_keys_enabled(conn: &Connection) -> Result<bool> {
    let enabled: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    Ok(enabled == 1)
}

/// 特定テーブルのカラム情報を取得
pub fn get_table_info(conn: &Connection, table_name: &str) -> Result<Vec<TableColumnInfo>> {
    let query = format!("PRAGMA table_info({})", table_name);
    let mut stmt = conn.prepare(&query)?;

    let columns = stmt
        .query_map([], |row| {
            Ok(TableColumnInfo {
                cid: row.get(0)?,
                name: row.get(1)?,
                type_name: row.get(2)?,
                notnull: row.get::<_, i64>(3)? == 1,
                dflt_value: row.get(4)?,
                pk: row.get::<_, i64>(5)? == 1,
            })
        })?
        .collect::<std::result::Result<Vec<TableColumnInfo>, _>>()?;

    Ok(columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        let tables = get_tables(&conn).unwrap();
        assert!(tables.contains(&"media".to_string()));
        assert!(tables.contains(&"tags".to_string()));
        assert!(tables.contains(&"media_tags".to_string()));
    }

    #[test]
    fn test_schema_version() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, 4);
    }

    #[test]
    fn test_foreign_keys() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        assert!(is_foreign_keys_enabled(&conn).unwrap());
    }
}
