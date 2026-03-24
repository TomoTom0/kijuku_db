//! きじゅくDB - Rust SDK
//!
//! メディア管理のためのSQLiteベースのデータベースSDK
//!
//! # 機能
//!
//! - メディア情報のCRUD操作
//! - タグ管理
//! - 全文検索
//! - 一括操作
//! - トランザクション管理
//! - スキーマ情報取得
//!
//! # 使用例
//!
//! ```no_run
//! use kijuku_db::{KijukuDB, MediaInput, MediaType};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // データベースを開く
//! let db = KijukuDB::open("my_media.db")?;
//!
//! // 新しいメディアを作成準備
//! let input = MediaInput {
//!     title: "サンプルコミック".to_string(),
//!     media_type: MediaType::Comic,
//!     artist: Some("サンプル作者".to_string()),
//!     ..Default::default()
//! };
//!
//! // CRUD操作は今後実装されます
//! # Ok(())
//! # }
//! ```

pub mod attribute;
pub mod backup;
pub mod bulk;
pub mod config;
pub mod crud;
pub mod error;
pub mod migration;
pub mod remote;
pub mod search;
pub mod server;
pub mod tag;
pub mod types;
pub mod update_exist;

pub use backup::{
    AutoRecord, AutoRecordStatus, BackupInfo, BackupKind, BackupManager, BackupOptions,
    BackupScope, BackupSelector, RetentionPolicy, RetentionTier,
};
pub use config::{load_config, BackupConfig, KijukuConfig};
pub use error::{KijukuError, Result};
pub use migration::TableColumnInfo;
pub use remote::{RemoteConfig, RemoteKijukuDB};
pub use server::auth::{AuthManager, generate_password};
pub use server::{ServerOptions, start_server};
pub use types::*;
pub use update_exist::{UpdateExistItemResult, UpdateExistOptions, UpdateExistResult};

use rusqlite::Connection;
use std::path::Path;

/// きじゅくDBのメインクラス
pub struct KijukuDB {
    conn: Connection,
    options: DBOptions,
    backup_manager: Option<BackupManager>,
}

impl KijukuDB {
    /// データベースを開く
    ///
    /// # 引数
    ///
    /// * `path` - データベースファイルのパス
    ///
    /// # 戻り値
    ///
    /// データベース接続を返す
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(Self {
            conn,
            options: DBOptions::default(),
            backup_manager: None,
        })
    }

    /// オプション付きでデータベースを開く
    ///
    /// # 引数
    ///
    /// * `path` - データベースファイルのパス
    /// * `options` - データベース接続オプション
    pub fn open_with_options<P: AsRef<Path>>(path: P, options: DBOptions) -> Result<Self> {
        let path_ref = path.as_ref();
        let conn = if options.readonly {
            Connection::open_with_flags(
                path_ref,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?
        } else {
            Connection::open(path_ref)?
        };
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        // バックアップマネージャーの初期化
        let backup_manager = if let Some(backup_opts) = options.backup.clone() {
            Some(BackupManager::new(path_ref, backup_opts)?)
        } else {
            None
        };

        Ok(Self { conn, options, backup_manager })
    }

    /// インメモリデータベースを作成
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(Self {
            conn,
            options: DBOptions::default(),
            backup_manager: None,
        })
    }

    /// データベース接続への参照を取得
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// データベースオプションへの参照を取得
    pub fn options(&self) -> &DBOptions {
        &self.options
    }

    /// マイグレーションを実行
    pub fn migrate(&self) -> Result<()> {
        migration::migrate(&self.conn)
    }

    /// スキーマバージョンを取得
    pub fn get_schema_version(&self) -> Result<i64> {
        migration::get_schema_version(&self.conn)
    }

    /// テーブル一覧を取得
    pub fn get_tables(&self) -> Result<Vec<String>> {
        migration::get_tables(&self.conn)
    }

    /// 外部キー制約が有効かチェック
    pub fn is_foreign_keys_enabled(&self) -> Result<bool> {
        migration::is_foreign_keys_enabled(&self.conn)
    }

    /// 特定テーブルのカラム情報を取得
    pub fn get_table_info(&self, table_name: &str) -> Result<Vec<TableColumnInfo>> {
        migration::get_table_info(&self.conn, table_name)
    }

    /// メディアを作成
    pub fn create_media(&self, input: &MediaInput) -> Result<Media> {
        crud::create_media(&self.conn, input)
    }

    /// IDでメディアを取得
    pub fn get_media(&self, id: i64) -> Option<Media> {
        crud::get_media(&self.conn, id)
    }

    /// メディアを更新（部分更新）
    ///
    /// 指定されたフィールドのみ更新します。
    pub fn update_media(&self, id: i64, input: &MediaUpdateInput) -> Result<()> {
        crud::update_media(&self.conn, id, input)
    }

    /// メディアを削除
    pub fn delete_media(&self, id: i64) -> Result<()> {
        crud::delete_media(&self.conn, id)
    }

    /// メディアを検索
    pub fn find_media(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
    ) -> Result<Vec<Media>> {
        search::find_media(&self.conn, filter, options)
    }

    /// フィルタで絞り込んだメディアのflag_existをファイル存在状態に基づいて更新する
    pub fn update_exist(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        update_options: &UpdateExistOptions,
    ) -> Result<UpdateExistResult> {
        update_exist::update_exist(&self.conn, filter, options, update_options)
    }

    /// 複数のメディアを一括作成
    pub fn bulk_create_media(&self, data_list: &[MediaInput]) -> Result<Vec<Media>> {
        bulk::bulk_create_media(&self.conn, data_list)
    }

    /// 複数のメディアを一括削除
    pub fn bulk_delete_media(&self, ids: &[i64]) -> Result<()> {
        bulk::bulk_delete_media(&self.conn, ids)
    }

    /// 複数のメディアを一括更新
    pub fn bulk_update_media(&self, updates: &[BulkUpdateItem]) -> Result<()> {
        bulk::bulk_update_media(&self.conn, updates)
    }

    /// タグを作成
    pub fn create_tag(&self, name: &str) -> Result<Tag> {
        tag::create_tag(&self.conn, name)
    }

    /// タグ名でタグを取得
    pub fn get_tag_by_name(&self, name: &str) -> Option<Tag> {
        tag::get_tag_by_name(&self.conn, name)
    }

    /// 全てのタグを取得
    pub fn get_all_tags(&self) -> Result<Vec<Tag>> {
        tag::get_all_tags(&self.conn)
    }

    /// メディアにタグを追加
    pub fn add_tag_to_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        tag::add_tag_to_media(&self.conn, media_id, tag_id)
    }

    /// メディアからタグを削除
    pub fn remove_tag_from_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        tag::remove_tag_from_media(&self.conn, media_id, tag_id)
    }

    /// メディアに関連付けられたタグを取得
    pub fn get_media_tags(&self, media_id: i64) -> Result<Vec<Tag>> {
        tag::get_media_tags(&self.conn, media_id)
    }

    /// タグの使用数統計を取得
    pub fn get_tag_usage_stats(&self) -> Result<Vec<TagUsageStats>> {
        tag::get_tag_usage_stats(&self.conn)
    }

    /// 未使用のタグを取得
    pub fn find_unused_tags(&self) -> Result<Vec<Tag>> {
        tag::find_unused_tags(&self.conn)
    }

    /// メディアに属性を設定
    pub fn set_media_attribute(
        &self,
        media_id: i64,
        key: &str,
        value: Option<&str>,
        value_type: Option<AttributeValueType>,
    ) -> Result<()> {
        attribute::set_media_attribute(&self.conn, media_id, key, value, value_type)
    }

    /// メディアの属性を取得
    pub fn get_media_attribute(&self, media_id: i64, key: &str) -> Result<Option<MediaAttribute>> {
        attribute::get_media_attribute(&self.conn, media_id, key)
    }

    /// メディアの全ての属性を取得
    pub fn get_media_attributes(&self, media_id: i64) -> Result<Vec<MediaAttribute>> {
        attribute::get_media_attributes(&self.conn, media_id)
    }

    /// メディアの属性を削除
    pub fn delete_media_attribute(&self, media_id: i64, key: &str) -> Result<()> {
        attribute::delete_media_attribute(&self.conn, media_id, key)
    }

    /// メディアの全ての属性を削除
    pub fn delete_all_media_attributes(&self, media_id: i64) -> Result<()> {
        attribute::delete_all_media_attributes(&self.conn, media_id)
    }

    /// トランザクション内で複数の操作を実行
    ///
    /// # 引数
    ///
    /// * `f` - トランザクション内で実行するクロージャ
    ///
    /// # 戻り値
    ///
    /// クロージャの実行結果。クロージャがエラーを返した場合、トランザクションはロールバックされます。
    ///
    /// # 例
    ///
    /// ```no_run
    /// # use kijuku_db::{KijukuDB, MediaInput, MediaType};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = KijukuDB::open("test.db")?;
    /// db.transaction(|db| {
    ///     let input = MediaInput {
    ///         title: "メディア1".to_string(),
    ///         media_type: MediaType::Comic,
    ///         ..Default::default()
    ///     };
    ///     db.create_media(&input)?;
    ///
    ///     let input2 = MediaInput {
    ///         title: "メディア2".to_string(),
    ///         media_type: MediaType::Comic,
    ///         ..Default::default()
    ///     };
    ///     db.create_media(&input2)?;
    ///
    ///     Ok(())
    /// })?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Self) -> Result<T>,
    {
        self.conn.execute("BEGIN TRANSACTION", [])?;

        match f(self) {
            Ok(result) => {
                self.conn.execute("COMMIT", [])?;

                // バックアップマネージャーがあれば操作を記録
                if let Some(manager) = &self.backup_manager {
                    // エラーは記録するが、トランザクションの成功は妨げない
                    if let Err(e) = manager.record_operation() {
                        eprintln!("Backup operation failed: {}", e);
                    }
                }

                Ok(result)
            }
            Err(e) => {
                let _ = self.conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    /// 手動でバックアップを実行
    ///
    /// バックアップマネージャーが設定されている場合、バックアップを作成します。
    ///
    /// # 戻り値
    ///
    /// バックアップファイルのパス。バックアップマネージャーが設定されていない場合は None。
    ///
    /// # 例
    ///
    /// ```no_run
    /// # use kijuku_db::{KijukuDB, BackupOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = KijukuDB::open("test.db")?;
    ///
    /// // バックアップを実行
    /// if let Some(backup_path) = db.backup()? {
    ///     println!("Backup created: {}", backup_path);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    /// 手動バックアップを実行（ラベルなし）
    pub fn backup(&self) -> Result<Option<String>> {
        match &self.backup_manager {
            Some(manager) => manager.backup(None).map(Some),
            None => Ok(None),
        }
    }

    /// ラベル付き手動バックアップを実行
    ///
    /// # 引数
    /// * `label` - バックアップのラベル（例: "before_import"）
    pub fn backup_with_label(&self, label: &str) -> Result<Option<String>> {
        match &self.backup_manager {
            Some(manager) => manager.backup(Some(label)).map(Some),
            None => Ok(None),
        }
    }

    /// バックアップを現在のDBに復元する
    ///
    /// 指定したバックアップの内容を現在のDB接続に上書きします。
    ///
    /// # 引数
    /// * `selector` - 復元するバックアップの選択条件
    ///
    /// # 戻り値
    /// 復元に使用したバックアップファイルのパス
    pub fn restore(&mut self, selector: &BackupSelector) -> Result<std::path::PathBuf> {
        let manager = self
            .backup_manager
            .as_ref()
            .ok_or_else(|| KijukuError::Other("Backup manager not configured".to_string()))?;
        manager.restore(&mut self.conn, selector)
    }

    /// バックアップ一覧を取得
    ///
    /// バックアップマネージャーが設定されている場合、バックアップファイルの一覧を返します。
    ///
    /// # 戻り値
    ///
    /// バックアップ情報のリスト。バックアップマネージャーが設定されていない場合は空のリスト。
    ///
    /// # 例
    ///
    /// ```no_run
    /// # use kijuku_db::{KijukuDB, BackupOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = KijukuDB::open("test.db")?;
    ///
    /// // バックアップ一覧を取得
    /// let backups = db.list_backups()?;
    /// for backup in backups {
    ///     println!("Backup: {} at {:?}", backup.name, backup.created_at);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        match &self.backup_manager {
            Some(manager) => manager.list_backups(),
            None => Ok(Vec::new()),
        }
    }

    /// データベース接続を明示的に閉じる
    ///
    /// Rustでは通常、Dropトレイトによって自動的にクローズされますが、
    /// TypeScript SDKとのAPI互換性のため、明示的なクローズメソッドを提供します。
    ///
    /// # 注意
    ///
    /// このメソッドは所有権を取るため、呼び出し後はこのインスタンスを使用できません。
    ///
    /// # 例
    ///
    /// ```no_run
    /// # use kijuku_db::KijukuDB;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = KijukuDB::open("test.db")?;
    ///
    /// // データベースを使用
    /// // ...
    ///
    /// // 明示的にクローズ
    /// db.close();
    ///
    /// // この後、db は使用できない
    /// # Ok(())
    /// # }
    /// ```
    pub fn close(self) {
        // 所有権を取り、スコープを抜けることでDropトレイトが自動的に呼ばれる
        // 明示的な処理は不要
    }

    /// バックアップマネージャーを取得
    pub fn get_backup_manager(&self) -> Option<&BackupManager> {
        self.backup_manager.as_ref()
    }

    // ========== バックアップからの取得メソッド ==========

    /// バックアップDBに対してコールバックを実行するヘルパーメソッド
    ///
    /// バックアップファイルを読み取り専用で開き、コールバックを実行します。
    fn with_backup_db<F, T>(&self, selector: &BackupSelector, f: F) -> Result<T>
    where
        F: FnOnce(&KijukuDB) -> Result<T>,
    {
        let backup_path = self
            .backup_manager
            .as_ref()
            .ok_or_else(|| KijukuError::Other("Backup manager not configured".to_string()))?
            .get_backup_path(selector)?
            .ok_or_else(|| KijukuError::Other("No backup found matching selector".to_string()))?;

        // バックアップファイルは読み取り専用で開く
        let backup_db = KijukuDB::open_with_options(
            &backup_path,
            DBOptions { readonly: true, ..Default::default() },
        )?;
        f(&backup_db)
    }

    /// バックアップからIDでメディアを取得
    pub fn get_media_from_backup(
        &self,
        id: i64,
        selector: &BackupSelector,
    ) -> Result<Option<Media>> {
        self.with_backup_db(selector, |backup_db| Ok(backup_db.get_media(id)))
    }

    /// バックアップからメディアを検索
    pub fn find_media_from_backup(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        selector: &BackupSelector,
    ) -> Result<Vec<Media>> {
        self.with_backup_db(selector, |backup_db| backup_db.find_media(filter, options))
    }

    /// バックアップからタグ名でタグを取得
    pub fn get_tag_by_name_from_backup(
        &self,
        name: &str,
        selector: &BackupSelector,
    ) -> Result<Option<Tag>> {
        self.with_backup_db(selector, |backup_db| Ok(backup_db.get_tag_by_name(name)))
    }

    /// バックアップから全てのタグを取得
    pub fn get_all_tags_from_backup(&self, selector: &BackupSelector) -> Result<Vec<Tag>> {
        self.with_backup_db(selector, |backup_db| backup_db.get_all_tags())
    }

    /// バックアップからメディアに関連付けられたタグを取得
    pub fn get_media_tags_from_backup(
        &self,
        media_id: i64,
        selector: &BackupSelector,
    ) -> Result<Vec<Tag>> {
        self.with_backup_db(selector, |backup_db| backup_db.get_media_tags(media_id))
    }

    /// バックアップからメディアの属性を取得
    pub fn get_media_attribute_from_backup(
        &self,
        media_id: i64,
        key: &str,
        selector: &BackupSelector,
    ) -> Result<Option<MediaAttribute>> {
        self.with_backup_db(selector, |backup_db| backup_db.get_media_attribute(media_id, key))
    }

    /// バックアップからメディアの全ての属性を取得
    pub fn get_media_attributes_from_backup(
        &self,
        media_id: i64,
        selector: &BackupSelector,
    ) -> Result<Vec<MediaAttribute>> {
        self.with_backup_db(selector, |backup_db| backup_db.get_media_attributes(media_id))
    }
}

// Defaultトレイトの実装
impl Default for MediaInput {
    fn default() -> Self {
        Self {
            title: String::new(),
            media_type: MediaType::Comic,
            uuid: None,
            title_id: None,
            path: None,
            thumbnail_path: None,
            artist: None,
            artist_id: None,
            description: None,
            file_size: None,
            duration_sec: None,
            page_count: None,
            series: None,
            volume_number: None,
            volume_text: None,
            volume_title: None,
            magazine: None,
            magazine_id: None,
            language: None,
            source: None,
            external_id: None,
            artist_en: None,
            title_en: None,
            chapters: None,
            extension: None,
            flag_exist: None,
            title_pron: None,
            artist_pron: None,
            series_pron: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let db = KijukuDB::open_in_memory();
        assert!(db.is_ok());
    }

    #[test]
    fn test_media_type_conversion() {
        assert_eq!(MediaType::from_str("comic"), Some(MediaType::Comic));
        assert_eq!(MediaType::from_str("video"), Some(MediaType::Video));
        assert_eq!(MediaType::from_str("music"), Some(MediaType::Music));
        assert_eq!(MediaType::from_str("invalid"), None);
    }

    #[test]
    fn test_media_type_as_str() {
        assert_eq!(MediaType::Comic.as_str(), "comic");
        assert_eq!(MediaType::Video.as_str(), "video");
        assert_eq!(MediaType::Music.as_str(), "music");
    }
}
