//! stg（ステージングDB）の排他ロックと sync 元 revision 記録（設計 §15-11・TASK-55）。
//!
//! - `StgLock`: `<stg>.lock` の排他 advisory ロック（RAII・fs2）。書込系（sync / stg 編集
//!   セッション）が取得し、複数セッション/LLM の stg 同時編集を防止する。
//! - `StgMeta` / `ProdRevision`: sync 時に prod の指紋を `<stg>.meta.json` に原子書き込みし、
//!   observe が prod の drift（sync 後更新）を検出して stale な promote をブロックする。

use crate::error::{KijukuError, Result};
use crate::migration;
use chrono::{DateTime, Utc};
use fs2::FileExt;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

// ========== revision 指紋 ==========

/// prod の revision 指紋（schema_version + 各テーブル件数/max-id・設計 §15-11）。
///
/// sync 元 prod のスナップショットを安価に表現する。完全な内容ハッシュではなく、
/// 行レベルの drift 検出用の「警告信号」であり、権威ある整合性判定は observe の
/// 実 diff + gate が担う。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProdRevision {
    pub schema_version: i64,
    pub media_count: i64,
    pub media_max_id: i64,
    pub tags_count: i64,
    pub tags_max_id: i64,
    pub media_tags_count: i64,
    pub media_attributes_count: i64,
    pub media_hashes_count: i64,
}

/// 接続から prod revision 指紋を算出（5テーブルの集計 + schema_version）。
pub fn compute_prod_revision(conn: &Connection) -> Result<ProdRevision> {
    let count_max = |table: &str, id_col: &str| -> Result<(i64, i64)> {
        let sql = format!(
            "SELECT COUNT(*), COALESCE(MAX({id}), 0) FROM {t}",
            id = id_col,
            t = table
        );
        let row: (i64, i64) = conn.query_row(&sql, [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(row)
    };
    let count_only = |table: &str| -> Result<i64> {
        Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {t}", t = table), [], |r| {
            r.get(0)
        })?)
    };

    let (media_count, media_max_id) = count_max("media", "id")?;
    let (tags_count, tags_max_id) = count_max("tags", "id")?;
    let media_tags_count = count_only("media_tags")?;
    let media_attributes_count = count_only("media_attributes")?;
    let media_hashes_count = count_only("media_hashes")?;
    let schema_version = migration::get_schema_version(conn)?;

    Ok(ProdRevision {
        schema_version,
        media_count,
        media_max_id,
        tags_count,
        tags_max_id,
        media_tags_count,
        media_attributes_count,
        media_hashes_count,
    })
}

// ========== メタデータサイドカー ==========

/// sync 元 prod 情報（revision 記録・設計 §15-11）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedFrom {
    pub prod_path: String,
    pub revision: ProdRevision,
    pub synced_at: DateTime<Utc>,
}

/// stg メタデータ（`<stg>.meta.json`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StgMeta {
    pub synced_from: SyncedFrom,
}

/// `<stg_db_path>.meta.json` のパス。
pub fn meta_path(stg_db_path: &Path) -> PathBuf {
    with_suffix(stg_db_path, ".meta.json")
}

/// `<stg_db_path>.lock` のパス。
pub fn lock_path(stg_db_path: &Path) -> PathBuf {
    with_suffix(stg_db_path, ".lock")
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// stg メタを読込（ファイル不在時は None・後方互換）。
pub fn read_stg_meta(stg_db_path: &Path) -> Result<Option<StgMeta>> {
    let path = meta_path(stg_db_path);
    match fs::read(&path) {
        Ok(bytes) => {
            let meta: StgMeta = serde_json::from_slice(&bytes)
                .map_err(|e| KijukuError::Parse(format!("stg meta parse error: {e}")))?;
            Ok(Some(meta))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// stg メタを原子書き込み（tmp → rename・TASK-27 の backup-meta パターンと同一）。
pub fn write_stg_meta(stg_db_path: &Path, meta: &StgMeta) -> Result<()> {
    let path = meta_path(stg_db_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(meta)
        .map_err(|e| KijukuError::Parse(format!("stg meta serialize error: {e}")))?;
    let tmp = with_suffix(stg_db_path, ".meta.json.tmp");
    fs::write(&tmp, &json)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

// ========== 排他ロック ==========

/// stg 排他ロック（advisory・設計 §15-11）。`<stg>.lock` の排他ロックを保持し、
/// `Drop` で解放（OS がプロセス終了/クラッシュ時にも自動解放・stale なし）。
///
/// ロックファイル自体は削除しない（unlink 競合で別プロセスのロックが無効化されるのを防ぐ）。
pub struct StgLock {
    file: File,
    path: PathBuf,
}

impl std::fmt::Debug for StgLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StgLock").field("path", &self.path).finish()
    }
}

impl StgLock {
    /// stg の排他ロックを取得（既に取得済みなら `StgBusy`）。
    pub fn acquire(stg_db_path: &Path) -> Result<Self> {
        let path = lock_path(stg_db_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)?;
        file.try_lock_exclusive().map_err(|_| KijukuError::StgBusy {
            stg_path: stg_db_path.display().to_string(),
            holder_pid: None,
        })?;
        Ok(Self { file, path })
    }

    /// ロックファイルのパス。
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StgLock {
    fn drop(&mut self) {
        // ベストエフォートで解放（失敗は無視・OS もプロセス終了時に解放する）。
        // ファイルは削除せず残す（unlink 競合回避）。
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_stg_lock_acquire_then_reject_then_release() {
        let dir = TempDir::new().unwrap();
        let stg = dir.path().join("kijuku.stg.db");

        let lock1 = StgLock::acquire(&stg).expect("1回目の取得は成功");
        // 同一プロセスでも別 fd なので排他（flock は open file description 単位）
        let err = StgLock::acquire(&stg).unwrap_err();
        assert!(matches!(err, KijukuError::StgBusy { .. }), "2回目は StgBusy: {err:?}");

        drop(lock1);
        // 解放後は再取得可能
        let _lock2 = StgLock::acquire(&stg).expect("解放後の再取得は成功");
    }

    #[test]
    fn test_stg_meta_write_read_roundtrip() {
        let dir = TempDir::new().unwrap();
        let stg = dir.path().join("kijuku.stg.db");
        assert!(read_stg_meta(&stg).expect("read").is_none(), "不在時は None");

        let meta = StgMeta {
            synced_from: SyncedFrom {
                prod_path: "/tmp/prod.db".to_string(),
                revision: ProdRevision {
                    schema_version: 6,
                    media_count: 3,
                    media_max_id: 3,
                    tags_count: 0,
                    tags_max_id: 0,
                    media_tags_count: 0,
                    media_attributes_count: 0,
                    media_hashes_count: 0,
                },
                synced_at: Utc::now(),
            },
        };
        write_stg_meta(&stg, &meta).expect("write");
        let read = read_stg_meta(&stg).expect("read after write").expect("Some");
        assert_eq!(read.synced_from.revision.media_count, 3);
        assert_eq!(read.synced_from.prod_path, "/tmp/prod.db");
    }
}
