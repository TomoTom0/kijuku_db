//! Cloudflare D1 REST API クライアント
//!
//! wrangler の OAuth トークン（`~/.config/.wrangler/config/default.toml`）を再利用し、
//! API token の手動発行なしに D1 の REST query API を叩く。`wrangler login` 済みなら
//! `D1Client::from_wrangler` で接続できる。
//!
//! エンドポイント: `POST /client/v4/accounts/{account_id}/d1/database/{database_id}/query`
//! 認証: `Authorization: Bearer {oauth_token}`（wrangler キャッシュの `oauth_token`）

use crate::error::{KijukuError, Result};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

/// D1 接続設定
#[derive(Debug, Clone)]
pub struct D1Config {
    pub account_id: String,
    pub database_id: String,
}

/// D1 REST クライアント
pub struct D1Client {
    config: D1Config,
    token: String,
    http: reqwest::Client,
}

impl D1Client {
    /// wrangler のキャッシュから OAuth トークンを読んで構築（`wrangler login` 済み前提）
    pub fn from_wrangler(config: D1Config) -> Result<Self> {
        let token = read_wrangler_token()?;
        Ok(Self::with_token(config, token))
    }

    /// 明示的なトークンで構築（API token を使う場合やテスト用）
    pub fn with_token(config: D1Config, token: String) -> Self {
        Self {
            config,
            token,
            http: reqwest::Client::new(),
        }
    }

    /// D1 query API を実行。1文を想定（複文は `execute_raw` 経由で別途扱う）。
    pub async fn query(&self, sql: &str, params: &[Value]) -> Result<D1Result> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/d1/database/{}/query",
            self.config.account_id, self.config.database_id
        );
        let body = serde_json::json!({ "sql": sql, "params": params });

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(KijukuError::Http)?;
        let status = resp.status();
        // ボディは text で受け取り、status チェック後に JSON へパースする。
        // 非 2xx で HTML/プレーンテキスト（WAF・ゲートウェイ障害・レート制限）が返っても、
        // JSON パースエラーに埋もれず本来の HTTP ステータスとボディ断片を伝えるため。
        let body_text = resp.text().await.map_err(KijukuError::Http)?;

        if !status.is_success() {
            let snippet: String = body_text.chars().take(300).collect();
            return Err(KijukuError::D1(format!(
                "D1 query failed (HTTP {}): {}",
                status, snippet
            )));
        }

        let parsed: D1Response = serde_json::from_str(&body_text).map_err(|e| {
            KijukuError::D1(format!(
                "D1 response parse failed (HTTP {}): {}",
                status, e
            ))
        })?;

        if !parsed.success {
            let msg = if parsed.errors.is_empty() {
                format!("D1 query returned success=false (HTTP {})", status)
            } else {
                parsed
                    .errors
                    .iter()
                    .map(|e| format!("{}: {}", e.code, e.message))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            return Err(KijukuError::D1(msg));
        }

        parsed
            .result
            .into_iter()
            .next()
            .ok_or_else(|| KijukuError::D1("D1 query returned no result".to_string()))
    }
}

/// 1クエリの実行結果
#[derive(Debug, Deserialize)]
pub struct D1Result {
    /// 結果行（各要素は列名→値の JSON オブジェクト）
    pub results: Vec<Value>,
    pub success: bool,
    pub meta: D1Meta,
}

#[derive(Debug, Default, Deserialize)]
pub struct D1Meta {
    /// 影響を受けた行数（INSERT/UPDATE/DELETE）
    #[serde(default)]
    pub changes: i64,
    /// 最後に挿入された row id（D1 では複数クライアント共有のため信頼できない→RETURNING を使用）
    #[serde(default)]
    pub last_row_id: i64,
}

#[derive(Debug, Deserialize)]
struct D1Response {
    result: Vec<D1Result>,
    success: bool,
    #[serde(default)]
    errors: Vec<D1Error>,
    #[serde(default)]
    #[allow(dead_code)]
    messages: Vec<D1Error>,
}

#[derive(Debug, Deserialize)]
struct D1Error {
    code: i64,
    message: String,
}

/// wrangler の OAuth トークンをキャッシュから読む。
///
/// NOTE: 期限切れ（`expiration_time`）時のリフレッシュは未対応。期限切れの場合は
/// エラーで包み、`wrangler login` の再実行を促す（本格運用では API token 採用も検討）。
fn read_wrangler_token() -> Result<String> {
    let path = wrangler_config_path()?;
    let content = std::fs::read_to_string(&path).map_err(|e| {
        KijukuError::Other(format!(
            "wrangler config の読み込みに失敗 ({}): {}。`wrangler login` 済みか確認してください。",
            path.display(),
            e
        ))
    })?;
    let toml_val: toml::Value = toml::from_str(&content)
        .map_err(|e| KijukuError::Other(format!("wrangler config のパースに失敗: {}", e)))?;
    let token = toml_val
        .get("oauth_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            KijukuError::Other("wrangler config に oauth_token がありません".to_string())
        })?;
    Ok(token.to_string())
}

/// wrangler のキャッシュパス（`~/.config/.wrangler/config/default.toml`）
fn wrangler_config_path() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| KijukuError::Other("HOME 環境変数が設定されていません".to_string()))?;
    Ok(PathBuf::from(home).join(".config/.wrangler/config/default.toml"))
}
