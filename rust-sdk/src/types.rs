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
    pub uuid: String,
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
    /// UUIDを手動指定する場合はここに設定。省略時は自動生成。
    pub uuid: Option<String>,
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
    pub flag_exist: Option<bool>,
    pub language: Option<String>,
    pub magazine: Option<String>,
    pub magazine_id: Option<String>,
    pub extension: Option<String>,
    pub external_id: Option<String>,
    pub volume_title: Option<String>,
    pub title_en: Option<String>,
    pub artist_en: Option<String>,
    /// IDのIN句フィルタ（複数IDを一括フェッチする場合に使用）
    pub id_in: Option<Vec<i64>>,
    /// OR条件で結合する追加フィルタ
    /// 各フィルタ内の条件はAND結合、or_filters間はOR結合される
    #[serde(default)]
    pub or_filters: Option<Vec<MediaFilter>>,
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

/// メディア更新時の入力型（部分更新用）
///
/// フィールドの更新セマンティクス:
/// - `None`: フィールドを更新しない
/// - `Some(None)`: フィールドをNULLに設定する
/// - `Some(Some(value))`: フィールドを新しい値で更新する
///
/// 注意: title, media_type, flag_existはNOT NULL制約があるためOption<T>のまま
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaUpdateInput {
    /// UUID - Noneで更新しない、Some(Some(v))で更新（空文字列は不可）
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub uuid: Option<Option<String>>,
    /// タイトル（NOT NULL）- Noneで更新しない、Someで更新
    pub title: Option<String>,
    /// メディアタイプ（NOT NULL）- Noneで更新しない、Someで更新
    pub media_type: Option<MediaType>,
    /// タイトルID - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub title_id: Option<Option<String>>,
    /// パス - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub path: Option<Option<String>>,
    /// サムネイルパス - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub thumbnail_path: Option<Option<String>>,
    /// アーティスト - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub artist: Option<Option<String>>,
    /// アーティストID - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub artist_id: Option<Option<String>>,
    /// 説明 - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub description: Option<Option<String>>,
    /// ファイルサイズ - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub file_size: Option<Option<i64>>,
    /// 再生時間（秒） - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub duration_sec: Option<Option<i32>>,
    /// ページ数 - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub page_count: Option<Option<i32>>,
    /// シリーズ - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub series: Option<Option<String>>,
    /// ボリュームテキスト - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub volume_text: Option<Option<String>>,
    /// ボリュームタイトル - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub volume_title: Option<Option<String>>,
    /// 雑誌 - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub magazine: Option<Option<String>>,
    /// 雑誌ID - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub magazine_id: Option<Option<String>>,
    /// 言語 - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub language: Option<Option<String>>,
    /// ソース - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub source: Option<Option<String>>,
    /// 外部ID - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub external_id: Option<Option<String>>,
    /// 英語アーティスト名 - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub artist_en: Option<Option<String>>,
    /// 英語タイトル - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub title_en: Option<Option<String>>,
    /// チャプター - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub chapters: Option<Option<String>>,
    /// 拡張子 - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub extension: Option<Option<String>>,
    /// 存在フラグ（NOT NULL）- Noneで更新しない、Someで更新
    pub flag_exist: Option<bool>,
    /// タイトル読み - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub title_pron: Option<Option<String>>,
    /// アーティスト読み - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub artist_pron: Option<Option<String>>,
    /// シリーズ読み - Noneで更新しない、Some(None)でNULL、Some(Some(v))で更新
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub series_pron: Option<Option<String>>,
}

/// Option<Option<T>>のデシリアライズヘルパー
/// JSONでnullを明示的に指定した場合はSome(None)、
/// フィールドが存在しない場合はNoneになる
fn deserialize_nullable_field<'de, T, D>(deserializer: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    // フィールドが存在する場合、Option<T>としてデシリアライズ
    // null -> Some(None), 値あり -> Some(Some(値))
    Option::<T>::deserialize(deserializer).map(Some)
}

/// 一括更新時の個別アイテム
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkUpdateItem {
    pub id: i64,
    pub data: MediaUpdateInput,
}

/// ソートキー（フィールドと方向）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortKey {
    pub field: String,
    pub order: SortOrder,
}

/// クエリオプション（ソート、ページネーション）
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct QueryOptions {
    #[serde(default)]
    pub sort_keys: Vec<SortKey>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// タグ情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

/// タグ使用統計情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagUsageStats {
    pub tag_id: i64,
    pub tag_name: String,
    pub count: i64,
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
        assert!(options.sort_keys.is_empty());
        assert!(options.limit.is_none());
        assert!(options.offset.is_none());
    }
}
