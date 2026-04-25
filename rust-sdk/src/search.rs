use crate::crud::row_to_media;
use crate::error::{KijukuError, Result};
use crate::types::{Media, MediaFilter, QueryOptions};
use rusqlite::Connection;

/// 取得可能なフィールド名のホワイトリスト（SQLインジェクション防止）
pub const ALLOWED_DISTINCT_FIELDS: &[&str] = &[
    "title",
    "title_id",
    "artist",
    "artist_id",
    "media_type",
    "series",
    "volume_text",
    "volume_title",
    "magazine",
    "magazine_id",
    "language",
    "source",
    "external_id",
    "artist_en",
    "title_en",
    "chapters",
    "extension",
    "title_pron",
    "artist_pron",
    "series_pron",
];

fn add_like_filter(
    where_clauses: &mut Vec<String>,
    params: &mut Vec<Box<dyn rusqlite::ToSql>>,
    column_name: &str,
    value: &Option<String>,
) {
    if let Some(ref val) = value {
        where_clauses.push(format!("m.{} LIKE ?", column_name));
        params.push(Box::new(format!("%{}%", val)));
    }
}

/// フィルタ条件の構築結果
struct FilterConditions {
    /// WHERE句の条件（AND結合済み）
    condition: Option<String>,
    /// パラメータ
    params: Vec<Box<dyn rusqlite::ToSql>>,
    /// タグフィルタを使用するか
    needs_tag_join: bool,
}

/// 単一のMediaFilterから条件を構築
fn build_filter_conditions(filter: &MediaFilter) -> FilterConditions {
    let mut where_clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut needs_tag_join = false;

    // 各フィルタ条件を追加
    add_like_filter(&mut where_clauses, &mut params, "title", &filter.title);
    if let Some(ref title_id) = filter.title_id {
        where_clauses.push("m.title_id = ?".to_string());
        params.push(Box::new(title_id.clone()));
    }
    add_like_filter(&mut where_clauses, &mut params, "artist", &filter.artist);
    if let Some(ref artist_id) = filter.artist_id {
        where_clauses.push("m.artist_id = ?".to_string());
        params.push(Box::new(artist_id.clone()));
    }
    if let Some(ref media_type) = filter.media_type {
        where_clauses.push("m.media_type = ?".to_string());
        params.push(Box::new(media_type.as_str().to_string()));
    }
    add_like_filter(&mut where_clauses, &mut params, "series", &filter.series);
    if let Some(ref source) = filter.source {
        where_clauses.push("m.source = ?".to_string());
        params.push(Box::new(source.clone()));
    }
    if let Some(flag_exist) = filter.flag_exist {
        where_clauses.push("m.flag_exist = ?".to_string());
        params.push(Box::new(if flag_exist { 1 } else { 0 }));
    }
    if let Some(ref language) = filter.language {
        where_clauses.push("m.language = ?".to_string());
        params.push(Box::new(language.clone()));
    }
    add_like_filter(&mut where_clauses, &mut params, "magazine", &filter.magazine);
    if let Some(ref magazine_id) = filter.magazine_id {
        where_clauses.push("m.magazine_id = ?".to_string());
        params.push(Box::new(magazine_id.clone()));
    }
    if let Some(ref extension) = filter.extension {
        where_clauses.push("m.extension = ?".to_string());
        params.push(Box::new(extension.clone()));
    }
    if let Some(ref external_id) = filter.external_id {
        where_clauses.push("m.external_id = ?".to_string());
        params.push(Box::new(external_id.clone()));
    }
    add_like_filter(&mut where_clauses, &mut params, "volume_title", &filter.volume_title);
    add_like_filter(&mut where_clauses, &mut params, "title_en", &filter.title_en);
    add_like_filter(&mut where_clauses, &mut params, "artist_en", &filter.artist_en);

    // id_inフィルタの処理
    // SQLiteのパラメータ数上限（デフォルト999）を考慮してチャンク分割
    if let Some(ref ids) = filter.id_in {
        if !ids.is_empty() {
            const CHUNK_SIZE: usize = 999;
            let in_clauses: Vec<String> = ids.chunks(CHUNK_SIZE).map(|chunk| {
                format!("m.id IN ({})", vec!["?"; chunk.len()].join(", "))
            }).collect();
            where_clauses.push(format!("({})", in_clauses.join(" OR ")));
            for id in ids {
                params.push(Box::new(*id));
            }
        }
    }

    // タグフィルタの処理
    if let Some(ref tag_ids) = filter.tag_ids {
        if !tag_ids.is_empty() {
            needs_tag_join = true;
            let placeholders: Vec<String> = tag_ids.iter().map(|_| "?".to_string()).collect();
            where_clauses.push(format!("mt.tag_id IN ({})", placeholders.join(", ")));
            for tag_id in tag_ids {
                params.push(Box::new(*tag_id));
            }
        }
    }

    // 条件をAND結合
    let condition = if !where_clauses.is_empty() {
        Some(where_clauses.join(" AND "))
    } else {
        None
    };

    FilterConditions {
        condition,
        params,
        needs_tag_join,
    }
}

/// メディアを検索
pub fn find_media(
    conn: &Connection,
    filter: &MediaFilter,
    options: Option<&QueryOptions>,
) -> Result<Vec<Media>> {
    // メインフィルタの条件を構築
    let main_conditions = build_filter_conditions(filter);
    let mut needs_tag_join = main_conditions.needs_tag_join;
    let mut all_params = main_conditions.params;

    // or_filtersの条件を構築
    let mut or_conditions: Vec<String> = Vec::new();
    if let Some(ref or_filters) = filter.or_filters {
        for or_filter in or_filters {
            let conditions = build_filter_conditions(or_filter);
            if conditions.needs_tag_join {
                needs_tag_join = true;
            }
            all_params.extend(conditions.params);
            if let Some(cond) = conditions.condition {
                or_conditions.push(cond);
            }
        }
    }

    // WHERE句の構築
    let where_clause = build_where_clause(&main_conditions.condition, &or_conditions);

    // FROM句の構築
    let from_clause = if needs_tag_join {
        "FROM media m INNER JOIN media_tags mt ON m.id = mt.media_id"
    } else {
        "FROM media m"
    };

    // GROUP BY句（タグフィルタ使用時に重複を排除）
    let group_by_clause = if needs_tag_join {
        "GROUP BY m.id"
    } else {
        ""
    };

    // ORDER BY句の構築
    let order_by_clause = if let Some(opts) = options {
        if !opts.sort_keys.is_empty() {
            let parts: Vec<String> = opts.sort_keys.iter()
                .map(|sk| format!("m.{} {}", sk.field, sk.order.as_str()))
                .collect();
            format!("ORDER BY {}", parts.join(", "))
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // LIMIT/OFFSET句の構築
    let limit_clause = if let Some(opts) = options {
        let mut clause = String::new();
        if let Some(limit) = opts.limit {
            clause.push_str(&format!("LIMIT {}", limit));
            if let Some(offset) = opts.offset {
                clause.push_str(&format!(" OFFSET {}", offset));
            }
        } else if let Some(offset) = opts.offset {
            clause.push_str(&format!("LIMIT -1 OFFSET {}", offset));
        }
        clause
    } else {
        String::new()
    };

    // SQLクエリの組み立て
    let sql = format!(
        "SELECT m.* {} {} {} {} {}",
        from_clause, where_clause, group_by_clause, order_by_clause, limit_clause
    );

    let mut stmt = conn.prepare(&sql)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> = all_params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_ref.as_slice(), row_to_media)?;

    let mut media_list = Vec::new();
    for row_result in rows {
        media_list.push(row_result?);
    }

    Ok(media_list)
}

/// 指定したフィールドの重複なしの値一覧を取得する
///
/// 各フィールドはホワイトリストで検証されるため、SQLインジェクションは発生しない。
/// 戻り値の各要素は `fields` と同じ順序のフィールド値。
pub fn get_distinct_values(
    conn: &Connection,
    fields: &[&str],
    filter: &MediaFilter,
) -> Result<Vec<Vec<Option<String>>>> {
    if fields.is_empty() {
        return Err(KijukuError::Validation("fieldsは1つ以上指定してください".to_string()));
    }
    for field in fields {
        if !ALLOWED_DISTINCT_FIELDS.contains(field) {
            return Err(KijukuError::Validation(format!(
                "無効なフィールド: {}。使用可能: {}",
                field,
                ALLOWED_DISTINCT_FIELDS.join(", ")
            )));
        }
    }

    let main_conditions = build_filter_conditions(filter);
    let mut needs_tag_join = main_conditions.needs_tag_join;
    let mut all_params = main_conditions.params;

    let mut or_conditions: Vec<String> = Vec::new();
    if let Some(ref or_filters) = filter.or_filters {
        for or_filter in or_filters {
            let conditions = build_filter_conditions(or_filter);
            if conditions.needs_tag_join {
                needs_tag_join = true;
            }
            all_params.extend(conditions.params);
            if let Some(cond) = conditions.condition {
                or_conditions.push(cond);
            }
        }
    }

    let filter_where = build_where_clause(&main_conditions.condition, &or_conditions);

    let from_clause = if needs_tag_join {
        "FROM media m INNER JOIN media_tags mt ON m.id = mt.media_id"
    } else {
        "FROM media m"
    };

    let select_cols: Vec<String> = fields.iter().map(|f| format!("m.{}", f)).collect();
    let order_cols: Vec<String> = fields.iter().map(|f| format!("m.{} ASC", f)).collect();

    let sql = format!(
        "SELECT DISTINCT {} {} {} ORDER BY {}",
        select_cols.join(", "),
        from_clause,
        filter_where,
        order_cols.join(", ")
    );

    let mut stmt = conn.prepare(&sql)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> = all_params.iter().map(|p| p.as_ref()).collect();
    let field_count = fields.len();
    let rows = stmt.query_map(params_ref.as_slice(), move |row| {
        let mut values = Vec::with_capacity(field_count);
        for i in 0..field_count {
            values.push(row.get::<_, Option<String>>(i)?);
        }
        Ok(values)
    })?;

    let result: rusqlite::Result<Vec<_>> = rows.collect();
    Ok(result?)
}

/// WHERE句を構築（OR条件を含む）
fn build_where_clause(main_condition: &Option<String>, or_conditions: &[String]) -> String {
    let mut all_conditions = Vec::new();

    // メイン条件を追加
    if let Some(cond) = main_condition {
        all_conditions.push(format!("({})", cond));
    }

    // OR条件を追加
    for cond in or_conditions {
        all_conditions.push(format!("({})", cond));
    }

    if all_conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", all_conditions.join(" OR "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::create_media;
    use crate::migration;
    use crate::types::{MediaInput, MediaType, SortKey, SortOrder};

    #[test]
    fn test_find_all() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // テストデータを作成
        for i in 1..=3 {
            let input = MediaInput {
                title: format!("テスト{}", i),
                media_type: MediaType::Comic,
                ..Default::default()
            };
            create_media(&conn, &input).unwrap();
        }

        let filter = MediaFilter::default();
        let results = find_media(&conn, &filter, None).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_find_by_media_type() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_media(&conn, &MediaInput {
            title: "コミック".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        create_media(&conn, &MediaInput {
            title: "動画".to_string(),
            media_type: MediaType::Video,
            ..Default::default()
        }).unwrap();

        let filter = MediaFilter {
            media_type: Some(MediaType::Comic),
            ..Default::default()
        };

        let results = find_media(&conn, &filter, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "コミック");
    }

    #[test]
    fn test_find_with_pagination() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        for i in 1..=10 {
            create_media(&conn, &MediaInput {
                title: format!("メディア{}", i),
                media_type: MediaType::Comic,
                ..Default::default()
            }).unwrap();
        }

        let filter = MediaFilter::default();
        let options = QueryOptions {
            limit: Some(5),
            offset: Some(3),
            ..Default::default()
        };

        let results = find_media(&conn, &filter, Some(&options)).unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_find_by_volume_title() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_media(&conn, &MediaInput {
            title: "冒険コミック".to_string(),
            media_type: MediaType::Comic,
            volume_title: Some("序章".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "別作品".to_string(),
            media_type: MediaType::Comic,
            volume_title: Some("最終章".to_string()),
            ..Default::default()
        }).unwrap();

        let filter = MediaFilter {
            volume_title: Some("序".to_string()),
            ..Default::default()
        };
        let results = find_media(&conn, &filter, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "冒険コミック");
    }

    #[test]
    fn test_find_by_title_en() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_media(&conn, &MediaInput {
            title: "作品A".to_string(),
            media_type: MediaType::Comic,
            title_en: Some("Adventure Story".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品B".to_string(),
            media_type: MediaType::Comic,
            title_en: Some("Mystery Novel".to_string()),
            ..Default::default()
        }).unwrap();

        let filter = MediaFilter {
            title_en: Some("venture".to_string()),
            ..Default::default()
        };
        let results = find_media(&conn, &filter, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "作品A");
    }

    #[test]
    fn test_find_by_artist_en() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_media(&conn, &MediaInput {
            title: "作品A".to_string(),
            media_type: MediaType::Comic,
            artist_en: Some("John Smith".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品B".to_string(),
            media_type: MediaType::Comic,
            artist_en: Some("Jane Doe".to_string()),
            ..Default::default()
        }).unwrap();

        let filter = MediaFilter {
            artist_en: Some("Smith".to_string()),
            ..Default::default()
        };
        let results = find_media(&conn, &filter, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "作品A");
    }

    #[test]
    fn test_find_with_order() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_media(&conn, &MediaInput {
            title: "C".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        create_media(&conn, &MediaInput {
            title: "A".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        create_media(&conn, &MediaInput {
            title: "B".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        let filter = MediaFilter::default();
        let options = QueryOptions {
            sort_keys: vec![SortKey { field: "title".to_string(), order: SortOrder::Asc }],
            ..Default::default()
        };

        let results = find_media(&conn, &filter, Some(&options)).unwrap();
        assert_eq!(results[0].title, "A");
        assert_eq!(results[1].title, "B");
        assert_eq!(results[2].title, "C");
    }

    #[test]
    fn test_find_by_id_in() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        for i in 1..=5 {
            create_media(&conn, &MediaInput {
                title: format!("作品{}", i),
                media_type: MediaType::Comic,
                ..Default::default()
            }).unwrap();
        }

        // id=1,3,5のみ取得
        let filter = MediaFilter {
            id_in: Some(vec![1, 3, 5]),
            ..Default::default()
        };
        let results = find_media(&conn, &filter, None).unwrap();
        assert_eq!(results.len(), 3);

        let ids: Vec<i64> = results.iter().map(|m| m.id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&3));
        assert!(ids.contains(&5));
        assert!(!ids.contains(&2));
        assert!(!ids.contains(&4));
    }

    #[test]
    fn test_find_by_id_in_empty() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_media(&conn, &MediaInput {
            title: "作品1".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        // 空のid_inは条件なしと同様（全件取得）
        let filter = MediaFilter {
            id_in: Some(vec![]),
            ..Default::default()
        };
        let results = find_media(&conn, &filter, None).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_find_by_id_in_with_other_filter() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_media(&conn, &MediaInput {
            title: "コミック".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "動画".to_string(),
            media_type: MediaType::Video,
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "コミック2".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        // id_in=[1,2,3] AND media_type=Comic → id=1,3のみ
        let filter = MediaFilter {
            id_in: Some(vec![1, 2, 3]),
            media_type: Some(MediaType::Comic),
            ..Default::default()
        };
        let results = find_media(&conn, &filter, None).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|m| m.media_type == MediaType::Comic));
    }

    #[test]
    fn test_find_with_or_filters() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // テストデータを作成
        create_media(&conn, &MediaInput {
            title: "作品A".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author1".to_string()),
            series: Some("Series1".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品B".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author2".to_string()),
            series: Some("Series2".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品C".to_string(),
            media_type: MediaType::Video,
            artist: Some("Author3".to_string()),
            series: Some("Series3".to_string()),
            ..Default::default()
        }).unwrap();

        // OR条件: artist="Author1" OR artist="Author2"
        let filter = MediaFilter {
            or_filters: Some(vec![
                MediaFilter {
                    artist: Some("Author1".to_string()),
                    ..Default::default()
                },
                MediaFilter {
                    artist: Some("Author2".to_string()),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };

        let results = find_media(&conn, &filter, None).unwrap();
        assert_eq!(results.len(), 2);

        let titles: Vec<&str> = results.iter().map(|m| m.title.as_str()).collect();
        assert!(titles.contains(&"作品A"));
        assert!(titles.contains(&"作品B"));
        assert!(!titles.contains(&"作品C"));
    }

    #[test]
    fn test_find_with_or_filters_complex() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // テストデータを作成
        create_media(&conn, &MediaInput {
            title: "作品A".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author1".to_string()),
            series: Some("Series1".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品B".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author1".to_string()),
            series: Some("Series2".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品C".to_string(),
            media_type: MediaType::Video,
            artist: Some("Author2".to_string()),
            series: Some("Series1".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品D".to_string(),
            media_type: MediaType::Music,
            artist: Some("Author3".to_string()),
            series: Some("Series3".to_string()),
            ..Default::default()
        }).unwrap();

        // 複雑なOR条件: (artist="Author1" AND series="Series1") OR (artist="Author2")
        let filter = MediaFilter {
            or_filters: Some(vec![
                MediaFilter {
                    artist: Some("Author1".to_string()),
                    series: Some("Series1".to_string()),
                    ..Default::default()
                },
                MediaFilter {
                    artist: Some("Author2".to_string()),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };

        let results = find_media(&conn, &filter, None).unwrap();
        assert_eq!(results.len(), 2);

        let titles: Vec<&str> = results.iter().map(|m| m.title.as_str()).collect();
        assert!(titles.contains(&"作品A")); // Author1 + Series1
        assert!(titles.contains(&"作品C")); // Author2
        assert!(!titles.contains(&"作品B")); // Author1 but Series2
        assert!(!titles.contains(&"作品D")); // Author3
    }

    #[test]
    fn test_find_with_main_condition_and_or_filters() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // テストデータを作成
        create_media(&conn, &MediaInput {
            title: "作品A".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author1".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品B".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author2".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品C".to_string(),
            media_type: MediaType::Video,
            artist: Some("Author1".to_string()),
            ..Default::default()
        }).unwrap();

        // メイン条件 + OR条件: (media_type=Comic) OR (artist="Author1")
        let filter = MediaFilter {
            media_type: Some(MediaType::Comic),
            or_filters: Some(vec![
                MediaFilter {
                    artist: Some("Author1".to_string()),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };

        let results = find_media(&conn, &filter, None).unwrap();
        assert_eq!(results.len(), 3); // A, B (Comic), C (Author1)

        let titles: Vec<&str> = results.iter().map(|m| m.title.as_str()).collect();
        assert!(titles.contains(&"作品A"));
        assert!(titles.contains(&"作品B"));
        assert!(titles.contains(&"作品C"));
    }

    #[test]
    fn test_get_distinct_values_single_field() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        for artist in &["Author1", "Author2", "Author1"] {
            create_media(&conn, &MediaInput {
                title: format!("作品_{}", artist),
                media_type: MediaType::Comic,
                artist: Some(artist.to_string()),
                ..Default::default()
            }).unwrap();
        }

        let filter = MediaFilter::default();
        let rows = get_distinct_values(&conn, &["artist"], &filter).unwrap();
        assert_eq!(rows, vec![
            vec![Some("Author1".to_string())],
            vec![Some("Author2".to_string())],
        ]);
    }

    #[test]
    fn test_get_distinct_values_multiple_fields() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_media(&conn, &MediaInput {
            title: "作品A".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author1".to_string()),
            series: Some("Series1".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品B".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author1".to_string()),
            series: Some("Series2".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品C".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author2".to_string()),
            series: Some("Series1".to_string()),
            ..Default::default()
        }).unwrap();
        // artist+seriesが重複
        create_media(&conn, &MediaInput {
            title: "作品D".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author1".to_string()),
            series: Some("Series1".to_string()),
            ..Default::default()
        }).unwrap();

        let filter = MediaFilter::default();
        let rows = get_distinct_values(&conn, &["artist", "series"], &filter).unwrap();
        // 重複なし・昇順ソート
        assert_eq!(rows.len(), 3);
        assert!(rows.contains(&vec![Some("Author1".to_string()), Some("Series1".to_string())]));
        assert!(rows.contains(&vec![Some("Author1".to_string()), Some("Series2".to_string())]));
        assert!(rows.contains(&vec![Some("Author2".to_string()), Some("Series1".to_string())]));
    }

    #[test]
    fn test_get_distinct_values_with_filter() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_media(&conn, &MediaInput {
            title: "コミック1".to_string(),
            media_type: MediaType::Comic,
            artist: Some("AuthorA".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "コミック2".to_string(),
            media_type: MediaType::Comic,
            artist: Some("AuthorB".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "動画1".to_string(),
            media_type: MediaType::Video,
            artist: Some("AuthorC".to_string()),
            ..Default::default()
        }).unwrap();

        let filter = MediaFilter {
            media_type: Some(MediaType::Comic),
            ..Default::default()
        };
        let rows = get_distinct_values(&conn, &["artist"], &filter).unwrap();
        let artists: Vec<_> = rows.iter().map(|r| r[0].as_deref()).collect();
        assert!(artists.contains(&Some("AuthorA")));
        assert!(artists.contains(&Some("AuthorB")));
        assert!(!artists.contains(&Some("AuthorC")));
    }

    #[test]
    fn test_get_distinct_values_includes_null() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_media(&conn, &MediaInput {
            title: "作品1".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author1".to_string()),
            series: Some("Series1".to_string()),
            ..Default::default()
        }).unwrap();
        create_media(&conn, &MediaInput {
            title: "作品2".to_string(),
            media_type: MediaType::Comic,
            artist: Some("Author1".to_string()),
            series: None,
            ..Default::default()
        }).unwrap();

        let filter = MediaFilter::default();
        let rows = get_distinct_values(&conn, &["artist", "series"], &filter).unwrap();
        // NULLの組み合わせも含まれる
        assert_eq!(rows.len(), 2);
        assert!(rows.contains(&vec![Some("Author1".to_string()), None]));
        assert!(rows.contains(&vec![Some("Author1".to_string()), Some("Series1".to_string())]));
    }

    #[test]
    fn test_get_distinct_values_invalid_field() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let filter = MediaFilter::default();
        let result = get_distinct_values(&conn, &["id"], &filter);
        assert!(result.is_err());

        let result = get_distinct_values(&conn, &["DROP TABLE media; --"], &filter);
        assert!(result.is_err());

        let result = get_distinct_values(&conn, &[], &filter);
        assert!(result.is_err());
    }
}
