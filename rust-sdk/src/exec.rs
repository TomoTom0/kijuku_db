//! 低レベル SQL 実行の抽象 trait
//!
//! 組み立て層（search/crud/...）は `&dyn SqlExec` を通じて SQL を実行する。
//! Local（rusqlite）と D1（REST）がこれを実装し、SQL 文字列とパラメータの
//! 組み立てロジックを両者で共有する。

use crate::db_value::{SqlParam, SqlRow};
use crate::error::Result;
use async_trait::async_trait;

/// バックエンド非依存の SQL 実行インターフェース
#[async_trait]
pub trait SqlExec: Send + Sync {
    /// SELECT 系。行のリストを返す。
    async fn query(&self, sql: &str, params: &[SqlParam]) -> Result<Vec<SqlRow>>;

    /// INSERT/UPDATE/DELETE 系。影響を受けた行数を返す。
    async fn execute(&self, sql: &str, params: &[SqlParam]) -> Result<usize>;

    /// 複数文を実行する。原子性は実装依存:
    /// Local は単一トランザクション内で原子的に実行、D1 は REST がトランザクション非対応のため
    /// 順次実行（途中失敗で部分適用が残る可能性・best-effort）。
    async fn execute_batch(&self, stmts: Vec<(String, Vec<SqlParam>)>) -> Result<()>;

    /// バインドパラメータを持たない生の SQL スクリプトを実行（`;` 区切りの複数文を含む）。
    /// migration（`schema.sql` 全体や v4/v5/v6 のテーブル再構築等）向け。
    /// Local は rusqlite `Connection::execute_batch` のネイティブ複数文パーサに委譲する
    /// （`CREATE TRIGGER ... BEGIN ... END;` 内の `;` を正しく扱うため文字列分割はしない）。
    async fn execute_raw(&self, sql: &str) -> Result<()>;
}
