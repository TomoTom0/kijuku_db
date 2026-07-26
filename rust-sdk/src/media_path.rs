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

/// 書込先（cp/mv/sync の dst など、存在しない新規パス）を `root` 配下に解決し、
/// 既存のシンボリックリンク祖先を実体まで解決して再検査する。
///
/// `resolve_within_root` は lexical 正規化のみのため、`root/link -> /outside` のような
/// 既存 symlink 配下の新規パス（例: `link/new.db`）を通るとファイル操作が symlink を辿って
/// root 外へ書き込んでしまう。これを防ぐため、dst が存在すれば [`resolve_existing_within_root`]
/// と同等に canonicalize し、存在しなければ**最近傍の既存祖先**を canonicalize して `root` 配下か
/// 再検査する（中間の symlink が root 外へ脱出する場合、その symlink を含む最深既存コンポーネントが
/// canonicalize されて検査で弾かれる）。非存在の `root`（作成前）は lexical 結果を返す。
pub fn resolve_destination_within_root(root: &Path, rel: &str) -> Result<PathBuf> {
    let lexical = resolve_within_root(root, rel)?;
    let root_canonical = match root.canonicalize() {
        Ok(c) => c,
        Err(_) => return Ok(lexical),
    };
    if lexical.exists() {
        let canonical = lexical.canonicalize()?;
        ensure_within(&canonical, &root_canonical)?;
        return Ok(canonical);
    }
    // 非存在の dst: 最近傍の既存祖先まで遡り canonicalize して root 配下を再検査する。
    // lexical は root 配下（lexical 正規化済み）なので、遡れば必ず root（既存）に到達する。
    let mut ancestor = lexical.as_path();
    while !ancestor.exists() {
        match ancestor.parent() {
            Some(p) if p != ancestor => ancestor = p,
            _ => break,
        }
    }
    if ancestor.exists() {
        let canonical_ancestor = ancestor.canonicalize()?;
        ensure_within(&canonical_ancestor, &root_canonical)?;
    }
    Ok(lexical)
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

    #[test]
    fn resolve_destination_accepts_new_descendant() {
        // 非存在の新規パスは lexical 結果を返す（root が非存在でも同様）。
        let root = Path::new("/media");
        assert_eq!(
            resolve_destination_within_root(root, "new/sub/x.jpg").unwrap(),
            PathBuf::from("/media/new/sub/x.jpg")
        );
        // トラバーサルは従来通り拒否
        assert!(resolve_destination_within_root(root, "../escape").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_destination_rejects_symlink_escape() {
        use std::fs;
        use std::os::unix::fs::symlink;
        use tempfile::TempDir;

        let media = TempDir::new().unwrap();
        let root = media.path();
        // root 配下に root 外への symlink を仕掛ける: root/link -> outside
        let outside = TempDir::new().unwrap();
        symlink(outside.path(), root.join("link")).unwrap();

        // dst = link/new.db は lexical には root 配下だが、symlink 先は root 外。
        // 最近傍既存祖先（root/link）を canonicalize すると outside になり拒否される。
        assert!(resolve_destination_within_root(root, "link/new.db").is_err());
        // 多段でも: root/a/b/b-link -> outside の dst a/b-link/x も拒否される。
        fs::create_dir_all(root.join("a/b")).unwrap();
        symlink(outside.path(), root.join("a/b/b-link")).unwrap();
        assert!(resolve_destination_within_root(root, "a/b/b-link/x").is_err());

        // 正常な（root 内を指す）新規パスは受理される
        assert!(resolve_destination_within_root(root, "plain/new.db").is_ok());
    }
}
