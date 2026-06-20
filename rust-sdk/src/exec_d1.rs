//! D1（REST）バックエンドの `SqlExec` 実装
//!
//! `D1Client`（reqwest async）を通じて D1 REST API を叩く。`SqlParam`/`SqlRow` と
//! D1 の JSON 表現を相互変換し、組み立て層（Local と共有）からの `&dyn SqlExec` 呼び出しを受ける。
//!
//! - BLOB（`media_hashes.content_hash` 等）は D1 REST で **hex 文字列** として送受信
//!   （`to_d1_json_values` / `SqlRow::get_blob` が相互変換）。
//! - `execute_batch` は D1 REST がトランザクションをサポートしないため順次実行で近似（原子性なし）。
//! - `execute_raw` は SQL 文字列をそのまま送信（複文は D1 側で分割されることを前提・要実証）。

use crate::d1_client::D1Client;
use crate::db_value::{to_d1_json_values, SqlParam, SqlRow};
use crate::error::{KijukuError, Result};
use crate::exec::SqlExec;
use async_trait::async_trait;
use serde_json::Value;

/// D1 REST を包んだ実行バックエンド
pub struct D1Exec {
    client: D1Client,
}

impl D1Exec {
    pub fn new(client: D1Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl SqlExec for D1Exec {
    async fn query(&self, sql: &str, params: &[SqlParam]) -> Result<Vec<SqlRow>> {
        let json_params = to_d1_json_values(params);
        let result = self.client.query(sql, &json_params).await?;
        result.results.iter().map(json_obj_to_sqlrow).collect()
    }

    async fn execute(&self, sql: &str, params: &[SqlParam]) -> Result<usize> {
        let json_params = to_d1_json_values(params);
        let result = self.client.query(sql, &json_params).await?;
        Ok(result.meta.changes as usize)
    }

    async fn execute_batch(&self, stmts: Vec<(String, Vec<SqlParam>)>) -> Result<()> {
        // D1 REST はトランザクション未サポートのため、各文を順次実行で近似（原子性なし）。
        for (sql, params) in stmts {
            let json_params = to_d1_json_values(&params);
            self.client.query(&sql, &json_params).await?;
        }
        Ok(())
    }

    async fn execute_raw(&self, sql: &str) -> Result<()> {
        // バインドパラメータ無しの生 SQL（schema.sql 全体や多文マイグレーション）。
        // D1 REST は複文をサーバ側で分割して実行する（要実証）。
        self.client.query(sql, &[]).await?;
        Ok(())
    }
}

/// D1 の結果行（列名→値の JSON オブジェクト）を `SqlRow` へ変換
fn json_obj_to_sqlrow(obj: &Value) -> Result<SqlRow> {
    let map = obj
        .as_object()
        .ok_or_else(|| KijukuError::Parse("D1 row is not an object".to_string()))?;
    let mut row = SqlRow::new();
    for (k, v) in map {
        row.set(k, json_value_to_sqlparam(v));
    }
    Ok(row)
}

/// JSON 値を `SqlParam` へ変換。
/// BLOB は D1 から hex 文字列で返るため `Text` に入れる（`get_blob` が hex デコード）。
fn json_value_to_sqlparam(v: &Value) -> SqlParam {
    match v {
        Value::Null => SqlParam::Null,
        Value::Bool(b) => SqlParam::Int(if *b { 1 } else { 0 }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                SqlParam::Int(i)
            } else if let Some(f) = n.as_f64() {
                SqlParam::Real(f)
            } else {
                SqlParam::Text(n.to_string())
            }
        }
        Value::String(s) => SqlParam::Text(s.clone()),
        _ => SqlParam::Text(v.to_string()),
    }
}
