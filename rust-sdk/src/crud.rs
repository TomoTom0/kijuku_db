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

/// メディアを更新
pub fn update_media(conn: &Connection, id: i64, input: &MediaInput) -> Result<()> {
    // メディアが存在するか確認
    if get_media(conn, id).is_none() {
        return Err(KijukuError::NotFound(format!(
            "メディアが見つかりません: id={}",
            id
        )));
    }

    // 動的にUPDATE文を構築（指定されたフィールドのみ更新）
    let mut update_fields = vec!["title = ?1".to_string()];
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(input.title.clone())];
    let mut param_idx = 2;

    if let Some(ref val) = input.title_id {
        update_fields.push(format!("title_id = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.path {
        update_fields.push(format!("path = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    update_fields.push(format!("media_type = ?{}", param_idx));
    params.push(Box::new(input.media_type.as_str().to_string()));
    param_idx += 1;
    
    if let Some(ref val) = input.thumbnail_path {
        update_fields.push(format!("thumbnail_path = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.artist {
        update_fields.push(format!("artist = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.artist_id {
        update_fields.push(format!("artist_id = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.description {
        update_fields.push(format!("description = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(val) = input.file_size {
        update_fields.push(format!("file_size = ?{}", param_idx));
        params.push(Box::new(val));
        param_idx += 1;
    }
    if let Some(val) = input.duration_sec {
        update_fields.push(format!("duration_sec = ?{}", param_idx));
        params.push(Box::new(val));
        param_idx += 1;
    }
    if let Some(val) = input.page_count {
        update_fields.push(format!("page_count = ?{}", param_idx));
        params.push(Box::new(val));
        param_idx += 1;
    }
    if let Some(ref val) = input.series {
        update_fields.push(format!("series = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.volume_text {
        let volume_number = calculate_volume_number(Some(val.as_str()));
        update_fields.push(format!("volume_text = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
        update_fields.push(format!("volume_number = ?{}", param_idx));
        params.push(Box::new(volume_number));
        param_idx += 1;
    }
    if let Some(ref val) = input.volume_title {
        update_fields.push(format!("volume_title = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.magazine {
        update_fields.push(format!("magazine = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.magazine_id {
        update_fields.push(format!("magazine_id = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.language {
        update_fields.push(format!("language = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.source {
        update_fields.push(format!("source = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.external_id {
        update_fields.push(format!("external_id = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.artist_en {
        update_fields.push(format!("artist_en = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.title_en {
        update_fields.push(format!("title_en = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.chapters {
        update_fields.push(format!("chapters = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.extension {
        update_fields.push(format!("extension = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(val) = input.flag_exist {
        update_fields.push(format!("flag_exist = ?{}", param_idx));
        params.push(Box::new(val));
        param_idx += 1;
    }
    if let Some(ref val) = input.title_pron {
        update_fields.push(format!("title_pron = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.artist_pron {
        update_fields.push(format!("artist_pron = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.series_pron {
        update_fields.push(format!("series_pron = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
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

/// メディアを部分更新（指定されたフィールドのみ更新）
///
/// MediaUpdateInputを使用して、指定されたフィールドのみを更新します。
/// 更新するフィールドが1つも指定されていない場合はエラーを返します。
pub fn update_media_partial(conn: &Connection, id: i64, input: &MediaUpdateInput) -> Result<()> {
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

    if let Some(ref val) = input.title {
        update_fields.push(format!("title = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.title_id {
        update_fields.push(format!("title_id = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.path {
        update_fields.push(format!("path = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.media_type {
        update_fields.push(format!("media_type = ?{}", param_idx));
        params.push(Box::new(val.as_str().to_string()));
        param_idx += 1;
    }
    if let Some(ref val) = input.thumbnail_path {
        update_fields.push(format!("thumbnail_path = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.artist {
        update_fields.push(format!("artist = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.artist_id {
        update_fields.push(format!("artist_id = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.description {
        update_fields.push(format!("description = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(val) = input.file_size {
        update_fields.push(format!("file_size = ?{}", param_idx));
        params.push(Box::new(val));
        param_idx += 1;
    }
    if let Some(val) = input.duration_sec {
        update_fields.push(format!("duration_sec = ?{}", param_idx));
        params.push(Box::new(val));
        param_idx += 1;
    }
    if let Some(val) = input.page_count {
        update_fields.push(format!("page_count = ?{}", param_idx));
        params.push(Box::new(val));
        param_idx += 1;
    }
    if let Some(ref val) = input.series {
        update_fields.push(format!("series = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.volume_text {
        let volume_number = calculate_volume_number(Some(val.as_str()));
        update_fields.push(format!("volume_text = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
        update_fields.push(format!("volume_number = ?{}", param_idx));
        params.push(Box::new(volume_number));
        param_idx += 1;
    }
    if let Some(ref val) = input.volume_title {
        update_fields.push(format!("volume_title = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.magazine {
        update_fields.push(format!("magazine = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.magazine_id {
        update_fields.push(format!("magazine_id = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.language {
        update_fields.push(format!("language = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.source {
        update_fields.push(format!("source = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.external_id {
        update_fields.push(format!("external_id = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.artist_en {
        update_fields.push(format!("artist_en = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.title_en {
        update_fields.push(format!("title_en = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.chapters {
        update_fields.push(format!("chapters = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.extension {
        update_fields.push(format!("extension = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(val) = input.flag_exist {
        update_fields.push(format!("flag_exist = ?{}", param_idx));
        params.push(Box::new(val));
        param_idx += 1;
    }
    if let Some(ref val) = input.title_pron {
        update_fields.push(format!("title_pron = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.artist_pron {
        update_fields.push(format!("artist_pron = ?{}", param_idx));
        params.push(Box::new(val.clone()));
        param_idx += 1;
    }
    if let Some(ref val) = input.series_pron {
        update_fields.push(format!("series_pron = ?{}", param_idx));
        params.push(Box::new(val.clone()));
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
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();

        let update_input = MediaInput {
            title: "更新されたタイトル".to_string(),
            media_type: MediaType::Comic,
            description: Some("説明追加".to_string()),
            ..Default::default()
        };

        update_media(&conn, media.id, &update_input).unwrap();

        let updated = get_media(&conn, media.id).unwrap();
        assert_eq!(updated.title, "更新されたタイトル");
        assert_eq!(updated.description, Some("説明追加".to_string()));
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
}
