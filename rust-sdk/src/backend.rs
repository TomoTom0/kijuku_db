//! バックエンド透過の高レベル DB 操作 trait
//!
//! Local（`KijukuDB`）/ SSH（`RemoteKijukuDB`）/ D1（`D1KijukuDB`・Phase 3）を
//! `Box<dyn KijukuBackend>` で実行時切替可能にする。呼び出し側はバックエンドを区別せず
//! 純 DB 操作を行える。ローカルファイル・外部コマンド依存の操作（thumbnail 生成 /
//! `compute_media_hash(s)` / `update_exist` / backup / transaction）は対象外で、各バックエンドの
//! 固有メソッドとして残る（D1 では提供しない）。

use crate::error::Result;
use crate::migration::TableColumnInfo;
use crate::types::{
    AttributeValueType, BulkUpdateItem, Media, MediaAttribute, MediaFilter, MediaHash,
    MediaHashInput, MediaInput, MediaUpdateInput, QueryOptions, Tag,
    TagUsageStats,
};
use async_trait::async_trait;

/// 純 DB 操作のみを抽象化するバックエンド trait（async）。
///
/// 実装は Local（`LocalExec` 経由）/ D1（`D1Exec` 経由）が組み立て層の `_async` 関数を共有し、
/// Remote（`RemoteKijukuDB`）は高レベル RPC を `spawn_blocking` で包む。
#[async_trait]
pub trait KijukuBackend: Send + Sync {
    // ========== migration / schema ==========

    /// マイグレーションを実行
    async fn migrate(&self) -> Result<()>;

    /// スキーマバージョンを取得
    async fn get_schema_version(&self) -> Result<i64>;

    /// テーブル一覧を取得
    async fn get_tables(&self) -> Result<Vec<String>>;

    /// 特定テーブルのカラム情報を取得
    async fn get_table_info(&self, table_name: &str) -> Result<Vec<TableColumnInfo>>;

    // ========== media CRUD ==========

    /// メディアを作成
    async fn create_media(&self, input: &MediaInput) -> Result<Media>;

    /// ID でメディアを取得
    async fn get_media(&self, id: i64) -> Result<Option<Media>>;

    /// メディアを更新（部分更新）
    async fn update_media(&self, id: i64, input: &MediaUpdateInput) -> Result<()>;

    /// メディアを削除
    async fn delete_media(&self, id: i64) -> Result<()>;

    // ========== search ==========

    /// メディアを検索
    async fn find_media(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
    ) -> Result<Vec<Media>>;

    /// 指定フィールド群の重複なし値の組み合わせ一覧を取得
    async fn get_distinct_values(
        &self,
        fields: &[&str],
        filter: &MediaFilter,
    ) -> Result<Vec<Vec<Option<String>>>>;

    // ========== bulk ==========

    /// 複数メディアを一括作成
    async fn bulk_create_media(&self, data_list: &[MediaInput]) -> Result<Vec<Media>>;

    /// 複数メディアを一括削除
    async fn bulk_delete_media(&self, ids: &[i64]) -> Result<()>;

    /// 複数メディアを一括更新
    async fn bulk_update_media(&self, updates: &[BulkUpdateItem]) -> Result<()>;

    // ========== tag ==========

    /// タグを作成
    async fn create_tag(&self, name: &str) -> Result<Tag>;

    /// タグ名でタグを取得
    async fn get_tag_by_name(&self, name: &str) -> Result<Option<Tag>>;

    /// 全タグを取得
    async fn get_all_tags(&self) -> Result<Vec<Tag>>;

    /// メディアにタグを追加
    async fn add_tag_to_media(&self, media_id: i64, tag_id: i64) -> Result<()>;

    /// メディアからタグを削除
    async fn remove_tag_from_media(&self, media_id: i64, tag_id: i64) -> Result<()>;

    /// メディアのタグを取得
    async fn get_media_tags(&self, media_id: i64) -> Result<Vec<Tag>>;

    /// タグ使用数統計を取得
    async fn get_tag_usage_stats(&self) -> Result<Vec<TagUsageStats>>;

    /// 未使用タグを取得
    async fn find_unused_tags(&self) -> Result<Vec<Tag>>;

    // ========== attribute ==========

    /// メディアに属性を設定
    async fn set_media_attribute(
        &self,
        media_id: i64,
        key: &str,
        value: Option<&str>,
        value_type: Option<AttributeValueType>,
    ) -> Result<()>;

    /// メディアの属性を取得
    async fn get_media_attribute(&self, media_id: i64, key: &str) -> Result<Option<MediaAttribute>>;

    /// メディアの全属性を取得
    async fn get_media_attributes(&self, media_id: i64) -> Result<Vec<MediaAttribute>>;

    /// メディアの属性を削除
    async fn delete_media_attribute(&self, media_id: i64, key: &str) -> Result<()>;

    /// メディアの全属性を削除
    async fn delete_all_media_attributes(&self, media_id: i64) -> Result<()>;

    // ========== hash（compute_* 以外）==========

    /// メディアハッシュを登録（単件・upsert）
    async fn add_media_hash(&self, input: &MediaHashInput) -> Result<MediaHash>;

    /// メディアハッシュを一括登録
    async fn add_media_hashes(&self, inputs: &[MediaHashInput]) -> Result<Vec<MediaHash>>;

    /// 特定作品の全ハッシュを取得
    async fn get_media_hashes(&self, item_uuid: &str) -> Result<Vec<MediaHash>>;

    /// 特定位置のハッシュを取得
    async fn get_media_hash(
        &self,
        item_uuid: &str,
        filename: &str,
        time_range: &str,
    ) -> Result<Option<MediaHash>>;

    /// SHA256 による完全一致検索
    async fn find_by_content_hash(&self, hash: &[u8]) -> Result<Vec<MediaHash>>;

    /// 特定位置のハッシュを削除（代替行の連鎖削除を含む）
    async fn delete_media_hash(
        &self,
        item_uuid: &str,
        filename: &str,
        time_range: &str,
    ) -> Result<()>;

    /// 特定作品のハッシュを全削除
    async fn delete_media_hashes(&self, item_uuid: &str) -> Result<()>;

    /// 重複ハッシュを検出
    async fn find_duplicate_hashes(&self) -> Result<Vec<(Vec<u8>, i64)>>;
}
