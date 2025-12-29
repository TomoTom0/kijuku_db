use crate::crud::{create_media, delete_media, update_media};
use crate::error::Result;
use crate::types::{BulkUpdateItem, Media, MediaInput};
use rusqlite::Connection;

/// 複数のメディアを一括作成
pub fn bulk_create_media(conn: &Connection, data_list: &[MediaInput]) -> Result<Vec<Media>> {
    if data_list.is_empty() {
        return Ok(Vec::new());
    }

    // トランザクション内で一括処理
    let tx = conn.unchecked_transaction()?;

    let mut results = Vec::new();
    for data in data_list {
        let media = create_media(&tx, data)?;
        results.push(media);
    }

    tx.commit()?;

    Ok(results)
}

/// 複数のメディアを一括削除
pub fn bulk_delete_media(conn: &Connection, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }

    // トランザクション内で一括処理
    let tx = conn.unchecked_transaction()?;

    for id in ids {
        delete_media(&tx, *id)?;
    }

    tx.commit()?;

    Ok(())
}

/// 複数のメディアを一括更新
pub fn bulk_update_media(conn: &Connection, updates: &[BulkUpdateItem]) -> Result<()> {
    if updates.is_empty() {
        return Ok(());
    }

    // トランザクション内で一括処理
    let tx = conn.unchecked_transaction()?;

    for item in updates {
        update_media(&tx, item.id, &item.data)?;
    }

    tx.commit()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration;
    use crate::types::MediaType;

    #[test]
    fn test_bulk_create_media() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let data_list = vec![
            MediaInput {
                title: "メディア1".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
            MediaInput {
                title: "メディア2".to_string(),
                media_type: MediaType::Video,
                ..Default::default()
            },
            MediaInput {
                title: "メディア3".to_string(),
                media_type: MediaType::Music,
                ..Default::default()
            },
        ];

        let results = bulk_create_media(&conn, &data_list).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].title, "メディア1");
        assert_eq!(results[1].title, "メディア2");
        assert_eq!(results[2].title, "メディア3");
    }

    #[test]
    fn test_bulk_create_empty() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let results = bulk_create_media(&conn, &[]).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_bulk_delete_media() {
        use crate::crud::get_media;

        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // テスト用のメディアを作成
        let data_list = vec![
            MediaInput {
                title: "メディア1".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
            MediaInput {
                title: "メディア2".to_string(),
                media_type: MediaType::Video,
                ..Default::default()
            },
            MediaInput {
                title: "メディア3".to_string(),
                media_type: MediaType::Music,
                ..Default::default()
            },
        ];

        let results = bulk_create_media(&conn, &data_list).unwrap();
        assert_eq!(results.len(), 3);

        // 最初の2つを一括削除
        let ids_to_delete = vec![results[0].id, results[1].id];
        bulk_delete_media(&conn, &ids_to_delete).unwrap();

        // 削除されたことを確認
        assert!(get_media(&conn, results[0].id).is_none());
        assert!(get_media(&conn, results[1].id).is_none());
        assert!(get_media(&conn, results[2].id).is_some());
    }

    #[test]
    fn test_bulk_delete_empty() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // 空のリストで削除してもエラーにならない
        bulk_delete_media(&conn, &[]).unwrap();
    }

    #[test]
    fn test_bulk_update_media() {
        use crate::crud::get_media;

        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // テスト用のメディアを作成
        let data_list = vec![
            MediaInput {
                title: "メディア1".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
            MediaInput {
                title: "メディア2".to_string(),
                media_type: MediaType::Video,
                ..Default::default()
            },
        ];

        let results = bulk_create_media(&conn, &data_list).unwrap();
        assert_eq!(results.len(), 2);

        // 一括更新
        let updates = vec![
            BulkUpdateItem {
                id: results[0].id,
                data: MediaInput {
                    title: "更新後メディア1".to_string(),
                    media_type: MediaType::Comic,
                    artist: Some("アーティスト1".to_string()),
                    ..Default::default()
                },
            },
            BulkUpdateItem {
                id: results[1].id,
                data: MediaInput {
                    title: "更新後メディア2".to_string(),
                    media_type: MediaType::Video,
                    artist: Some("アーティスト2".to_string()),
                    ..Default::default()
                },
            },
        ];
        bulk_update_media(&conn, &updates).unwrap();

        // 更新されたことを確認
        let updated1 = get_media(&conn, results[0].id).unwrap();
        assert_eq!(updated1.title, "更新後メディア1");
        assert_eq!(updated1.artist, Some("アーティスト1".to_string()));

        let updated2 = get_media(&conn, results[1].id).unwrap();
        assert_eq!(updated2.title, "更新後メディア2");
        assert_eq!(updated2.artist, Some("アーティスト2".to_string()));
    }

    #[test]
    fn test_bulk_update_empty() {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        // 空のリストで更新してもエラーにならない
        bulk_update_media(&conn, &[]).unwrap();
    }
}
