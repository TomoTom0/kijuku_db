use crate::error::Result;
use crate::types::Tag;
use rusqlite::{params, Connection};

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
}
