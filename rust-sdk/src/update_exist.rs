use crate::crud::update_media;
use crate::error::Result;
use crate::search::find_media;
use crate::types::{Media, MediaFilter, MediaType, MediaUpdateInput, QueryOptions};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

const UPDATED_IDS_INLINE_LIMIT: usize = 1000;

fn temp_file_path(suffix: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = process::id();
    std::env::temp_dir()
        .join(format!("kijuku-update-exist-{}-{}{}", pid, ts, suffix))
        .to_string_lossy()
        .into_owned()
}

/// update_existの動作オプション
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateExistOptions {
    /// trueの場合、DBを更新せず結果を出力のみ
    pub dry_run: bool,
}

/// 1件のメディアに対するupdate_existの結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateExistItemResult {
    pub id: i64,
    pub uuid: String,
    pub title: String,
    /// 更新前のflag_exist
    pub flag_exist_before: bool,
    /// 更新後のflag_exist
    pub flag_exist_after: bool,
    /// 使用した拡張子（実際にチェックに用いた拡張子）
    pub extension_used: Option<String>,
    /// 代替拡張子が見つかった場合（元のextensionと異なる拡張子で存在）
    pub found_extension: Option<String>,
    /// page_countに関する警告
    pub page_count_warning: Option<String>,
    /// page_countを新規設定した値
    pub page_count_set: Option<i32>,
}

/// update_exist全体の結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateExistResult {
    pub total: usize,
    pub updated: usize,
    /// 変更があったメディアのID一覧（1000件以下の場合のみインライン）
    pub updated_ids: Option<Vec<i64>>,
    /// updated_idsが1000件超の場合のファイルパス
    pub updated_ids_file: Option<String>,
    /// 全件の詳細結果を含むJSONファイルのパス
    pub detail_file: String,
}

/// media_typeごとのデフォルト拡張子
fn default_extension(media_type: MediaType) -> &'static str {
    match media_type {
        MediaType::Comic => "jpg",
        MediaType::Video => "mp4",
        MediaType::Music => "m4a",
    }
}

/// comicのフォルダ内ページ数を数える（001.{ext}形式のファイルを対象）
fn count_pages_in_dir(dir: &Path, ext: &str) -> i32 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0i32;
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let fname = fname.to_string_lossy();
        // {数字}.{ext} 形式かチェック
        if let Some(stem) = fname.strip_suffix(&format!(".{}", ext)) {
            if stem.chars().all(|c| c.is_ascii_digit()) {
                count += 1;
            }
        }
    }
    count
}

/// フォルダ内で {数字}.* 形式のファイルを拡張子ごとにカウントし、
/// 最も多い拡張子を返す
fn find_dominant_extension_in_dir(dir: &Path) -> Option<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return None;
    };
    let mut ext_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let fname_str = fname.to_string_lossy();
        if let Some(dot_pos) = fname_str.rfind('.') {
            let stem = &fname_str[..dot_pos];
            let ext = &fname_str[dot_pos + 1..];
            if stem.chars().all(|c| c.is_ascii_digit()) && !ext.is_empty() {
                *ext_counts.entry(ext.to_string()).or_insert(0) += 1;
            }
        }
    }
    ext_counts.into_iter().max_by_key(|(_, v)| *v).map(|(k, _)| k)
}

/// ファイルパスのディレクトリ内で {uuid}.* 形式の代替拡張子を探す
fn find_alternative_extension_for_file(path: &Path, uuid: &str) -> Option<String> {
    let parent = path.parent()?;
    let Ok(entries) = std::fs::read_dir(parent) else {
        return None;
    };
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let fname_str = fname.to_string_lossy();
        if let Some(dot_pos) = fname_str.rfind('.') {
            let stem = &fname_str[..dot_pos];
            let ext = &fname_str[dot_pos + 1..];
            if stem == uuid && !ext.is_empty() {
                return Some(ext.to_string());
            }
        }
    }
    None
}

/// 1件のメディアに対してflag_existチェック・更新を行う
fn process_media(
    conn: &Connection,
    media: &Media,
    options: &UpdateExistOptions,
) -> Result<UpdateExistItemResult> {
    let ext = media
        .extension
        .clone()
        .unwrap_or_else(|| default_extension(media.media_type).to_string());

    let flag_exist_before = media.flag_exist;
    let mut flag_exist_after = false;
    let mut found_extension: Option<String> = None;
    let mut page_count_warning: Option<String> = None;
    let mut page_count_set: Option<i32> = None;
    let mut extension_used = Some(ext.clone());

    match &media.path {
        None => {
            // path未設定 → 存在しない（初期値false のまま）
        }
        Some(path_str) => {
            let path = Path::new(path_str);

            match media.media_type {
                MediaType::Comic => {
                    // comic: pathはフォルダ、中に001.{ext}が存在するか
                    let first_page = path.join(format!("001.{}", ext));
                    if first_page.exists() {
                        flag_exist_after = true;

                        // ページ数チェック
                        let actual_count = count_pages_in_dir(path, &ext);
                        match media.page_count {
                            None => {
                                // page_countが未設定 → 設定する
                                if actual_count > 0 {
                                    page_count_set = Some(actual_count);
                                }
                            }
                            Some(db_count) => {
                                if db_count != actual_count {
                                    page_count_warning = Some(format!(
                                        "page_count不一致: DB={}, 実際={}",
                                        db_count, actual_count
                                    ));
                                }
                            }
                        }
                    } else {
                        // 代替拡張子を探す
                        if let Some(alt_ext) = find_dominant_extension_in_dir(path) {
                            found_extension = Some(alt_ext.clone());
                            extension_used = Some(alt_ext);
                        }
                        flag_exist_after = false;
                    }
                }
                MediaType::Video | MediaType::Music => {
                    // video/music: pathがファイル自体
                    if path.exists() {
                        flag_exist_after = true;
                    } else {
                        // uuid.* 形式で代替拡張子を探す
                        let alt_ext = find_alternative_extension_for_file(path, &media.uuid);
                        if let Some(ref ae) = alt_ext {
                            found_extension = Some(ae.clone());
                            extension_used = Some(ae.clone());
                        }
                        flag_exist_after = false;
                    }
                }
            }
        }
    }

    // DB更新（dry_runでない場合）
    if !options.dry_run {
        let mut update = MediaUpdateInput {
            flag_exist: Some(flag_exist_after),
            ..Default::default()
        };

        // 代替拡張子が見つかった場合はextensionも更新
        if let Some(ref new_ext) = found_extension {
            update.extension = Some(Some(new_ext.clone()));
            update.flag_exist = Some(true);
            flag_exist_after = true; // 結果オブジェクトのために更新
        }

        // page_countの設定
        if let Some(count) = page_count_set {
            update.page_count = Some(Some(count));
        }

        update_media(conn, media.id, &update)?;
    }

    Ok(UpdateExistItemResult {
        id: media.id,
        uuid: media.uuid.clone(),
        title: media.title.clone(),
        flag_exist_before,
        flag_exist_after,
        extension_used,
        found_extension,
        page_count_warning,
        page_count_set,
    })
}

/// フィルタで絞り込んだメディアのflag_existをファイル存在状態に基づいて更新する
///
/// # 一時ファイル
///
/// この関数は結果を格納するために一時ファイルを作成します。
/// 返される [`UpdateExistResult`] の `detail_file` および `updated_ids_file` に含まれる
/// ファイルパスは、呼び出し側が不要になった時点で削除する責任があります。
pub fn update_exist(
    conn: &Connection,
    filter: &MediaFilter,
    query_options: Option<&QueryOptions>,
    options: &UpdateExistOptions,
) -> Result<UpdateExistResult> {
    let media_list = find_media(conn, filter, query_options)?;
    let total = media_list.len();
    let mut updated = 0usize;
    let mut items: Vec<UpdateExistItemResult> = Vec::new();
    let mut updated_ids: Vec<i64> = Vec::new();

    for media in &media_list {
        let item = process_media(conn, media, options)?;
        if item.flag_exist_before != item.flag_exist_after
            || item.found_extension.is_some()
            || item.page_count_set.is_some()
        {
            updated += 1;
            updated_ids.push(item.id);
        }
        items.push(item);
    }

    // 警告の出力（dry_run含め常に出力）
    for item in &items {
        if let Some(ref warn) = item.page_count_warning {
            eprintln!("WARNING [{}] {}: {}", item.id, item.title, warn);
        }
        if let Some(ref new_ext) = item.found_extension {
            if options.dry_run {
                eprintln!(
                    "INFO [{}] {}: extension={} で発見（dry_run: 更新なし）",
                    item.id, item.title, new_ext
                );
            } else {
                eprintln!(
                    "INFO [{}] {}: extension を {} に更新",
                    item.id, item.title, new_ext
                );
            }
        }
    }

    // 詳細結果を一時ファイルに書き出す
    let detail_file = temp_file_path("-detail.json");
    let detail_json = serde_json::to_string(&items)
        .map_err(|e| crate::error::KijukuError::Parse(e.to_string()))?;
    std::fs::write(&detail_file, &detail_json)?;

    // updated_idsが1000件超の場合はファイルに書き出す
    let (updated_ids_inline, updated_ids_file) = if updated_ids.len() > UPDATED_IDS_INLINE_LIMIT {
        let ids_file = temp_file_path("-ids.json");
        let ids_json = serde_json::to_string(&updated_ids)
            .map_err(|e| crate::error::KijukuError::Parse(e.to_string()))?;
        std::fs::write(&ids_file, &ids_json)?;
        (None, Some(ids_file))
    } else {
        (Some(updated_ids), None)
    };

    Ok(UpdateExistResult {
        total,
        updated,
        updated_ids: updated_ids_inline,
        updated_ids_file,
        detail_file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::{create_media, get_media};
    use crate::migration;
    use crate::types::MediaInput;
    use std::fs;
    use tempfile::TempDir;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();
        conn
    }

    fn read_detail_file(result: &UpdateExistResult) -> Vec<UpdateExistItemResult> {
        let json = fs::read_to_string(&result.detail_file).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    fn read_ids_file(path: &str) -> Vec<i64> {
        let json = fs::read_to_string(path).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn test_path_none_returns_false() {
        let conn = setup_db();
        let media = create_media(&conn, &MediaInput {
            title: "pathなし".to_string(),
            media_type: MediaType::Music,
            flag_exist: Some(true),
            ..Default::default()
        }).unwrap();

        let filter = MediaFilter { ..Default::default() };
        let result = update_exist(&conn, &filter, None, &UpdateExistOptions::default()).unwrap();
        let items = read_detail_file(&result);
        assert_eq!(items[0].flag_exist_after, false);
        let updated = get_media(&conn, media.id).unwrap();
        assert!(!updated.flag_exist);
    }

    #[test]
    fn test_music_file_exists() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-music-001";
        let file_path = tmp.path().join(format!("{}.m4a", uuid));
        fs::write(&file_path, b"dummy").unwrap();

        create_media(&conn, &MediaInput {
            title: "music".to_string(),
            media_type: MediaType::Music,
            uuid: Some(uuid.to_string()),
            path: Some(file_path.to_str().unwrap().to_string()),
            extension: Some("m4a".to_string()),
            ..Default::default()
        }).unwrap();

        let result = update_exist(&conn, &MediaFilter::default(), None, &UpdateExistOptions::default()).unwrap();
        let items = read_detail_file(&result);
        assert_eq!(items[0].flag_exist_after, true);
    }

    #[test]
    fn test_music_file_not_exists() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-music-002";
        let file_path = tmp.path().join(format!("{}.m4a", uuid));
        // ファイルは作成しない

        create_media(&conn, &MediaInput {
            title: "music missing".to_string(),
            media_type: MediaType::Music,
            uuid: Some(uuid.to_string()),
            path: Some(file_path.to_str().unwrap().to_string()),
            extension: Some("m4a".to_string()),
            ..Default::default()
        }).unwrap();

        let result = update_exist(&conn, &MediaFilter::default(), None, &UpdateExistOptions::default()).unwrap();
        let items = read_detail_file(&result);
        assert_eq!(items[0].flag_exist_after, false);
    }

    #[test]
    fn test_music_alternative_extension_found() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-music-003";
        // mp3として存在
        let alt_file_path = tmp.path().join(format!("{}.mp3", uuid));
        fs::write(&alt_file_path, b"dummy").unwrap();

        let expected_path = tmp.path().join(format!("{}.m4a", uuid));
        create_media(&conn, &MediaInput {
            title: "music alt ext".to_string(),
            media_type: MediaType::Music,
            uuid: Some(uuid.to_string()),
            path: Some(expected_path.to_str().unwrap().to_string()),
            extension: Some("m4a".to_string()),
            ..Default::default()
        }).unwrap();

        let result = update_exist(&conn, &MediaFilter::default(), None, &UpdateExistOptions::default()).unwrap();
        let items = read_detail_file(&result);
        assert_eq!(items[0].found_extension, Some("mp3".to_string()));
        assert_eq!(items[0].flag_exist_after, true);
    }

    #[test]
    fn test_comic_first_page_exists() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("comic-uuid");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("001.jpg"), b"dummy").unwrap();
        fs::write(dir.join("002.jpg"), b"dummy").unwrap();
        fs::write(dir.join("003.jpg"), b"dummy").unwrap();

        create_media(&conn, &MediaInput {
            title: "comic".to_string(),
            media_type: MediaType::Comic,
            path: Some(dir.to_str().unwrap().to_string()),
            extension: Some("jpg".to_string()),
            ..Default::default()
        }).unwrap();

        let result = update_exist(&conn, &MediaFilter::default(), None, &UpdateExistOptions::default()).unwrap();
        let items = read_detail_file(&result);
        assert_eq!(items[0].flag_exist_after, true);
        assert_eq!(items[0].page_count_set, Some(3));
    }

    #[test]
    fn test_comic_page_count_mismatch_warning() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("comic-uuid-warn");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("001.jpg"), b"dummy").unwrap();
        fs::write(dir.join("002.jpg"), b"dummy").unwrap();

        create_media(&conn, &MediaInput {
            title: "comic warn".to_string(),
            media_type: MediaType::Comic,
            path: Some(dir.to_str().unwrap().to_string()),
            extension: Some("jpg".to_string()),
            page_count: Some(5), // 実際は2ページ
            ..Default::default()
        }).unwrap();

        let result = update_exist(&conn, &MediaFilter::default(), None, &UpdateExistOptions::default()).unwrap();
        let items = read_detail_file(&result);
        assert!(items[0].page_count_warning.is_some());
        assert!(items[0].page_count_warning.as_ref().unwrap().contains("5"));
        assert!(items[0].page_count_warning.as_ref().unwrap().contains("2"));
    }

    #[test]
    fn test_comic_alternative_extension() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("comic-uuid-alt");
        fs::create_dir(&dir).unwrap();
        // pngとして存在
        fs::write(dir.join("001.png"), b"dummy").unwrap();
        fs::write(dir.join("002.png"), b"dummy").unwrap();

        create_media(&conn, &MediaInput {
            title: "comic alt".to_string(),
            media_type: MediaType::Comic,
            path: Some(dir.to_str().unwrap().to_string()),
            extension: Some("jpg".to_string()), // jpgとして登録されているがpngで存在
            ..Default::default()
        }).unwrap();

        let result = update_exist(&conn, &MediaFilter::default(), None, &UpdateExistOptions::default()).unwrap();
        let items = read_detail_file(&result);
        assert_eq!(items[0].found_extension, Some("png".to_string()));
        assert_eq!(items[0].flag_exist_after, true);
    }

    #[test]
    fn test_dry_run_does_not_update_db() {
        let conn = setup_db();
        let media = create_media(&conn, &MediaInput {
            title: "dry run test".to_string(),
            media_type: MediaType::Music,
            flag_exist: Some(true),
            ..Default::default()
        }).unwrap();

        let result = update_exist(
            &conn,
            &MediaFilter::default(),
            None,
            &UpdateExistOptions { dry_run: true },
        ).unwrap();
        let items = read_detail_file(&result);
        assert_eq!(items[0].flag_exist_after, false);

        // DBは更新されていない
        let db_media = get_media(&conn, media.id).unwrap();
        assert!(db_media.flag_exist); // 元のtrueのまま
    }

    #[test]
    fn test_default_extension_used_when_null() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-default-ext";
        let file_path = tmp.path().join(format!("{}.m4a", uuid));
        fs::write(&file_path, b"dummy").unwrap();

        // extensionをNoneで登録（デフォルトm4aが使われるはず）
        create_media(&conn, &MediaInput {
            title: "music default ext".to_string(),
            media_type: MediaType::Music,
            uuid: Some(uuid.to_string()),
            path: Some(file_path.to_str().unwrap().to_string()),
            extension: None,
            ..Default::default()
        }).unwrap();

        let result = update_exist(&conn, &MediaFilter::default(), None, &UpdateExistOptions::default()).unwrap();
        let items = read_detail_file(&result);
        assert_eq!(items[0].flag_exist_after, true);
        assert_eq!(items[0].extension_used, Some("m4a".to_string()));
    }

    #[test]
    fn test_updated_ids_inline_when_few() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();
        let uuid = "test-uuid-ids-inline";
        let file_path = tmp.path().join(format!("{}.m4a", uuid));
        fs::write(&file_path, b"dummy").unwrap();

        let media = create_media(&conn, &MediaInput {
            title: "music inline ids".to_string(),
            media_type: MediaType::Music,
            uuid: Some(uuid.to_string()),
            path: Some(file_path.to_str().unwrap().to_string()),
            extension: Some("m4a".to_string()),
            flag_exist: Some(false),
            ..Default::default()
        }).unwrap();

        let result = update_exist(&conn, &MediaFilter::default(), None, &UpdateExistOptions::default()).unwrap();
        assert!(result.updated_ids.is_some());
        assert!(result.updated_ids_file.is_none());
        assert!(result.updated_ids.as_ref().unwrap().contains(&media.id));
        assert!(std::path::Path::new(&result.detail_file).exists());
    }

    #[test]
    fn test_detail_file_always_written() {
        let conn = setup_db();
        create_media(&conn, &MediaInput {
            title: "detail file test".to_string(),
            media_type: MediaType::Music,
            ..Default::default()
        }).unwrap();

        let result = update_exist(&conn, &MediaFilter::default(), None, &UpdateExistOptions::default()).unwrap();
        assert!(std::path::Path::new(&result.detail_file).exists());
        let items = read_detail_file(&result);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_updated_ids_file_when_over_limit() {
        let conn = setup_db();
        let tmp = TempDir::new().unwrap();

        // UPDATED_IDS_INLINE_LIMIT + 1 件のメディアを作成（全てflag_exist変化あり）
        for i in 0..=UPDATED_IDS_INLINE_LIMIT {
            let uuid = format!("test-uuid-many-{}", i);
            let file_path = tmp.path().join(format!("{}.m4a", uuid));
            fs::write(&file_path, b"dummy").unwrap();
            create_media(&conn, &MediaInput {
                title: format!("music {}", i),
                media_type: MediaType::Music,
                uuid: Some(uuid),
                path: Some(file_path.to_str().unwrap().to_string()),
                extension: Some("m4a".to_string()),
                flag_exist: Some(false),
                ..Default::default()
            }).unwrap();
        }

        let result = update_exist(&conn, &MediaFilter::default(), None, &UpdateExistOptions::default()).unwrap();
        assert!(result.updated_ids.is_none());
        assert!(result.updated_ids_file.is_some());
        let ids = read_ids_file(result.updated_ids_file.as_ref().unwrap());
        assert_eq!(ids.len(), UPDATED_IDS_INLINE_LIMIT + 1);
    }
}
