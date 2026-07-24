use thiserror::Error;

/// きじゅくDBのエラー型
#[derive(Error, Debug)]
pub enum KijukuError {
    /// データベースエラー
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// IO エラー
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// データが見つからないエラー
    #[error("Not found: {0}")]
    NotFound(String),

    /// バリデーションエラー
    #[error("Validation error: {0}")]
    Validation(String),

    /// パース/変換エラー
    #[error("Parse error: {0}")]
    Parse(String),

    /// その他のエラー
    #[error("Error: {0}")]
    Other(String),

    /// HTTPエラー（D1 REST API等）
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// D1 APIエラー
    #[error("D1 error: {0}")]
    D1(String),

    /// サポートされていない操作（D1等の制約）
    #[error("Not supported: {0}")]
    NotSupported(String),

    /// stg が別セッションで使用中（排他ロック取得失敗・設計 §15-11）
    #[error("Stg is busy (locked by another session): {stg_path}")]
    StgBusy {
        stg_path: String,
        holder_pid: Option<u32>,
    },

    /// prod が別セッション/promote で使用中（prod 排他ロック取得失敗・設計 §5.3・TASK-56）
    #[error("Prod is busy (locked by another promote/admin session): {prod_path}")]
    ProdBusy {
        prod_path: String,
        holder_pid: Option<u32>,
    },

    /// promote gate 不合格（prod に触る前に拒否・設計 §4.5/§6.4・TASK-57）。
    /// `failed_checks` は `{name}: {detail}` 形式の不合格 gate 一覧。
    #[error("Promote gate failed (prod not touched): {}", failed_checks.join("; "))]
    PromoteGateFailed { failed_checks: Vec<String> },
}

/// きじゅくDBのResult型
pub type Result<T> = std::result::Result<T, KijukuError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_error() {
        let error = KijukuError::NotFound("メディアが見つかりません".to_string());
        assert_eq!(
            error.to_string(),
            "Not found: メディアが見つかりません"
        );
    }

    #[test]
    fn test_validation_error() {
        let error = KijukuError::Validation("タイトルは必須です".to_string());
        assert_eq!(
            error.to_string(),
            "Validation error: タイトルは必須です"
        );
    }

    #[test]
    fn test_parse_error() {
        let error = KijukuError::Parse("無効なJSONフォーマット".to_string());
        assert_eq!(
            error.to_string(),
            "Parse error: 無効なJSONフォーマット"
        );
    }

    #[test]
    fn test_other_error() {
        let error = KijukuError::Other("予期しないエラー".to_string());
        assert_eq!(
            error.to_string(),
            "Error: 予期しないエラー"
        );
    }

    #[test]
    fn test_error_debug_output() {
        let error = KijukuError::NotFound("test".to_string());
        let debug_output = format!("{:?}", error);
        assert!(debug_output.contains("NotFound"));
    }

    #[test]
    fn test_result_type_ok() {
        let result: Result<i32> = Ok(42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_result_type_err() {
        let result: Result<i32> = Err(KijukuError::NotFound("not found".to_string()));
        assert!(result.is_err());
    }
}
