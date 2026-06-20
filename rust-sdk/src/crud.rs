use crate::db_value::{SqlParam, SqlRow};
use crate::error::{KijukuError, Result};
use crate::exec::SqlExec;
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
        id: row.get("id")?,
        uuid: row.get("uuid")?,
        title: row.get("title")?,
        title_id: row.get("title_id")?,
        path: row.get("path")?,
        media_type: {
            let type_str: String = row.get("media_type")?;
            MediaType::from_str(&type_str).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    0, // 列名アクセスのため動的インデックス取得は不要; プレースホルダーとして0を使用
                    rusqlite::types::Type::Text,
                    Box::new(KijukuError::Parse(format!("Invalid media_type: {}", type_str))),
                )
            })?
        },
        thumbnail_path: row.get("thumbnail_path")?,
        artist: row.get("artist")?,
        artist_id: row.get("artist_id")?,
        description: row.get("description")?,
        file_size: row.get("file_size")?,
        duration_sec: row.get("duration_sec")?,
        page_count: row.get("page_count")?,
        series: row.get("series")?,
        volume_number: row.get("volume_number")?,
        volume_text: row.get("volume_text")?,
        volume_title: row.get("volume_title")?,
        magazine: row.get("magazine")?,
        magazine_id: row.get("magazine_id")?,
        language: row.get("language")?,
        source: row.get("source")?,
        external_id: row.get("external_id")?,
        artist_en: row.get("artist_en")?,
        title_en: row.get("title_en")?,
        chapters: row.get("chapters")?,
        extension: row.get("extension")?,
        flag_exist: {
            let flag: i64 = row.get("flag_exist")?;
            flag != 0
        },
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        title_pron: row.get("title_pron")?,
        artist_pron: row.get("artist_pron")?,
        series_pron: row.get("series_pron")?,
    })
}

/// SqlRow から Media へ変換（Local/D1 の async バックエンド共通）
pub(crate) fn row_to_media_from_sqlrow(row: &SqlRow) -> Result<Media> {
    let type_str = row.get_text("media_type")?;
    let media_type = MediaType::from_str(&type_str)
        .ok_or_else(|| KijukuError::Parse(format!("Invalid media_type: {}", type_str)))?;
    Ok(Media {
        id: row.get_int("id")?,
        uuid: row.get_text("uuid")?,
        title: row.get_text("title")?,
        title_id: row.get_opt_text("title_id")?,
        path: row.get_opt_text("path")?,
        media_type,
        thumbnail_path: row.get_opt_text("thumbnail_path")?,
        artist: row.get_opt_text("artist")?,
        artist_id: row.get_opt_text("artist_id")?,
        description: row.get_opt_text("description")?,
        file_size: row.get_opt_int("file_size")?,
        duration_sec: row.get_opt_i32("duration_sec")?,
        page_count: row.get_opt_i32("page_count")?,
        series: row.get_opt_text("series")?,
        volume_number: row.get_opt_i32("volume_number")?,
        volume_text: row.get_opt_text("volume_text")?,
        volume_title: row.get_opt_text("volume_title")?,
        magazine: row.get_opt_text("magazine")?,
        magazine_id: row.get_opt_text("magazine_id")?,
        language: row.get_opt_text("language")?,
        source: row.get_opt_text("source")?,
        external_id: row.get_opt_text("external_id")?,
        artist_en: row.get_opt_text("artist_en")?,
        title_en: row.get_opt_text("title_en")?,
        chapters: row.get_opt_text("chapters")?,
        extension: row.get_opt_text("extension")?,
        flag_exist: row.get_bool("flag_exist")?,
        created_at: row.get_datetime("created_at")?,
        updated_at: row.get_datetime("updated_at")?,
        title_pron: row.get_opt_text("title_pron")?,
        artist_pron: row.get_opt_text("artist_pron")?,
        series_pron: row.get_opt_text("series_pron")?,
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

    // UUID: 手動指定があればそれを使用、なければv4を自動生成
    let uuid = input.uuid.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    conn.execute(
        "INSERT INTO media (
            uuid, title, title_id, path, media_type, thumbnail_path,
            artist, artist_id, description, file_size, duration_sec,
            page_count, series, volume_number, volume_text, volume_title,
            magazine, magazine_id, language, source, external_id,
            artist_en, title_en, chapters, extension, flag_exist,
            title_pron, artist_pron, series_pron
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29
        )",
        params![
            uuid,
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
/// 更新するフィールドが1つも指定されていない場合は何もしません。
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

    // UUID更新（NOT NULL制約あり: Some(None)はエラー）
    if let Some(ref opt_val) = input.uuid {
        match opt_val {
            Some(val) => {
                update_fields.push(format!("uuid = ?{}", param_idx));
                params.push(Box::new(val.clone()));
                param_idx += 1;
            }
            None => {
                return Err(KijukuError::Validation(
                    "UUID cannot be set to null.".to_string(),
                ));
            }
        }
    }

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

    // 更新するフィールドがない場合は何もしない（TypeScript SDKとの動作統一）
    if update_fields.is_empty() {
        return Ok(());
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

// ========== async バックエンド（SqlExec）用 ==========
//
// 同期版（`create_media(conn, ...)` 等）は現状維持。async 版は `build_*_sql` 純粋関数で
// SQL とパラメータを組み立て、`&dyn SqlExec` で実行する。Local と D1 がこの組み立て層を
// 共有し、D1 互換SQL を自動生成する（クエリ再実装の回避）。
// `INSERT ... RETURNING *` で挿入行を1往復で取得（D1 で last_insert_rowid が信頼できないため）。

/// `create_media` の SQL とパラメータを構築（`RETURNING *`）
pub fn build_create_media_sql(input: &MediaInput) -> (String, Vec<SqlParam>) {
    let flag_exist = if input.flag_exist.unwrap_or(false) {
        1i64
    } else {
        0
    };
    let volume_number = calculate_volume_number(input.volume_text.as_deref());
    let uuid = input
        .uuid
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let params = vec![
        SqlParam::Text(uuid),
        SqlParam::Text(input.title.clone()),
        SqlParam::from_opt_str(&input.title_id),
        SqlParam::from_opt_str(&input.path),
        SqlParam::Text(input.media_type.as_str().to_string()),
        SqlParam::from_opt_str(&input.thumbnail_path),
        SqlParam::from_opt_str(&input.artist),
        SqlParam::from_opt_str(&input.artist_id),
        SqlParam::from_opt_str(&input.description),
        SqlParam::from_opt_i64(input.file_size),
        SqlParam::from_opt_i64(input.duration_sec.map(|i| i as i64)),
        SqlParam::from_opt_i64(input.page_count.map(|i| i as i64)),
        SqlParam::from_opt_str(&input.series),
        SqlParam::from_opt_i64(volume_number.map(|i| i as i64)),
        SqlParam::from_opt_str(&input.volume_text),
        SqlParam::from_opt_str(&input.volume_title),
        SqlParam::from_opt_str(&input.magazine),
        SqlParam::from_opt_str(&input.magazine_id),
        SqlParam::from_opt_str(&input.language),
        SqlParam::from_opt_str(&input.source),
        SqlParam::from_opt_str(&input.external_id),
        SqlParam::from_opt_str(&input.artist_en),
        SqlParam::from_opt_str(&input.title_en),
        SqlParam::from_opt_str(&input.chapters),
        SqlParam::from_opt_str(&input.extension),
        SqlParam::Int(flag_exist),
        SqlParam::from_opt_str(&input.title_pron),
        SqlParam::from_opt_str(&input.artist_pron),
        SqlParam::from_opt_str(&input.series_pron),
    ];

    let sql = "INSERT INTO media (
        uuid, title, title_id, path, media_type, thumbnail_path,
        artist, artist_id, description, file_size, duration_sec,
        page_count, series, volume_number, volume_text, volume_title,
        magazine, magazine_id, language, source, external_id,
        artist_en, title_en, chapters, extension, flag_exist,
        title_pron, artist_pron, series_pron
    ) VALUES (
        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
        ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
        ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29
    ) RETURNING *";

    (sql.to_string(), params)
}

/// async バックエンド経由でメディアを作成
pub async fn create_media_async(exec: &dyn SqlExec, input: &MediaInput) -> Result<Media> {
    let (sql, params) = build_create_media_sql(input);
    let rows = exec.query(&sql, &params).await?;
    rows.into_iter()
        .next()
        .ok_or_else(|| KijukuError::NotFound("作成されたメディアが見つかりません".to_string()))
        .and_then(|row| row_to_media_from_sqlrow(&row))
}

/// async バックエンド経由で ID からメディアを取得
pub async fn get_media_async(exec: &dyn SqlExec, id: i64) -> Result<Option<Media>> {
    let sql = "SELECT * FROM media WHERE id = ?1";
    let params = vec![SqlParam::Int(id)];
    let rows = exec.query(sql, &params).await?;
    match rows.into_iter().next() {
        Some(row) => Ok(Some(row_to_media_from_sqlrow(&row)?)),
        None => Ok(None),
    }
}

/// async バックエンド経由でメディアを削除
pub async fn delete_media_async(exec: &dyn SqlExec, id: i64) -> Result<()> {
    if get_media_async(exec, id).await?.is_none() {
        return Err(KijukuError::NotFound(format!(
            "メディアが見つかりません: id={}",
            id
        )));
    }
    let sql = "DELETE FROM media WHERE id = ?1";
    let params = vec![SqlParam::Int(id)];
    exec.execute(sql, &params).await?;
    Ok(())
}

/// `update_media` の SQL とパラメータを構築。更新フィールドが1つも無い場合は None。
///
/// 同期版 `update_media(conn, ...)` と同じ意味論（NOT NULL 項目の NULL 拒否、
/// volume_text 更新時の volume_number 再計算、空入力は何もしない）。
pub fn build_update_media_sql(
    id: i64,
    input: &MediaUpdateInput,
) -> Result<Option<(String, Vec<SqlParam>)>> {
    let mut fields: Vec<String> = Vec::new();
    let mut params: Vec<SqlParam> = Vec::new();
    let mut idx = 1usize;

    // NULLable 文字列（Option<Option<String>>）の共通処理
    macro_rules! push_str_field {
        ($name:literal, $opt:expr) => {
            if let Some(ref opt_val) = $opt {
                fields.push(format!("{} = ?{}", $name, idx));
                params.push(SqlParam::from_opt_str(opt_val));
                idx += 1;
            }
        };
    }

    // NOT NULL: uuid（Some(None) はエラー）
    if let Some(ref opt_val) = input.uuid {
        match opt_val {
            Some(val) => {
                fields.push(format!("uuid = ?{}", idx));
                params.push(SqlParam::Text(val.clone()));
                idx += 1;
            }
            None => {
                return Err(KijukuError::Validation(
                    "UUID cannot be set to null.".to_string(),
                ));
            }
        }
    }
    // NOT NULL: title, media_type
    if let Some(ref val) = input.title {
        fields.push(format!("title = ?{}", idx));
        params.push(SqlParam::Text(val.clone()));
        idx += 1;
    }
    if let Some(ref val) = input.media_type {
        fields.push(format!("media_type = ?{}", idx));
        params.push(SqlParam::Text(val.as_str().to_string()));
        idx += 1;
    }

    push_str_field!("title_id", input.title_id);
    push_str_field!("path", input.path);
    push_str_field!("thumbnail_path", input.thumbnail_path);
    push_str_field!("artist", input.artist);
    push_str_field!("artist_id", input.artist_id);
    push_str_field!("description", input.description);

    // 整数系（Option<Option<i64>> / Option<Option<i32>>）
    if let Some(ref opt_val) = input.file_size {
        fields.push(format!("file_size = ?{}", idx));
        params.push(SqlParam::from_opt_i64(*opt_val));
        idx += 1;
    }
    if let Some(ref opt_val) = input.duration_sec {
        fields.push(format!("duration_sec = ?{}", idx));
        params.push(SqlParam::from_opt_i64(opt_val.map(|i| i as i64)));
        idx += 1;
    }
    if let Some(ref opt_val) = input.page_count {
        fields.push(format!("page_count = ?{}", idx));
        params.push(SqlParam::from_opt_i64(opt_val.map(|i| i as i64)));
        idx += 1;
    }

    push_str_field!("series", input.series);

    // volume_text 更新時は volume_number も再計算
    if let Some(ref opt_val) = input.volume_text {
        let volume_number =
            opt_val.as_ref().and_then(|v| calculate_volume_number(Some(v.as_str())));
        fields.push(format!("volume_text = ?{}", idx));
        params.push(SqlParam::from_opt_str(opt_val));
        idx += 1;
        fields.push(format!("volume_number = ?{}", idx));
        params.push(SqlParam::from_opt_i64(volume_number.map(|i| i as i64)));
        idx += 1;
    }

    push_str_field!("volume_title", input.volume_title);
    push_str_field!("magazine", input.magazine);
    push_str_field!("magazine_id", input.magazine_id);
    push_str_field!("language", input.language);
    push_str_field!("source", input.source);
    push_str_field!("external_id", input.external_id);
    push_str_field!("artist_en", input.artist_en);
    push_str_field!("title_en", input.title_en);
    push_str_field!("chapters", input.chapters);
    push_str_field!("extension", input.extension);

    // NOT NULL: flag_exist（Option<bool>）
    if let Some(val) = input.flag_exist {
        fields.push(format!("flag_exist = ?{}", idx));
        params.push(SqlParam::Int(if val { 1 } else { 0 }));
        idx += 1;
    }

    push_str_field!("title_pron", input.title_pron);
    push_str_field!("artist_pron", input.artist_pron);
    push_str_field!("series_pron", input.series_pron);

    if fields.is_empty() {
        return Ok(None);
    }

    let sql = format!("UPDATE media SET {} WHERE id = ?{}", fields.join(", "), idx);
    params.push(SqlParam::Int(id));

    Ok(Some((sql, params)))
}

/// async バックエンド経由でメディアを更新（部分更新）
pub async fn update_media_async(
    exec: &dyn SqlExec,
    id: i64,
    input: &MediaUpdateInput,
) -> Result<()> {
    if get_media_async(exec, id).await?.is_none() {
        return Err(KijukuError::NotFound(format!(
            "メディアが見つかりません: id={}",
            id
        )));
    }
    if let Some((sql, params)) = build_update_media_sql(id, input)? {
        exec.execute(&sql, &params).await?;
    }
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

        // 空の更新はエラーにならず、何もしない（TypeScript SDKとの動作統一）
        let update_input = crate::types::MediaUpdateInput::default();
        let result = update_media(&conn, media.id, &update_input);
        assert!(result.is_ok());

        // メディアの内容が変更されていないことを確認
        let fetched = get_media(&conn, media.id).unwrap();
        assert_eq!(fetched.title, "テスト");
        assert_eq!(fetched.media_type, MediaType::Comic);
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

    #[test]
    fn test_create_media_with_manual_uuid() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let manual_uuid = "550e8400-e29b-41d4-a716-446655440000".to_string();
        let input = MediaInput {
            title: "UUIDテスト".to_string(),
            media_type: MediaType::Comic,
            uuid: Some(manual_uuid.clone()),
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();
        assert_eq!(media.uuid, manual_uuid);
    }

    #[test]
    fn test_create_media_auto_uuid() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let input = MediaInput {
            title: "自動UUID".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        };

        let media = create_media(&conn, &input).unwrap();
        // 自動生成されたUUIDがUUID v4形式であることを確認
        assert_eq!(media.uuid.len(), 36);
        assert_eq!(media.uuid.chars().filter(|&c| c == '-').count(), 4);
    }

    #[test]
    fn test_update_media_uuid() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let input = MediaInput {
            title: "UUID更新テスト".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        };
        let media = create_media(&conn, &input).unwrap();
        let original_uuid = media.uuid.clone();

        let new_uuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_string();
        let update_input = crate::types::MediaUpdateInput {
            uuid: Some(Some(new_uuid.clone())),
            ..Default::default()
        };
        update_media(&conn, media.id, &update_input).unwrap();

        let updated = get_media(&conn, media.id).unwrap();
        assert_eq!(updated.uuid, new_uuid);
        assert_ne!(updated.uuid, original_uuid);
    }

    #[test]
    fn test_duplicate_uuid_error() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let uuid = "550e8400-e29b-41d4-a716-446655440000".to_string();

        let input1 = MediaInput {
            title: "メディア1".to_string(),
            media_type: MediaType::Comic,
            uuid: Some(uuid.clone()),
            ..Default::default()
        };
        create_media(&conn, &input1).unwrap();

        let input2 = MediaInput {
            title: "メディア2".to_string(),
            media_type: MediaType::Comic,
            uuid: Some(uuid.clone()),
            ..Default::default()
        };
        let result = create_media(&conn, &input2);
        assert!(result.is_err());
    }
}
