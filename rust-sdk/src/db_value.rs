//! SQL パラメータ・行のバックエンド非依存表現
//!
//! rusqlite（Local）と D1 REST（D1）の橋渡しを行う。
//! 組み立て層は `SqlParam` / `SqlRow` だけを扱い、各バックエンドの実行層が
//! これを各々の表現（rusqlite `ToSql` / D1 JSON）に変換する。

use crate::error::{KijukuError, Result};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::types::ValueRef;
use std::collections::HashMap;

/// SQL パラメータのバックエンド非依存表現
#[derive(Debug, Clone, PartialEq)]
pub enum SqlParam {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl SqlParam {
    /// `Option<String>` から生成（None は Null）
    pub fn from_opt_str(v: &Option<String>) -> Self {
        match v {
            Some(s) => SqlParam::Text(s.clone()),
            None => SqlParam::Null,
        }
    }

    /// `Option<&str>` から生成
    pub fn from_opt_str_ref(v: Option<&str>) -> Self {
        match v {
            Some(s) => SqlParam::Text(s.to_string()),
            None => SqlParam::Null,
        }
    }

    pub fn from_opt_i64(v: Option<i64>) -> Self {
        match v {
            Some(i) => SqlParam::Int(i),
            None => SqlParam::Null,
        }
    }

    pub fn from_bool(b: bool) -> Self {
        SqlParam::Int(if b { 1 } else { 0 })
    }
}

/// rusqlite `ValueRef` から `SqlParam` へ変換（LocalExec の行構築用）
pub fn sql_param_from_value_ref(v: ValueRef) -> Result<SqlParam> {
    Ok(match v {
        ValueRef::Null => SqlParam::Null,
        ValueRef::Integer(i) => SqlParam::Int(i),
        ValueRef::Real(f) => SqlParam::Real(f),
        ValueRef::Text(bytes) => {
            let s = std::str::from_utf8(bytes)
                .map_err(|e| KijukuError::Parse(format!("UTF-8 decode error: {}", e)))?;
            SqlParam::Text(s.to_string())
        }
        ValueRef::Blob(bytes) => SqlParam::Blob(bytes.to_vec()),
    })
}

/// `&[SqlParam]` を rusqlite の ToSql 参照スライスへ変換（LocalExec 内部用）
pub fn to_rusqlite_refs(params: &[SqlParam]) -> Vec<Box<dyn rusqlite::ToSql>> {
    params
        .iter()
        .map(|p| -> Box<dyn rusqlite::ToSql> {
            match p {
                SqlParam::Null => Box::new(rusqlite::types::Null),
                SqlParam::Int(i) => Box::new(*i),
                SqlParam::Real(f) => Box::new(*f),
                SqlParam::Text(s) => Box::new(s.clone()),
                SqlParam::Blob(b) => Box::new(b.clone()),
            }
        })
        .collect()
}

/// `&[SqlParam]` を D1 REST の JSON params 配列へ変換（D1Exec 内部用）
///
/// BLOB は D1 REST では hex 文字列として送受信する（`media_hashes.content_hash` 等の
/// 固定長バイナリ想定）。取得時は文字列カラムを hex デコードして `Blob` に戻す。
pub fn to_d1_json_values(params: &[SqlParam]) -> Vec<serde_json::Value> {
    params
        .iter()
        .map(|p| match p {
            SqlParam::Null => serde_json::Value::Null,
            SqlParam::Int(i) => serde_json::Value::from(*i),
            SqlParam::Real(f) => serde_json::json!(*f),
            SqlParam::Text(s) => serde_json::Value::String(s.clone()),
            SqlParam::Blob(b) => serde_json::Value::String(hex::encode(b)),
        })
        .collect()
}

/// 行のバックエンド非依存表現（カラム名 → 値）
#[derive(Debug, Clone, Default)]
pub struct SqlRow {
    cols: HashMap<String, SqlParam>,
}

impl SqlRow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, col: &str, val: SqlParam) {
        self.cols.insert(col.to_string(), val);
    }

    pub fn get(&self, col: &str) -> Option<&SqlParam> {
        self.cols.get(col)
    }

    fn require(&self, col: &str) -> Result<&SqlParam> {
        self.cols
            .get(col)
            .ok_or_else(|| KijukuError::Parse(format!("column not found: {}", col)))
    }

    pub fn get_int(&self, col: &str) -> Result<i64> {
        match self.require(col)? {
            SqlParam::Int(i) => Ok(*i),
            SqlParam::Real(f) => Ok(*f as i64),
            SqlParam::Text(s) => s
                .parse::<i64>()
                .map_err(|_| KijukuError::Parse(format!("expected int for {}, got '{}'", col, s))),
            other => Err(KijukuError::Parse(format!(
                "expected int for {}, got {:?}",
                col, other
            ))),
        }
    }

    pub fn get_opt_int(&self, col: &str) -> Result<Option<i64>> {
        match self.cols.get(col) {
            None | Some(SqlParam::Null) => Ok(None),
            Some(SqlParam::Int(i)) => Ok(Some(*i)),
            Some(SqlParam::Real(f)) => Ok(Some(*f as i64)),
            Some(SqlParam::Text(s)) => s
                .parse::<i64>()
                .map(Some)
                .map_err(|_| KijukuError::Parse(format!("expected int for {}, got '{}'", col, s))),
            Some(other) => Err(KijukuError::Parse(format!(
                "expected int for {}, got {:?}",
                col, other
            ))),
        }
    }

    pub fn get_opt_i32(&self, col: &str) -> Result<Option<i32>> {
        Ok(self.get_opt_int(col)?.map(|i| i as i32))
    }

    pub fn get_text(&self, col: &str) -> Result<String> {
        match self.require(col)? {
            SqlParam::Text(s) => Ok(s.clone()),
            SqlParam::Int(i) => Ok(i.to_string()),
            other => Err(KijukuError::Parse(format!(
                "expected text for {}, got {:?}",
                col, other
            ))),
        }
    }

    pub fn get_opt_text(&self, col: &str) -> Result<Option<String>> {
        match self.cols.get(col) {
            None | Some(SqlParam::Null) => Ok(None),
            Some(SqlParam::Text(s)) => Ok(Some(s.clone())),
            Some(SqlParam::Int(i)) => Ok(Some(i.to_string())),
            Some(SqlParam::Real(f)) => Ok(Some(f.to_string())),
            Some(other) => Err(KijukuError::Parse(format!(
                "expected text/null for {}, got {:?}",
                col, other
            ))),
        }
    }

    pub fn get_bool(&self, col: &str) -> Result<bool> {
        Ok(self.get_int(col)? != 0)
    }

    pub fn get_blob(&self, col: &str) -> Result<Vec<u8>> {
        match self.require(col)? {
            SqlParam::Blob(b) => Ok(b.clone()),
            // D1 からは hex 文字列で返ってくるため、テキストを hex デコード
            SqlParam::Text(s) => hex::decode(s).map_err(|e| {
                KijukuError::Parse(format!("hex decode error for {}: {}", col, e))
            }),
            other => Err(KijukuError::Parse(format!(
                "expected blob for {}, got {:?}",
                col, other
            ))),
        }
    }

    /// BLOB カラムの Option 版（NULL/不在は None）。
    /// `media_hashes.embedding` 等の NULLable バイナリカラム用。D1 からは hex 文字列で来る。
    pub fn get_opt_blob(&self, col: &str) -> Result<Option<Vec<u8>>> {
        match self.cols.get(col) {
            None | Some(SqlParam::Null) => Ok(None),
            Some(SqlParam::Blob(b)) => Ok(Some(b.clone())),
            Some(SqlParam::Text(s)) => hex::decode(s).map(Some).map_err(|e| {
                KijukuError::Parse(format!("hex decode error for {}: {}", col, e))
            }),
            Some(other) => Err(KijukuError::Parse(format!(
                "expected blob/null for {}, got {:?}",
                col, other
            ))),
        }
    }

    /// タイムスタンプ文字列を `DateTime<Utc>` へ変換。
    /// SQLite の `CURRENT_TIMESTAMP`（"YYYY-MM-DD HH:MM:SS"）と
    /// RFC3339（"YYYY-MM-DDTHH:MM:SSZ"）の両方を受け付ける。
    pub fn get_datetime(&self, col: &str) -> Result<DateTime<Utc>> {
        let s = self.get_text(col)?;
        DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                    .map(|ndt| Utc.from_utc_datetime(&ndt))
            })
            .map_err(|e| KijukuError::Parse(format!("datetime parse error for {}: {}", col, e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlrow_getters() {
        let mut row = SqlRow::new();
        row.set("id", SqlParam::Int(42));
        row.set("title", SqlParam::Text("hello".to_string()));
        row.set("flag_exist", SqlParam::Int(1));
        row.set("opt", SqlParam::Null);

        assert_eq!(row.get_int("id").unwrap(), 42);
        assert_eq!(row.get_text("title").unwrap(), "hello");
        assert!(row.get_bool("flag_exist").unwrap());
        assert_eq!(row.get_opt_text("opt").unwrap(), None);
    }

    #[test]
    fn test_blob_hex_roundtrip() {
        let params = vec![SqlParam::Blob(vec![0xde, 0xad, 0xbe, 0xef])];
        let json = to_d1_json_values(&params);
        assert_eq!(json[0], serde_json::Value::String("deadbeef".to_string()));

        let mut row = SqlRow::new();
        row.set("h", SqlParam::Text("deadbeef".to_string()));
        assert_eq!(row.get_blob("h").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
    }
}
