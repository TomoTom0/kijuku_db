//! Local（rusqlite）バックエンドの `SqlExec` 実装
//!
//! `Arc<Mutex<Connection>>` を持ち、各操作を `spawn_blocking` で_blocking_ スレッドプールで
//! 実行する。async コンテキストから呼んでもランタイムスレッドを占有しない。

use crate::db_value::{sql_param_from_value_ref, to_rusqlite_refs, SqlParam, SqlRow};
use crate::error::{KijukuError, Result};
use crate::exec::SqlExec;
use async_trait::async_trait;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// rusqlite `Connection` を包んだ Local 実行バックエンド
pub struct LocalExec {
    conn: Arc<Mutex<Connection>>,
}

impl LocalExec {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// 内部の `Connection` を指す `Arc` を取得（KijukuDB と共有するため）
    pub fn conn_handle(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }
}

fn lock_err<E: std::fmt::Display>(e: E) -> KijukuError {
    KijukuError::Other(format!("connection lock error: {}", e))
}

#[async_trait]
impl SqlExec for LocalExec {
    async fn query(&self, sql: &str, params: &[SqlParam]) -> Result<Vec<SqlRow>> {
        let conn = Arc::clone(&self.conn);
        let sql = sql.to_string();
        let params = params.to_vec();
        tokio::task::spawn_blocking(move || -> Result<Vec<SqlRow>> {
            let conn = conn.lock().map_err(lock_err)?;
            let mut stmt = conn.prepare(&sql)?;
            let col_names: Vec<String> =
                stmt.column_names().iter().map(|s| s.to_string()).collect();
            let refs = to_rusqlite_refs(&params);
            let refs_dyn: Vec<&dyn rusqlite::ToSql> = refs.iter().map(|r| r.as_ref()).collect();

            let mut rows = stmt.query(refs_dyn.as_slice())?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                let mut sqlrow = SqlRow::new();
                for (i, name) in col_names.iter().enumerate() {
                    let val = sql_param_from_value_ref(row.get_ref(i)?)?;
                    sqlrow.set(name, val);
                }
                out.push(sqlrow);
            }
            Ok(out)
        })
        .await
        .map_err(|e| KijukuError::Other(format!("spawn_blocking join error: {}", e)))?
    }

    async fn execute(&self, sql: &str, params: &[SqlParam]) -> Result<usize> {
        let conn = Arc::clone(&self.conn);
        let sql = sql.to_string();
        let params = params.to_vec();
        tokio::task::spawn_blocking(move || -> Result<usize> {
            let conn = conn.lock().map_err(lock_err)?;
            let refs = to_rusqlite_refs(&params);
            let refs_dyn: Vec<&dyn rusqlite::ToSql> = refs.iter().map(|r| r.as_ref()).collect();
            let n = conn.execute(&sql, refs_dyn.as_slice())?;
            Ok(n)
        })
        .await
        .map_err(|e| KijukuError::Other(format!("spawn_blocking join error: {}", e)))?
    }

    async fn execute_batch(&self, stmts: Vec<(String, Vec<SqlParam>)>) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut conn = conn.lock().map_err(lock_err)?;
            // 可視性の問題で borrow checker を満たすため、transaction は unchecked で取得
            let tx = conn.transaction()?;
            for (sql, params) in &stmts {
                let refs = to_rusqlite_refs(params);
                let refs_dyn: Vec<&dyn rusqlite::ToSql> = refs.iter().map(|r| r.as_ref()).collect();
                tx.execute(sql, refs_dyn.as_slice())?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(|e| KijukuError::Other(format!("spawn_blocking join error: {}", e)))?
    }

    async fn execute_raw(&self, sql: &str) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let sql = sql.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = conn.lock().map_err(lock_err)?;
            conn.execute_batch(&sql)?;
            Ok(())
        })
        .await
        .map_err(|e| KijukuError::Other(format!("spawn_blocking join error: {}", e)))?
    }
}
