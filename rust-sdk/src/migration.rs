use crate::db_value::SqlParam;
use crate::error::Result;
use crate::exec::SqlExec;
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

/// D1 用スキーマSQLを取得（content_hash/embedding を TEXT(hex) で保持）
fn get_d1_schema_sql() -> &'static str {
    include_str!("../schema.d1.sql")
}

/// マイグレーションを実行
pub fn migrate(conn: &Connection) -> Result<()> {
    // 外部キー制約を有効化（接続ごとに設定が必要）
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    let current_version = get_current_version(conn)?;
    let target_version = 6;

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
            let uuid_exists = get_table_info(conn, "media")?.iter().any(|c| c.name == "uuid");
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
            conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
            let migrate_result = (|| -> crate::error::Result<()> {
                let tx = conn.unchecked_transaction()?;
            tx.execute_batch("
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
            ")?;
                tx.execute("INSERT OR IGNORE INTO schema_version (version) VALUES (?)", [version])?;
                tx.commit()?;
                Ok(())
            })();
            let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");
            migrate_result?;
            Ok(())
        }
        5 => {
            // media_tags と media_attributes の外部キーに ON DELETE CASCADE を追加するためテーブルを再作成

            conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
            let migrate_result = (|| -> crate::error::Result<()> {
                let tx = conn.unchecked_transaction()?;
                tx.execute_batch("
                    -- media_tags を再作成（ON DELETE CASCADE 追加）
                CREATE TABLE media_tags_new (
                    media_id INTEGER NOT NULL,
                    tag_id INTEGER NOT NULL,
                    PRIMARY KEY (media_id, tag_id),
                    FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
                    FOREIGN KEY (tag_id) REFERENCES tags(id)
                );
                INSERT INTO media_tags_new SELECT media_id, tag_id FROM media_tags;
                DROP TABLE media_tags;
                ALTER TABLE media_tags_new RENAME TO media_tags;
                CREATE INDEX IF NOT EXISTS idx_media_tags_tag_id ON media_tags(tag_id);
                CREATE INDEX IF NOT EXISTS idx_media_tags_media_id ON media_tags(media_id);

                -- media_attributes を再作成（ON DELETE CASCADE 追加）
                CREATE TABLE media_attributes_new (
                    media_id INTEGER NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT,
                    value_type TEXT,
                    PRIMARY KEY (media_id, key),
                    FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
                );
                INSERT INTO media_attributes_new SELECT media_id, key, value, value_type FROM media_attributes;
                DROP TABLE media_attributes;
                ALTER TABLE media_attributes_new RENAME TO media_attributes;
                ")?;
                tx.execute("INSERT OR IGNORE INTO schema_version (version) VALUES (?)", [version])?;
                tx.commit()?;
                Ok(())
            })();
            let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");
            migrate_result?;
            Ok(())
        }
        6 => {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS media_hashes (
                    item_uuid       TEXT NOT NULL,
                    filename        TEXT NOT NULL DEFAULT '',
                    time_range      TEXT NOT NULL DEFAULT '',
                    content_hash    BLOB NOT NULL CHECK(length(content_hash) = 32),
                    alternative_of  TEXT,
                    embedding       BLOB,
                    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
                    PRIMARY KEY (item_uuid, filename, time_range),
                    FOREIGN KEY (item_uuid) REFERENCES media(uuid) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_media_hashes_content ON media_hashes(content_hash);

                CREATE TRIGGER IF NOT EXISTS update_media_hashes_timestamp
                AFTER UPDATE ON media_hashes
                FOR EACH ROW
                BEGIN
                    UPDATE media_hashes SET updated_at = datetime('now')
                    WHERE item_uuid = NEW.item_uuid AND filename = NEW.filename AND time_range = NEW.time_range;
                END;
                "
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

/// 外部キー制約違反の行（`PRAGMA foreign_key_check`）。空 = 整合性OK（observe gate 用・設計 §3.4）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FkViolation {
    /// 違反のあるテーブル名
    pub table: String,
    /// 違反行の rowid
    pub rowid: i64,
    /// 親テーブル名（該当なしは NULL）
    #[serde(default)]
    pub parent: Option<String>,
    /// FK 制約の id
    pub fkid: i64,
}

/// `PRAGMA integrity_check` の結果（observe gate 用・設計 §3.4）。
/// 正常時は "ok" 1行、異常時はエラー行が複数返る。全行を `; ` 区切りで結合して返す。
pub fn integrity_check(conn: &Connection) -> Result<String> {
    let mut stmt = conn.prepare("PRAGMA integrity_check")?;
    let rows: std::result::Result<Vec<String>, _> =
        stmt.query_map([], |row| row.get::<_, String>(0))?.collect();
    Ok(rows?.join("; "))
}

/// `PRAGMA foreign_key_check` の違反リスト（observe gate 用・設計 §3.4）。空 = 違反なし。
pub fn foreign_key_check(conn: &Connection) -> Result<Vec<FkViolation>> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
    let rows: std::result::Result<Vec<FkViolation>, _> = stmt
        .query_map([], |row| {
            Ok(FkViolation {
                table: row.get(0)?,
                rowid: row.get(1)?,
                parent: row.get(2)?,
                fkid: row.get(3)?,
            })
        })?
        .collect();
    Ok(rows?)
}

// ========== async バックエンド（SqlExec）用 ==========
//
// Local と D1 が共有。多文 SQL（schema.sql 全体・テーブル再構築・トリガ）は `execute_raw`
// でネイティブの複数文パーサに委譲し、`;` を含む `CREATE TRIGGER ... BEGIN ... END;` を
// 正しく扱う。version 挿入などパラメータ付き単文は `execute`。`PRAGMA table_info` は
// D1（REST）で非互換の可能性があるため `pragma_table_info` テーブル値関数に切替
// （D1 実証は Phase 3）。

/// 現在のスキーマバージョンを取得（テーブルが存在しない場合は0）
async fn get_current_version_async(exec: &dyn SqlExec) -> Result<i64> {
    let sql = "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1";
    match exec.query(sql, &[]).await {
        Ok(rows) => match rows.into_iter().next() {
            Some(row) => Ok(row.get_int("version")?),
            None => Ok(0),
        },
        Err(_) => Ok(0), // テーブル未存在等は 0 扱い（同期版と同じ挙動）
    }
}

/// async バックエンド経由でマイグレーションを実行
pub async fn migrate_async(exec: &dyn SqlExec) -> Result<()> {
    migrate_async_with(exec, get_schema_sql()).await
}

/// async バックエンド経由でマイグレーションを実行（D1 用: `schema.d1.sql` 使用）。
///
/// D1 REST は params を全て TEXT 扱いし BLOB 型カラムにバイナリを格納できないため、
/// `content_hash` / `embedding` を TEXT（hex）で持つ `schema.d1.sql` を使う。
/// 新規 database は version 0 → schema.d1.sql 一発で version 6 になる（段階マイグ不要）。
pub async fn migrate_async_d1(exec: &dyn SqlExec) -> Result<()> {
    migrate_async_with(exec, get_d1_schema_sql()).await
}

async fn migrate_async_with(exec: &dyn SqlExec, schema_sql: &'static str) -> Result<()> {
    // 外部キー制約を有効化（Local は接続ごと設定が必要。D1 は ON 固定・無害）
    exec.execute_raw("PRAGMA foreign_keys = ON;").await?;

    let current_version = get_current_version_async(exec).await?;
    let target_version = 6;

    if current_version == 0 {
        // 初回マイグレーション: 指定スキーマを実行
        exec.execute_raw(schema_sql).await?;
    } else {
        // 段階的マイグレーション（Local 既存 DB 向け。D1 は新規前提で未到達）
        for version in (current_version + 1)..=target_version {
            apply_migration_async(exec, version).await?;
        }
    }
    Ok(())
}

/// 特定バージョンへのマイグレーションを適用（async）
async fn apply_migration_async(exec: &dyn SqlExec, version: i64) -> Result<()> {
    match version {
        2 => {
            exec.execute_raw("ALTER TABLE media DROP COLUMN volume_number;")
                .await?;
            exec.execute(
                "INSERT OR IGNORE INTO schema_version (version) VALUES (?1)",
                &[SqlParam::Int(version)],
            )
            .await?;
            Ok(())
        }
        3 => {
            exec.execute_raw("ALTER TABLE media ADD COLUMN volume_number INTEGER;")
                .await?;
            exec.execute_raw(
                "UPDATE media
                 SET volume_number = CAST(volume_text AS INTEGER)
                 WHERE volume_text IS NOT NULL
                   AND volume_text <> ''
                   AND CAST(volume_text AS INTEGER) IS NOT NULL
                   AND TRIM(volume_text) = CAST(CAST(volume_text AS INTEGER) AS TEXT)",
            )
            .await?;
            exec.execute(
                "INSERT OR IGNORE INTO schema_version (version) VALUES (?1)",
                &[SqlParam::Int(version)],
            )
            .await?;
            Ok(())
        }
        4 => {
            // uuid列が存在しない場合のみ追加（冪等性）
            let uuid_exists = get_table_info_async(exec, "media")
                .await?
                .iter()
                .any(|c| c.name == "uuid");
            if !uuid_exists {
                exec.execute_raw("ALTER TABLE media ADD COLUMN uuid TEXT;")
                    .await?;
            }

            // uuid が NULL のレコードに UUID v4 を生成して設定
            let rows = exec.query("SELECT id FROM media WHERE uuid IS NULL", &[]).await?;
            for row in rows {
                let id = row.get_int("id")?;
                let uuid = uuid::Uuid::new_v4().to_string();
                exec.execute(
                    "UPDATE media SET uuid = ?1 WHERE id = ?2",
                    &[SqlParam::Text(uuid), SqlParam::Int(id)],
                )
                .await?;
            }

            // テーブル再構築（NOT NULL 制約付与）。
            // PRAGMA foreign_keys はトランザクション内で変更できないため、
            // OFF → BEGIN ... DDL ... version挿入 ... COMMIT → ON を1つのスクリプトで原子的実行。
            let ddl = "
                PRAGMA foreign_keys = OFF;
                BEGIN;
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

                INSERT OR IGNORE INTO schema_version (version) VALUES (4);
                COMMIT;
                PRAGMA foreign_keys = ON;
            ";
            exec.execute_raw(ddl).await?;
            Ok(())
        }
        5 => {
            // media_tags と media_attributes の外部キーに ON DELETE CASCADE を追加するため再作成。
            // v4 と同様、PRAGMA 切替 + BEGIN/COMMIT を1スクリプトで原子的実行。
            let ddl = "
                PRAGMA foreign_keys = OFF;
                BEGIN;
                CREATE TABLE media_tags_new (
                    media_id INTEGER NOT NULL,
                    tag_id INTEGER NOT NULL,
                    PRIMARY KEY (media_id, tag_id),
                    FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
                    FOREIGN KEY (tag_id) REFERENCES tags(id)
                );
                INSERT INTO media_tags_new SELECT media_id, tag_id FROM media_tags;
                DROP TABLE media_tags;
                ALTER TABLE media_tags_new RENAME TO media_tags;
                CREATE INDEX IF NOT EXISTS idx_media_tags_tag_id ON media_tags(tag_id);
                CREATE INDEX IF NOT EXISTS idx_media_tags_media_id ON media_tags(media_id);

                CREATE TABLE media_attributes_new (
                    media_id INTEGER NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT,
                    value_type TEXT,
                    PRIMARY KEY (media_id, key),
                    FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
                );
                INSERT INTO media_attributes_new SELECT media_id, key, value, value_type FROM media_attributes;
                DROP TABLE media_attributes;
                ALTER TABLE media_attributes_new RENAME TO media_attributes;

                INSERT OR IGNORE INTO schema_version (version) VALUES (5);
                COMMIT;
                PRAGMA foreign_keys = ON;
            ";
            exec.execute_raw(ddl).await?;
            Ok(())
        }
        6 => {
            let ddl = "
                CREATE TABLE IF NOT EXISTS media_hashes (
                    item_uuid       TEXT NOT NULL,
                    filename        TEXT NOT NULL DEFAULT '',
                    time_range      TEXT NOT NULL DEFAULT '',
                    content_hash    BLOB NOT NULL CHECK(length(content_hash) = 32),
                    alternative_of  TEXT,
                    embedding       BLOB,
                    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
                    PRIMARY KEY (item_uuid, filename, time_range),
                    FOREIGN KEY (item_uuid) REFERENCES media(uuid) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_media_hashes_content ON media_hashes(content_hash);

                CREATE TRIGGER IF NOT EXISTS update_media_hashes_timestamp
                AFTER UPDATE ON media_hashes
                FOR EACH ROW
                BEGIN
                    UPDATE media_hashes SET updated_at = datetime('now')
                    WHERE item_uuid = NEW.item_uuid AND filename = NEW.filename AND time_range = NEW.time_range;
                END;
            ";
            exec.execute_raw(ddl).await?;
            exec.execute(
                "INSERT OR IGNORE INTO schema_version (version) VALUES (?1)",
                &[SqlParam::Int(version)],
            )
            .await?;
            Ok(())
        }
        _ => Err(crate::error::KijukuError::Other(format!(
            "Unknown migration version: {}",
            version
        ))),
    }
}

/// async バックエンド経由で現在のスキーマバージョンを取得
pub async fn get_schema_version_async(exec: &dyn SqlExec) -> Result<i64> {
    let sql = "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1";
    let rows = exec.query(sql, &[]).await?;
    let row = rows.into_iter().next().ok_or_else(|| {
        crate::error::KijukuError::Other("schema_version table is empty".to_string())
    })?;
    Ok(row.get_int("version")?)
}

/// async バックエンド経由でテーブル一覧を取得
pub async fn get_tables_async(exec: &dyn SqlExec) -> Result<Vec<String>> {
    let sql = "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name";
    let rows = exec.query(sql, &[]).await?;
    rows.iter().map(|row| row.get_text("name")).collect()
}

/// async バックエンド経由で外部キー制約が有効かチェック
pub async fn is_foreign_keys_enabled_async(exec: &dyn SqlExec) -> Result<bool> {
    let rows = exec.query("PRAGMA foreign_keys", &[]).await?;
    let row = rows.into_iter().next().ok_or_else(|| {
        crate::error::KijukuError::Other("PRAGMA foreign_keys returned no row".to_string())
    })?;
    Ok(row.get_int("foreign_keys")? != 0)
}

/// async バックエンド経由で特定テーブルのカラム情報を取得。
///
/// `PRAGMA table_info(name)` ではなく `pragma_table_info` テーブル値関数を用いる
/// （D1 REST 互換のため・実証は Phase 3）。`SELECT *` で pragma のネイティブ列名
/// （cid/name/type/notnull/dflt_value/pk）をそのまま得る。
pub async fn get_table_info_async(
    exec: &dyn SqlExec,
    table_name: &str,
) -> Result<Vec<TableColumnInfo>> {
    let sql = "SELECT * FROM pragma_table_info(?1)";
    let params = vec![SqlParam::Text(table_name.to_string())];
    let rows = exec.query(sql, &params).await?;
    rows.iter()
        .map(|row| {
            Ok(TableColumnInfo {
                cid: row.get_int("cid")?,
                name: row.get_text("name")?,
                type_name: row.get_text("type")?,
                notnull: row.get_int("notnull")? != 0,
                dflt_value: row.get_opt_text("dflt_value")?,
                pk: row.get_int("pk")? != 0,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec_local::LocalExec;
    use parking_lot::ReentrantMutex;
    use std::sync::Arc;

    /// ベーススキーマは全マイグレーション履歴（1..=最新）を schema_version に記録する
    /// （3コピー統一・TASK-89）。新バージョン追加時にベーススキーマの INSERT 行を
    /// 更新しないとここで失敗する（トリップワイヤ）。
    #[test]
    fn test_base_schema_records_full_migration_history() {
        for sql in [get_schema_sql(), get_d1_schema_sql()] {
            let mut versions: Vec<u32> = sql
                .lines()
                .filter_map(|l| {
                    let l = l.trim();
                    l.strip_prefix("INSERT OR IGNORE INTO schema_version (version) VALUES (")?
                        .strip_suffix(");")?
                        .parse()
                        .ok()
                })
                .collect();
            versions.sort_unstable();
            versions.dedup();
            let expected: Vec<u32> = (1..=6).collect();
            assert_eq!(versions, expected, "base schema must record migrations 1..=6");
        }
    }

    fn setup_exec() -> LocalExec {
        let conn = Connection::open_in_memory().unwrap();
        LocalExec::new(Arc::new(ReentrantMutex::new(conn)))
    }

    #[tokio::test]
    async fn test_migrate_async() {
        let exec = setup_exec();
        migrate_async(&exec).await.unwrap();

        let tables = get_tables_async(&exec).await.unwrap();
        assert!(tables.contains(&"media".to_string()));
        assert!(tables.contains(&"tags".to_string()));
        assert!(tables.contains(&"media_tags".to_string()));
        assert!(tables.contains(&"media_hashes".to_string()));
    }

    #[tokio::test]
    async fn test_schema_version_async() {
        let exec = setup_exec();
        migrate_async(&exec).await.unwrap();

        let version = get_schema_version_async(&exec).await.unwrap();
        assert_eq!(version, 6);
    }

    #[tokio::test]
    async fn test_foreign_keys_enabled_after_migrate_async() {
        let exec = setup_exec();
        migrate_async(&exec).await.unwrap();

        assert!(is_foreign_keys_enabled_async(&exec).await.unwrap());
    }

    #[tokio::test]
    async fn test_get_table_info_async() {
        let exec = setup_exec();
        migrate_async(&exec).await.unwrap();

        let cols = get_table_info_async(&exec, "media").await.unwrap();
        let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"id"));
        assert!(names.contains(&"uuid"));
        assert!(names.contains(&"title"));
        // uuid は NOT NULL
        let uuid_col = cols.iter().find(|c| c.name == "uuid").unwrap();
        assert!(uuid_col.notnull);
    }

    #[tokio::test]
    async fn test_migrate_async_idempotent() {
        let exec = setup_exec();
        migrate_async(&exec).await.unwrap();
        // 2回目もエラーなく完了（schema_version=6 で no-op）
        migrate_async(&exec).await.unwrap();
        let version = get_schema_version_async(&exec).await.unwrap();
        assert_eq!(version, 6);
    }

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
        assert_eq!(version, 6);
    }

    #[test]
    fn test_foreign_keys() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        assert!(is_foreign_keys_enabled(&conn).unwrap());
    }

    #[test]
    fn test_foreign_keys_enabled_after_migrate() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        assert!(is_foreign_keys_enabled(&conn).unwrap());
    }

    #[test]
    fn test_cascade_delete_attributes() {
        use crate::attribute::{get_media_attributes, set_media_attribute};
        use crate::crud::{create_media, delete_media};
        use crate::types::{MediaInput, MediaType};

        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        let media = create_media(
            &conn,
            &MediaInput {
                title: "テスト".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
        )
        .unwrap();

        set_media_attribute(&conn, media.id, "key1", Some("val1"), None).unwrap();
        set_media_attribute(&conn, media.id, "key2", Some("val2"), None).unwrap();

        let attrs = get_media_attributes(&conn, media.id).unwrap();
        assert_eq!(attrs.len(), 2);

        delete_media(&conn, media.id).unwrap();

        let attrs_after = get_media_attributes(&conn, media.id).unwrap();
        assert_eq!(attrs_after.len(), 0, "media削除時にattributesもカスケード削除されること");
    }

    #[test]
    fn test_cascade_delete_tags() {
        use crate::crud::{create_media, delete_media};
        use crate::tag::{add_tag_to_media, create_tag, get_media_tags};
        use crate::types::{MediaInput, MediaType};

        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        let media = create_media(
            &conn,
            &MediaInput {
                title: "テスト".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
        )
        .unwrap();

        let tag = create_tag(&conn, "test-tag").unwrap();
        add_tag_to_media(&conn, media.id, tag.id).unwrap();

        let tags = get_media_tags(&conn, media.id).unwrap();
        assert_eq!(tags.len(), 1);

        delete_media(&conn, media.id).unwrap();

        let tags_after = get_media_tags(&conn, media.id).unwrap();
        assert_eq!(tags_after.len(), 0, "media削除時にmedia_tagsもカスケード削除されること");
    }
}
