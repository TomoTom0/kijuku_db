use crate::crud::row_to_media;
use crate::error::Result;
use crate::types::{Media, MediaFilter, QueryOptions};
use rusqlite::Connection;

/// メディアを検索
pub fn find_media(
    conn: &Connection,
    filter: &MediaFilter,
    options: Option<&QueryOptions>,
) -> Result<Vec<Media>> {
    let mut where_clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    // フィルタ条件を構築
    if let Some(ref title) = filter.title {
        where_clauses.push("m.title = ?".to_string());
        params.push(Box::new(title.clone()));
    }
    if let Some(ref title_id) = filter.title_id {
        where_clauses.push("m.title_id = ?".to_string());
        params.push(Box::new(title_id.clone()));
    }
    if let Some(ref artist) = filter.artist {
        where_clauses.push("m.artist = ?".to_string());
        params.push(Box::new(artist.clone()));
    }
    if let Some(ref artist_id) = filter.artist_id {
        where_clauses.push("m.artist_id = ?".to_string());
        params.push(Box::new(artist_id.clone()));
    }
    if let Some(ref media_type) = filter.media_type {
        where_clauses.push("m.media_type = ?".to_string());
        params.push(Box::new(media_type.as_str().to_string()));
    }
    if let Some(ref series) = filter.series {
        where_clauses.push("m.series = ?".to_string());
        params.push(Box::new(series.clone()));
    }
    if let Some(ref source) = filter.source {
        where_clauses.push("m.source = ?".to_string());
        params.push(Box::new(source.clone()));
    }

    // FROM句の構築
    let mut from_clause = "FROM media m".to_string();

    // タグフィルタの処理
    if let Some(ref tag_ids) = filter.tag_ids {
        if !tag_ids.is_empty() {
            from_clause = "FROM media m INNER JOIN media_tags mt ON m.id = mt.media_id".to_string();
            let placeholders: Vec<String> = tag_ids.iter().map(|_| "?".to_string()).collect();
            where_clauses.push(format!("mt.tag_id IN ({})", placeholders.join(", ")));
            for tag_id in tag_ids {
                params.push(Box::new(*tag_id));
            }
        }
    }

    // WHERE句の構築
    let where_clause = if !where_clauses.is_empty() {
        format!("WHERE {}", where_clauses.join(" AND "))
    } else {
        String::new()
    };

    // GROUP BY句（タグフィルタ使用時に重複を排除）
    let group_by_clause = if filter.tag_ids.is_some() && !filter.tag_ids.as_ref().unwrap().is_empty() {
        "GROUP BY m.id"
    } else {
        ""
    };

    // ORDER BY句の構築
    let order_by_clause = if let Some(opts) = options {
        if let Some(ref order_by) = opts.order_by {
            let order = opts.order.as_ref().map(|o| o.as_str()).unwrap_or("ASC");
            format!("ORDER BY m.{} {}", order_by, order)
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
    let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_ref.as_slice(), row_to_media)?;

    let mut media_list = Vec::new();
    for row_result in rows {
        media_list.push(row_result?);
    }

    Ok(media_list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::create_media;
    use crate::migration;
    use crate::types::{MediaInput, MediaType, SortOrder};

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
            order_by: Some("title".to_string()),
            order: Some(SortOrder::Asc),
            ..Default::default()
        };

        let results = find_media(&conn, &filter, Some(&options)).unwrap();
        assert_eq!(results[0].title, "A");
        assert_eq!(results[1].title, "B");
        assert_eq!(results[2].title, "C");
    }
}
