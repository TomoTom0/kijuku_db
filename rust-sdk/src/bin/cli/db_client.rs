//! CLI 用 DB クライアント facade（Local / Remote 統合・TASK-64 Gap e）。
//!
//! サブコマンドハンドラが `--db host:path`（リモート）と通常パス（ローカル）を意識せず
//! 透過的に操作するための enum dispatch。`KijukuDB` と `RemoteKijukuDB` の固有メソッドは
//! ともに同期であり、ほぼ同名だが一部シグネチャ差（restore/diff/observe 等）があるため、
//! その正規化をこのモジュール1箇所に集約する（TS `cli.ts:createDatabase` パリティ）。
//!
//! stdin/server モードはローカル DB 専用（`build_backend`）であり、この facade の対象外。

use std::path::Path;

use serde::Serialize;

use kijuku_db::{
    parse_db_path, AttributeValueType, BackupInfo, BackupOptions, BackupSelector, DBOptions,
    KijukuDB, KijukuError, Media, MediaFilter, MediaHash, MediaInput, ParsedDbPath, QueryOptions,
    RemoteConfig, RemoteKijukuDB, Result, Tag, ThumbnailOptions, UpdateExistOptions,
    UpdateExistResult,
    backup::BackupMetaEntry,
    diff::{BackupDiff, DiffOptions, ObserveOptions, ObserveResult},
    file_ops::{FileOpOptions, FileOpResult},
    hash::ComputeHashResult,
    thumbnail::{CheckThumbnailResult, UpdateThumbnailResult},
    trash::{TrashEntry, TrashId, TrashOperation},
};

use super::CliContext;

/// サブコマンドが操作する DB クライアント（Local / Remote）。
///
/// ハンドラは `open_db_client(ctx)` でこれを得て、`client.observe(...)` のように統一呼び出しする。
/// Local/Remote の分岐とシグネチャ正規化は各メソッド内に隠蔽される。
pub enum DbClient {
    /// ローカル SQLite（`KijukuDB`）。
    Local(KijukuDB),
    /// SSH リモート（`RemoteKijukuDB`）。各操作が SSH 先の `kijuku-cli` stdin に RPC する。
    Remote(RemoteKijukuDB),
}

/// `--db` 解析結果と `CliContext` から `RemoteConfig` を組み立てる（共通ヘルパ）。
///
/// `ssh_host`/`db_path` は `parse_db_path` の結果から、`target`/`media_root` は ctx から設定。
/// `stg_db_path`/`binary_path`/`port`/`username`/`private_key_path` はデフォルト（stg は prod から
/// 導出・binary は `~/.local/bin/kijuku-cli`・SSH 認証は `~/.ssh/config` から解決）。
pub(super) fn remote_config_from_ctx(ctx: &CliContext, parsed: &ParsedDbPath) -> RemoteConfig {
    RemoteConfig {
        ssh_host: parsed.ssh_host.clone().unwrap_or_default(),
        db_path: Some(parsed.path.clone()),
        stg_db_path: None,
        target: ctx.target,
        read_source: None,
        media_root: ctx.media_root.clone(),
        ..Default::default()
    }
}

/// `CliContext` から DB クライアントを構築する（`--db host:path` で Remote、それ以外は Local）。
///
/// Local: `open_with_options`（readonly/backup/media_root）+ `should_migrate` で `migrate()`
///   + `!readonly` で `acquire_stg_lock()`（既存 `open_and_migrate_db` 等と同等）。
/// Remote: `RemoteKijukuDB::new`（接続せず config 格納のみ）。migrate/stg_lock は各 RPC 先の
///   リモート `handle_stdin` が処理するため、クライアント側では何もしない。
///
/// 戻り値のエラーは人間向けメッセージ（ハンドラが eprintln/output_response で出力）。
pub fn open_db_client(ctx: &CliContext) -> std::result::Result<DbClient, String> {
    let parsed = parse_db_path(&ctx.db_path);
    if parsed.is_remote {
        let config = remote_config_from_ctx(ctx, &parsed);
        Ok(DbClient::Remote(RemoteKijukuDB::new(config)))
    } else {
        let mut db = KijukuDB::open_with_options(
            &ctx.db_path,
            DBOptions {
                backup: Some(BackupOptions::default()),
                verbose: ctx.verbose,
                readonly: ctx.readonly,
                media_root: ctx.media_root.clone(),
                ..Default::default()
            },
        )
        .map_err(|e| format!("データベースのオープンに失敗: {}", e))?;
        if ctx.should_migrate {
            db.migrate()
                .map_err(|e| format!("マイグレーションに失敗: {}", e))?;
        }
        // stg 書込セッション（!readonly）は排他ロックを取得（設計 §15-11）。
        if !ctx.readonly {
            db.acquire_stg_lock().map_err(|e| {
                format!(
                    "stg の排他ロック取得に失敗しました（別セッションが使用中の可能性）: {}",
                    e
                )
            })?;
        }
        Ok(DbClient::Local(db))
    }
}

/// `BackupSelector` を JSON Value へ直列化（Remote の restore/diff_with_backup が Value 受けのため）。
///
/// `BackupSelector::to_wire_value` に委譲（サーバ側 `BackupSelectorJson` と対称な `{"type":...}` 形式）。
fn selector_to_value(selector: &BackupSelector) -> Result<serde_json::Value> {
    selector.to_wire_value()
}

/// import 1件分の入力レコード（media フィールド + タグ + 追加属性）。
///
/// `tags` はカンマ区切り文字列を分割したタグ名群。`attributes` は `--additional-columns` で
/// 指定した列を `(key, value)` とした追加属性（`media_attributes` テーブルへ string 型で格納）。
/// いずれも `MediaInput` には対応フィールドが無いため、`bulk_create_media` 後に個別登録する
/// 情報を保持する（TS `cli.ts:runImport` の `_tags`/`_attributes` に相当）。
#[derive(Debug, Clone)]
pub struct ImportRecord {
    pub media: MediaInput,
    pub tags: Vec<String>,
    pub attributes: Vec<(String, String)>,
}

/// import 実行結果の統計（`CommandResponse::success` で JSON 出力される）。
#[derive(Debug, Serialize)]
pub struct ImportStats {
    pub imported: usize,
    pub tags: usize,
    pub attributes: usize,
}

impl DbClient {
    // --- backup / restore / diff / observe ---

    /// バックアップ作成。`label` があればラベル付き（Local は `backup_with_label`、無ければ `backup`）。
    pub fn backup(&self, label: Option<&str>, timeout_ms: Option<u32>) -> Result<Option<String>> {
        match self {
            DbClient::Local(d) => match label {
                Some(l) => d.backup_with_label(l),
                None => d.backup(),
            },
            DbClient::Remote(r) => r.backup(label, timeout_ms),
        }
    }

    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        match self {
            DbClient::Local(d) => d.list_backups(),
            DbClient::Remote(r) => r.list_backups(),
        }
    }

    /// pre-stash（promote/(b)操作直前の prod snapshot）一覧（設計 §8）。
    pub fn list_pre_stashes(&self) -> Result<Vec<BackupInfo>> {
        match self {
            DbClient::Local(d) => d.list_pre_stashes(),
            DbClient::Remote(r) => r.list_pre_stashes(),
        }
    }

    /// 監査ログを取得（設計 §10・TASK-46）。監査ログは prod に集約されるため、通常は
    /// prod を open して呼ぶ（`list_pre_stashes` と同方針）。
    pub fn list_audit_logs(
        &self,
        filter: &kijuku_db::AuditLogFilter,
    ) -> Result<Vec<kijuku_db::AuditRecord>> {
        match self {
            DbClient::Local(d) => d.list_audit_logs(filter),
            DbClient::Remote(r) => r.list_audit_logs(filter),
        }
    }

    /// バックアップから復元。戻り値は復元先パス文字列（Local の PathBuf を文字列化）。
    /// Local は `&mut self`（DB 再オープンを伴う）のため、このメソッドも `&mut self`。
    pub fn restore(&mut self, selector: &BackupSelector, timeout_ms: Option<u32>) -> Result<String> {
        match self {
            DbClient::Local(d) => d
                .restore(selector)
                .map(|p| p.to_string_lossy().into_owned()),
            DbClient::Remote(r) => {
                let value = selector_to_value(selector)?;
                r.restore(&value, timeout_ms)
            }
        }
    }

    /// バックアップと現在DBの差分。Remote は selector を JSON Value に変換。
    pub fn diff_with_backup(
        &self,
        selector: &BackupSelector,
        options: &DiffOptions,
        timeout_ms: Option<u32>,
    ) -> Result<BackupDiff> {
        match self {
            DbClient::Local(d) => d.diff_with_backup(selector, options),
            DbClient::Remote(r) => {
                let value = selector_to_value(selector)?;
                r.diff_with_backup(&value, options, timeout_ms)
            }
        }
    }

    /// prod と stg（現在DB）の差分。`prod` はローカルパス文字列。
    /// Local は必須（`&Path`）、Remote は無視してサーバ側の prod デフォルト解決に委譲（None）。
    pub fn diff_with_prod(
        &self,
        prod: Option<&str>,
        options: &DiffOptions,
        timeout_ms: Option<u32>,
    ) -> Result<BackupDiff> {
        match self {
            DbClient::Local(d) => {
                let p = prod.ok_or_else(|| {
                    KijukuError::Other("ローカル diff には prod パスが必要です".to_string())
                })?;
                d.diff_with_prod(Path::new(p), options)
            }
            DbClient::Remote(r) => r.diff_with_prod(None, options, timeout_ms),
        }
    }

    /// prod/stg 整合性観測と promote gate 評価（設計 §3.4）。
    /// Local は prod パス必須、Remote はサーバ側 prod デフォルト解決に委譲（None）。
    pub fn observe(
        &self,
        prod: Option<&str>,
        options: &ObserveOptions,
        timeout_ms: Option<u32>,
    ) -> Result<ObserveResult> {
        match self {
            DbClient::Local(d) => {
                let p = prod.ok_or_else(|| {
                    KijukuError::Other("ローカル observe には prod パスが必要です".to_string())
                })?;
                d.observe(Path::new(p), options)
            }
            DbClient::Remote(r) => r.observe(None, options, timeout_ms),
        }
    }

    pub fn set_backup_label(&self, id: &str, label: Option<&str>) -> Result<()> {
        match self {
            DbClient::Local(d) => d.set_backup_label(id, label),
            DbClient::Remote(r) => r.set_backup_label(id, label),
        }
    }

    pub fn set_backup_note(&self, id: &str, note: Option<&str>) -> Result<()> {
        match self {
            DbClient::Local(d) => d.set_backup_note(id, note),
            DbClient::Remote(r) => r.set_backup_note(id, note),
        }
    }

    /// バックアップのメタ情報取得。CLI サブコマンド経由では未使用（stdin プロトコルの
    /// `getBackupMeta` のみサーバ側で使用）。Local/Remote parity のため facade に用意し、
    /// CLI サブコマンド追加時に `#[allow(dead_code)]` を外す。
    #[allow(dead_code)]
    pub fn get_backup_meta(&self, id: &str) -> Result<Option<BackupMetaEntry>> {
        match self {
            DbClient::Local(d) => d.get_backup_meta(id),
            DbClient::Remote(r) => r.get_backup_meta(id),
        }
    }

    // --- update_exist / thumbnail ---

    pub fn update_exist(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        update_options: &UpdateExistOptions,
    ) -> Result<UpdateExistResult> {
        match self {
            DbClient::Local(d) => d.update_exist(filter, options, update_options),
            DbClient::Remote(r) => r.update_exist(filter, options, update_options),
        }
    }

    pub fn check_thumbnail(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
    ) -> Result<CheckThumbnailResult> {
        match self {
            DbClient::Local(d) => d.check_thumbnail(filter, options),
            DbClient::Remote(r) => r.check_thumbnail(filter, options),
        }
    }

    pub fn update_thumbnail(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        thumbnail_options: &ThumbnailOptions,
    ) -> Result<UpdateThumbnailResult> {
        match self {
            DbClient::Local(d) => d.update_thumbnail(filter, options, thumbnail_options),
            DbClient::Remote(r) => r.update_thumbnail(filter, options, thumbnail_options),
        }
    }

    // --- hash ---

    /// メディア検索（Hash Compute の UUID 解決等で使用）。trait 操作と同等の同期固有メソッド。
    pub fn find_media(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
    ) -> Result<Vec<Media>> {
        match self {
            DbClient::Local(d) => d.find_media(filter, options),
            DbClient::Remote(r) => r.find_media(filter, options),
        }
    }

    pub fn compute_media_hash(
        &self,
        item_uuid: &str,
        media_path: &str,
        media_type: &str,
        duration_sec: Option<i32>,
    ) -> Result<ComputeHashResult> {
        match self {
            DbClient::Local(d) => {
                d.compute_media_hash(item_uuid, media_path, media_type, duration_sec)
            }
            DbClient::Remote(r) => {
                r.compute_media_hash(item_uuid, media_path, media_type, duration_sec)
            }
        }
    }

    pub fn compute_media_hashes(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        force: bool,
    ) -> Result<Vec<ComputeHashResult>> {
        match self {
            DbClient::Local(d) => d.compute_media_hashes(filter, options, force),
            DbClient::Remote(r) => r.compute_media_hashes(filter, options, force),
        }
    }

    pub fn get_media_hashes(&self, item_uuid: &str) -> Result<Vec<MediaHash>> {
        match self {
            DbClient::Local(d) => d.get_media_hashes(item_uuid),
            DbClient::Remote(r) => r.get_media_hashes(item_uuid),
        }
    }

    pub fn find_by_content_hash(&self, hash: &[u8]) -> Result<Vec<MediaHash>> {
        match self {
            DbClient::Local(d) => d.find_by_content_hash(hash),
            DbClient::Remote(r) => r.find_by_content_hash(hash),
        }
    }

    pub fn find_duplicate_hashes(&self) -> Result<Vec<(Vec<u8>, i64)>> {
        match self {
            DbClient::Local(d) => d.find_duplicate_hashes(),
            DbClient::Remote(r) => r.find_duplicate_hashes(),
        }
    }

    // --- file ops ---

    pub fn media_cp(
        &self,
        src: &str,
        dst: &str,
        opts: &FileOpOptions,
    ) -> Result<FileOpResult> {
        match self {
            DbClient::Local(d) => d.media_cp(src, dst, opts),
            DbClient::Remote(r) => r.media_cp(src, dst, opts),
        }
    }

    pub fn media_mv(
        &self,
        src: &str,
        dst: &str,
        opts: &FileOpOptions,
    ) -> Result<FileOpResult> {
        match self {
            DbClient::Local(d) => d.media_mv(src, dst, opts),
            DbClient::Remote(r) => r.media_mv(src, dst, opts),
        }
    }

    pub fn media_sync(
        &self,
        src: &str,
        dst: &str,
        opts: &FileOpOptions,
    ) -> Result<FileOpResult> {
        match self {
            DbClient::Local(d) => d.media_sync(src, dst, opts),
            DbClient::Remote(r) => r.media_sync(src, dst, opts),
        }
    }

    // --- trash ---

    pub fn move_to_trash(
        &self,
        target_rel: &str,
        operation: TrashOperation,
        reason: Option<&str>,
    ) -> Result<TrashId> {
        match self {
            DbClient::Local(d) => d.move_to_trash(target_rel, operation, reason),
            DbClient::Remote(r) => r.move_to_trash(target_rel, operation, reason),
        }
    }

    pub fn list_trash(&self) -> Result<Vec<TrashEntry>> {
        match self {
            DbClient::Local(d) => d.list_trash(),
            DbClient::Remote(r) => r.list_trash(),
        }
    }

    /// trash から復元。戻り値は文字列（Local の PathBuf を文字列化。Remote は元々 String）。
    pub fn restore_from_trash(&self, id: &str) -> Result<String> {
        match self {
            DbClient::Local(d) => d.restore_from_trash(id).map(|p| p.to_string_lossy().into_owned()),
            DbClient::Remote(r) => r.restore_from_trash(id),
        }
    }

    pub fn purge_trash(&self, ids: Option<&[String]>, dry_run: bool) -> Result<Vec<String>> {
        match self {
            DbClient::Local(d) => d.purge_trash(ids, dry_run),
            DbClient::Remote(r) => r.purge_trash(ids, dry_run),
        }
    }

    /// JSON/CSV/TSV から変換したレコード群を一括登録（media 作成 + タグ関連付け + 追加属性）。
    ///
    /// TS `cli.ts:runImport` と同等。`bulk_create_media` は media 行のみ作成するため、戻り値の
    /// `Media::id` を使ってタグ・属性を個別に登録する。Local/Remote のシグネチャ差
    /// （`get_tag_by_name` の Option/Result、`set_media_attribute` の value_type 型）は各アームの
    /// クロージャで正規化し、ループ本体は `import_media_impl` に一本化する。
    /// リモート(`--db host:path`)ではタグ/属性ごとに RPC が走る（TS と同一 N+1 パターン）。
    pub fn import_media(&self, records: &[ImportRecord]) -> Result<ImportStats> {
        let media_inputs: Vec<MediaInput> = records.iter().map(|r| r.media.clone()).collect();
        match self {
            DbClient::Local(d) => {
                let created = d.bulk_create_media(&media_inputs)?;
                import_media_impl(
                    &created,
                    records,
                    |name| Ok(d.get_tag_by_name(name)),
                    |name| d.create_tag(name),
                    |media_id, tag_id| d.add_tag_to_media(media_id, tag_id),
                    |media_id, key, value| {
                        d.set_media_attribute(media_id, key, value, Some(AttributeValueType::String))
                    },
                )
            }
            DbClient::Remote(r) => {
                let created = r.bulk_create_media(&media_inputs)?;
                import_media_impl(
                    &created,
                    records,
                    |name| r.get_tag_by_name(name),
                    |name| r.create_tag(name),
                    |media_id, tag_id| r.add_tag_to_media(media_id, tag_id),
                    |media_id, key, value| {
                        r.set_media_attribute(media_id, key, value, Some("string"))
                    },
                )
            }
        }
    }
}

/// `import_media` の共通ループ本体（Local/Remote のシグネチャ差は呼出側クロージャで吸収）。
///
/// `created`（`bulk_create_media` の戻り値）と `records` を先頭から対応付け、各レコードの
/// `tags`・`attributes` を登録する。タグは get→無ければ create→関連付け。属性は string 型で設定。
fn import_media_impl<G, CR, A, S>(
    created: &[Media],
    records: &[ImportRecord],
    mut get_tag: G,
    mut create_tag: CR,
    mut add_tag: A,
    mut set_attr: S,
) -> Result<ImportStats>
where
    G: FnMut(&str) -> Result<Option<Tag>>,
    CR: FnMut(&str) -> Result<Tag>,
    A: FnMut(i64, i64) -> Result<()>,
    S: FnMut(i64, &str, Option<&str>) -> Result<()>,
{
    let mut tag_count = 0usize;
    let mut attr_count = 0usize;

    for (media, record) in created.iter().zip(records.iter()) {
        for name in &record.tags {
            if name.is_empty() {
                continue;
            }
            let tag = match get_tag(name)? {
                Some(t) => t,
                None => create_tag(name)?,
            };
            add_tag(media.id, tag.id)?;
            tag_count += 1;
        }
        for (key, value) in &record.attributes {
            set_attr(media.id, key, Some(value))?;
            attr_count += 1;
        }
    }

    Ok(ImportStats {
        imported: created.len(),
        tags: tag_count,
        attributes: attr_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kijuku_db::config::Target;

    /// `CliContext` 相当の最小ダミーを構築（フィールド追加時に更新）。
    fn make_ctx(db_path: &str) -> CliContext {
        CliContext {
            db_path: db_path.to_string(),
            verbose: false,
            backend: crate::BackendKind::Local,
            media_root: None,
            target: Target::Stg,
            readonly: false,
            should_migrate: true,
        }
    }

    #[test]
    fn test_open_db_client_remote_host_path() {
        // RemoteKijukuDB::new は接続しないため、host:path → Remote variant の判定のみ安全に検証可能。
        let ctx = make_ctx("nas:/data/kijuku.db");
        let client = open_db_client(&ctx).expect("remote client should construct");
        assert!(matches!(client, DbClient::Remote(_)));
    }

    #[test]
    fn test_open_db_client_remote_user_at_host() {
        let ctx = make_ctx("tomo@nas:~/.local/share/kijuku/kijuku.db");
        let client = open_db_client(&ctx).expect("remote client should construct");
        assert!(matches!(client, DbClient::Remote(_)));
    }

    #[test]
    fn test_remote_config_from_ctx_carries_ssh_host_and_path() {
        let ctx = make_ctx("nas:/data/kijuku.db");
        let parsed = parse_db_path(&ctx.db_path);
        let cfg = remote_config_from_ctx(&ctx, &parsed);
        assert_eq!(cfg.ssh_host, "nas");
        assert_eq!(cfg.db_path.as_deref(), Some("/data/kijuku.db"));
        assert_eq!(cfg.target, Target::Stg);
    }
}
