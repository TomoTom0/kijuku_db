use crate::db_value::{SqlParam, SqlRow};
use crate::error::{KijukuError, Result};
use crate::exec::SqlExec;
use crate::types::{MediaTagAssoc, Tag, TagUsageStats};
use rusqlite::{params, Connection};
use std::collections::HashMap;

/// タグを作成
pub fn create_tag(conn: &Connection, name: &str) -> Result<Tag> {
    conn.execute("INSERT INTO tags (name) VALUES (?1)", params![name])?;

    let id = conn.last_insert_rowid();
    Ok(Tag { id, name: name.to_string() })
}

/// タグ名でタグを取得
pub fn get_tag_by_name(conn: &Connection, name: &str) -> Option<Tag> {
    conn.query_row(
        "SELECT id, name FROM tags WHERE name = ?1",
        params![name],
        |row| Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
        }),
    )
    .ok()
}

/// 全てのタグを取得
pub fn get_all_tags(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare("SELECT id, name FROM tags ORDER BY name")?;
    let tags = stmt
        .query_map([], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<Tag>, _>>()?;

    Ok(tags)
}

/// メディアにタグを追加
pub fn add_tag_to_media(conn: &Connection, media_id: i64, tag_id: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?1, ?2)",
        params![media_id, tag_id],
    )?;
    Ok(())
}

/// メディアからタグを削除
pub fn remove_tag_from_media(conn: &Connection, media_id: i64, tag_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM media_tags WHERE media_id = ?1 AND tag_id = ?2",
        params![media_id, tag_id],
    )?;
    Ok(())
}

/// メディアに関連付けられたタグを取得
pub fn get_media_tags(conn: &Connection, media_id: i64) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name
         FROM tags t
         INNER JOIN media_tags mt ON t.id = mt.tag_id
         WHERE mt.media_id = ?1
         ORDER BY t.name",
    )?;

    let tags = stmt
        .query_map(params![media_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<Tag>, _>>()?;

    Ok(tags)
}

/// 全てのメディア-タグ紐付けを取得（差分比較用）
pub fn get_all_media_tags(conn: &Connection) -> Result<Vec<MediaTagAssoc>> {
    let mut stmt = conn.prepare(
        "SELECT media_id, tag_id FROM media_tags ORDER BY media_id, tag_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(MediaTagAssoc {
            media_id: row.get(0)?,
            tag_id: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// タグの使用数統計を取得
pub fn get_tag_usage_stats(conn: &Connection) -> Result<Vec<TagUsageStats>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, COUNT(mt.media_id) as count
         FROM tags t
         LEFT JOIN media_tags mt ON t.id = mt.tag_id
         GROUP BY t.id, t.name
         ORDER BY count DESC, t.name ASC",
    )?;

    let stats = stmt
        .query_map([], |row| {
            Ok(TagUsageStats {
                tag_id: row.get(0)?,
                tag_name: row.get(1)?,
                count: row.get(2)?,
            })
        })?
        .collect::<std::result::Result<Vec<TagUsageStats>, _>>()?;

    Ok(stats)
}

/// 未使用のタグを取得
pub fn find_unused_tags(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name
         FROM tags t
         LEFT JOIN media_tags mt ON t.id = mt.tag_id
         WHERE mt.media_id IS NULL
         ORDER BY t.name",
    )?;

    let tags = stmt
        .query_map([], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<Tag>, _>>()?;

    Ok(tags)
}

// ========== async バックエンド（SqlExec）用 ==========

fn row_to_tag(row: &SqlRow) -> Result<Tag> {
    Ok(Tag {
        id: row.get_int("id")?,
        name: row.get_text("name")?,
    })
}

fn row_to_tag_usage_stats(row: &SqlRow) -> Result<TagUsageStats> {
    Ok(TagUsageStats {
        tag_id: row.get_int("id")?,
        tag_name: row.get_text("name")?,
        count: row.get_int("count")?,
    })
}

/// async バックエンド経由でタグを作成（`RETURNING` で id/name を取得）
pub async fn create_tag_async(exec: &dyn SqlExec, name: &str) -> Result<Tag> {
    let sql = "INSERT INTO tags (name) VALUES (?1) RETURNING id, name";
    let params = vec![SqlParam::Text(name.to_string())];
    let rows = exec.query(sql, &params).await?;
    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| KijukuError::Other("作成されたタグが見つかりません".to_string()))?;
    row_to_tag(&row)
}

/// async バックエンド経由でタグ名から取得
pub async fn get_tag_by_name_async(exec: &dyn SqlExec, name: &str) -> Result<Option<Tag>> {
    let sql = "SELECT id, name FROM tags WHERE name = ?1";
    let params = vec![SqlParam::Text(name.to_string())];
    let rows = exec.query(sql, &params).await?;
    match rows.into_iter().next() {
        Some(row) => Ok(Some(row_to_tag(&row)?)),
        None => Ok(None),
    }
}

/// async バックエンド経由で全タグを取得
pub async fn get_all_tags_async(exec: &dyn SqlExec) -> Result<Vec<Tag>> {
    let sql = "SELECT id, name FROM tags ORDER BY name";
    let rows = exec.query(sql, &[]).await?;
    rows.iter().map(row_to_tag).collect()
}

/// async バックエンド経由でメディアにタグを追加（INSERT OR IGNORE）
pub async fn add_tag_to_media_async(
    exec: &dyn SqlExec,
    media_id: i64,
    tag_id: i64,
) -> Result<()> {
    let sql = "INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?1, ?2)";
    let params = vec![SqlParam::Int(media_id), SqlParam::Int(tag_id)];
    exec.execute(sql, &params).await?;
    Ok(())
}

/// async バックエンド経由でメディアからタグを削除
pub async fn remove_tag_from_media_async(
    exec: &dyn SqlExec,
    media_id: i64,
    tag_id: i64,
) -> Result<()> {
    let sql = "DELETE FROM media_tags WHERE media_id = ?1 AND tag_id = ?2";
    let params = vec![SqlParam::Int(media_id), SqlParam::Int(tag_id)];
    exec.execute(sql, &params).await?;
    Ok(())
}

/// async バックエンド経由でメディアのタグを取得
pub async fn get_media_tags_async(exec: &dyn SqlExec, media_id: i64) -> Result<Vec<Tag>> {
    let sql = "SELECT t.id, t.name
         FROM tags t
         INNER JOIN media_tags mt ON t.id = mt.tag_id
         WHERE mt.media_id = ?1
         ORDER BY t.name";
    let params = vec![SqlParam::Int(media_id)];
    let rows = exec.query(sql, &params).await?;
    rows.iter().map(row_to_tag).collect()
}

/// async バックエンド経由で複数メディアのタグを一括取得（N+1回避）
///
/// `media_tags` と `tags` の JOIN 1発で複数メディアのタグを取得し、
/// `media_id -> Vec<Tag>` のマップに集約する。タグを持たないメディアは
/// 結果のエントリに含まれない（呼び出し側で `get(&id).map(|v| v.as_slice()).unwrap_or(&[])`
/// 等で補完すること）。999件超はチャンク分割して順次取得・マージする。
pub async fn get_media_tags_bulk_async(
    exec: &dyn SqlExec,
    media_ids: &[i64],
) -> Result<HashMap<i64, Vec<Tag>>> {
    let mut result: HashMap<i64, Vec<Tag>> = HashMap::new();
    if media_ids.is_empty() {
        return Ok(result);
    }

    // 重複IDを排除（IN句は集合扱いで結果の重複は生じないが、プレースホルダーの
    // 無駄な増加と999件チャンク制限への早期到達を防ぐ）
    let mut unique_ids = media_ids.to_vec();
    unique_ids.sort_unstable();
    unique_ids.dedup();

    const CHUNK_SIZE: usize = 999;
    for chunk in unique_ids.chunks(CHUNK_SIZE) {
        let placeholders = vec!["?"; chunk.len()].join(", ");
        let sql = format!(
            "SELECT mt.media_id, t.id, t.name
             FROM media_tags mt
             INNER JOIN tags t ON t.id = mt.tag_id
             WHERE mt.media_id IN ({})
             ORDER BY t.name",
            placeholders
        );
        let params: Vec<SqlParam> = chunk.iter().map(|id| SqlParam::Int(*id)).collect();
        let rows = exec.query(&sql, &params).await?;
        for row in rows.iter() {
            let media_id = row.get_int("media_id")?;
            let tag = row_to_tag(row)?;
            result.entry(media_id).or_default().push(tag);
        }
    }

    Ok(result)
}

/// async バックエンド経由でタグ使用数統計を取得
pub async fn get_tag_usage_stats_async(exec: &dyn SqlExec) -> Result<Vec<TagUsageStats>> {
    let sql = "SELECT t.id, t.name, COUNT(mt.media_id) as count
         FROM tags t
         LEFT JOIN media_tags mt ON t.id = mt.tag_id
         GROUP BY t.id, t.name
         ORDER BY count DESC, t.name ASC";
    let rows = exec.query(sql, &[]).await?;
    rows.iter().map(row_to_tag_usage_stats).collect()
}

/// async バックエンド経由で未使用タグを取得
pub async fn find_unused_tags_async(exec: &dyn SqlExec) -> Result<Vec<Tag>> {
    let sql = "SELECT t.id, t.name
         FROM tags t
         LEFT JOIN media_tags mt ON t.id = mt.tag_id
         WHERE mt.media_id IS NULL
         ORDER BY t.name";
    let rows = exec.query(sql, &[]).await?;
    rows.iter().map(row_to_tag).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::create_media;
    use crate::migration;
    use crate::types::{MediaInput, MediaType};

    #[test]
    fn test_create_tag() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let tag = create_tag(&conn, "アクション").unwrap();
        assert_eq!(tag.name, "アクション");
    }

    #[test]
    fn test_get_tag_by_name() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_tag(&conn, "ファンタジー").unwrap();

        let tag = get_tag_by_name(&conn, "ファンタジー").unwrap();
        assert_eq!(tag.name, "ファンタジー");
    }

    #[test]
    fn test_get_all_tags() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        create_tag(&conn, "タグA").unwrap();
        create_tag(&conn, "タグB").unwrap();

        let tags = get_all_tags(&conn).unwrap();
        assert_eq!(tags.len(), 2);
    }

    #[test]
    fn test_add_tag_to_media() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let media = create_media(&conn, &MediaInput {
            title: "テスト".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        let tag = create_tag(&conn, "新作").unwrap();

        add_tag_to_media(&conn, media.id, tag.id).unwrap();

        let media_tags = get_media_tags(&conn, media.id).unwrap();
        assert_eq!(media_tags.len(), 1);
        assert_eq!(media_tags[0].name, "新作");
    }

    #[test]
    fn test_remove_tag_from_media() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let media = create_media(&conn, &MediaInput {
            title: "テスト".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        let tag = create_tag(&conn, "削除テスト").unwrap();
        add_tag_to_media(&conn, media.id, tag.id).unwrap();

        remove_tag_from_media(&conn, media.id, tag.id).unwrap();

        let media_tags = get_media_tags(&conn, media.id).unwrap();
        assert_eq!(media_tags.len(), 0);
    }

    #[test]
    fn test_duplicate_tag_name_error() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // 1つ目のタグを作成
        create_tag(&conn, "重複タグ").unwrap();

        // 同じ名前で2つ目のタグを作成しようとするとエラー
        let result = create_tag(&conn, "重複タグ");
        assert!(result.is_err());
    }

    #[test]
    fn test_add_duplicate_tag_to_media_ignored() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let media = create_media(
            &conn,
            &MediaInput {
                title: "テスト".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
        )
        .unwrap();

        let tag = create_tag(&conn, "テストタグ").unwrap();

        // 1回目の追加
        add_tag_to_media(&conn, media.id, tag.id).unwrap();

        // 2回目の追加（INSERT OR IGNOREなのでエラーにならない）
        add_tag_to_media(&conn, media.id, tag.id).unwrap();

        // タグは1つだけ
        let media_tags = get_media_tags(&conn, media.id).unwrap();
        assert_eq!(media_tags.len(), 1);
    }

    #[test]
    fn test_get_all_tags_sorted_by_name() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // 順番をバラバラに作成
        create_tag(&conn, "タグC").unwrap();
        create_tag(&conn, "タグA").unwrap();
        create_tag(&conn, "タグB").unwrap();

        let tags = get_all_tags(&conn).unwrap();
        assert_eq!(tags.len(), 3);
        // 名前順にソートされている
        assert_eq!(tags[0].name, "タグA");
        assert_eq!(tags[1].name, "タグB");
        assert_eq!(tags[2].name, "タグC");
    }

    #[test]
    fn test_get_tag_usage_stats() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // メディアを作成
        let media1 = create_media(&conn, &MediaInput {
            title: "作品1".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        let media2 = create_media(&conn, &MediaInput {
            title: "作品2".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        let media3 = create_media(&conn, &MediaInput {
            title: "作品3".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        // タグを作成
        let tag1 = create_tag(&conn, "人気").unwrap();
        let tag2 = create_tag(&conn, "新作").unwrap();
        let _tag3 = create_tag(&conn, "未使用").unwrap();

        // タグを付与
        add_tag_to_media(&conn, media1.id, tag1.id).unwrap(); // 人気: 1
        add_tag_to_media(&conn, media2.id, tag1.id).unwrap(); // 人気: 2
        add_tag_to_media(&conn, media3.id, tag1.id).unwrap(); // 人気: 3
        add_tag_to_media(&conn, media1.id, tag2.id).unwrap(); // 新作: 1
        // tag3は未使用

        // 統計を取得
        let stats = super::get_tag_usage_stats(&conn).unwrap();

        assert_eq!(stats.len(), 3);

        // カウント順にソート（降順）
        assert_eq!(stats[0].tag_name, "人気");
        assert_eq!(stats[0].count, 3);

        assert_eq!(stats[1].tag_name, "新作");
        assert_eq!(stats[1].count, 1);

        assert_eq!(stats[2].tag_name, "未使用");
        assert_eq!(stats[2].count, 0);
    }

    #[test]
    fn test_find_unused_tags() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // メディアを作成
        let media = create_media(&conn, &MediaInput {
            title: "作品1".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        }).unwrap();

        // タグを作成
        let tag_used = create_tag(&conn, "使用中").unwrap();
        let tag_unused1 = create_tag(&conn, "未使用1").unwrap();
        let tag_unused2 = create_tag(&conn, "未使用2").unwrap();

        // 1つだけ使用
        add_tag_to_media(&conn, media.id, tag_used.id).unwrap();

        // 未使用タグを取得
        let unused = super::find_unused_tags(&conn).unwrap();

        assert_eq!(unused.len(), 2);
        assert_eq!(unused[0].name, "未使用1");
        assert_eq!(unused[0].id, tag_unused1.id);
        assert_eq!(unused[1].name, "未使用2");
        assert_eq!(unused[1].id, tag_unused2.id);
    }
}
