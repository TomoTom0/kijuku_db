//! メディア追加属性の操作

use rusqlite::{params, Connection, Result};
use crate::db_value::{SqlParam, SqlRow};
use crate::exec::SqlExec;
use crate::types::{MediaAttribute, AttributeValueType};
use crate::error::KijukuError;

/// メディアに属性を設定
pub fn set_media_attribute(
    conn: &Connection,
    media_id: i64,
    key: &str,
    value: Option<&str>,
    value_type: Option<AttributeValueType>,
) -> Result<(), KijukuError> {
    let value_type_str = match value_type.unwrap_or(AttributeValueType::String) {
        AttributeValueType::String => "string",
        AttributeValueType::Integer => "integer",
        AttributeValueType::Boolean => "boolean",
    };

    conn.execute(
        "INSERT INTO media_attributes (media_id, key, value, value_type)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(media_id, key) DO UPDATE SET
           value = ?3,
           value_type = ?4",
        params![media_id, key, value, value_type_str],
    )?;

    Ok(())
}

/// メディアの属性を取得
pub fn get_media_attribute(
    conn: &Connection,
    media_id: i64,
    key: &str,
) -> Result<Option<MediaAttribute>, KijukuError> {
    let mut stmt = conn.prepare(
        "SELECT media_id, key, value, value_type
         FROM media_attributes
         WHERE media_id = ?1 AND key = ?2",
    )?;

    let result = stmt.query_row(params![media_id, key], |row| {
        let value_type_str: String = row.get(3)?;
        let value_type = match value_type_str.as_str() {
            "integer" => AttributeValueType::Integer,
            "boolean" => AttributeValueType::Boolean,
            _ => AttributeValueType::String,
        };

        Ok(MediaAttribute {
            media_id: row.get(0)?,
            key: row.get(1)?,
            value: row.get(2)?,
            value_type,
        })
    });

    match result {
        Ok(attr) => Ok(Some(attr)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// メディアの全ての属性を取得
pub fn get_media_attributes(
    conn: &Connection,
    media_id: i64,
) -> Result<Vec<MediaAttribute>, KijukuError> {
    let mut stmt = conn.prepare(
        "SELECT media_id, key, value, value_type
         FROM media_attributes
         WHERE media_id = ?1
         ORDER BY key",
    )?;

    let rows = stmt.query_map(params![media_id], |row| {
        let value_type_str: String = row.get(3)?;
        let value_type = match value_type_str.as_str() {
            "integer" => AttributeValueType::Integer,
            "boolean" => AttributeValueType::Boolean,
            _ => AttributeValueType::String,
        };

        Ok(MediaAttribute {
            media_id: row.get(0)?,
            key: row.get(1)?,
            value: row.get(2)?,
            value_type,
        })
    })?;

    let mut attributes = Vec::new();
    for attr in rows {
        attributes.push(attr?);
    }

    Ok(attributes)
}

/// メディアの属性を削除
pub fn delete_media_attribute(
    conn: &Connection,
    media_id: i64,
    key: &str,
) -> Result<(), KijukuError> {
    conn.execute(
        "DELETE FROM media_attributes WHERE media_id = ?1 AND key = ?2",
        params![media_id, key],
    )?;

    Ok(())
}

/// メディアの全ての属性を削除
pub fn delete_all_media_attributes(
    conn: &Connection,
    media_id: i64,
) -> Result<(), KijukuError> {
    conn.execute(
        "DELETE FROM media_attributes WHERE media_id = ?1",
        params![media_id],
    )?;

    Ok(())
}

// ========== async バックエンド（SqlExec）用 ==========

fn row_to_media_attribute(row: &SqlRow) -> Result<MediaAttribute, KijukuError> {
    let value_type_str = row.get_text("value_type")?;
    let value_type = match value_type_str.as_str() {
        "integer" => AttributeValueType::Integer,
        "boolean" => AttributeValueType::Boolean,
        _ => AttributeValueType::String,
    };
    Ok(MediaAttribute {
        media_id: row.get_int("media_id")?,
        key: row.get_text("key")?,
        value: row.get_opt_text("value")?,
        value_type,
    })
}

/// async バックエンド経由でメディア属性を設定（UPSERT）
pub async fn set_media_attribute_async(
    exec: &dyn SqlExec,
    media_id: i64,
    key: &str,
    value: Option<&str>,
    value_type: Option<AttributeValueType>,
) -> Result<(), KijukuError> {
    let value_type_str = match value_type.unwrap_or(AttributeValueType::String) {
        AttributeValueType::String => "string",
        AttributeValueType::Integer => "integer",
        AttributeValueType::Boolean => "boolean",
    };
    let sql = "INSERT INTO media_attributes (media_id, key, value, value_type)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(media_id, key) DO UPDATE SET
           value = ?3,
           value_type = ?4";
    let params = vec![
        SqlParam::Int(media_id),
        SqlParam::Text(key.to_string()),
        SqlParam::from_opt_str_ref(value),
        SqlParam::Text(value_type_str.to_string()),
    ];
    exec.execute(sql, &params).await?;
    Ok(())
}

/// async バックエンド経由でメディア属性を1件取得
pub async fn get_media_attribute_async(
    exec: &dyn SqlExec,
    media_id: i64,
    key: &str,
) -> Result<Option<MediaAttribute>, KijukuError> {
    let sql = "SELECT media_id, key, value, value_type
         FROM media_attributes
         WHERE media_id = ?1 AND key = ?2";
    let params = vec![SqlParam::Int(media_id), SqlParam::Text(key.to_string())];
    let rows = exec.query(sql, &params).await?;
    match rows.into_iter().next() {
        Some(row) => Ok(Some(row_to_media_attribute(&row)?)),
        None => Ok(None),
    }
}

/// async バックエンド経由でメディアの全属性を取得
pub async fn get_media_attributes_async(
    exec: &dyn SqlExec,
    media_id: i64,
) -> Result<Vec<MediaAttribute>, KijukuError> {
    let sql = "SELECT media_id, key, value, value_type
         FROM media_attributes
         WHERE media_id = ?1
         ORDER BY key";
    let params = vec![SqlParam::Int(media_id)];
    let rows = exec.query(sql, &params).await?;
    rows.iter().map(row_to_media_attribute).collect()
}

/// async バックエンド経由でメディア属性を削除
pub async fn delete_media_attribute_async(
    exec: &dyn SqlExec,
    media_id: i64,
    key: &str,
) -> Result<(), KijukuError> {
    let sql = "DELETE FROM media_attributes WHERE media_id = ?1 AND key = ?2";
    let params = vec![SqlParam::Int(media_id), SqlParam::Text(key.to_string())];
    exec.execute(sql, &params).await?;
    Ok(())
}

/// async バックエンド経由でメディアの全属性を削除
pub async fn delete_all_media_attributes_async(
    exec: &dyn SqlExec,
    media_id: i64,
) -> Result<(), KijukuError> {
    let sql = "DELETE FROM media_attributes WHERE media_id = ?1";
    let params = vec![SqlParam::Int(media_id)];
    exec.execute(sql, &params).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::create_media;
    use crate::migration;
    use crate::types::{MediaInput, MediaType};

    fn setup() -> (Connection, i64) {
        let conn = Connection::open_in_memory().unwrap();
        migration::migrate(&conn).unwrap();

        let media = create_media(
            &conn,
            &MediaInput {
                title: "テストメディア".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
        )
        .unwrap();

        (conn, media.id)
    }

    #[test]
    fn test_set_and_get_attribute() {
        let (conn, media_id) = setup();

        // 属性を設定
        set_media_attribute(&conn, media_id, "rating", Some("5"), None).unwrap();

        // 属性を取得
        let attr = get_media_attribute(&conn, media_id, "rating").unwrap();
        assert!(attr.is_some());

        let attr = attr.unwrap();
        assert_eq!(attr.media_id, media_id);
        assert_eq!(attr.key, "rating");
        assert_eq!(attr.value, Some("5".to_string()));
        assert_eq!(attr.value_type, AttributeValueType::String); // デフォルト
    }

    #[test]
    fn test_get_nonexistent_attribute() {
        let (conn, media_id) = setup();

        // 存在しない属性を取得
        let attr = get_media_attribute(&conn, media_id, "nonexistent").unwrap();
        assert!(attr.is_none());
    }

    #[test]
    fn test_attribute_value_types() {
        let (conn, media_id) = setup();

        // String型
        set_media_attribute(
            &conn,
            media_id,
            "str_attr",
            Some("hello"),
            Some(AttributeValueType::String),
        )
        .unwrap();

        // Integer型
        set_media_attribute(
            &conn,
            media_id,
            "int_attr",
            Some("42"),
            Some(AttributeValueType::Integer),
        )
        .unwrap();

        // Boolean型
        set_media_attribute(
            &conn,
            media_id,
            "bool_attr",
            Some("true"),
            Some(AttributeValueType::Boolean),
        )
        .unwrap();

        // 取得して型を確認
        let str_attr = get_media_attribute(&conn, media_id, "str_attr")
            .unwrap()
            .unwrap();
        assert_eq!(str_attr.value_type, AttributeValueType::String);

        let int_attr = get_media_attribute(&conn, media_id, "int_attr")
            .unwrap()
            .unwrap();
        assert_eq!(int_attr.value_type, AttributeValueType::Integer);

        let bool_attr = get_media_attribute(&conn, media_id, "bool_attr")
            .unwrap()
            .unwrap();
        assert_eq!(bool_attr.value_type, AttributeValueType::Boolean);
    }

    #[test]
    fn test_attribute_with_none_value() {
        let (conn, media_id) = setup();

        // NULL値を設定
        set_media_attribute(&conn, media_id, "empty_attr", None, None).unwrap();

        let attr = get_media_attribute(&conn, media_id, "empty_attr")
            .unwrap()
            .unwrap();
        assert_eq!(attr.value, None);
    }

    #[test]
    fn test_update_existing_attribute() {
        let (conn, media_id) = setup();

        // 初回設定
        set_media_attribute(&conn, media_id, "score", Some("80"), None).unwrap();

        // 更新（ON CONFLICT DO UPDATE）
        set_media_attribute(
            &conn,
            media_id,
            "score",
            Some("90"),
            Some(AttributeValueType::Integer),
        )
        .unwrap();

        let attr = get_media_attribute(&conn, media_id, "score")
            .unwrap()
            .unwrap();
        assert_eq!(attr.value, Some("90".to_string()));
        assert_eq!(attr.value_type, AttributeValueType::Integer);
    }

    #[test]
    fn test_get_all_media_attributes() {
        let (conn, media_id) = setup();

        // 複数の属性を設定
        set_media_attribute(&conn, media_id, "attr_c", Some("c"), None).unwrap();
        set_media_attribute(&conn, media_id, "attr_a", Some("a"), None).unwrap();
        set_media_attribute(&conn, media_id, "attr_b", Some("b"), None).unwrap();

        // 全属性を取得（keyでソートされる）
        let attrs = get_media_attributes(&conn, media_id).unwrap();
        assert_eq!(attrs.len(), 3);
        assert_eq!(attrs[0].key, "attr_a");
        assert_eq!(attrs[1].key, "attr_b");
        assert_eq!(attrs[2].key, "attr_c");
    }

    #[test]
    fn test_delete_media_attribute() {
        let (conn, media_id) = setup();

        set_media_attribute(&conn, media_id, "to_delete", Some("value"), None).unwrap();

        // 削除
        delete_media_attribute(&conn, media_id, "to_delete").unwrap();

        // 取得できないことを確認
        let attr = get_media_attribute(&conn, media_id, "to_delete").unwrap();
        assert!(attr.is_none());
    }

    #[test]
    fn test_delete_nonexistent_attribute() {
        let (conn, media_id) = setup();

        // 存在しない属性の削除はエラーにならない
        let result = delete_media_attribute(&conn, media_id, "nonexistent");
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_all_media_attributes() {
        let (conn, media_id) = setup();

        // 複数の属性を設定
        set_media_attribute(&conn, media_id, "attr1", Some("1"), None).unwrap();
        set_media_attribute(&conn, media_id, "attr2", Some("2"), None).unwrap();
        set_media_attribute(&conn, media_id, "attr3", Some("3"), None).unwrap();

        // 全削除
        delete_all_media_attributes(&conn, media_id).unwrap();

        // 全て削除されたことを確認
        let attrs = get_media_attributes(&conn, media_id).unwrap();
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn test_value_type_default_is_string() {
        let (conn, media_id) = setup();

        // value_type = None で設定
        set_media_attribute(&conn, media_id, "default_type", Some("test"), None).unwrap();

        let attr = get_media_attribute(&conn, media_id, "default_type")
            .unwrap()
            .unwrap();
        assert_eq!(attr.value_type, AttributeValueType::String);
    }
}
