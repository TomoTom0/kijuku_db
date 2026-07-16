//! media root 配下のファイル操作（cp/mv/sync）。
//!
//! すべて dry-run ファースト。上書き・削除で消えるファイルは trash（論理削除）経由。
//! `media_root` 配下への拘束と保護パスの拒否は [`crate::media_path`] が担う。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{KijukuError, Result};
use crate::media_path::{is_protected, resolve_existing_within_root, resolve_within_root};
use crate::trash::{move_to_trash, TrashOperation};

/// ファイル操作のオプション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOpOptions {
    /// 実際に変更を適用するか。`false`（既定）なら dry-run で計画のみ返す。
    pub apply: bool,
    /// DB の `Media.path` を追従させるか。既定 `false`（ゆるい方針）。
    /// ※ 当面はフラグを受け取るのみで DB 更新は未サポート（後続タスクで拡張）。
    pub update_db: bool,
}

impl Default for FileOpOptions {
    fn default() -> Self {
        Self {
            apply: false,
            update_db: false,
        }
    }
}

/// 個々の操作ステップ（dry-run の計画表示にも使う）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FileOpStep {
    Copy { from: String, to: String },
    Move { from: String, to: String },
    Trash { path: String, reason: String },
}

/// 操作の実行結果（dry-run 含む）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOpResult {
    pub steps: Vec<FileOpStep>,
    /// 実際にファイルシステムへ変更を加えたか
    pub applied: bool,
}

fn canonicalize_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

/// root からの相対文字列を返す（canonical 同士でなければエラー）。
fn rel_string(target: &Path, root: &Path) -> Result<String> {
    target
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|_| {
            KijukuError::Validation(format!(
                "target is not under media root: {} (root: {})",
                target.display(),
                root.display()
            ))
        })
}

/// 1件コピー（ファイル/ディレクトリ両対応）。
fn copy_recursive(src: &Path, dst: &Path) -> Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
        fs::copy(src, dst)?;
    }
    Ok(())
}

/// src と dst を解決し、保護パスでないことを検証する。
fn resolve_pair(
    root: &Path,
    src_rel: &str,
    dst_rel: &str,
    protected: &[PathBuf],
) -> Result<(PathBuf, PathBuf)> {
    let src = resolve_existing_within_root(root, src_rel)?;
    let dst = resolve_within_root(root, dst_rel)?;
    if is_protected(&src, protected) || is_protected(&dst, protected) {
        return Err(KijukuError::Validation(format!(
            "source or destination is a protected path (src: {}, dst: {})",
            src.display(),
            dst.display()
        )));
    }
    Ok((src, dst))
}

/// `src_rel` を `dst_rel` へ複製する。
///
/// `dst` が既存の場合は trash へ退避（overwrite）してから複製する。
pub fn media_cp(
    root: &Path,
    src_rel: &str,
    dst_rel: &str,
    opts: &FileOpOptions,
) -> Result<FileOpResult> {
    let root_c = canonicalize_root(root);
    let protected = vec![crate::media_path::trash_dir(&root_c)];
    let (src, dst) = resolve_pair(root, src_rel, dst_rel, &protected)?;

    let mut steps = Vec::new();
    if dst.exists() {
        steps.push(FileOpStep::Trash {
            path: rel_string(&dst, &root_c)?,
            reason: "media_cp overwrite".to_string(),
        });
    }
    steps.push(FileOpStep::Copy {
        from: rel_string(&src, &root_c)?,
        to: rel_string(&dst, &root_c)?,
    });

    if !opts.apply {
        return Ok(FileOpResult {
            steps,
            applied: false,
        });
    }

    if dst.exists() {
        move_to_trash(root, dst_rel, TrashOperation::Overwrite, Some("media_cp"))?;
    }
    copy_recursive(&src, &dst)?;
    Ok(FileOpResult { steps, applied: true })
}

/// `src_rel` を `dst_rel` へ移動する。
///
/// `dst` が既存の場合は trash へ退避（overwrite）してから移動する。
pub fn media_mv(
    root: &Path,
    src_rel: &str,
    dst_rel: &str,
    opts: &FileOpOptions,
) -> Result<FileOpResult> {
    let root_c = canonicalize_root(root);
    let protected = vec![crate::media_path::trash_dir(&root_c)];
    let (src, dst) = resolve_pair(root, src_rel, dst_rel, &protected)?;

    let mut steps = Vec::new();
    if dst.exists() {
        steps.push(FileOpStep::Trash {
            path: rel_string(&dst, &root_c)?,
            reason: "media_mv overwrite".to_string(),
        });
    }
    steps.push(FileOpStep::Move {
        from: rel_string(&src, &root_c)?,
        to: rel_string(&dst, &root_c)?,
    });

    if !opts.apply {
        return Ok(FileOpResult {
            steps,
            applied: false,
        });
    }

    if dst.exists() {
        move_to_trash(root, dst_rel, TrashOperation::Overwrite, Some("media_mv"))?;
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&src, &dst)?;
    Ok(FileOpResult { steps, applied: true })
}

/// ディレクトリ配下の相対ファイルパス一覧を収集する。
fn collect_files(base: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_files_into(base, base, &mut out)?;
    Ok(out)
}

fn collect_files_into(base: &Path, cur: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(cur)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            collect_files_into(base, &p, out)?;
        } else {
            out.push(p.strip_prefix(base).unwrap_or(&p).to_path_buf());
        }
    }
    Ok(())
}

/// `src_rel`（ディレクトリ）の内容を `dst_rel`（ディレクトリ）へ同期する。
///
/// **safe モードのみ**: `dst` にあって `src` に無いファイルは trash へ回す（生 `--delete` 相当だが、
/// すべて trash 経由で物理削除しない）。`src` にあって `dst` に無い、または内容が異なる
/// ファイルはコピーする。
pub fn media_sync(
    root: &Path,
    src_rel: &str,
    dst_rel: &str,
    opts: &FileOpOptions,
) -> Result<FileOpResult> {
    let root_c = canonicalize_root(root);
    let protected = vec![crate::media_path::trash_dir(&root_c)];
    let (src, dst) = resolve_pair(root, src_rel, dst_rel, &protected)?;

    if !src.is_dir() {
        return Err(KijukuError::Validation(format!(
            "media_sync source must be a directory: {}",
            src.display()
        )));
    }

    let src_files = collect_files(&src)?;
    let dst_files = if dst.is_dir() {
        collect_files(&dst)?
    } else {
        Vec::new()
    };
    let src_set: std::collections::HashSet<&PathBuf> = src_files.iter().collect();

    let mut steps = Vec::new();
    // 1. src に無い dst のファイルは trash
    for rel in &dst_files {
        if !src_set.contains(rel) {
            steps.push(FileOpStep::Trash {
                path: rel_string(&dst.join(rel), &root_c)?,
                reason: "media_sync extra".to_string(),
            });
        }
    }
    // 2. src の全ファイルを dst へコピー（差分: 内容が異なる場合のみ）。
    //    既存ファイルの上書きは旧内容を trash へ退避してから行う（不可逆防止）。
    for rel in &src_files {
        let to = dst.join(rel);
        let need_copy = match fs::read(&src.join(rel)) {
            Ok(from_bytes) => match fs::read(&to) {
                Ok(to_bytes) => from_bytes != to_bytes,
                Err(_) => true,
            },
            Err(_) => true,
        };
        if need_copy {
            if to.exists() {
                steps.push(FileOpStep::Trash {
                    path: rel_string(&to, &root_c)?,
                    reason: "media_sync update".to_string(),
                });
            }
            steps.push(FileOpStep::Copy {
                from: rel_string(&src.join(rel), &root_c)?,
                to: rel_string(&to, &root_c)?,
            });
        }
    }

    if !opts.apply {
        return Ok(FileOpResult {
            steps,
            applied: false,
        });
    }

    // 実行
    fs::create_dir_all(&dst)?;
    for rel in &dst_files {
        if !src_set.contains(rel) {
            let target_rel = format!(
                "{}/{}",
                dst_rel.trim_end_matches('/'),
                rel.to_string_lossy()
            );
            move_to_trash(root, &target_rel, TrashOperation::SyncExtra, Some("media_sync"))?;
        }
    }
    for rel in &src_files {
        let from = src.join(rel);
        let to = dst.join(rel);
        let differs = match (fs::read(&from), fs::read(&to)) {
            (Ok(a), Ok(b)) => a != b,
            _ => true,
        };
        if !differs {
            continue;
        }
        if to.exists() {
            let target_rel = format!(
                "{}/{}",
                dst_rel.trim_end_matches('/'),
                rel.to_string_lossy()
            );
            move_to_trash(root, &target_rel, TrashOperation::Overwrite, Some("media_sync"))?;
        }
        copy_recursive(&from, &to)?;
    }

    Ok(FileOpResult { steps, applied: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src/sub")).unwrap();
        fs::write(dir.path().join("src/a.txt"), "A").unwrap();
        fs::write(dir.path().join("src/sub/b.txt"), "B").unwrap();
        fs::write(dir.path().join("lonely.txt"), "L").unwrap();
        dir
    }

    #[test]
    fn cp_dry_run_does_not_write() {
        let dir = setup();
        let root = dir.path();
        let opts = FileOpOptions::default(); // apply=false
        let r = media_cp(root, "lonely.txt", "copied.txt", &opts).unwrap();
        assert!(!r.applied);
        assert!(!root.join("copied.txt").exists());
    }

    #[test]
    fn cp_apply_copies_file() {
        let dir = setup();
        let root = dir.path();
        let opts = FileOpOptions { apply: true, ..Default::default() };
        let r = media_cp(root, "lonely.txt", "copied.txt", &opts).unwrap();
        assert!(r.applied);
        assert!(root.join("copied.txt").exists());
        // src は残る（copy）
        assert!(root.join("lonely.txt").exists());
    }

    #[test]
    fn cp_overwrite_trashes_existing() {
        let dir = setup();
        let root = dir.path();
        fs::write(root.join("dst.txt"), "OLD").unwrap();
        let opts = FileOpOptions { apply: true, ..Default::default() };
        media_cp(root, "lonely.txt", "dst.txt", &opts).unwrap();
        // 新内容で上書き
        assert_eq!(fs::read_to_string(root.join("dst.txt")).unwrap(), "L");
        // 旧ファイルは trash へ
        assert_eq!(crate::trash::list_trash(root).unwrap().len(), 1);
    }

    #[test]
    fn mv_apply_moves_file() {
        let dir = setup();
        let root = dir.path();
        let opts = FileOpOptions { apply: true, ..Default::default() };
        media_mv(root, "lonely.txt", "moved.txt", &opts).unwrap();
        assert!(root.join("moved.txt").exists());
        assert!(!root.join("lonely.txt").exists());
    }

    #[test]
    fn mv_overwrite_trashes_existing() {
        let dir = setup();
        let root = dir.path();
        fs::write(root.join("dst.txt"), "OLD").unwrap();
        let opts = FileOpOptions { apply: true, ..Default::default() };
        media_mv(root, "lonely.txt", "dst.txt", &opts).unwrap();
        assert_eq!(fs::read_to_string(root.join("dst.txt")).unwrap(), "L");
        assert_eq!(crate::trash::list_trash(root).unwrap().len(), 1);
    }

    #[test]
    fn sync_safe_mode_trashes_extras_and_copies() {
        let dir = setup();
        let root = dir.path();
        // dst を準備: a.txt は古い内容、extra.txt は src に無い（trash 対象）
        fs::create_dir_all(root.join("dst")).unwrap();
        fs::write(root.join("dst/a.txt"), "OLD-A").unwrap();
        fs::write(root.join("dst/extra.txt"), "EXTRA").unwrap();

        let opts = FileOpOptions { apply: true, ..Default::default() };
        let r = media_sync(root, "src", "dst", &opts).unwrap();
        assert!(r.applied);

        // a.txt は src の内容で更新
        assert_eq!(fs::read_to_string(root.join("dst/a.txt")).unwrap(), "A");
        // extra.txt は trash 行き（消えていない、trash にある）
        assert!(!root.join("dst/extra.txt").exists());
        // extra.txt（余分）と a.txt（旧内容）の2件が trash へ
        assert_eq!(crate::trash::list_trash(root).unwrap().len(), 2);
        // sub/b.txt もコピーされる
        assert!(root.join("dst/sub/b.txt").exists());
    }

    #[test]
    fn rejects_protected_trash_as_target() {
        let dir = setup();
        let root = dir.path();
        // dst に .trash を指定すると保護パスで拒否
        let opts = FileOpOptions { apply: true, ..Default::default() };
        assert!(media_cp(root, "lonely.txt", ".trash", &opts).is_err());
    }

    #[test]
    fn rejects_outside_root() {
        let dir = setup();
        let root = dir.path();
        let opts = FileOpOptions::default();
        assert!(media_cp(root, "../escape.txt", "x", &opts).is_err());
        assert!(media_cp(root, "lonely.txt", "../escape.txt", &opts).is_err());
    }
}
