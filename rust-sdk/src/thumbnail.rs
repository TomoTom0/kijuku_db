use crate::crud::update_media;
use crate::error::Result;
use crate::search::find_media;
use crate::types::{Media, MediaFilter, MediaUpdateInput, QueryOptions};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_file_path(suffix: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = process::id();
    std::env::temp_dir()
        .join(format!("kijuku-thumbnail-{}-{}{}", pid, ts, suffix))
        .to_string_lossy()
        .into_owned()
}

/// pathからサムネイルの期待パスを計算する
///
/// pathに含まれる最後の"content"コンポーネントを見つけ、
/// その親ディレクトリの cover/{uuid}.jpg を返す。
/// contentが含まれない場合はNoneを返す。
pub fn resolve_thumbnail_path(path_str: &str, uuid: &str) -> Option<String> {
    let path = Path::new(path_str);
    let components: Vec<Component> = path.components().collect();

    // 最後の"content"コンポーネントを探す
    let last_content_idx = components.iter().rposition(|c| {
        matches!(c, Component::Normal(s) if s.to_str() == Some("content"))
    })?;

    // content以前のコンポーネントからparentパスを構築
    // ルートディレクトリのみ（例: /content）はNoneとして扱う
    let has_normal = components[..last_content_idx]
        .iter()
        .any(|c| matches!(c, Component::Normal(_)));
    if !has_normal {
        return None;
    }
    let parent: PathBuf = components[..last_content_idx].iter().collect();

    let thumbnail = parent.join("cover").join(format!("{}.jpg", uuid));
    Some(thumbnail.to_string_lossy().into_owned())
}

/// サムネイルチェック・更新のオプション
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThumbnailOptions {
    /// trueの場合、DBを更新せず結果を出力のみ（update-thumbnailのみ有効）
    pub dry_run: bool,
    /// trueの場合、既にサムネイルが存在しても再生成する（update-thumbnailのみ有効）
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
    /// 全件の詳細結果を含むJSONファイルのパス
    pub detail_file: String,
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
    /// 全件の詳細結果を含むJSONファイルのパス
    pub detail_file: String,
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
        match &media.thumbnail_path {
            None => CheckThumbnailStatus::Missing,
            Some(current) if current != expected => CheckThumbnailStatus::Missing,
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
///
/// # 一時ファイル
///
/// この関数は結果を格納するために一時ファイルを作成します。
/// 返される [`CheckThumbnailResult`] の `detail_file` に含まれる
/// ファイルパスは、呼び出し側が不要になった時点で削除する責任があります。
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
    let mut items: Vec<CheckThumbnailItemResult> = Vec::new();

    for media in &media_list {
        let item = check_media_thumbnail(media);
        match &item.status {
            CheckThumbnailStatus::Ok => ok += 1,
            CheckThumbnailStatus::Missing => missing += 1,
            CheckThumbnailStatus::FileNotFound => file_not_found += 1,
            CheckThumbnailStatus::Skipped { .. } => skipped += 1,
        }
        items.push(item);
    }

    let detail_file = temp_file_path("-check-detail.json");
    let detail_json = serde_json::to_string(&items)
        .map_err(|e| crate::error::KijukuError::Parse(e.to_string()))?;
    std::fs::write(&detail_file, &detail_json)?;

    Ok(CheckThumbnailResult { total, ok, missing, file_not_found, skipped, detail_file })
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
            return Ok(UpdateThumbnailItemResult {
                id: media.id,
                uuid: media.uuid.clone(),
                title: media.title.clone(),
                thumbnail_path: None,
                status: UpdateThumbnailStatus::Skipped { reason: "pathが未設定".to_string() },
            });
        }
        Some(p) => p.clone(),
    };

    // contentが含まれない場合はスキップ
    let expected_path = match resolve_thumbnail_path(&path_str, &media.uuid) {
        None => {
            return Ok(UpdateThumbnailItemResult {
                id: media.id,
                uuid: media.uuid.clone(),
                title: media.title.clone(),
                thumbnail_path: None,
                status: UpdateThumbnailStatus::Skipped {
                    reason: "pathにcontentが含まれていない".to_string(),
                },
            });
        }
        Some(p) => p,
    };

    // 001.{ext} が存在しない場合はスキップ
    let ext = media.extension.as_deref().unwrap_or("jpg");
    let first_page = Path::new(&path_str).join(format!("001.{}", ext));
    if !first_page.exists() {
        return Ok(UpdateThumbnailItemResult {
            id: media.id,
            uuid: media.uuid.clone(),
            title: media.title.clone(),
            thumbnail_path: None,
            status: UpdateThumbnailStatus::Skipped {
                reason: format!("001.{} が存在しない", ext),
            },
        });
    }

    // forceでない場合、既存サムネイルが正常であればスキップ
    if !options.force {
        if let Some(ref current) = media.thumbnail_path {
            if current == &expected_path && Path::new(&expected_path).exists() {
                return Ok(UpdateThumbnailItemResult {
                    id: media.id,
                    uuid: media.uuid.clone(),
                    title: media.title.clone(),
                    thumbnail_path: Some(expected_path),
                    status: UpdateThumbnailStatus::AlreadyExists,
                });
            }
        }
    }

    // dry_runの場合は生成予定として返す
    if options.dry_run {
        return Ok(UpdateThumbnailItemResult {
            id: media.id,
            uuid: media.uuid.clone(),
            title: media.title.clone(),
            thumbnail_path: Some(expected_path),
            status: UpdateThumbnailStatus::Generated,
        });
    }

    // cover/ ディレクトリを作成
    let cover_dir = match Path::new(&expected_path).parent() {
        Some(d) => d.to_path_buf(),
        None => {
            return Ok(UpdateThumbnailItemResult {
                id: media.id,
                uuid: media.uuid.clone(),
                title: media.title.clone(),
                thumbnail_path: None,
                status: UpdateThumbnailStatus::Error {
                    message: "サムネイルパスの親ディレクトリが取得できない".to_string(),
                },
            });
        }
    };

    if let Err(e) = std::fs::create_dir_all(&cover_dir) {
        return Ok(UpdateThumbnailItemResult {
            id: media.id,
            uuid: media.uuid.clone(),
            title: media.title.clone(),
            thumbnail_path: None,
            status: UpdateThumbnailStatus::Error {
                message: format!("cover/ディレクトリの作成に失敗: {}", e),
            },
        });
    }

    // ImageMagick の convert でサムネイルを生成（高さ180px固定、品質85）
    let convert_result = std::process::Command::new("convert")
        .arg(first_page.as_os_str())
        .arg("-resize")
        .arg("x180")
        .arg("-quality")
        .arg("85")
        .arg(&expected_path)
        .output();

    match convert_result {
        Err(e) => {
            return Ok(UpdateThumbnailItemResult {
                id: media.id,
                uuid: media.uuid.clone(),
                title: media.title.clone(),
                thumbnail_path: None,
                status: UpdateThumbnailStatus::Error {
                    message: format!("convertの実行に失敗: {}", e),
                },
            });
        }
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(UpdateThumbnailItemResult {
                id: media.id,
                uuid: media.uuid.clone(),
                title: media.title.clone(),
                thumbnail_path: None,
                status: UpdateThumbnailStatus::Error {
                    message: format!("convert失敗: {}", stderr.trim()),
                },
            });
        }
        Ok(_) => {}
    }

    // DB更新
    let update = MediaUpdateInput {
        thumbnail_path: Some(Some(expected_path.clone())),
        ..Default::default()
    };
    if let Err(e) = update_media(conn, media.id, &update) {
        return Ok(UpdateThumbnailItemResult {
            id: media.id,
            uuid: media.uuid.clone(),
            title: media.title.clone(),
            thumbnail_path: Some(expected_path),
            status: UpdateThumbnailStatus::Error {
                message: format!("DB更新に失敗: {}", e),
            },
        });
    }

    Ok(UpdateThumbnailItemResult {
        id: media.id,
        uuid: media.uuid.clone(),
        title: media.title.clone(),
        thumbnail_path: Some(expected_path),
        status: UpdateThumbnailStatus::Generated,
    })
}

/// フィルタで絞り込んだメディアのサムネイルを生成・更新する
///
/// # 一時ファイル
///
/// この関数は結果を格納するために一時ファイルを作成します。
/// 返される [`UpdateThumbnailResult`] の `detail_file` に含まれる
/// ファイルパスは、呼び出し側が不要になった時点で削除する責任があります。
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
    let mut items: Vec<UpdateThumbnailItemResult> = Vec::new();

    for media in &media_list {
        let item = update_media_thumbnail(conn, media, options)?;
        match &item.status {
            UpdateThumbnailStatus::Generated => generated += 1,
            UpdateThumbnailStatus::AlreadyExists => already_exists += 1,
            UpdateThumbnailStatus::Skipped { .. } => skipped += 1,
            UpdateThumbnailStatus::Error { .. } => errors += 1,
        }
        items.push(item);
    }

    let detail_file = temp_file_path("-update-detail.json");
    let detail_json = serde_json::to_string(&items)
        .map_err(|e| crate::error::KijukuError::Parse(e.to_string()))?;
    std::fs::write(&detail_file, &detail_json)?;

    Ok(UpdateThumbnailResult { total, generated, already_exists, skipped, errors, detail_file })
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
        let items: Vec<CheckThumbnailItemResult> =
            serde_json::from_str(&fs::read_to_string(&result.detail_file).unwrap()).unwrap();
        assert_eq!(result.skipped, 1);
        assert!(matches!(items[0].status, CheckThumbnailStatus::Skipped { .. }));
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
}
