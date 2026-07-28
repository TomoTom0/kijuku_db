//! 監査ログ（prod 保護操作の事後追跡・設計 §10/§15-6・TASK-46）。
//!
//! - prod DB 本体に入れない（promote で破壊的上書きされるため）・prod 外の append-only
//!   サイドカー `backup/meta/audit.log`（JSONL）に記録。
//! - 監査は人間の承認フローでなく事後追跡用。環境 gate が安全を担保し、監査は事後追跡のみ。
//! - 記録対象: sync/discard・observe/diffProdStg・promote・(b)操作（restore/mediaMv/purgeTrash）。

use serde::{Deserialize, Serialize};

/// `replicate_db` の操作種別（sync/discard）。監査 operation 文字列の区別用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOp {
    /// prod → stg の複製（§4.2）。
    Sync,
    /// stg 破棄・再 sync（§4.6）。
    Discard,
}

impl SyncOp {
    /// 監査 operation 文字列。
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncOp::Sync => "sync",
            SyncOp::Discard => "discard",
        }
    }
}

/// 監査対象 DB。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditTarget {
    Prod,
    Stg,
}

/// 操作結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditResult {
    Success,
    Failure,
    DryRun,
}

/// 監査レコード（`audit.log` の1行・JSONL）。
///
/// 設計 §10「timestamp・操作・対象範囲・実行者・diff/gate サマリ」。Rust/TS 共通で
/// camelCase（既存 parity 慣行）。`summary` は操作ごとに構造が異なるため `serde_json::Value`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecord {
    /// ISO 8601 UTC（ミリ秒・`chrono::Utc::now()`）。
    pub timestamp: String,
    /// sync|discard|observe|diffProdStg|promote|b-restore|b-mediaMv|b-purgeTrash
    pub operation: String,
    /// 操作の対象 DB。
    pub target: AuditTarget,
    /// 実行者（`USER` env・best-effort・取れなければ null）。LLM vs 人間の区別はしない。
    pub actor: Option<String>,
    /// prod DB の絶対パス（監査ログの所在 HB）。
    pub prod_db_path: String,
    /// 操作結果。
    pub result: AuditResult,
    /// failure 時のエラー文字列・それ以外は null。
    pub error: Option<String>,
    /// 操作ごとの構造的サマリ（diff/gate 結果・revision・preStashPath 等）。
    pub summary: serde_json::Value,
}

impl AuditRecord {
    /// 現在時刻の ISO 8601 UTC（ミリ秒）。
    pub fn now_timestamp() -> String {
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    /// 実行者（`USER` env・best-effort）。
    pub fn actor_from_env() -> Option<String> {
        std::env::var("USER").ok()
    }
}

/// `list_audit_logs` のフィルタ（設計 §10・TASK-46）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AuditLogFilter {
    /// 操作種別（完全一致）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// 開始日時 ISO 8601（包含・timestamp 辞書順比較）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// 終了日時 ISO 8601（包含・timestamp 辞書順比較）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// 上限件数（既定 1000・上限 10000）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

impl AuditLogFilter {
    pub const DEFAULT_LIMIT: usize = 1000;
    pub const MAX_LIMIT: usize = 10_000;
}
