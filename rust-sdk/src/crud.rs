use crate::error::{KijukuError, Result};
use crate::types::{Media, MediaInput, MediaType, MediaUpdateInput};
use rusqlite::{params, Connection, Row};

/// volume_textからvolume_numberを計算
/// volume_textが整数に変換可能な場合のみ、その値を返す
fn calculate_volume_number(volume_text: Option<&str>) -> Option<i32> {
    volume_text.and_then(|text| {
        let trimmed = text.trim();
        trimmed.parse::<i32>().ok().filter(|num| num.to_string() == trimmed)
    })
}

/// SQLiteの行データをMediaオブジェクトに変換
pub(crate) fn row_to_media(row: &Row) -> rusqlite::Result<Media> {
    Ok(Media {
        id: row.get(0)?,
        title: row.get(1)?,
        title_id: row.get(2)?,
        path: row.get(3)?,
        media_type: {
            let type_str: String = row.get(4)?;
            MediaType::from_str(&type_str).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(KijukuError::Parse(format!("Invalid media_type: {}", type_str))),
                )
            })?
        },
        thumbnail_path: row.get(5)?,
        artist: row.get(6)?,
        artist_id: row.get(7)?,
        description: row.get(8)?,
        file_size: row.get(9)?,
        duration_sec: row.get(10)?,
        page_count: row.get(11)?,
        series: row.get(12)?,
        volume_number: row.get(13)?,
        volume_text: row.get(14)?,
        volume_title: row.get(15)?,
        magazine: row.get(16)?,
        magazine_id: row.get(17)?,
        language: row.get(18)?,
        source: row.get(19)?,
        external_id: row.get(20)?,
        artist_en: row.get(21)?,
        title_en: row.get(22)?,
        chapters: row.get(23)?,
        extension: row.get(24)?,
        flag_exist: {
            let flag: i64 = row.get(25)?;
            flag != 0
        },
        created_at: row.get(26)?,
        updated_at: row.get(27)?,
        title_pron: row.get(28)?,
        artist_pron: row.get(29)?,
        series_pron: row.get(30)?,
    })
}

/// メディアを作成
pub fn create_media(conn: &Connection, input: &MediaInput) -> Result<Media> {
    let flag_exist = if input.flag_exist.unwrap_or(false) {
        1
    } else {
        0
    };

    // volume_textからvolume_numberを自動計算
    let volume_number = calculate_volume_number(input.volume_text.as_deref());

    conn.execute(
        "INSERT INTO media (
            title, title_id, path, media_type, thumbnail_path,
            artist, artist_id, description, file_size, duration_sec,
            page_count, series, volume_number, volume_text, volume_title,
            magazine, magazine_id, language, source, external_id,
            artist_en, title_en, chapters, extension, flag_exist,
            title_pron, artist_pron, series_pron
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28
        )",
        params![
            input.title,
            input.title_id,
            input.path,
            input.media_type.as_str(),
            input.thumbnail_path,
            input.artist,
            input.artist_id,
            input.description,
            input.file_size,
            input.duration_sec,
            input.page_count,
            input.series,
            volume_number,
            input.volume_text,
            input.volume_title,
            input.magazine,
            input.magazine_id,
            input.language,
            input.source,
            input.external_id,
            input.artist_en,
            input.title_en,
            input.chapters,
            input.extension,
            flag_exist,
            input.title_pron,
            input.artist_pron,
            input.series_pron,
        ],
    )?;

    let id = conn.last_insert_rowid();
    get_media(conn, id).ok_or_else(|| KijukuError::NotFound("作成されたメディアが見つかりません".to_string()))
}

/// IDでメディアを取得
pub fn get_media(conn: &Connection, id: i64) -> Option<Media> {
    conn.query_row("SELECT * FROM media WHERE id = ?1", params![id], row_to_media)
        .ok()
}

/// メディアを更新（部分更新）
///
/// 指定されたフィールドのみ更新します。
/// 更新するフィールドが1つも指定されていない場合はエラーを返します。
pub fn update_media(conn: &Connection, id: i64, input: &MediaUpdateInput) -> Result<()> {
    // メディアが存在するか確認
    if get_media(conn, id).is_none() {
        return Err(KijukuError::NotFound(format!(
            "メディアが見つかりません: id={}",
            id
        )));
    }

    // 動的にUPDATE文を構築（指定されたフィールドのみ更新）
    let mut update_fields: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_idx = 1;

    // NOT NULLフィールド: titleとmedia_type
    if let Some(ref val) = input.title {
        update_fields.push(format!("title = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.media_type {
        update_fields.push(format!("media_type = ?{}", param_idx));
        params.push(Box::new(val.as_str().to_string()));
        param_idx += 1;
    }

    // NULLableフィールド: Option<Option<T>>パターン
    // Some(Some(val)) -> 値を設定, Some(None) -> NULLを設定, None -> 更新しない
    if let Some(ref opt_val) = input.title_id {
        update_fields.push(format!("title_id = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.path {
        update_fields.push(format!("path = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.thumbnail_path {
        update_fields.push(format!("thumbnail_path = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.artist {
        update_fields.push(format!("artist = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.artist_id {
        update_fields.push(format!("artist_id = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.description {
        update_fields.push(format!("description = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.file_size {
        update_fields.push(format!("file_size = ?{}", param_idx));
        params.push(Box::new(*opt_val));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.duration_sec {
        update_fields.push(format!("duration_sec = ?{}", param_idx));
        params.push(Box::new(*opt_val));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.page_count {
        update_fields.push(format!("page_count = ?{}", param_idx));
        params.push(Box::new(*opt_val));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.series {
        update_fields.push(format!("series = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.volume_text {
        // volume_textが更新される場合、volume_numberも再計算
        let volume_number = opt_val.as_ref().and_then(|v| calculate_volume_number(Some(v.as_str())));
        update_fields.push(format!("volume_text = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
        update_fields.push(format!("volume_number = ?{}", param_idx));
        params.push(Box::new(volume_number));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.volume_title {
        update_fields.push(format!("volume_title = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.magazine {
        update_fields.push(format!("magazine = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.magazine_id {
        update_fields.push(format!("magazine_id = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.language {
        update_fields.push(format!("language = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.source {
        update_fields.push(format!("source = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.external_id {
        update_fields.push(format!("external_id = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.artist_en {
        update_fields.push(format!("artist_en = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.title_en {
        update_fields.push(format!("title_en = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.chapters {
        update_fields.push(format!("chapters = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.extension {
        update_fields.push(format!("extension = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    // NOT NULLフィールド: flag_exist
    if let Some(val) = input.flag_exist {
        update_fields.push(format!("flag_exist = ?{}", param_idx));
        params.push(Box::new(val));
        param_idx += 1;
    }
    // NULLableフィールド: pron系
    if let Some(ref opt_val) = input.title_pron {
        update_fields.push(format!("title_pron = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.artist_pron {
        update_fields.push(format!("artist_pron = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }
    if let Some(ref opt_val) = input.series_pron {
        update_fields.push(format!("series_pron = ?{}", param_idx));
        params.push(Box::new(opt_val.clone()));
        param_idx += 1;
    }

    // 更新するフィールドがない場合はエラー
    if update_fields.is_empty() {
        return Err(KijukuError::Validation(
            "更新するフィールドが指定されていません".to_string(),
        ));
    }

    let sql = format!(
        "UPDATE media SET {} WHERE id = ?{}",
        update_fields.join(", "),
        param_idx
    );
    params.push(Box::new(id));

    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    conn.execute(&sql, params_refs.as_slice())?;

    Ok(())
}

/// メディアを削除
pub fn delete_media(conn: &Connection, id: i64) -> Result<()> {
    // メディアが存在するか確認
    if get_media(conn, id).is_none() {
        return Err(KijukuError::NotFound(format!(
            "メディアが見つかりません: id={}",
            id
        )));
    }

    conn.execute("DELETE FROM media WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration;

    #[test]
    fn test_create_and_get_media() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let input = MediaInput {
            title: "テストコミック".to_string(),
            media_type: MediaType::Comic,
            artist: Some("テスト作者".to_string()),
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();
        assert_eq!(media.title, "テストコミック");
        assert_eq!(media.media_type, MediaType::Comic);
        assert_eq!(media.artist, Some("テスト作者".to_string()));

        let fetched = get_media(&conn, media.id).unwrap();
        assert_eq!(fetched.title, media.title);
    }

    #[test]
    fn test_update_media() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let input = MediaInput {
            title: "元のタイトル".to_string(),
            media_type: MediaType::Comic,
            artist: Some("元の作者".to_string()),
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();

        // descriptionのみ更新（titleやartistは変更されない）
        // Option<Option<T>>: Some(Some(val)) で値を設定
        let update_input = crate::types::MediaUpdateInput {
            description: Some(Some("説明追加".to_string())),
            ..Default::default()
        };

        update_media(&conn, media.id, &update_input).unwrap();

        let updated = get_media(&conn, media.id).unwrap();
        // titleは元のまま
        assert_eq!(updated.title, "元のタイトル");
        // artistも元のまま
        assert_eq!(updated.artist, Some("元の作者".to_string()));
        // descriptionのみ更新された
        assert_eq!(updated.description, Some("説明追加".to_string()));
    }

    #[test]
    fn test_update_media_partial() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let input = MediaInput {
            title: "テストタイトル".to_string(),
            media_type: MediaType::Comic,
            artist: Some("作者A".to_string()),
            series: Some("シリーズA".to_string()),
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();

        // titleのみ更新
        let update_input = crate::types::MediaUpdateInput {
            title: Some("新しいタイトル".to_string()),
            ..Default::default()
        };

        update_media(&conn, media.id, &update_input).unwrap();

        let updated = get_media(&conn, media.id).unwrap();
        assert_eq!(updated.title, "新しいタイトル");
        // 他のフィールドは変更されていない
        assert_eq!(updated.artist, Some("作者A".to_string()));
        assert_eq!(updated.series, Some("シリーズA".to_string()));
        assert_eq!(updated.media_type, MediaType::Comic);
    }

    #[test]
    fn test_update_media_empty_input() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let input = MediaInput {
            title: "テスト".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();

        // 空の更新はエラーになる
        let update_input = crate::types::MediaUpdateInput::default();
        let result = update_media(&conn, media.id, &update_input);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_media() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let input = MediaInput {
            title: "削除テスト".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();
        assert!(get_media(&conn, media.id).is_some());

        delete_media(&conn, media.id).unwrap();
        assert!(get_media(&conn, media.id).is_none());
    }

    #[test]
    fn test_delete_nonexistent() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let result = delete_media(&conn, 9999);
        assert!(result.is_err());
    }

    #[test]
    fn test_volume_number_auto_calculation_integer() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // 整数のvolume_textはvolume_numberに変換される
        let input = MediaInput {
            title: "テスト".to_string(),
            media_type: MediaType::Comic,
            volume_text: Some("5".to_string()),
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();
        assert_eq!(media.volume_number, Some(5));
        assert_eq!(media.volume_text, Some("5".to_string()));
    }

    #[test]
    fn test_volume_number_auto_calculation_non_integer() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // 非整数のvolume_textはvolume_numberがNone
        let input = MediaInput {
            title: "テスト".to_string(),
            media_type: MediaType::Comic,
            volume_text: Some("1.5".to_string()),
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();
        assert_eq!(media.volume_number, None);
        assert_eq!(media.volume_text, Some("1.5".to_string()));
    }

    #[test]
    fn test_volume_number_auto_calculation_text() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // テキストのvolume_textはvolume_numberがNone
        let input = MediaInput {
            title: "テスト".to_string(),
            media_type: MediaType::Comic,
            volume_text: Some("上巻".to_string()),
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();
        assert_eq!(media.volume_number, None);
        assert_eq!(media.volume_text, Some("上巻".to_string()));
    }

    #[test]
    fn test_volume_number_auto_calculation_with_whitespace() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // 前後にスペースがある整数はトリムされてvolume_numberに変換
        let input = MediaInput {
            title: "テスト".to_string(),
            media_type: MediaType::Comic,
            volume_text: Some(" 10 ".to_string()),
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();
        assert_eq!(media.volume_number, Some(10));
    }

    #[test]
    fn test_volume_number_update_recalculation() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let input = MediaInput {
            title: "テスト".to_string(),
            media_type: MediaType::Comic,
            volume_text: Some("1".to_string()),
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();
        assert_eq!(media.volume_number, Some(1));

        // volume_textを更新するとvolume_numberも再計算される
        // Option<Option<T>>: Some(Some(val)) で値を設定
        let update_input = crate::types::MediaUpdateInput {
            volume_text: Some(Some("2".to_string())),
            ..Default::default()
        };
        update_media(&conn, media.id, &update_input).unwrap();

        let updated = get_media(&conn, media.id).unwrap();
        assert_eq!(updated.volume_number, Some(2));
        assert_eq!(updated.volume_text, Some("2".to_string()));
    }

    #[test]
    fn test_update_media_set_null() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // artistとdescriptionを持つメディアを作成
        let input = MediaInput {
            title: "テスト".to_string(),
            media_type: MediaType::Comic,
            artist: Some("作者名".to_string()),
            description: Some("説明文".to_string()),
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();
        assert_eq!(media.artist, Some("作者名".to_string()));
        assert_eq!(media.description, Some("説明文".to_string()));

        // artistをNULLに設定（Some(None)）、descriptionは更新しない（None）
        let update_input = crate::types::MediaUpdateInput {
            artist: Some(None),  // NULLに設定
            // description: None - 更新しない
            ..Default::default()
        };
        update_media(&conn, media.id, &update_input).unwrap();

        let updated = get_media(&conn, media.id).unwrap();
        // artistがNULLになった
        assert_eq!(updated.artist, None);
        // descriptionは元のまま
        assert_eq!(updated.description, Some("説明文".to_string()));
    }

    #[test]
    fn test_update_media_volume_text_to_null() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let input = MediaInput {
            title: "テスト".to_string(),
            media_type: MediaType::Comic,
            volume_text: Some("5".to_string()),
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();
        assert_eq!(media.volume_number, Some(5));
        assert_eq!(media.volume_text, Some("5".to_string()));

        // volume_textをNULLに設定するとvolume_numberもNULLになる
        let update_input = crate::types::MediaUpdateInput {
            volume_text: Some(None),  // NULLに設定
            ..Default::default()
        };
        update_media(&conn, media.id, &update_input).unwrap();

        let updated = get_media(&conn, media.id).unwrap();
        assert_eq!(updated.volume_number, None);
        assert_eq!(updated.volume_text, None);
    }

    #[test]
    fn test_duplicate_path_error() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let path = "/path/to/media.cbz";

        // 1つ目のメディアを作成
        let input1 = MediaInput {
            title: "メディア1".to_string(),
            media_type: MediaType::Comic,
            path: Some(path.to_string()),
            ..Default::default()
        };
        create_media(&conn, &input1).unwrap();

        // 同じpathで2つ目のメディアを作成しようとするとエラー
        let input2 = MediaInput {
            title: "メディア2".to_string(),
            media_type: MediaType::Comic,
            path: Some(path.to_string()),
            ..Default::default()
        };
        let result = create_media(&conn, &input2);
        assert!(result.is_err());
    }
}
