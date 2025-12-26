use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// メディアタイプの定義
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Comic,
    Video,
    Music,
}

impl MediaType {
    /// 文字列からMediaTypeへの変換
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "comic" => Some(MediaType::Comic),
            "video" => Some(MediaType::Video),
            "music" => Some(MediaType::Music),
            _ => None,
        }
    }

    /// MediaTypeを文字列に変換
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Comic => "comic",
            MediaType::Video => "video",
            MediaType::Music => "music",
        }
    }
}

/// メディア情報の完全な型定義
///
/// 注意: volume_numberは自動計算されます。
/// 保存時にvolume_textが整数なら、自動的にvolume_numberに設定されます。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    pub id: i64,
    pub title: String,
    pub title_id: Option<String>,
    pub path: Option<String>,
    pub media_type: MediaType,
    pub thumbnail_path: Option<String>,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub description: Option<String>,
    pub file_size: Option<i64>,
    pub duration_sec: Option<i32>,
    pub page_count: Option<i32>,
    pub series: Option<String>,
    pub volume_number: Option<i32>,  // volume_textから自動計算（ソート・フィルタ可能）
    pub volume_text: Option<String>,
    pub volume_title: Option<String>,
    pub magazine: Option<String>,
    pub magazine_id: Option<String>,
    pub language: Option<String>,
    pub source: Option<String>,
    pub external_id: Option<String>,
    pub artist_en: Option<String>,
    pub title_en: Option<String>,
    pub chapters: Option<String>,
    pub extension: Option<String>,
    pub flag_exist: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub title_pron: Option<String>,
    pub artist_pron: Option<String>,
    pub series_pron: Option<String>,
}

/// メディア作成時の入力型
///
/// 注意: volume_numberは自動計算されるため、手動設定は無視されます。
/// volume_textに整数を設定すると、保存時に自動的にvolume_numberが計算されます。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInput {
    pub title: String,
    pub media_type: MediaType,
    pub title_id: Option<String>,
    pub path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub description: Option<String>,
    pub file_size: Option<i64>,
    pub duration_sec: Option<i32>,
    pub page_count: Option<i32>,
    pub series: Option<String>,
    #[serde(skip_deserializing)]  // 手動設定は無視
    pub volume_number: Option<i32>,
    pub volume_text: Option<String>,
    pub volume_title: Option<String>,
    pub magazine: Option<String>,
    pub magazine_id: Option<String>,
    pub language: Option<String>,
    pub source: Option<String>,
    pub external_id: Option<String>,
    pub artist_en: Option<String>,
    pub title_en: Option<String>,
    pub chapters: Option<String>,
    pub extension: Option<String>,
    pub flag_exist: Option<bool>,
    pub title_pron: Option<String>,
    pub artist_pron: Option<String>,
    pub series_pron: Option<String>,
}

/// メディア検索時のフィルタ条件
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MediaFilter {
    pub title: Option<String>,
    pub title_id: Option<String>,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub media_type: Option<MediaType>,
    pub series: Option<String>,
    pub source: Option<String>,
    pub tag_ids: Option<Vec<i64>>,
}

/// ソート順序
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SortOrder {
    Asc,
    Desc,
}

impl SortOrder {
    pub fn as_str(&self) -> &'static str {
        match self {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        }
    }
}

/// クエリオプション（ソート、ページネーション）
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct QueryOptions {
    pub order_by: Option<String>,
    pub order: Option<SortOrder>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// タグ情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

/// データベース接続オプション
#[derive(Debug, Default, Clone)]
pub struct DBOptions {
    pub timeout: Option<u64>,
    pub readonly: bool,
    pub verbose: bool,
    pub backup: Option<crate::backup::BackupOptions>,
}

/// メディア属性（EAVモデル）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAttribute {
    pub media_id: i64,
    pub key: String,
    pub value: Option<String>,
    pub value_type: AttributeValueType,
}

/// 属性値の型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttributeValueType {
    String,
    Integer,
    Boolean,
}

impl AttributeValueType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttributeValueType::String => "string",
            AttributeValueType::Integer => "integer",
            AttributeValueType::Boolean => "boolean",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_type_from_str() {
        // 有効なメディアタイプの変換
        assert_eq!(MediaType::from_str("comic"), Some(MediaType::Comic));
        assert_eq!(MediaType::from_str("video"), Some(MediaType::Video));
        assert_eq!(MediaType::from_str("music"), Some(MediaType::Music));

        // 大文字小文字を区別しない
        assert_eq!(MediaType::from_str("COMIC"), Some(MediaType::Comic));
        assert_eq!(MediaType::from_str("Video"), Some(MediaType::Video));
        assert_eq!(MediaType::from_str("MuSiC"), Some(MediaType::Music));

        // 無効な文字列の場合はNoneを返す
        assert_eq!(MediaType::from_str("invalid"), None);
        assert_eq!(MediaType::from_str(""), None);
        assert_eq!(MediaType::from_str("book"), None);
    }

    #[test]
    fn test_media_type_as_str() {
        // MediaTypeを文字列に変換
        assert_eq!(MediaType::Comic.as_str(), "comic");
        assert_eq!(MediaType::Video.as_str(), "video");
        assert_eq!(MediaType::Music.as_str(), "music");
    }

    #[test]
    fn test_media_type_roundtrip() {
        // 変換の往復が正しく動作することを確認
        let types = vec![MediaType::Comic, MediaType::Video, MediaType::Music];
        for media_type in types {
            let str_repr = media_type.as_str();
            let parsed = MediaType::from_str(str_repr);
            assert_eq!(parsed, Some(media_type));
        }
    }

    #[test]
    fn test_sort_order_as_str() {
        // SortOrderを文字列に変換
        assert_eq!(SortOrder::Asc.as_str(), "ASC");
        assert_eq!(SortOrder::Desc.as_str(), "DESC");
    }

    #[test]
    fn test_attribute_value_type_as_str() {
        // AttributeValueTypeを文字列に変換
        assert_eq!(AttributeValueType::String.as_str(), "string");
        assert_eq!(AttributeValueType::Integer.as_str(), "integer");
        assert_eq!(AttributeValueType::Boolean.as_str(), "boolean");
    }

    #[test]
    fn test_media_filter_default() {
        // MediaFilterのデフォルト値を確認
        let filter = MediaFilter::default();
        assert!(filter.title.is_none());
        assert!(filter.title_id.is_none());
        assert!(filter.artist.is_none());
        assert!(filter.artist_id.is_none());
        assert!(filter.media_type.is_none());
        assert!(filter.series.is_none());
        assert!(filter.source.is_none());
        assert!(filter.tag_ids.is_none());
    }

    #[test]
    fn test_query_options_default() {
        // QueryOptionsのデフォルト値を確認
        let options = QueryOptions::default();
        assert!(options.order_by.is_none());
        assert!(options.order.is_none());
        assert!(options.limit.is_none());
        assert!(options.offset.is_none());
    }
}
