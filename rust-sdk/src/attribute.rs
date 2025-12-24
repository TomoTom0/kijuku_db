//! メディア追加属性の操作

use rusqlite::{params, Connection, Result};
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
