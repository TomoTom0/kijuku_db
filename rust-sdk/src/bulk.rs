use crate::crud::create_media;
use crate::error::Result;
use crate::types::{Media, MediaInput};
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
}
