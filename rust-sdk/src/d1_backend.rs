//! D1 バックエンドの高レベル実装（`impl KijukuBackend`）
//!
//! `D1Exec`（`SqlExec` 実装）を持ち、組み立て層の `_async` 関数を経由して `KijukuBackend`
//! を実装する。Local（`KijukuDB`）と組み立て層を完全共有するため、クエリ再実装なし。
//! `record_operation`（バックアップ記録）は Local 専用のため持たない。

use crate::backend::KijukuBackend;
use crate::d1_client::{D1Client, D1Config};
use crate::error::Result;
use crate::exec_d1::D1Exec;
use crate::migration::TableColumnInfo;
use crate::types::{
    AttributeValueType, BulkUpdateItem, Media, MediaAttribute, MediaFilter, MediaHash,
    MediaHashInput, MediaInput, MediaUpdateInput, QueryOptions, Tag, TagUsageStats,
};
use async_trait::async_trait;

/// D1（REST）を対象とした KijukuDB 相当のバックエンド
pub struct D1KijukuDB {
    exec: D1Exec,
}

impl D1KijukuDB {
    /// wrangler のキャッシュから OAuth トークンを読んで構築（`wrangler login` 済み前提）
    pub fn from_wrangler(config: D1Config) -> Result<Self> {
        let client = D1Client::from_wrangler(config)?;
        Ok(Self {
            exec: D1Exec::new(client),
        })
    }

    /// 明示的なトークンで構築（API token 使用時やテスト用）
    pub fn with_token(config: D1Config, token: String) -> Self {
        let client = D1Client::with_token(config, token);
        Self {
            exec: D1Exec::new(client),
        }
    }

    /// 生 SQL を実行する（統合テスト・メンテナンス用）。
    ///
    /// バインドパラメータを持たない SQL をそのまま D1 へ送る。`;` 区切りの複数文は
    /// D1 REST 側で分割実行される（信頼性の観点から単文ずつ呼ぶことを推奨）。
    /// 本番のデータ操作は `KijukuBackend` の型安全なメソッドを使うべきで、本メソッドは
    /// テスト用インスタンスのクリーンアップなど、整備用途に限定する。
    #[cfg(feature = "d1-integration")]
    pub async fn execute_raw(&self, sql: &str) -> Result<()> {
        crate::exec::SqlExec::execute_raw(&self.exec, sql).await
    }
}

#[async_trait]
impl KijukuBackend for D1KijukuDB {
    async fn migrate(&self) -> Result<()> {
        // D1 REST は BLOB 型カラムにバイナリを格納できないため、content_hash/embedding
        // を TEXT(hex) で持つ schema.d1.sql を使用する。
        crate::migration::migrate_async_d1(&self.exec).await
    }

    async fn get_schema_version(&self) -> Result<i64> {
        crate::migration::get_schema_version_async(&self.exec).await
    }

    async fn get_tables(&self) -> Result<Vec<String>> {
        crate::migration::get_tables_async(&self.exec).await
    }

    async fn get_table_info(&self, table_name: &str) -> Result<Vec<TableColumnInfo>> {
        crate::migration::get_table_info_async(&self.exec, table_name).await
    }

    async fn create_media(&self, input: &MediaInput) -> Result<Media> {
        crate::crud::create_media_async(&self.exec, input).await
    }

    async fn get_media(&self, id: i64) -> Result<Option<Media>> {
        crate::crud::get_media_async(&self.exec, id).await
    }

    async fn update_media(&self, id: i64, input: &MediaUpdateInput) -> Result<()> {
        crate::crud::update_media_async(&self.exec, id, input).await
    }

    async fn delete_media(&self, id: i64) -> Result<()> {
        crate::crud::delete_media_async(&self.exec, id).await
    }

    async fn find_media(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
    ) -> Result<Vec<Media>> {
        crate::search::find_media_async(&self.exec, filter, options).await
    }

    async fn get_distinct_values(
        &self,
        fields: &[&str],
        filter: &MediaFilter,
    ) -> Result<Vec<Vec<Option<String>>>> {
        crate::search::get_distinct_values_async(&self.exec, fields, filter).await
    }

    async fn bulk_create_media(&self, data_list: &[MediaInput]) -> Result<Vec<Media>> {
        crate::bulk::bulk_create_media_async(&self.exec, data_list).await
    }

    async fn bulk_delete_media(&self, ids: &[i64]) -> Result<()> {
        crate::bulk::bulk_delete_media_async(&self.exec, ids).await
    }

    async fn bulk_update_media(&self, updates: &[BulkUpdateItem]) -> Result<()> {
        crate::bulk::bulk_update_media_async(&self.exec, updates).await
    }

    async fn create_tag(&self, name: &str) -> Result<Tag> {
        crate::tag::create_tag_async(&self.exec, name).await
    }

    async fn get_tag_by_name(&self, name: &str) -> Result<Option<Tag>> {
        crate::tag::get_tag_by_name_async(&self.exec, name).await
    }

    async fn get_all_tags(&self) -> Result<Vec<Tag>> {
        crate::tag::get_all_tags_async(&self.exec).await
    }

    async fn add_tag_to_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        crate::tag::add_tag_to_media_async(&self.exec, media_id, tag_id).await
    }

    async fn remove_tag_from_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        crate::tag::remove_tag_from_media_async(&self.exec, media_id, tag_id).await
    }

    async fn get_media_tags(&self, media_id: i64) -> Result<Vec<Tag>> {
        crate::tag::get_media_tags_async(&self.exec, media_id).await
    }

    async fn get_tag_usage_stats(&self) -> Result<Vec<TagUsageStats>> {
        crate::tag::get_tag_usage_stats_async(&self.exec).await
    }

    async fn find_unused_tags(&self) -> Result<Vec<Tag>> {
        crate::tag::find_unused_tags_async(&self.exec).await
    }

    async fn set_media_attribute(
        &self,
        media_id: i64,
        key: &str,
        value: Option<&str>,
        value_type: Option<AttributeValueType>,
    ) -> Result<()> {
        crate::attribute::set_media_attribute_async(&self.exec, media_id, key, value, value_type)
            .await
    }

    async fn get_media_attribute(&self, media_id: i64, key: &str) -> Result<Option<MediaAttribute>> {
        crate::attribute::get_media_attribute_async(&self.exec, media_id, key).await
    }

    async fn get_media_attributes(&self, media_id: i64) -> Result<Vec<MediaAttribute>> {
        crate::attribute::get_media_attributes_async(&self.exec, media_id).await
    }

    async fn delete_media_attribute(&self, media_id: i64, key: &str) -> Result<()> {
        crate::attribute::delete_media_attribute_async(&self.exec, media_id, key).await
    }

    async fn delete_all_media_attributes(&self, media_id: i64) -> Result<()> {
        crate::attribute::delete_all_media_attributes_async(&self.exec, media_id).await
    }

    async fn add_media_hash(&self, input: &MediaHashInput) -> Result<MediaHash> {
        crate::hash::add_media_hash_async(&self.exec, input).await
    }

    async fn add_media_hashes(&self, inputs: &[MediaHashInput]) -> Result<Vec<MediaHash>> {
        crate::hash::add_media_hashes_async(&self.exec, inputs).await
    }

    async fn get_media_hashes(&self, item_uuid: &str) -> Result<Vec<MediaHash>> {
        crate::hash::get_media_hashes_async(&self.exec, item_uuid).await
    }

    async fn get_media_hash(
        &self,
        item_uuid: &str,
        filename: &str,
        time_range: &str,
    ) -> Result<Option<MediaHash>> {
        crate::hash::get_media_hash_async(&self.exec, item_uuid, filename, time_range).await
    }

    async fn find_by_content_hash(&self, hash_bytes: &[u8]) -> Result<Vec<MediaHash>> {
        crate::hash::find_by_content_hash_async(&self.exec, hash_bytes).await
    }

    async fn delete_media_hash(
        &self,
        item_uuid: &str,
        filename: &str,
        time_range: &str,
    ) -> Result<()> {
        crate::hash::delete_media_hash_async(&self.exec, item_uuid, filename, time_range).await
    }

    async fn delete_media_hashes(&self, item_uuid: &str) -> Result<()> {
        crate::hash::delete_media_hashes_async(&self.exec, item_uuid).await
    }

    async fn find_duplicate_hashes(&self) -> Result<Vec<(Vec<u8>, i64)>> {
        crate::hash::find_duplicate_hashes_async(&self.exec).await
    }
}
