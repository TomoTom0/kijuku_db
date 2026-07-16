//! trash（論理削除）方式: ファイル操作で削除・上書きされるファイルを
//! `media_root/.trash/` 配下へ退避し、物理削除を防ぐ。
//!
//! 物理削除を伴うのは [`purge_trash`] の apply のみ（かつ dry-run ファースト）。
//! 通常の削除・上書きはすべて trash への移動となり、[`restore_from_trash`] で復元できる。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{KijukuError, Result};
use crate::media_path::{resolve_existing_within_root, trash_dir};

/// trash エントリの ID（タイムスタンプ + カウンタ）
pub type TrashId = String;

/// trash エントリのメタ情報（`.trash/<id>/.meta.json` に保存）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashMeta {
    /// 元のパス（media root からの相対）
    pub original_path: String,
    /// trash へ移動した日時（RFC3339）
    pub trashed_at: String,
    /// どの操作で trash 行きになったか（delete / overwrite / sync_extra）
    pub operation: String,
    /// 操作の呼び出し元（media_mv / media_cp 等、任意）
    pub reason: Option<String>,
}

/// 一覧取得で返される trash エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashEntry {
    pub id: TrashId,
    pub meta: TrashMeta,
}

/// trash 行きの原因となった操作の種類
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrashOperation {
    /// 明示的な削除
    Delete,
    /// 上書きで消える旧ファイル
    Overwrite,
    /// ミラーリング（sync）で余分と判断されたファイル
    SyncExtra,
}

impl TrashOperation {
    fn as_str(&self) -> &'static str {
        match self {
            TrashOperation::Delete => "delete",
            TrashOperation::Overwrite => "overwrite",
            TrashOperation::SyncExtra => "sync_extra",
        }
    }
}

/// 一意な trash ID を生成する（ナノ秒タイムスタンプ + プロセス内カウンタ）。
fn generate_trash_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{nanos:020}-{c}")
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

/// JSON を一時ファイル経由でアトミックに書き込む（backup.rs のパターン踏襲）。
fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(value)
        .map_err(|e| KijukuError::Parse(format!("failed to serialize trash meta: {e}")))?;
    fs::write(&tmp, &data)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn canonicalize_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

/// `target_rel` を trash へ移動し、trash ID を返す。
///
/// target は media root 配下の既存パスで、かつ trash ディレクトリ自身でなければならない。
/// 存在しないパスはエラー。.trash/ 配下のパスは（再帰退避を避けるため）拒否する。
pub fn move_to_trash(
    root: &Path,
    target_rel: &str,
    operation: TrashOperation,
    reason: Option<&str>,
) -> Result<TrashId> {
    let root_c = canonicalize_root(root);
    let trash = trash_dir(&root_c);
    let target = resolve_existing_within_root(root, target_rel)?;

    // trash ディレクトリ配下は再帰退避を避けて拒否
    if target.starts_with(&trash) {
        return Err(KijukuError::Validation(format!(
            "cannot trash a path inside the trash directory: {}",
            target.display()
        )));
    }
    if !target.exists() {
        return Err(KijukuError::NotFound(format!(
            "trash target not found: {}",
            target.display()
        )));
    }

    let rel = target.strip_prefix(&root_c).map_err(|_| {
        KijukuError::Validation(format!(
            "target is not under media root: {} (root: {})",
            target.display(),
            root_c.display()
        ))
    })?;

    let id = generate_trash_id();
    let entry_dir = trash.join(&id);
    fs::create_dir_all(&entry_dir)?;

    let dest = entry_dir.join(rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&target, &dest)?;

    let meta = TrashMeta {
        original_path: rel.to_string_lossy().to_string(),
        trashed_at: now_iso(),
        operation: operation.as_str().to_string(),
        reason: reason.map(|s| s.to_string()),
    };
    atomic_write_json(&entry_dir.join(".meta.json"), &meta)?;

    Ok(id)
}

/// trash 内の全エントリを一覧する（trash が無ければ空）。
pub fn list_trash(root: &Path) -> Result<Vec<TrashEntry>> {
    let root_c = canonicalize_root(root);
    let trash = trash_dir(&root_c);
    if !trash.exists() {
        return Ok(vec![]);
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&trash)? {
        let entry = entry?;
        let meta_path = entry.path().join(".meta.json");
        if !meta_path.exists() {
            continue;
        }
        let data = fs::read(&meta_path)?;
        let meta: TrashMeta = serde_json::from_slice(&data).map_err(|e| {
            KijukuError::Parse(format!("failed to parse trash meta: {e}"))
        })?;
        entries.push(TrashEntry {
            id: entry.file_name().to_string_lossy().to_string(),
            meta,
        });
    }
    Ok(entries)
}

/// trash エントリ `id` を元の位置へ復元し、復元先のパスを返す。
///
/// 元の位置に既にファイルが存在する場合は**上書きせずエラー**にする（安全停止）。
pub fn restore_from_trash(root: &Path, id: &str) -> Result<PathBuf> {
    let root_c = canonicalize_root(root);
    let trash = trash_dir(&root_c);
    let entry_dir = trash.join(id);

    let meta_path = entry_dir.join(".meta.json");
    if !meta_path.exists() {
        return Err(KijukuError::NotFound(format!(
            "trash entry not found: {id}"
        )));
    }
    let data = fs::read(&meta_path)?;
    let meta: TrashMeta = serde_json::from_slice(&data).map_err(|e| {
        KijukuError::Parse(format!("failed to parse trash meta: {e}"))
    })?;

    let dest = root_c.join(&meta.original_path);
    if dest.exists() {
        return Err(KijukuError::Validation(format!(
            "restore destination already exists (not overwritten): {}",
            dest.display()
        )));
    }

    let src = entry_dir.join(&meta.original_path);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&src, &dest)?;

    // エントリディレクトリ（.meta.json と空ディレクトリ）を掃除
    let _ = fs::remove_dir_all(&entry_dir);
    Ok(dest)
}

/// trash を物理削除する。dry_run の場合は対象 ID の一覧を返すだけで削除しない。
///
/// `ids` が Some ならその ID のみ、None なら全エントリを対象とする。
/// これが trash からファイルを完全に消す唯一の経路である。
pub fn purge_trash(
    root: &Path,
    ids: Option<&[String]>,
    dry_run: bool,
) -> Result<Vec<String>> {
    let root_c = canonicalize_root(root);
    let trash = trash_dir(&root_c);

    let to_purge: Vec<String> = match ids {
        Some(ids) => ids.to_vec(),
        None => {
            if !trash.exists() {
                return Ok(vec![]);
            }
            fs::read_dir(&trash)?
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        }
    };

    if dry_run {
        return Ok(to_purge);
    }

    for id in &to_purge {
        let p = trash.join(id);
        if p.is_dir() {
            fs::remove_dir_all(&p)?;
        } else if p.exists() {
            fs::remove_file(&p)?;
        }
    }
    Ok(to_purge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_root() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();
        fs::write(dir.path().join("a/b/c.jpg"), "content").unwrap();
        fs::write(dir.path().join("top.txt"), "top").unwrap();
        dir
    }

    #[test]
    fn move_list_restore_roundtrip() {
        let dir = setup_root();
        let root = dir.path();

        let id = move_to_trash(root, "a/b/c.jpg", TrashOperation::Delete, Some("test")).unwrap();
        // 元の場所からは消え、trash に退避されている
        assert!(!root.join("a/b/c.jpg").exists());
        assert!(trash_dir(root).join(&id).join("a/b/c.jpg").exists());

        let entries = list_trash(root).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, id);
        assert_eq!(entries[0].meta.original_path, "a/b/c.jpg");

        let restored = restore_from_trash(root, &id).unwrap();
        assert_eq!(restored, root.join("a/b/c.jpg"));
        assert!(root.join("a/b/c.jpg").exists());
        // 復元後は trash エントリが掃除される
        assert!(list_trash(root).unwrap().is_empty());
    }

    #[test]
    fn purge_dry_run_then_apply() {
        let dir = setup_root();
        let root = dir.path();

        let id = move_to_trash(root, "top.txt", TrashOperation::Delete, None).unwrap();

        let planned = purge_trash(root, None, true).unwrap();
        assert_eq!(planned, vec![id.clone()]);
        // dry-run では実体は残る
        assert!(trash_dir(root).join(&id).exists());

        let applied = purge_trash(root, None, false).unwrap();
        assert_eq!(applied, vec![id.clone()]);
        assert!(!trash_dir(root).join(&id).exists());
    }

    #[test]
    fn move_rejects_trash_itself() {
        let dir = setup_root();
        let root = dir.path();
        // trash を作ってから、trash 配下のパスを trash しようとすると拒否される
        let _id = move_to_trash(root, "top.txt", TrashOperation::Delete, None).unwrap();
        assert!(move_to_trash(root, ".trash", TrashOperation::Delete, None).is_err());
    }

    #[test]
    fn move_rejects_outside_root() {
        let dir = setup_root();
        let root = dir.path();
        assert!(move_to_trash(root, "../escape.txt", TrashOperation::Delete, None).is_err());
        assert!(move_to_trash(root, "/etc/passwd", TrashOperation::Delete, None).is_err());
    }

    #[test]
    fn move_errors_when_not_found() {
        let dir = setup_root();
        let root = dir.path();
        assert!(move_to_trash(root, "nope.txt", TrashOperation::Delete, None).is_err());
    }

    #[test]
    fn restore_errors_on_collision() {
        let dir = setup_root();
        let root = dir.path();
        let id = move_to_trash(root, "top.txt", TrashOperation::Delete, None).unwrap();
        // 元位置に別ファイルを復活させる
        fs::write(root.join("top.txt"), "new").unwrap();
        // 復元は衝突してエラー（上書きしない）
        assert!(restore_from_trash(root, &id).is_err());
        // trash エントリは残る
        assert_eq!(list_trash(root).unwrap().len(), 1);
    }

    #[test]
    fn restore_errors_on_unknown_id() {
        let dir = setup_root();
        let root = dir.path();
        assert!(restore_from_trash(root, "does-not-exist").is_err());
    }

    #[test]
    fn trash_operation_serde_roundtrip() {
        // snake_case で直列化される（JSON-RPC プロトコルと整合）
        assert_eq!(
            serde_json::to_string(&TrashOperation::Delete).unwrap(),
            "\"delete\""
        );
        assert_eq!(
            serde_json::to_string(&TrashOperation::Overwrite).unwrap(),
            "\"overwrite\""
        );
        assert_eq!(
            serde_json::to_string(&TrashOperation::SyncExtra).unwrap(),
            "\"sync_extra\""
        );
        // 往復
        for op in [
            TrashOperation::Delete,
            TrashOperation::Overwrite,
            TrashOperation::SyncExtra,
        ] {
            let v = serde_json::to_value(op).unwrap();
            let back: TrashOperation = serde_json::from_value(v.clone()).unwrap();
            assert_eq!(serde_json::to_value(back).unwrap(), v);
        }
    }

    #[test]
    fn trash_entry_serializes() {
        let dir = setup_root();
        let root = dir.path();
        let id = move_to_trash(root, "top.txt", TrashOperation::Delete, None).unwrap();
        let entries = list_trash(root).unwrap();
        // TrashEntry が JSON に直列化できる（JSON-RPC レスポンス用）
        let v = serde_json::to_value(&entries[0]).unwrap();
        assert_eq!(v["id"], id);
        assert_eq!(v["meta"]["original_path"], "top.txt");
    }
}
