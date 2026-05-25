use crate::crud::update_media;
use crate::error::Result;
use crate::search::find_media;
use crate::types::{Media, MediaFilter, MediaType, MediaUpdateInput, QueryOptions};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// ffmpegでフレームを抽出するタイムスタンプ
const VIDEO_THUMBNAIL_TIMESTAMP: &str = "00:00:01";

/// pathからcontentの親ディレクトリ（サムネイル基準ディレクトリ）を取得する
fn get_parent_before_content(path_str: &str) -> Option<PathBuf> {
    let path = Path::new(path_str);
    let components: Vec<Component> = path.components().collect();

    let last_content_idx = components.iter().rposition(|c| {
        matches!(c, Component::Normal(s) if s.to_str() == Some("content"))
    })?;

    let has_normal = components[..last_content_idx]
        .iter()
        .any(|c| matches!(c, Component::Normal(_)));
    if !has_normal {
        return None;
    }

    Some(components[..last_content_idx].iter().collect())
}

/// pathからサムネイルの期待パスを計算する
///
/// pathに含まれる最後の"content"コンポーネントを見つけ、
/// その親ディレクトリの cover/{uuid}.jpg を返す。
/// contentが含まれない場合はNoneを返す。
pub fn resolve_thumbnail_path(path_str: &str, uuid: &str) -> Option<String> {
    let parent = get_parent_before_content(path_str)?;
    let thumbnail = parent.join("cover").join(format!("{}.jpg", uuid));
    Some(thumbnail.to_string_lossy().into_owned())
}

/// thumbnail_pathとexpected_pathを比較する
///
/// 先頭文字で絶対パスか相対パスかを判定し、相対パスの場合は
/// media_pathの基準ディレクトリを用いて絶対パスに解決してから比較する。
fn thumbnail_path_matches(current: &str, expected: &str, media_path: &str) -> bool {
    if current == expected {
        return true;
    }
    // currentが相対パス（先頭が/ではない）の場合、基準ディレクトリから解決する
    if !current.starts_with('/') {
        if let Some(parent) = get_parent_before_content(media_path) {
            let resolved = parent.join(current);
            return resolved.to_string_lossy() == expected;
        }
    }
    false
}

/// video/musicの実際のファイルパスを解決する
///
/// pathにextが含まれていればそのまま、含まれていなければ
/// {path}.{ext} または {uuid}.* 形式で代替を探す。
fn resolve_media_file_path(path_str: &str, ext: &str, uuid: &str) -> Option<String> {
    let path = Path::new(path_str);
    if path.exists() {
        return Some(path_str.to_string());
    }
    // path.{ext} を試す
    let with_ext = format!("{}.{}", path_str, ext);
    if Path::new(&with_ext).exists() {
        return Some(with_ext);
    }
    // 同じディレクトリ内で {uuid}.* 形式のファイルを探す
    let parent = path.parent()?;
    let Ok(entries) = std::fs::read_dir(parent) else {
        return None;
    };
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let fname_str = fname.to_string_lossy();
        if let Some(dot_pos) = fname_str.rfind('.') {
            let stem = &fname_str[..dot_pos];
            let file_ext = &fname_str[dot_pos + 1..];
            if stem == uuid && !file_ext.is_empty() {
                return Some(parent.join(&*fname_str).to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// サムネイルチェック・更新のオプション
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThumbnailOptions {
    /// trueの場合、DBを更新せず結果を出力のみ（update-thumbnailのみ有効）
    #[serde(default)]
    pub dry_run: bool,
    /// trueの場合、既にサムネイルが存在しても再生成する（update-thumbnailのみ有効）
    #[serde(default)]
    pub force: bool,
}

/// check_thumbnailの1件のステータス
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CheckThumbnailStatus {
    /// thumbnail_pathが設定されており、ファイルも存在する
    Ok,
    /// サムネイルが不要なメディア（pathなし、contentなし）
    Skipped { reason: String },
    /// thumbnail_pathが未設定（または期待パスと不一致）
    Missing,
    /// thumbnail_pathは設定されているがファイルが存在しない
    FileNotFound,
}

/// check_thumbnailの1件の結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckThumbnailItemResult {
    pub id: i64,
    pub uuid: String,
    pub title: String,
    pub expected_path: Option<String>,
    pub current_path: Option<String>,
    pub status: CheckThumbnailStatus,
}

/// check_thumbnail全体の結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckThumbnailResult {
    pub total: usize,
    pub ok: usize,
    pub missing: usize,
    pub file_not_found: usize,
    pub skipped: usize,
    /// 全件の詳細結果
    pub details: Vec<CheckThumbnailItemResult>,
}

/// update_thumbnailの1件のステータス
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UpdateThumbnailStatus {
    /// サムネイルを生成した（dry_runの場合は生成予定）
    Generated,
    /// 既にサムネイルが存在するためスキップ
    AlreadyExists,
    /// 条件を満たさないためスキップ
    Skipped { reason: String },
    /// 生成中にエラーが発生
    Error { message: String },
}

/// update_thumbnailの1件の結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateThumbnailItemResult {
    pub id: i64,
    pub uuid: String,
    pub title: String,
    pub thumbnail_path: Option<String>,
    pub status: UpdateThumbnailStatus,
}

/// update_thumbnail全体の結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateThumbnailResult {
    pub total: usize,
    pub generated: usize,
    pub already_exists: usize,
    pub skipped: usize,
    pub errors: usize,
    /// 全件の詳細結果
    pub details: Vec<UpdateThumbnailItemResult>,
}

/// 1件のメディアのサムネイル状態をチェックする
fn check_media_thumbnail(media: &Media) -> CheckThumbnailItemResult {
    let expected_path = media
        .path
        .as_deref()
        .and_then(|p| resolve_thumbnail_path(p, &media.uuid));

    let status = if media.path.is_none() {
        CheckThumbnailStatus::Skipped { reason: "pathが未設定".to_string() }
    } else if expected_path.is_none() {
        CheckThumbnailStatus::Skipped { reason: "pathにcontentが含まれていない".to_string() }
    } else {
        let expected = expected_path.as_deref().unwrap();
        let media_path = media.path.as_deref().unwrap();
        match &media.thumbnail_path {
            None => CheckThumbnailStatus::Missing,
            Some(current)
                if !thumbnail_path_matches(current, expected, media_path) =>
                CheckThumbnailStatus::Missing,
            Some(_) => {
                if Path::new(expected).exists() {
                    CheckThumbnailStatus::Ok
                } else {
                    CheckThumbnailStatus::FileNotFound
                }
            }
        }
    };

    CheckThumbnailItemResult {
        id: media.id,
        uuid: media.uuid.clone(),
        title: media.title.clone(),
        expected_path,
        current_path: media.thumbnail_path.clone(),
        status,
    }
}

/// フィルタで絞り込んだメディアのサムネイル状態をチェックする
pub fn check_thumbnail(
    conn: &Connection,
    filter: &MediaFilter,
    query_options: Option<&QueryOptions>,
) -> Result<CheckThumbnailResult> {
    let media_list = find_media(conn, filter, query_options)?;
    let total = media_list.len();
    let mut ok = 0usize;
    let mut missing = 0usize;
    let mut file_not_found = 0usize;
    let mut skipped = 0usize;
    let mut details: Vec<CheckThumbnailItemResult> = Vec::new();

    for media in &media_list {
        let item = check_media_thumbnail(media);
        match &item.status {
            CheckThumbnailStatus::Ok => ok += 1,
            CheckThumbnailStatus::Missing => missing += 1,
            CheckThumbnailStatus::FileNotFound => file_not_found += 1,
            CheckThumbnailStatus::Skipped { .. } => skipped += 1,
        }
        details.push(item);
    }

    Ok(CheckThumbnailResult { total, ok, missing, file_not_found, skipped, details })
}

fn build_item_result(
    media: &Media,
    status: UpdateThumbnailStatus,
    thumbnail_path: Option<String>,
) -> UpdateThumbnailItemResult {
    UpdateThumbnailItemResult {
        id: media.id,
        uuid: media.uuid.clone(),
        title: media.title.clone(),
        thumbnail_path,
        status,
    }
}

/// 1件のメディアのサムネイルを生成する
fn update_media_thumbnail(
    conn: &Connection,
    media: &Media,
    options: &ThumbnailOptions,
) -> Result<UpdateThumbnailItemResult> {
    // pathが未設定の場合はスキップ
    let path_str = match &media.path {
        None => {
            return Ok(build_item_result(
                media,
                UpdateThumbnailStatus::Skipped { reason: "pathが未設定".to_string() },
                None,
            ));
        }
        Some(p) => p.clone(),
    };

    // contentが含まれない場合はスキップ
    let expected_path = match resolve_thumbnail_path(&path_str, &media.uuid) {
        None => {
            return Ok(build_item_result(
                media,
                UpdateThumbnailStatus::Skipped {
                    reason: "pathにcontentが含まれていない".to_string(),
                },
                None,
            ));
        }
        Some(p) => p,
    };

    // media_typeに応じたソースファイルのチェック
    match media.media_type {
        MediaType::Comic => {
            let ext = media.extension.as_deref().unwrap_or("jpg");
            let first_page = Path::new(&path_str).join(format!("001.{}", ext));
            if !first_page.exists() {
                return Ok(build_item_result(
                    media,
                    UpdateThumbnailStatus::Skipped {
                        reason: format!("001.{} が存在しない", ext),
                    },
                    None,
                ));
            }
        }
        MediaType::Video => {
            let ext = media.extension.as_deref().unwrap_or("mp4");
            if resolve_media_file_path(&path_str, ext, &media.uuid).is_none() {
                return Ok(build_item_result(
                    media,
                    UpdateThumbnailStatus::Skipped {
                        reason: "動画ファイルが存在しない".to_string(),
                    },
                    None,
                ));
            }
        }
        MediaType::Music => {
            return Ok(build_item_result(
                media,
                UpdateThumbnailStatus::Skipped {
                    reason: "musicはサムネイル対象外".to_string(),
                },
                None,
            ));
        }
    }

    // forceでない場合、既存サムネイルが正常であればスキップ
    if !options.force {
        if let Some(ref current) = media.thumbnail_path {
            if thumbnail_path_matches(current, &expected_path, &path_str)
                && Path::new(&expected_path).exists()
            {
                return Ok(build_item_result(
                    media,
                    UpdateThumbnailStatus::AlreadyExists,
                    Some(expected_path),
                ));
            }
        }
    }

    // dry_runの場合は生成予定として返す
    if options.dry_run {
        return Ok(build_item_result(
            media,
            UpdateThumbnailStatus::Generated,
            Some(expected_path),
        ));
    }

    // cover/ ディレクトリを作成
    let cover_dir = match Path::new(&expected_path).parent() {
        Some(d) => d.to_path_buf(),
        None => {
            return Ok(build_item_result(
                media,
                UpdateThumbnailStatus::Error {
                    message: "サムネイルパスの親ディレクトリが取得できない".to_string(),
                },
                None,
            ));
        }
    };

    if let Err(e) = std::fs::create_dir_all(&cover_dir) {
        return Ok(build_item_result(
            media,
            UpdateThumbnailStatus::Error {
                message: format!("cover/ディレクトリの作成に失敗: {}", e),
            },
            None,
        ));
    }

    // media_typeに応じたサムネイル生成
    let cmd_result = match media.media_type {
        MediaType::Comic => {
            let ext = media.extension.as_deref().unwrap_or("jpg");
            let first_page = Path::new(&path_str).join(format!("001.{}", ext));
            std::process::Command::new("convert")
                .arg(first_page.as_os_str())
                .args(["-resize", "x180", "-quality", "85"])
                .arg(&expected_path)
                .output()
        }
        MediaType::Video => {
            let ext = media.extension.as_deref().unwrap_or("mp4");
            let resolved = resolve_media_file_path(&path_str, ext, &media.uuid).unwrap();
            std::process::Command::new("ffmpeg")
                .args(["-ss", VIDEO_THUMBNAIL_TIMESTAMP])
                .arg("-i")
                .arg(&resolved)
                .args(["-vframes", "1", "-q:v", "2", "-y"])
                .arg(&expected_path)
                .output()
        }
        MediaType::Music => unreachable!(),
    };

    match cmd_result {
        Err(e) => {
            let cmd_name = match media.media_type {
                MediaType::Comic => "convert",
                MediaType::Video => "ffmpeg",
                MediaType::Music => unreachable!(),
            };
            return Ok(build_item_result(
                media,
                UpdateThumbnailStatus::Error {
                    message: format!("{}の実行に失敗: {}", cmd_name, e),
                },
                None,
            ));
        }
        Ok(output) if !output.status.success() => {
            let cmd_name = match media.media_type {
                MediaType::Comic => "convert",
                MediaType::Video => "ffmpeg",
                MediaType::Music => unreachable!(),
            };
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(build_item_result(
                media,
                UpdateThumbnailStatus::Error {
                    message: format!("{}失敗: {}", cmd_name, stderr.trim()),
                },
                None,
            ));
        }
        Ok(_) => {}
    }

    // DB更新
    let update = MediaUpdateInput {
        thumbnail_path: Some(Some(expected_path.clone())),
        ..Default::default()
    };
    if let Err(e) = update_media(conn, media.id, &update) {
        return Ok(build_item_result(
            media,
            UpdateThumbnailStatus::Error { message: format!("DB更新に失敗: {}", e) },
            Some(expected_path),
        ));
    }

    Ok(build_item_result(media, UpdateThumbnailStatus::Generated, Some(expected_path)))
}

/// フィルタで絞り込んだメディアのサムネイルを生成・更新する
pub fn update_thumbnail(
    conn: &Connection,
    filter: &MediaFilter,
    query_options: Option<&QueryOptions>,
    options: &ThumbnailOptions,
) -> Result<UpdateThumbnailResult> {
    let media_list = find_media(conn, filter, query_options)?;
    let total = media_list.len();
    let mut generated = 0usize;
    let mut already_exists = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;
    let mut details: Vec<UpdateThumbnailItemResult> = Vec::new();

    for media in &media_list {
        let item = update_media_thumbnail(conn, media, options)?;
        match &item.status {
            UpdateThumbnailStatus::Generated => generated += 1,
            UpdateThumbnailStatus::AlreadyExists => already_exists += 1,
            UpdateThumbnailStatus::Skipped { .. } => skipped += 1,
            UpdateThumbnailStatus::Error { .. } => errors += 1,
        }
        details.push(item);
    }

    Ok(UpdateThumbnailResult { total, generated, already_exists, skipped, errors, details })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::{create_media, get_media};
    use crate::migration;
    use crate::types::{MediaInput, MediaType};
    use std::fs;
    use tempfile::TempDir;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();
        conn
    }

    // ========== thumbnail_path_matches のテスト ==========

    #[test]
    fn test_thumbnail_path_matches_absolute_same() {
        assert!(thumbnail_path_matches(
            "/media/onepiece/cover/x.jpg",
            "/media/onepiece/cover/x.jpg",
            "/media/onepiece/content",
        ));
    }

    #[test]
    fn test_thumbnail_path_matches_absolute_different() {
        assert!(!thumbnail_path_matches(
            "/media/onepiece/cover/x.jpg",
            "/media/naruto/cover/x.jpg",
            "/media/onepiece/content",
        ));
    }

    #[test]
    fn test_thumbnail_path_matches_relative_resolved() {
        // currentが相対パスの場合、media_pathの基準ディレクトリから解決する
        assert!(thumbnail_path_matches(
            "cover/x.jpg",
            "/media/onepiece/cover/x.jpg",
            "/media/onepiece/content",
        ));
    }

    #[test]
    fn test_thumbnail_path_matches_relative_no_match() {
        assert!(!thumbnail_path_matches(
            "cover/y.jpg",
            "/media/onepiece/cover/x.jpg",
            "/media/onepiece/content",
        ));
    }

    // ========== resolve_thumbnail_path のテスト ==========

    #[test]
    fn test_resolve_thumbnail_path_basic() {
        let result = resolve_thumbnail_path("/media/onepiece/vol1/content", "abc-uuid");
        assert_eq!(result, Some("/media/onepiece/vol1/cover/abc-uuid.jpg".to_string()));
    }

    #[test]
    fn test_resolve_thumbnail_path_last_content() {
        // pathにcontentが複数ある場合、最後のcontentを使う
        let result = resolve_thumbnail_path("/media/content/onepiece/vol1/content", "abc-uuid");
        assert_eq!(result, Some("/media/content/onepiece/vol1/cover/abc-uuid.jpg".to_string()));
    }

    #[test]
    fn test_resolve_thumbnail_path_no_content() {
        let result = resolve_thumbnail_path("/media/onepiece/vol1", "abc-uuid");
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_thumbnail_path_content_at_root() {
        // contentがルート直下の場合、parentが空になるのでNone
        let result = resolve_thumbnail_path("/content", "abc-uuid");
        assert_eq!(result, None);
    }

    // ========== check_thumbnail のテスト ==========

    #[test]
    fn test_check_thumbnail_path_none_skipped() {
        let conn = setup_db();
        create_media(&conn, &MediaInput {
            title: "pathなし".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })
        .unwrap();

        let result = check_thumbnail(&conn, &MediaFilter::default(), None).unwrap();
        assert_eq!(result.skipped, 1);
        assert!(matches!(result.details[0].status, CheckThumbnailStatus::Skipped { .. }));
    }

    #[test]
    fn test_check_thumbnail_no_content_in_path_skipped() {
        let conn = setup_db();
        create_media(&conn, &MediaInput {
            title: "contentなし".to_string(),
            media_type: MediaType::Comic,
            path: Some("/media/onepiece/vol1".to_string()),
            uuid: Some("test-uuid-no-content".to_string()),
            ..Default::default()
        })
        .unwrap();

        let result = check_thumbnail(&conn, &MediaFilter::default(), None).unwrap();
        assert_eq!(result.skipped, 1);
    }

    #[test]
    fn test_check_thumbnail_missing_when_no_thumbnail_path() {
        let conn = setup_db();
        create_media(&conn, &MediaInput {
            title: "サムネなし".to_string(),
            media_type: MediaType::Comic,
            path: Some("/media/onepiece/vol1/content".to_string()),
            uuid: Some("test-uuid-missing".to_string()),
            ..Default::default()
        })
        .unwrap();

        let result = check_thumbnail(&conn, &MediaFilter::default(), None).unwrap();
        assert_eq!(result.missing, 1);
    }

    #[test]
    fn test_check_thumbnail_file_not_found() {
        let conn = setup_db();
        let uuid = "test-uuid-not-found";
        let path = "/media/onepiece/vol1/content";
        let thumb = format!("/media/onepiece/vol1/cover/{}.jpg", uuid);
        create_media(&conn, &MediaInput {
            title: "ファイルなし".to_string(),
            media_type: MediaType::Comic,
            path: Some(path.to_string()),
            uuid: Some(uuid.to_string()),
            thumbnail_path: Some(thumb),
            ..Default::default()
        })
        .unwrap();

        let result = check_thumbnail(&conn, &MediaFilter::default(), None).unwrap();
        assert_eq!(result.file_not_found, 1);
    }

    #[test]
    fn test_check_thumbnail_ok() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-ok";
        let cover_dir = tmp.path().join("cover");
        fs::create_dir(&cover_dir).unwrap();
        let thumb_path = cover_dir.join(format!("{}.jpg", uuid));
        fs::write(&thumb_path, b"dummy").unwrap();

        let content_path = tmp.path().join("content");
        fs::create_dir(&content_path).unwrap();

        create_media(&conn, &MediaInput {
            title: "OK".to_string(),
            media_type: MediaType::Comic,
            path: Some(content_path.to_str().unwrap().to_string()),
            uuid: Some(uuid.to_string()),
            thumbnail_path: Some(thumb_path.to_str().unwrap().to_string()),
            ..Default::default()
        })
        .unwrap();

        let result = check_thumbnail(&conn, &MediaFilter::default(), None).unwrap();
        assert_eq!(result.ok, 1);
    }

    // ========== update_thumbnail のテスト ==========

    #[test]
    fn test_update_thumbnail_skip_no_path() {
        let conn = setup_db();
        create_media(&conn, &MediaInput {
            title: "pathなし".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })
        .unwrap();

        let result =
            update_thumbnail(&conn, &MediaFilter::default(), None, &ThumbnailOptions::default())
                .unwrap();
        assert_eq!(result.skipped, 1);
        assert_eq!(result.generated, 0);
    }

    #[test]
    fn test_update_thumbnail_skip_no_content_in_path() {
        let conn = setup_db();
        create_media(&conn, &MediaInput {
            title: "contentなし".to_string(),
            media_type: MediaType::Comic,
            path: Some("/media/onepiece/vol1".to_string()),
            uuid: Some("test-uuid-no-content-update".to_string()),
            ..Default::default()
        })
        .unwrap();

        let result =
            update_thumbnail(&conn, &MediaFilter::default(), None, &ThumbnailOptions::default())
                .unwrap();
        assert_eq!(result.skipped, 1);
    }

    #[test]
    fn test_update_thumbnail_skip_no_first_page() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let content_dir = tmp.path().join("content");
        fs::create_dir(&content_dir).unwrap();
        // 001.jpgを作成しない

        create_media(&conn, &MediaInput {
            title: "001なし".to_string(),
            media_type: MediaType::Comic,
            path: Some(content_dir.to_str().unwrap().to_string()),
            uuid: Some("test-uuid-no-first-page".to_string()),
            extension: Some("jpg".to_string()),
            ..Default::default()
        })
        .unwrap();

        let result =
            update_thumbnail(&conn, &MediaFilter::default(), None, &ThumbnailOptions::default())
                .unwrap();
        assert_eq!(result.skipped, 1);
    }

    #[test]
    fn test_update_thumbnail_already_exists() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-already-exists";

        let content_dir = tmp.path().join("content");
        fs::create_dir(&content_dir).unwrap();
        fs::write(content_dir.join("001.jpg"), b"dummy page").unwrap();

        let cover_dir = tmp.path().join("cover");
        fs::create_dir(&cover_dir).unwrap();
        let thumb_path = cover_dir.join(format!("{}.jpg", uuid));
        fs::write(&thumb_path, b"dummy thumb").unwrap();

        create_media(&conn, &MediaInput {
            title: "既存あり".to_string(),
            media_type: MediaType::Comic,
            path: Some(content_dir.to_str().unwrap().to_string()),
            uuid: Some(uuid.to_string()),
            extension: Some("jpg".to_string()),
            thumbnail_path: Some(thumb_path.to_str().unwrap().to_string()),
            ..Default::default()
        })
        .unwrap();

        let result =
            update_thumbnail(&conn, &MediaFilter::default(), None, &ThumbnailOptions::default())
                .unwrap();
        assert_eq!(result.already_exists, 1);
        assert_eq!(result.generated, 0);
    }

    #[test]
    fn test_update_thumbnail_dry_run_does_not_modify_db() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-dry-run";

        let content_dir = tmp.path().join("content");
        fs::create_dir(&content_dir).unwrap();
        fs::write(content_dir.join("001.jpg"), b"dummy page").unwrap();

        let media = create_media(&conn, &MediaInput {
            title: "dry_runテスト".to_string(),
            media_type: MediaType::Comic,
            path: Some(content_dir.to_str().unwrap().to_string()),
            uuid: Some(uuid.to_string()),
            extension: Some("jpg".to_string()),
            ..Default::default()
        })
        .unwrap();

        let result = update_thumbnail(
            &conn,
            &MediaFilter::default(),
            None,
            &ThumbnailOptions { dry_run: true, force: false },
        )
        .unwrap();
        assert_eq!(result.generated, 1);

        // DBは更新されていない
        let db_media = get_media(&conn, media.id).unwrap();
        assert!(db_media.thumbnail_path.is_none());

        // ファイルも生成されていない
        let cover_dir = tmp.path().join("cover");
        assert!(!cover_dir.exists());
    }

    #[test]
    fn test_update_thumbnail_success_generates_and_updates_db() {
        // fake convertスクリプトを作成してPATHに追加
        let fake_bin_dir = TempDir::new().unwrap();
        let fake_convert = fake_bin_dir.path().join("convert");
        // 最後の引数（出力先）に空ファイルを作成するだけのスクリプト
        // POSIX互換の最終引数取得: for last; do true; done
        fs::write(
            &fake_convert,
            "#!/bin/sh\nfor last; do true; done\ntouch \"$last\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&fake_convert, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-success";

        let content_dir = tmp.path().join("content");
        fs::create_dir(&content_dir).unwrap();
        fs::write(content_dir.join("001.jpg"), b"dummy page").unwrap();

        let media = create_media(&conn, &MediaInput {
            title: "成功ケース".to_string(),
            media_type: MediaType::Comic,
            path: Some(content_dir.to_str().unwrap().to_string()),
            uuid: Some(uuid.to_string()),
            extension: Some("jpg".to_string()),
            ..Default::default()
        })
        .unwrap();

        // fakeのconvertをPATHの先頭に追加して実行
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", fake_bin_dir.path().display(), original_path);
        let result = {
            // SAFETY: テスト内でのみPATHを変更
            std::env::set_var("PATH", &new_path);
            let r = update_thumbnail(
                &conn,
                &MediaFilter::default(),
                None,
                &ThumbnailOptions { dry_run: false, force: false },
            );
            std::env::set_var("PATH", &original_path);
            r
        }
        .unwrap();

        assert_eq!(result.generated, 1);
        assert_eq!(result.errors, 0);

        // DBにthumbnail_pathが保存されている
        let db_media = get_media(&conn, media.id).unwrap();
        let expected_thumb = format!("{}/cover/{}.jpg", tmp.path().display(), uuid);
        assert_eq!(db_media.thumbnail_path, Some(expected_thumb));

        // coverディレクトリが作成されている
        let cover_dir = tmp.path().join("cover");
        assert!(cover_dir.exists());
    }

    // ========== Video update_thumbnail のテスト ==========

    #[test]
    fn test_update_thumbnail_video_skip_music() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let content_dir = tmp.path().join("content");
        fs::create_dir(&content_dir).unwrap();

        create_media(&conn, &MediaInput {
            title: "Music".to_string(),
            media_type: MediaType::Music,
            path: Some(content_dir.to_str().unwrap().to_string()),
            uuid: Some("test-uuid-music".to_string()),
            ..Default::default()
        })
        .unwrap();

        let result =
            update_thumbnail(&conn, &MediaFilter::default(), None, &ThumbnailOptions::default())
                .unwrap();
        assert_eq!(result.skipped, 1);
        assert!(matches!(
            &result.details[0].status,
            UpdateThumbnailStatus::Skipped { reason } if reason.contains("music")
        ));
    }

    #[test]
    fn test_update_thumbnail_video_skip_no_file() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let content_dir = tmp.path().join("content");
        fs::create_dir(&content_dir).unwrap();
        let video_path = content_dir.join("video.mp4");
        // ファイルを作成しない

        create_media(&conn, &MediaInput {
            title: "Video no file".to_string(),
            media_type: MediaType::Video,
            path: Some(video_path.to_str().unwrap().to_string()),
            uuid: Some("test-uuid-video-nofile".to_string()),
            extension: Some("mp4".to_string()),
            ..Default::default()
        })
        .unwrap();

        let result =
            update_thumbnail(&conn, &MediaFilter::default(), None, &ThumbnailOptions::default())
                .unwrap();
        assert_eq!(result.skipped, 1);
    }

    #[test]
    fn test_update_thumbnail_video_dry_run() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let content_dir = tmp.path().join("content");
        fs::create_dir(&content_dir).unwrap();
        let video_path = content_dir.join("video.mp4");
        fs::write(&video_path, b"dummy video").unwrap();

        create_media(&conn, &MediaInput {
            title: "Video dry_run".to_string(),
            media_type: MediaType::Video,
            path: Some(video_path.to_str().unwrap().to_string()),
            uuid: Some("test-uuid-video-dry".to_string()),
            extension: Some("mp4".to_string()),
            ..Default::default()
        })
        .unwrap();

        let result = update_thumbnail(
            &conn,
            &MediaFilter::default(),
            None,
            &ThumbnailOptions { dry_run: true, force: false },
        )
        .unwrap();
        assert_eq!(result.generated, 1);
        assert_eq!(result.errors, 0);

        // ファイルは生成されていない
        let cover_dir = tmp.path().join("cover");
        assert!(!cover_dir.exists());
    }

    #[test]
    fn test_update_thumbnail_video_success() {
        let fake_bin_dir = TempDir::new().unwrap();
        let fake_ffmpeg = fake_bin_dir.path().join("ffmpeg");
        fs::write(
            &fake_ffmpeg,
            "#!/bin/sh\nfor last; do true; done\ntouch \"$last\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&fake_ffmpeg, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-video-success";

        let content_dir = tmp.path().join("content");
        fs::create_dir(&content_dir).unwrap();
        let video_path = content_dir.join("video.mp4");
        fs::write(&video_path, b"dummy video").unwrap();

        let media = create_media(&conn, &MediaInput {
            title: "Video成功".to_string(),
            media_type: MediaType::Video,
            path: Some(video_path.to_str().unwrap().to_string()),
            uuid: Some(uuid.to_string()),
            extension: Some("mp4".to_string()),
            ..Default::default()
        })
        .unwrap();

        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", fake_bin_dir.path().display(), original_path);
        let result = {
            std::env::set_var("PATH", &new_path);
            let r = update_thumbnail(
                &conn,
                &MediaFilter::default(),
                None,
                &ThumbnailOptions { dry_run: false, force: false },
            );
            std::env::set_var("PATH", &original_path);
            r
        }
        .unwrap();

        assert_eq!(result.generated, 1);
        assert_eq!(result.errors, 0);

        let db_media = get_media(&conn, media.id).unwrap();
        let expected_thumb = format!("{}/cover/{}.jpg", tmp.path().display(), uuid);
        assert_eq!(db_media.thumbnail_path, Some(expected_thumb));

        let cover_dir = tmp.path().join("cover");
        assert!(cover_dir.exists());
    }

    #[test]
    fn test_update_thumbnail_video_already_exists() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-video-exists";

        let content_dir = tmp.path().join("content");
        fs::create_dir(&content_dir).unwrap();
        let video_path = content_dir.join("video.mp4");
        fs::write(&video_path, b"dummy video").unwrap();

        let cover_dir = tmp.path().join("cover");
        fs::create_dir(&cover_dir).unwrap();
        let thumb_path = cover_dir.join(format!("{}.jpg", uuid));
        fs::write(&thumb_path, b"dummy thumb").unwrap();

        create_media(&conn, &MediaInput {
            title: "Video既存あり".to_string(),
            media_type: MediaType::Video,
            path: Some(video_path.to_str().unwrap().to_string()),
            uuid: Some(uuid.to_string()),
            extension: Some("mp4".to_string()),
            thumbnail_path: Some(thumb_path.to_str().unwrap().to_string()),
            ..Default::default()
        })
        .unwrap();

        let result =
            update_thumbnail(&conn, &MediaFilter::default(), None, &ThumbnailOptions::default())
                .unwrap();
        assert_eq!(result.already_exists, 1);
        assert_eq!(result.generated, 0);
    }

    #[test]
    fn test_update_thumbnail_video_resolve_ext_from_uuid() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-video-resolve";

        let content_dir = tmp.path().join("content");
        fs::create_dir(&content_dir).unwrap();
        // pathにextを含めず、実際のファイルは {uuid}.mp4
        let path_without_ext = content_dir.join(uuid);
        let actual_file = content_dir.join(format!("{}.mp4", uuid));
        fs::write(&actual_file, b"dummy video").unwrap();

        create_media(&conn, &MediaInput {
            title: "Video extなし".to_string(),
            media_type: MediaType::Video,
            path: Some(path_without_ext.to_str().unwrap().to_string()),
            uuid: Some(uuid.to_string()),
            extension: Some("mp4".to_string()),
            ..Default::default()
        })
        .unwrap();

        let result = update_thumbnail(
            &conn,
            &MediaFilter::default(),
            None,
            &ThumbnailOptions { dry_run: true, force: false },
        )
        .unwrap();
        assert_eq!(result.generated, 1);
    }

    #[test]
    fn test_update_thumbnail_video_no_file_no_uuid_match() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-video-nofile2";

        let content_dir = tmp.path().join("content");
        fs::create_dir(&content_dir).unwrap();
        let path_without_ext = content_dir.join(uuid);
        // ファイルを作成しない

        create_media(&conn, &MediaInput {
            title: "Video ファイルなし".to_string(),
            media_type: MediaType::Video,
            path: Some(path_without_ext.to_str().unwrap().to_string()),
            uuid: Some(uuid.to_string()),
            extension: Some("mp4".to_string()),
            ..Default::default()
        })
        .unwrap();

        let result =
            update_thumbnail(&conn, &MediaFilter::default(), None, &ThumbnailOptions::default())
                .unwrap();
        assert_eq!(result.skipped, 1);
    }
}
