//! media root 配下のファイルパスを安全に解決するヘルパ。
//!
//! すべてのファイル操作（cp/mv/sync/upload/download 等）は、このモジュール経由で
//! パスを media root 配下に拘束する。これにより、`..`・絶対パス・シンボリックリンク
//! 経由での media root 外への脱出（パストラバーサル）を防ぐ。

use std::path::{Component, Path, PathBuf};

use crate::error::{KijukuError, Result};

/// パスを lexical に正規化する（`.` と `..` を解決）。
///
/// シンボリックリンクは解決しない（ファイルシステムアクセス不要）。そのため
/// 存在しないパス（新規作成先など）にも使用できる。ルート（`/`）より上には登らない。
pub fn normalize_lexical(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // 直前の通常コンポーネントを削除。ルート・プレフィックスだけのパスに対しては
                // PathBuf::pop は何もしないため、ルートより上には登らない。
                result.pop();
            }
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                result.push(component.as_os_str());
            }
        }
    }
    result
}

/// `target` が `root` 配下（または root 自身）であることを検査する。
fn ensure_within(target: &Path, root: &Path) -> Result<()> {
    if target.starts_with(root) {
        Ok(())
    } else {
        Err(KijukuError::Validation(format!(
            "path escapes media root: {} (root: {})",
            target.display(),
            root.display()
        )))
    }
}

/// `rel` を `root` 配下に解決し、脱出を検査する。
///
/// `rel` が相対パスなら `root` に結合する。絶対パスの場合はそのまま検査に回す
/// （結果が `root` 配下でなければ拒否）。lexical 正規化のみ行うため、新規パス
/// （存在しない作成先）にも使用できる。シンボリックリンク経由の脱出を防ぐには
/// [`resolve_existing_within_root`] を使うこと。
pub fn resolve_within_root(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel_path = Path::new(rel);
    let joined = if rel_path.is_absolute() {
        rel_path.to_path_buf()
    } else {
        root.join(rel_path)
    };
    let normalized = normalize_lexical(&joined);
    let root_normalized = normalize_lexical(root);
    ensure_within(&normalized, &root_normalized)?;
    Ok(normalized)
}

/// [`resolve_within_root`] に加え、既存パスのシンボリックリンクを実体まで解決して再検査する。
///
/// 操作対象が存在する場合（cp/mv の src など）に使う。存在しないパス（新規作成先など）は
/// lexical 解決結果をそのまま返す。
pub fn resolve_existing_within_root(root: &Path, rel: &str) -> Result<PathBuf> {
    let lexical = resolve_within_root(root, rel)?;
    if lexical.exists() {
        let canonical = lexical.canonicalize()?;
        // root が存在しない場合は lexical 正規化で代用（root 作成前など）。
        let root_canonical = match root.canonicalize() {
            Ok(c) => c,
            Err(_) => normalize_lexical(root),
        };
        ensure_within(&canonical, &root_canonical)?;
        Ok(canonical)
    } else {
        Ok(lexical)
    }
}

/// `target` が保護パス（操作禁止領域）に含まれるか判定する。
///
/// `protected` には正規化済みのパス（`.trash/`、DB ファイル、`backup_dir` など）を渡す。
/// `target` がいずれかの保護パスと同じ、またはその配下にある場合に true。
pub fn is_protected(target: &Path, protected: &[PathBuf]) -> bool {
    let target_norm = normalize_lexical(target);
    protected
        .iter()
        .any(|p| target_norm.starts_with(p) || p.starts_with(&target_norm))
}

/// media root 配下の trash（論理削除先）ディレクトリパスを返す。
pub fn trash_dir(root: &Path) -> PathBuf {
    root.join(".trash")
}

/// 設定から media root を取り出し、未設定ならエラーにする。
pub fn require_media_root(opt_root: &Option<String>) -> Result<PathBuf> {
    match opt_root {
        Some(root) => Ok(normalize_lexical(Path::new(root))),
        None => Err(KijukuError::Validation(
            "media root is not configured; set DBOptions.media_root to use file operations"
                .to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lexical_resolves_dots() {
        assert_eq!(
            normalize_lexical(Path::new("/a/./b/../c")),
            PathBuf::from("/a/c")
        );
        assert_eq!(
            normalize_lexical(Path::new("/a/b/../../c")),
            PathBuf::from("/c")
        );
        // ルートより上には登らない
        assert_eq!(
            normalize_lexical(Path::new("/../etc")),
            PathBuf::from("/etc")
        );
    }

    #[test]
    fn resolve_within_root_accepts_descendant() {
        let root = Path::new("/media");
        assert_eq!(
            resolve_within_root(root, "a/b.jpg").unwrap(),
            PathBuf::from("/media/a/b.jpg")
        );
    }

    #[test]
    fn resolve_within_root_rejects_traversal() {
        let root = Path::new("/media");
        assert!(resolve_within_root(root, "../escape").is_err());
        assert!(resolve_within_root(root, "a/../../../etc/passwd").is_err());
    }

    #[test]
    fn resolve_within_root_rejects_absolute_outside() {
        let root = Path::new("/media");
        assert!(resolve_within_root(root, "/etc/passwd").is_err());
    }

    #[test]
    fn resolve_within_root_rejects_sibling_prefix() {
        // /media-evil は /media の配下ではない（コンポーネント単位の starts_with で判定）
        let root = Path::new("/media");
        assert!(resolve_within_root(root, "../media-evil/x").is_err());
    }

    #[test]
    fn is_protected_detects_trash() {
        let root = Path::new("/media");
        let trash = trash_dir(root);
        assert!(is_protected(&trash, &[trash.clone()]));
        assert!(is_protected(&trash.join("x"), &[trash.clone()]));
        assert!(!is_protected(Path::new("/media/a.jpg"), &[trash]));
    }

    #[test]
    fn require_media_root_errors_when_unset() {
        assert!(require_media_root(&None).is_err());
        assert_eq!(
            require_media_root(&Some("/media".to_string())).unwrap(),
            PathBuf::from("/media")
        );
    }
}
