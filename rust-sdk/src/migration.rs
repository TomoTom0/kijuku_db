use crate::error::Result;
use rusqlite::Connection;
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
    let target_version = 3;

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
        assert_eq!(version, 3);
    }

    #[test]
    fn test_foreign_keys() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        assert!(is_foreign_keys_enabled(&conn).unwrap());
    }
}
