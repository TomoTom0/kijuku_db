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
pub mod backend;
pub mod backup;
pub mod bulk;
pub mod bulk_load;
pub mod config;
pub mod crud;
pub mod d1_backend;
pub mod d1_client;
pub mod db_value;
pub mod error;
pub mod exec;
pub mod exec_d1;
pub mod exec_local;
pub mod hash;
pub mod migration;
pub mod remote;
pub mod search;
pub mod server;
pub mod tag;
pub mod thumbnail;
pub mod types;
pub mod update_exist;

pub use backup::{
    AutoRecord, AutoRecordStatus, BackupInfo, BackupKind, BackupManager, BackupOptions,
    BackupScope, BackupSelector, RetentionPolicy, RetentionTier,
};
pub use backend::KijukuBackend;
pub use bulk_load::{transfer, verify, TransferOptions, TransferReport};
pub use d1_backend::D1KijukuDB;
pub use d1_client::D1Config;
pub use config::{load_config, BackupConfig, KijukuConfig};
pub use db_value::{SqlParam, SqlRow};
pub use error::{KijukuError, Result};
pub use exec::SqlExec;
pub use exec_local::LocalExec;
pub use migration::TableColumnInfo;
pub use remote::{RemoteConfig, RemoteKijukuDB};
pub use search::ALLOWED_DISTINCT_FIELDS;
pub use server::auth::{AuthManager, generate_password};
pub use server::{ServerOptions, start_server};
pub use types::*;
pub use thumbnail::{
    CheckThumbnailItemResult, CheckThumbnailResult, CheckThumbnailStatus, ThumbnailOptions,
    UpdateThumbnailItemResult, UpdateThumbnailResult, UpdateThumbnailStatus,
    resolve_thumbnail_path,
};
pub use update_exist::{UpdateExistItemResult, UpdateExistOptions, UpdateExistResult};

use parking_lot::ReentrantMutex;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// きじゅくDBのメインクラス（Local バックエンド）
///
/// 接続は `Arc<ReentrantMutex<Connection>>` で持ち、async 操作は共有 `LocalExec` 経由で
/// `spawn_blocking` に乗せる。既存の同期メソッドは deprecated だが後方互換のため維持され、
/// 内部で接続を都度ロックして動作する。`ReentrantMutex` により同一スレッドからの再入が
/// 許可されるため、`transaction` は接続ロックをトランザクション全体で保持しつつ、
/// クロージャ内の同期メソッドを安全に呼び出せる（他スレッドは COMMIT/ROLLBACK までブロック）。
pub struct KijukuDB {
    conn: Arc<ReentrantMutex<Connection>>,
    exec: LocalExec,
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
        let conn = Arc::new(ReentrantMutex::new(conn));
        let exec = LocalExec::new(Arc::clone(&conn));
        Ok(Self {
            conn,
            exec,
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
        let mut conn = if options.readonly {
            Connection::open_with_flags(
                path_ref,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?
        } else {
            Connection::open(path_ref)?
        };
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        if options.verbose {
            conn.trace(Some(|sql: &str| eprintln!("[SQL] {sql}")));
        }

        // バックアップマネージャーの初期化
        let backup_manager = if let Some(backup_opts) = options.backup.clone() {
            Some(BackupManager::new(path_ref, backup_opts)?)
        } else {
            None
        };

        let conn = Arc::new(ReentrantMutex::new(conn));
        let exec = LocalExec::new(Arc::clone(&conn));
        Ok(Self {
            conn,
            exec,
            options,
            backup_manager,
        })
    }

    /// インメモリデータベースを作成
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let conn = Arc::new(ReentrantMutex::new(conn));
        let exec = LocalExec::new(Arc::clone(&conn));
        Ok(Self {
            conn,
            exec,
            options: DBOptions::default(),
            backup_manager: None,
        })
    }

    /// 接続の `Arc` ハンドルを取得（同プロセス内で `LocalExec` 等と共有する用途・内部用）。
    ///
    /// NOTE: `connection()`（`&Connection` 直接取得）は conn の `Arc<ReentrantMutex>` 化に伴い廃止。
    /// 直接 SQL 実行が必要な呼び出し側は公開メソッド経由か async trait（`KijukuBackend`）へ移行。
    pub fn conn_handle(&self) -> Arc<ReentrantMutex<Connection>> {
        Arc::clone(&self.conn)
    }

    /// データベースオプションへの参照を取得
    pub fn options(&self) -> &DBOptions {
        &self.options
    }

    /// マイグレーションを実行
    #[deprecated(note = "async API を使用してください (KijukuBackend::migrate)")]
    pub fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock();
        migration::migrate(&conn)
    }

    /// スキーマバージョンを取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_schema_version)")]
    pub fn get_schema_version(&self) -> Result<i64> {
        let conn = self.conn.lock();
        migration::get_schema_version(&conn)
    }

    /// テーブル一覧を取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_tables)")]
    pub fn get_tables(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        migration::get_tables(&conn)
    }

    /// 外部キー制約が有効かチェック
    pub fn is_foreign_keys_enabled(&self) -> Result<bool> {
        let conn = self.conn.lock();
        migration::is_foreign_keys_enabled(&conn)
    }

    /// 特定テーブルのカラム情報を取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_table_info)")]
    pub fn get_table_info(&self, table_name: &str) -> Result<Vec<TableColumnInfo>> {
        let conn = self.conn.lock();
        migration::get_table_info(&conn, table_name)
    }

    /// メディアを作成
    #[deprecated(note = "async API を使用してください (KijukuBackend::create_media)")]
    pub fn create_media(&self, input: &MediaInput) -> Result<Media> {
        let result = {
            let conn = self.conn.lock();
            crud::create_media(&conn, input)?
        };
        self.record_operation();
        Ok(result)
    }

    /// IDでメディアを取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_media)")]
    pub fn get_media(&self, id: i64) -> Option<Media> {
        let conn = self.conn.lock();
        crud::get_media(&conn, id)
    }

    /// メディアを更新（部分更新）
    ///
    /// 指定されたフィールドのみ更新します。
    #[deprecated(note = "async API を使用してください (KijukuBackend::update_media)")]
    pub fn update_media(&self, id: i64, input: &MediaUpdateInput) -> Result<()> {
        {
            let conn = self.conn.lock();
            crud::update_media(&conn, id, input)?;
        }
        self.record_operation();
        Ok(())
    }

    /// メディアを削除
    #[deprecated(note = "async API を使用してください (KijukuBackend::delete_media)")]
    pub fn delete_media(&self, id: i64) -> Result<()> {
        {
            let conn = self.conn.lock();
            crud::delete_media(&conn, id)?;
        }
        self.record_operation();
        Ok(())
    }

    /// メディアを検索
    #[deprecated(note = "async API を使用してください (KijukuBackend::find_media)")]
    pub fn find_media(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
    ) -> Result<Vec<Media>> {
        let conn = self.conn.lock();
        search::find_media(&conn, filter, options)
    }

    /// 指定したフィールド群の重複なしの値の組み合わせ一覧を取得する
    ///
    /// 戻り値の各要素は `fields` と同じ順序のフィールド値（NULL含む）。
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_distinct_values)")]
    pub fn get_distinct_values(
        &self,
        fields: &[&str],
        filter: &MediaFilter,
    ) -> Result<Vec<Vec<Option<String>>>> {
        let conn = self.conn.lock();
        search::get_distinct_values(&conn, fields, filter)
    }

    /// フィルタで絞り込んだメディアのサムネイル状態をチェックする
    pub fn check_thumbnail(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
    ) -> Result<CheckThumbnailResult> {
        let conn = self.conn.lock();
        thumbnail::check_thumbnail(&conn, filter, options)
    }

    /// フィルタで絞り込んだメディアのサムネイルを生成・更新する
    pub fn update_thumbnail(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        thumbnail_options: &ThumbnailOptions,
    ) -> Result<UpdateThumbnailResult> {
        let result = {
            let conn = self.conn.lock();
            thumbnail::update_thumbnail(&conn, filter, options, thumbnail_options)?
        };
        if result.generated > 0 {
            self.record_operation();
        }
        Ok(result)
    }

    /// フィルタで絞り込んだメディアのflag_existをファイル存在状態に基づいて更新する
    pub fn update_exist(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        update_options: &UpdateExistOptions,
    ) -> Result<UpdateExistResult> {
        let result = {
            let conn = self.conn.lock();
            update_exist::update_exist(&conn, filter, options, update_options)?
        };
        if result.updated > 0 {
            self.record_operation();
        }
        Ok(result)
    }

    /// 複数のメディアを一括作成
    #[deprecated(note = "async API を使用してください (KijukuBackend::bulk_create_media)")]
    pub fn bulk_create_media(&self, data_list: &[MediaInput]) -> Result<Vec<Media>> {
        let result = {
            let conn = self.conn.lock();
            bulk::bulk_create_media(&conn, data_list)?
        };
        self.record_operation();
        Ok(result)
    }

    /// 複数のメディアを一括削除
    #[deprecated(note = "async API を使用してください (KijukuBackend::bulk_delete_media)")]
    pub fn bulk_delete_media(&self, ids: &[i64]) -> Result<()> {
        {
            let conn = self.conn.lock();
            bulk::bulk_delete_media(&conn, ids)?;
        }
        self.record_operation();
        Ok(())
    }

    /// 複数のメディアを一括更新
    #[deprecated(note = "async API を使用してください (KijukuBackend::bulk_update_media)")]
    pub fn bulk_update_media(&self, updates: &[BulkUpdateItem]) -> Result<()> {
        {
            let conn = self.conn.lock();
            bulk::bulk_update_media(&conn, updates)?;
        }
        self.record_operation();
        Ok(())
    }

    /// タグを作成
    #[deprecated(note = "async API を使用してください (KijukuBackend::create_tag)")]
    pub fn create_tag(&self, name: &str) -> Result<Tag> {
        let result = {
            let conn = self.conn.lock();
            tag::create_tag(&conn, name)?
        };
        self.record_operation();
        Ok(result)
    }

    /// タグ名でタグを取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_tag_by_name)")]
    pub fn get_tag_by_name(&self, name: &str) -> Option<Tag> {
        let conn = self.conn.lock();
        tag::get_tag_by_name(&conn, name)
    }

    /// 全てのタグを取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_all_tags)")]
    pub fn get_all_tags(&self) -> Result<Vec<Tag>> {
        let conn = self.conn.lock();
        tag::get_all_tags(&conn)
    }

    /// メディアにタグを追加
    #[deprecated(note = "async API を使用してください (KijukuBackend::add_tag_to_media)")]
    pub fn add_tag_to_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        {
            let conn = self.conn.lock();
            tag::add_tag_to_media(&conn, media_id, tag_id)?;
        }
        self.record_operation();
        Ok(())
    }

    /// メディアからタグを削除
    #[deprecated(note = "async API を使用してください (KijukuBackend::remove_tag_from_media)")]
    pub fn remove_tag_from_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        {
            let conn = self.conn.lock();
            tag::remove_tag_from_media(&conn, media_id, tag_id)?;
        }
        self.record_operation();
        Ok(())
    }

    /// メディアに関連付けられたタグを取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_media_tags)")]
    pub fn get_media_tags(&self, media_id: i64) -> Result<Vec<Tag>> {
        let conn = self.conn.lock();
        tag::get_media_tags(&conn, media_id)
    }

    /// タグの使用数統計を取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_tag_usage_stats)")]
    pub fn get_tag_usage_stats(&self) -> Result<Vec<TagUsageStats>> {
        let conn = self.conn.lock();
        tag::get_tag_usage_stats(&conn)
    }

    /// 未使用のタグを取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::find_unused_tags)")]
    pub fn find_unused_tags(&self) -> Result<Vec<Tag>> {
        let conn = self.conn.lock();
        tag::find_unused_tags(&conn)
    }

    /// メディアに属性を設定
    #[deprecated(note = "async API を使用してください (KijukuBackend::set_media_attribute)")]
    pub fn set_media_attribute(
        &self,
        media_id: i64,
        key: &str,
        value: Option<&str>,
        value_type: Option<AttributeValueType>,
    ) -> Result<()> {
        {
            let conn = self.conn.lock();
            attribute::set_media_attribute(&conn, media_id, key, value, value_type)?;
        }
        self.record_operation();
        Ok(())
    }

    // ========== メディアハッシュ操作 ==========

    /// メディアハッシュを登録（単件）
    #[deprecated(note = "async API を使用してください (KijukuBackend::add_media_hash)")]
    pub fn add_media_hash(&self, input: &MediaHashInput) -> Result<MediaHash> {
        let result = {
            let conn = self.conn.lock();
            hash::add_media_hash(&conn, input)?
        };
        self.record_operation();
        Ok(result)
    }

    /// メディアハッシュを一括登録
    #[deprecated(note = "async API を使用してください (KijukuBackend::add_media_hashes)")]
    pub fn add_media_hashes(&self, inputs: &[MediaHashInput]) -> Result<Vec<MediaHash>> {
        let result = {
            let conn = self.conn.lock();
            hash::add_media_hashes(&conn, inputs)?
        };
        self.record_operation();
        Ok(result)
    }

    /// 特定作品の全ハッシュを取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_media_hashes)")]
    pub fn get_media_hashes(&self, item_uuid: &str) -> Result<Vec<MediaHash>> {
        let conn = self.conn.lock();
        hash::get_media_hashes(&conn, item_uuid)
    }

    /// 特定位置のハッシュを取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_media_hash)")]
    pub fn get_media_hash(&self, item_uuid: &str, filename: &str, time_range: &str) -> Result<Option<MediaHash>> {
        let conn = self.conn.lock();
        hash::get_media_hash(&conn, item_uuid, filename, time_range)
    }

    /// SHA256による完全一致検索
    #[deprecated(note = "async API を使用してください (KijukuBackend::find_by_content_hash)")]
    pub fn find_by_content_hash(&self, hash_bytes: &[u8]) -> Result<Vec<MediaHash>> {
        let conn = self.conn.lock();
        hash::find_by_content_hash(&conn, hash_bytes)
    }

    /// 特定位置のハッシュを削除（代替行の連鎖削除を含む）
    #[deprecated(note = "async API を使用してください (KijukuBackend::delete_media_hash)")]
    pub fn delete_media_hash(&self, item_uuid: &str, filename: &str, time_range: &str) -> Result<()> {
        {
            let conn = self.conn.lock();
            hash::delete_media_hash(&conn, item_uuid, filename, time_range)?;
        }
        self.record_operation();
        Ok(())
    }

    /// 特定作品のハッシュを全削除
    #[deprecated(note = "async API を使用してください (KijukuBackend::delete_media_hashes)")]
    pub fn delete_media_hashes(&self, item_uuid: &str) -> Result<()> {
        {
            let conn = self.conn.lock();
            hash::delete_media_hashes(&conn, item_uuid)?;
        }
        self.record_operation();
        Ok(())
    }

    /// 重複ハッシュの検出
    #[deprecated(note = "async API を使用してください (KijukuBackend::find_duplicate_hashes)")]
    pub fn find_duplicate_hashes(&self) -> Result<Vec<(Vec<u8>, i64)>> {
        let conn = self.conn.lock();
        hash::find_duplicate_hashes(&conn)
    }

    /// 特定のメディアのハッシュを計算・登録
    pub fn compute_media_hash(
        &self,
        item_uuid: &str,
        media_path: &str,
        media_type: &str,
        duration_sec: Option<i32>,
    ) -> Result<hash::ComputeHashResult> {
        let conn = self.conn.lock();
        hash::compute_media_hash(&conn, item_uuid, media_path, media_type, duration_sec)
    }

    /// フィルタ条件でメディアを絞り込み、ハッシュを計算・登録
    pub fn compute_media_hashes(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        force: bool,
    ) -> Result<Vec<hash::ComputeHashResult>> {
        let conn = self.conn.lock();
        hash::compute_media_hashes(&conn, filter, options, force)
    }

    /// メディアの属性を取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_media_attribute)")]
    pub fn get_media_attribute(&self, media_id: i64, key: &str) -> Result<Option<MediaAttribute>> {
        let conn = self.conn.lock();
        attribute::get_media_attribute(&conn, media_id, key)
    }

    /// メディアの全ての属性を取得
    #[deprecated(note = "async API を使用してください (KijukuBackend::get_media_attributes)")]
    pub fn get_media_attributes(&self, media_id: i64) -> Result<Vec<MediaAttribute>> {
        let conn = self.conn.lock();
        attribute::get_media_attributes(&conn, media_id)
    }

    /// メディアの属性を削除
    #[deprecated(note = "async API を使用してください (KijukuBackend::delete_media_attribute)")]
    pub fn delete_media_attribute(&self, media_id: i64, key: &str) -> Result<()> {
        {
            let conn = self.conn.lock();
            attribute::delete_media_attribute(&conn, media_id, key)?;
        }
        self.record_operation();
        Ok(())
    }

    /// メディアの全ての属性を削除
    #[deprecated(note = "async API を使用してください (KijukuBackend::delete_all_media_attributes)")]
    pub fn delete_all_media_attributes(&self, media_id: i64) -> Result<()> {
        {
            let conn = self.conn.lock();
            attribute::delete_all_media_attributes(&conn, media_id)?;
        }
        self.record_operation();
        Ok(())
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
    /// バックアップマネージャーに操作を記録する
    ///
    /// バックアップマネージャーが設定されていない場合は何もしない。
    /// エラーはログ出力のみで、呼び出し元の操作は妨げない。
    fn record_operation(&self) {
        // トランザクション中（autocommit=false）は記録しない
        let autocommit = self.conn.lock().is_autocommit();
        if !autocommit {
            return;
        }
        if let Some(manager) = &self.backup_manager {
            if let Err(e) = manager.record_operation() {
                eprintln!("Backup operation failed: {}", e);
            }
        }
    }

    /// 複数の操作を単一のトランザクションで実行する（同期 API）。
    ///
    /// # 並行安全性
    ///
    /// 内部接続は `Arc<ReentrantMutex<Connection>>` で共有されます。本メソッドは
    /// トランザクション実行中（BEGIN〜COMMIT/ROLLBACK）接続ロックを保持し続けます。
    /// `ReentrantMutex` により同一スレッドからの再入のみ許可されるため、クロージャ内の
    /// 同期メソッド（`create_media` 等）は安全に呼び出せます。他スレッドからのクエリは
    /// COMMIT/ROLLBACK までブロックされ、トランザクション中に別スレッドのクエリが
    /// 同一トランザクションやロールバックに巻き込まれる競合状態は発生しません。
    pub fn transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Self) -> Result<T>,
    {
        // conn をトランザクション全体でロック保持。ReentrantMutex により同一スレッドの
        // 再入（クロージャ内の同期メソッド）が許可され、他スレッドは COMMIT/ROLLBACK
        // までブロックされる。これによりトランザクション中の並行競合を完全に防ぐ。
        //
        // unchecked_transaction を使用することで、クロージャ内でパニックが発生して
        // アンワインドが起きても、Transaction の Drop により自動的に ROLLBACK される。
        // 手動 SQL の BEGIN/COMMIT/ROLLBACK ではパニック経路を捕捉できずトランザクション
        // が開いたまま残留するリスクがあった。
        let conn = self.conn.lock();
        let tx = conn.unchecked_transaction()?;
        match f(self) {
            Ok(result) => {
                tx.commit()?;
                drop(conn);
                self.record_operation();
                Ok(result)
            }
            Err(e) => {
                // tx がスコープを抜ける際（conn より先に）Drop して自動 ROLLBACK される。
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
        // restore は &mut Connection を要求するが、ReentrantMutex の guard は &mut を提供しない。
        // self.exec（同じ Arc を共有）をダミーに差し替えて self.conn の Arc 参照カウントを 1 にし、
        // Arc::get_mut + ReentrantMutex::get_mut で &mut Connection を得る。
        // ※ conn_handle 等で外部に Arc が漏れている場合は get_mut が失敗しエラー。
        let dummy = Arc::new(ReentrantMutex::new(Connection::open_in_memory()?));
        self.exec = LocalExec::new(dummy);
        let restore_result = match Arc::get_mut(&mut self.conn) {
            Some(re_mutex) => manager.restore(ReentrantMutex::get_mut(re_mutex), selector),
            None => Err(KijukuError::Other(
                "cannot restore: connection is shared with other owners".to_string(),
            )),
        };
        // 成功/失敗問わず self.exec を self.conn と再共有して一貫状態に戻す
        self.exec = LocalExec::new(Arc::clone(&self.conn));
        restore_result
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
    #[allow(deprecated)]
    pub fn get_media_from_backup(
        &self,
        id: i64,
        selector: &BackupSelector,
    ) -> Result<Option<Media>> {
        self.with_backup_db(selector, |backup_db| Ok(backup_db.get_media(id)))
    }

    /// バックアップからメディアを検索
    #[allow(deprecated)]
    pub fn find_media_from_backup(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        selector: &BackupSelector,
    ) -> Result<Vec<Media>> {
        self.with_backup_db(selector, |backup_db| backup_db.find_media(filter, options))
    }

    /// バックアップからタグ名でタグを取得
    #[allow(deprecated)]
    pub fn get_tag_by_name_from_backup(
        &self,
        name: &str,
        selector: &BackupSelector,
    ) -> Result<Option<Tag>> {
        self.with_backup_db(selector, |backup_db| Ok(backup_db.get_tag_by_name(name)))
    }

    /// バックアップから全てのタグを取得
    #[allow(deprecated)]
    pub fn get_all_tags_from_backup(&self, selector: &BackupSelector) -> Result<Vec<Tag>> {
        self.with_backup_db(selector, |backup_db| backup_db.get_all_tags())
    }

    /// バックアップからメディアに関連付けられたタグを取得
    #[allow(deprecated)]
    pub fn get_media_tags_from_backup(
        &self,
        media_id: i64,
        selector: &BackupSelector,
    ) -> Result<Vec<Tag>> {
        self.with_backup_db(selector, |backup_db| backup_db.get_media_tags(media_id))
    }

    /// バックアップからメディアの属性を取得
    #[allow(deprecated)]
    pub fn get_media_attribute_from_backup(
        &self,
        media_id: i64,
        key: &str,
        selector: &BackupSelector,
    ) -> Result<Option<MediaAttribute>> {
        self.with_backup_db(selector, |backup_db| backup_db.get_media_attribute(media_id, key))
    }

    /// バックアップからメディアの全ての属性を取得
    #[allow(deprecated)]
    pub fn get_media_attributes_from_backup(
        &self,
        media_id: i64,
        selector: &BackupSelector,
    ) -> Result<Vec<MediaAttribute>> {
        self.with_backup_db(selector, |backup_db| backup_db.get_media_attributes(media_id))
    }
}

#[async_trait::async_trait]
impl KijukuBackend for KijukuDB {
    async fn migrate(&self) -> Result<()> {
        migration::migrate_async(&self.exec).await
    }

    async fn get_schema_version(&self) -> Result<i64> {
        migration::get_schema_version_async(&self.exec).await
    }

    async fn get_tables(&self) -> Result<Vec<String>> {
        migration::get_tables_async(&self.exec).await
    }

    async fn get_table_info(&self, table_name: &str) -> Result<Vec<TableColumnInfo>> {
        migration::get_table_info_async(&self.exec, table_name).await
    }

    async fn create_media(&self, input: &MediaInput) -> Result<Media> {
        let media = crud::create_media_async(&self.exec, input).await?;
        self.record_operation();
        Ok(media)
    }

    async fn get_media(&self, id: i64) -> Result<Option<Media>> {
        crud::get_media_async(&self.exec, id).await
    }

    async fn update_media(&self, id: i64, input: &MediaUpdateInput) -> Result<()> {
        crud::update_media_async(&self.exec, id, input).await?;
        self.record_operation();
        Ok(())
    }

    async fn delete_media(&self, id: i64) -> Result<()> {
        crud::delete_media_async(&self.exec, id).await?;
        self.record_operation();
        Ok(())
    }

    async fn find_media(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
    ) -> Result<Vec<Media>> {
        search::find_media_async(&self.exec, filter, options).await
    }

    async fn get_distinct_values(
        &self,
        fields: &[&str],
        filter: &MediaFilter,
    ) -> Result<Vec<Vec<Option<String>>>> {
        search::get_distinct_values_async(&self.exec, fields, filter).await
    }

    async fn bulk_create_media(&self, data_list: &[MediaInput]) -> Result<Vec<Media>> {
        let result = bulk::bulk_create_media_async(&self.exec, data_list).await?;
        self.record_operation();
        Ok(result)
    }

    async fn bulk_delete_media(&self, ids: &[i64]) -> Result<()> {
        bulk::bulk_delete_media_async(&self.exec, ids).await?;
        self.record_operation();
        Ok(())
    }

    async fn bulk_update_media(&self, updates: &[BulkUpdateItem]) -> Result<()> {
        bulk::bulk_update_media_async(&self.exec, updates).await?;
        self.record_operation();
        Ok(())
    }

    async fn create_tag(&self, name: &str) -> Result<Tag> {
        let tag = tag::create_tag_async(&self.exec, name).await?;
        self.record_operation();
        Ok(tag)
    }

    async fn get_tag_by_name(&self, name: &str) -> Result<Option<Tag>> {
        tag::get_tag_by_name_async(&self.exec, name).await
    }

    async fn get_all_tags(&self) -> Result<Vec<Tag>> {
        tag::get_all_tags_async(&self.exec).await
    }

    async fn add_tag_to_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        tag::add_tag_to_media_async(&self.exec, media_id, tag_id).await?;
        self.record_operation();
        Ok(())
    }

    async fn remove_tag_from_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        tag::remove_tag_from_media_async(&self.exec, media_id, tag_id).await?;
        self.record_operation();
        Ok(())
    }

    async fn get_media_tags(&self, media_id: i64) -> Result<Vec<Tag>> {
        tag::get_media_tags_async(&self.exec, media_id).await
    }

    async fn get_media_tags_bulk(
        &self,
        media_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<Tag>>> {
        tag::get_media_tags_bulk_async(&self.exec, media_ids).await
    }

    async fn get_tag_usage_stats(&self) -> Result<Vec<TagUsageStats>> {
        tag::get_tag_usage_stats_async(&self.exec).await
    }

    async fn find_unused_tags(&self) -> Result<Vec<Tag>> {
        tag::find_unused_tags_async(&self.exec).await
    }

    async fn set_media_attribute(
        &self,
        media_id: i64,
        key: &str,
        value: Option<&str>,
        value_type: Option<AttributeValueType>,
    ) -> Result<()> {
        attribute::set_media_attribute_async(&self.exec, media_id, key, value, value_type).await?;
        self.record_operation();
        Ok(())
    }

    async fn get_media_attribute(&self, media_id: i64, key: &str) -> Result<Option<MediaAttribute>> {
        attribute::get_media_attribute_async(&self.exec, media_id, key).await
    }

    async fn get_media_attributes(&self, media_id: i64) -> Result<Vec<MediaAttribute>> {
        attribute::get_media_attributes_async(&self.exec, media_id).await
    }

    async fn delete_media_attribute(&self, media_id: i64, key: &str) -> Result<()> {
        attribute::delete_media_attribute_async(&self.exec, media_id, key).await?;
        self.record_operation();
        Ok(())
    }

    async fn delete_all_media_attributes(&self, media_id: i64) -> Result<()> {
        attribute::delete_all_media_attributes_async(&self.exec, media_id).await?;
        self.record_operation();
        Ok(())
    }

    async fn add_media_hash(&self, input: &MediaHashInput) -> Result<MediaHash> {
        let hash = hash::add_media_hash_async(&self.exec, input).await?;
        self.record_operation();
        Ok(hash)
    }

    async fn add_media_hashes(&self, inputs: &[MediaHashInput]) -> Result<Vec<MediaHash>> {
        let hashes = hash::add_media_hashes_async(&self.exec, inputs).await?;
        self.record_operation();
        Ok(hashes)
    }

    async fn get_media_hashes(&self, item_uuid: &str) -> Result<Vec<MediaHash>> {
        hash::get_media_hashes_async(&self.exec, item_uuid).await
    }

    async fn get_media_hash(
        &self,
        item_uuid: &str,
        filename: &str,
        time_range: &str,
    ) -> Result<Option<MediaHash>> {
        hash::get_media_hash_async(&self.exec, item_uuid, filename, time_range).await
    }

    async fn find_by_content_hash(&self, hash_bytes: &[u8]) -> Result<Vec<MediaHash>> {
        hash::find_by_content_hash_async(&self.exec, hash_bytes).await
    }

    async fn delete_media_hash(
        &self,
        item_uuid: &str,
        filename: &str,
        time_range: &str,
    ) -> Result<()> {
        hash::delete_media_hash_async(&self.exec, item_uuid, filename, time_range).await?;
        self.record_operation();
        Ok(())
    }

    async fn delete_media_hashes(&self, item_uuid: &str) -> Result<()> {
        hash::delete_media_hashes_async(&self.exec, item_uuid).await?;
        self.record_operation();
        Ok(())
    }

    async fn find_duplicate_hashes(&self) -> Result<Vec<(Vec<u8>, i64)>> {
        hash::find_duplicate_hashes_async(&self.exec).await
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
    use crate::types::{MediaFilter, MediaHashInput};

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

    #[tokio::test]
    async fn test_kijukudb_backend_async() {
        // KijukuDB の impl KijukuBackend（async trait）経由で純DB操作が動くこと
        let db = KijukuDB::open_in_memory().unwrap();
        KijukuBackend::migrate(&db).await.unwrap();

        let media = KijukuBackend::create_media(
            &db,
            &MediaInput {
                title: "テスト".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(media.title, "テスト");

        let got = KijukuBackend::get_media(&db, media.id).await.unwrap().unwrap();
        assert_eq!(got.id, media.id);

        let found = KijukuBackend::find_media(&db, &MediaFilter::default(), None)
            .await
            .unwrap();
        assert_eq!(found.len(), 1);

        // tag・attribute・distinct も async trait 経由で
        let tag = KijukuBackend::create_tag(&db, "tag1").await.unwrap();
        KijukuBackend::add_tag_to_media(&db, media.id, tag.id)
            .await
            .unwrap();
        assert_eq!(
            KijukuBackend::get_media_tags(&db, media.id).await.unwrap().len(),
            1
        );

        KijukuBackend::set_media_attribute(&db, media.id, "k", Some("v"), None)
            .await
            .unwrap();
        assert_eq!(
            KijukuBackend::get_media_attribute(&db, media.id, "k")
                .await
                .unwrap()
                .unwrap()
                .value,
            Some("v".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_media_tags_bulk_async() {
        // get_media_tags_bulk: JOIN 1発で複数メディアのタグを取得（N+1回避）
        let db = KijukuDB::open_in_memory().unwrap();
        KijukuBackend::migrate(&db).await.unwrap();

        let m1 = KijukuBackend::create_media(
            &db,
            &MediaInput {
                title: "作品1".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let m2 = KijukuBackend::create_media(
            &db,
            &MediaInput {
                title: "作品2".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let m3 = KijukuBackend::create_media(
            &db,
            &MediaInput {
                title: "作品3".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let t1 = KijukuBackend::create_tag(&db, "tag-a").await.unwrap();
        let t2 = KijukuBackend::create_tag(&db, "tag-b").await.unwrap();

        // m1: tag-a, tag-b / m2: tag-a / m3: タグなし
        KijukuBackend::add_tag_to_media(&db, m1.id, t1.id).await.unwrap();
        KijukuBackend::add_tag_to_media(&db, m1.id, t2.id).await.unwrap();
        KijukuBackend::add_tag_to_media(&db, m2.id, t1.id).await.unwrap();

        let result =
            KijukuBackend::get_media_tags_bulk(&db, &[m1.id, m2.id, m3.id])
                .await
                .unwrap();

        // タグなしメディア(m3)は結果のエントリに含まれない
        assert_eq!(result.len(), 2);
        // m1 は2タグ（名前順: tag-a, tag-b）
        let m1_tags = result.get(&m1.id).unwrap();
        assert_eq!(m1_tags.len(), 2);
        assert_eq!(m1_tags[0].name, "tag-a");
        assert_eq!(m1_tags[1].name, "tag-b");
        // m2 は1タグ
        let m2_tags = result.get(&m2.id).unwrap();
        assert_eq!(m2_tags.len(), 1);
        assert_eq!(m2_tags[0].name, "tag-a");
        // m3 はエントリなし（タグなし）
        assert!(!result.contains_key(&m3.id));
    }

    #[tokio::test]
    async fn test_get_media_tags_bulk_empty_async() {
        // 空リストは空のマップを返す
        let db = KijukuDB::open_in_memory().unwrap();
        KijukuBackend::migrate(&db).await.unwrap();

        let result = KijukuBackend::get_media_tags_bulk(&db, &[]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_kijukudb_backend_hash_async() {
        let db = KijukuDB::open_in_memory().unwrap();
        KijukuBackend::migrate(&db).await.unwrap();
        let media = KijukuBackend::create_media(
            &db,
            &MediaInput {
                title: "h".to_string(),
                media_type: MediaType::Music,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let input = MediaHashInput {
            item_uuid: media.uuid.clone(),
            filename: String::new(),
            time_range: String::new(),
            content_hash: vec![0xABu8; 32],
            alternative_of: None,
        };
        let h = KijukuBackend::add_media_hash(&db, &input).await.unwrap();
        assert_eq!(h.content_hash, vec![0xABu8; 32]);

        let found = KijukuBackend::find_by_content_hash(&db, &vec![0xABu8; 32])
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
    }

    #[tokio::test]
    async fn test_kijukudb_backend_update_delete_async() {
        let db = KijukuDB::open_in_memory().unwrap();
        KijukuBackend::migrate(&db).await.unwrap();
        let media = KijukuBackend::create_media(
            &db,
            &MediaInput {
                title: "元".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        KijukuBackend::update_media(
            &db,
            media.id,
            &MediaUpdateInput {
                title: Some("更新後".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let got = KijukuBackend::get_media(&db, media.id).await.unwrap().unwrap();
        assert_eq!(got.title, "更新後");

        KijukuBackend::delete_media(&db, media.id).await.unwrap();
        assert!(KijukuBackend::get_media(&db, media.id).await.unwrap().is_none());
    }

    #[allow(deprecated)]
    #[test]
    fn test_kijukudb_transaction_sync_still_works() {
        // conn の Arc<ReentrantMutex> 化後も、deprecated 同期 API + transaction が動くこと
        // （transaction が接続ロックを保持しつつ、クロージャ内の同期メソッドは同一スレッドの
        // 再入で通過 → デッドロックせず、他スレッドは COMMIT/ROLLBACK までブロックされる）
        let db = KijukuDB::open_in_memory().unwrap();
        db.migrate().unwrap();

        let result = db.transaction(|db| {
            db.create_media(&MediaInput {
                title: "m1".to_string(),
                media_type: MediaType::Comic,
                ..Default::default()
            })?;
            db.create_media(&MediaInput {
                title: "m2".to_string(),
                media_type: MediaType::Video,
                ..Default::default()
            })?;
            Ok::<_, KijukuError>(())
        });
        assert!(result.is_ok());

        let found = db.find_media(&MediaFilter::default(), None).unwrap();
        assert_eq!(found.len(), 2);
    }
}
