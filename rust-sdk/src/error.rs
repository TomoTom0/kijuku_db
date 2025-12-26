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
