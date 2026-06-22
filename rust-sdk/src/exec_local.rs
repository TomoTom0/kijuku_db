//! Local（rusqlite）バックエンドの `SqlExec` 実装
//!
//! `Arc<ReentrantMutex<Connection>>` を持ち、各操作を `spawn_blocking` で_blocking_ スレッドプールで
//! 実行する。async コンテキストから呼んでもランタイムスレッドを占有しない。

use crate::db_value::{sql_param_from_value_ref, to_rusqlite_refs, SqlParam, SqlRow};
use crate::error::{KijukuError, Result};
use crate::exec::SqlExec;
use async_trait::async_trait;
use parking_lot::ReentrantMutex;
use rusqlite::Connection;
use std::sync::Arc;

/// rusqlite `Connection` を包んだ Local 実行バックエンド
pub struct LocalExec {
    conn: Arc<ReentrantMutex<Connection>>,
}

impl LocalExec {
    pub fn new(conn: Arc<ReentrantMutex<Connection>>) -> Self {
        Self { conn }
    }

    /// 内部の `Connection` を指す `Arc` を取得（KijukuDB と共有するため）
    pub fn conn_handle(&self) -> Arc<ReentrantMutex<Connection>> {
        Arc::clone(&self.conn)
    }
}

#[async_trait]
impl SqlExec for LocalExec {
    async fn query(&self, sql: &str, params: &[SqlParam]) -> Result<Vec<SqlRow>> {
        let conn = Arc::clone(&self.conn);
        let sql = sql.to_string();
        let params = params.to_vec();
        tokio::task::spawn_blocking(move || -> Result<Vec<SqlRow>> {
            let conn = conn.lock();
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
            let conn = conn.lock();
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
            let conn = conn.lock();
            // ReentrantMutex の guard は &mut を提供しないため、rusqlite::Connection::transaction
            // （&mut self 必要）の代わりに SQL の BEGIN/COMMIT/ROLLBACK で原子性を保証する。
            // エラー時は必ず ROLLBACK し、接続がトランザクション状態で残留するのを防ぐ。
            conn.execute("BEGIN", [])?;
            let exec_result: Result<()> = (|| {
                for (sql, params) in &stmts {
                    let refs = to_rusqlite_refs(params);
                    let refs_dyn: Vec<&dyn rusqlite::ToSql> =
                        refs.iter().map(|r| r.as_ref()).collect();
                    conn.execute(sql, refs_dyn.as_slice())?;
                }
                Ok(())
            })();
            match exec_result {
                Ok(()) => {
                    conn.execute("COMMIT", [])?;
                    Ok(())
                }
                Err(e) => {
                    let _ = conn.execute("ROLLBACK", []);
                    Err(e)
                }
            }
        })
        .await
        .map_err(|e| KijukuError::Other(format!("spawn_blocking join error: {}", e)))?
    }

    async fn execute_raw(&self, sql: &str) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let sql = sql.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = conn.lock();
            conn.execute_batch(&sql)?;
            Ok(())
        })
        .await
        .map_err(|e| KijukuError::Other(format!("spawn_blocking join error: {}", e)))?
    }
}
