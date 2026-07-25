//! Kijuku DB CLI
//!
//! JSON形式の入出力でリモート操作を可能にするCLIツール
//! Version: 0.2.2
//!
//! NOTE: CLI のハンドラ群は Phase 3 で async 化（KijukuBackend 経由）する予定。
//! それまで KijukuDB の deprecated 同期メソッドを使用するため、移行完了まで一時的に許容する。

// ハンドラの async 化（Phase 3）までの間、deprecated 同期 API の使用を一時的に許容
#![allow(deprecated)]

use clap::Parser;
use include_dir::{include_dir, Dir};
use kijuku_db::{
    AttributeValueType, BackupInfo, BackupKind, BackupOptions, BackupScope, BackupSelector,
    BulkUpdateItem, D1Config, D1KijukuDB, DBOptions, KijukuBackend, KijukuDB,
    MediaFilter, MediaInput, MediaHashInput, MediaType, MediaUpdateInput, QueryOptions,
    RemoteKijukuDB, SystemEnv, Target, ThumbnailOptions, TransferOptions,
    UpdateExistOptions, parse_db_path, resolve_prod_and_stg_paths, resolve_target, transfer,
    verify,
    file_ops::FileOpOptions,
    trash::TrashOperation,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

mod db_client;
use db_client::{ImportRecord, open_db_client, remote_config_from_ctx};

static DOCS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../docs");

/// コマンドリクエスト
#[derive(Debug, Deserialize)]
struct CommandRequest {
    operation: String,
    params: serde_json::Value,
}

/// コマンドレスポンス
#[derive(Debug, Serialize)]
struct CommandResponse {
    success: bool,
    data: Option<serde_json::Value>,
    error: Option<String>,
}

impl CommandResponse {
    fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
        }
    }

    fn ack() -> Self {
        Self::success(serde_json::json!({"ok": true}))
    }

    fn from_result<T: serde::Serialize>(result: kijuku_db::Result<T>, error_prefix: &str) -> Self {
        match result {
            Ok(data) => match serde_json::to_value(data) {
                Ok(v) => Self::success(v),
                Err(e) => Self::error(format!("レスポンスのシリアライズに失敗: {}", e)),
            },
            Err(e) => Self::error(format!("{}{}", error_prefix, e)),
        }
    }
}

fn deserialize_params<T: serde::de::DeserializeOwned>(params: &serde_json::Value) -> Result<T, CommandResponse> {
    T::deserialize(params)
        .map_err(|e| CommandResponse::error(format!("パラメータエラー: {}", e)))
}

/// メディア作成のパラメータ
#[derive(Debug, Deserialize)]
struct CreateMediaParams {
    data: MediaInput,
}

/// メディア取得のパラメータ
#[derive(Debug, Deserialize)]
struct GetMediaParams {
    id: i64,
}

/// メディア更新のパラメータ（部分更新）
#[derive(Debug, Deserialize)]
struct UpdateMediaParams {
    id: i64,
    data: MediaUpdateInput,
}

/// メディア削除のパラメータ
#[derive(Debug, Deserialize)]
struct DeleteMediaParams {
    id: i64,
}

/// メディア検索のパラメータ
#[derive(Debug, Deserialize)]
struct FindMediaParams {
    filter: MediaFilter,
    options: Option<QueryOptions>,
}

#[derive(Debug, Deserialize)]
struct GetDistinctValuesParams {
    fields: Vec<String>,
    filter: MediaFilter,
}

/// ファイル複製（cp）のパラメータ
#[derive(Debug, Deserialize)]
struct MediaCpParams {
    src: String,
    dst: String,
    #[serde(default)]
    options: FileOpOptions,
}

/// ファイル移動（mv）のパラメータ
#[derive(Debug, Deserialize)]
struct MediaMvParams {
    src: String,
    dst: String,
    #[serde(default)]
    options: FileOpOptions,
}

/// ディレクトリ同期（sync）のパラメータ
#[derive(Debug, Deserialize)]
struct MediaSyncParams {
    src: String,
    dst: String,
    #[serde(default)]
    options: FileOpOptions,
}

/// trash への移動のパラメータ
#[derive(Debug, Deserialize)]
struct MoveToTrashParams {
    target_rel: String,
    operation: TrashOperation,
    #[serde(default)]
    reason: Option<String>,
}

/// trash からの復元のパラメータ
#[derive(Debug, Deserialize)]
struct RestoreFromTrashParams {
    id: String,
}

/// trash の物理削除のパラメータ
#[derive(Debug, Deserialize)]
struct PurgeTrashParams {
    #[serde(default)]
    ids: Option<Vec<String>>,
    #[serde(default)]
    dry_run: bool,
}

/// 一括作成のパラメータ
#[derive(Debug, Deserialize)]
struct BulkCreateMediaParams {
    data_list: Vec<MediaInput>,
}

/// 一括削除のパラメータ
#[derive(Debug, Deserialize)]
struct BulkDeleteMediaParams {
    ids: Vec<i64>,
}

/// 一括更新のパラメータ
#[derive(Debug, Deserialize)]
struct BulkUpdateMediaParams {
    updates: Vec<BulkUpdateItem>,
}

/// タグ作成のパラメータ
#[derive(Debug, Deserialize)]
struct CreateTagParams {
    name: String,
}

/// タグ取得のパラメータ
#[derive(Debug, Deserialize)]
struct GetTagByNameParams {
    name: String,
}

/// テーブル情報取得のパラメータ
#[derive(Debug, Deserialize)]
struct GetTableInfoParams {
    table_name: String,
}

/// タグ追加のパラメータ
#[derive(Debug, Deserialize)]
struct AddTagToMediaParams {
    media_id: i64,
    tag_id: i64,
}

/// タグ削除のパラメータ
#[derive(Debug, Deserialize)]
struct RemoveTagFromMediaParams {
    media_id: i64,
    tag_id: i64,
}

/// メディアタグ取得のパラメータ
#[derive(Debug, Deserialize)]
struct GetMediaTagsParams {
    media_id: i64,
}

/// 複数メディアのタグ一括取得のパラメータ
#[derive(Debug, Deserialize)]
struct GetMediaTagsBulkParams {
    media_ids: Vec<i64>,
}

/// 属性設定のパラメータ
#[derive(Debug, Deserialize)]
struct SetMediaAttributeParams {
    media_id: i64,
    key: String,
    value: Option<String>,
    value_type: Option<String>,
}

/// 属性取得のパラメータ
#[derive(Debug, Deserialize)]
struct GetMediaAttributeParams {
    media_id: i64,
    key: String,
}

/// 全属性取得のパラメータ
#[derive(Debug, Deserialize)]
struct GetMediaAttributesParams {
    media_id: i64,
}

/// 属性削除のパラメータ
#[derive(Debug, Deserialize)]
struct DeleteMediaAttributeParams {
    media_id: i64,
    key: String,
}

/// 全属性削除のパラメータ
#[derive(Debug, Deserialize)]
struct DeleteAllMediaAttributesParams {
    media_id: i64,
}

/// ハッシュ追加のパラメータ
#[derive(Debug, Deserialize)]
struct AddMediaHashParams {
    input: MediaHashInput,
}

/// ハッシュ一括追加のパラメータ
#[derive(Debug, Deserialize)]
struct AddMediaHashesParams {
    inputs: Vec<MediaHashInput>,
}

/// ハッシュ取得（作品別）のパラメータ
#[derive(Debug, Deserialize)]
struct GetMediaHashesParams {
    item_uuid: String,
}

/// ハッシュ取得（位置指定）のパラメータ
#[derive(Debug, Deserialize)]
struct GetMediaHashParams {
    item_uuid: String,
    filename: String,
    time_range: String,
}

/// content_hash検索のパラメータ
#[derive(Debug, Deserialize)]
struct FindByContentHashParams {
    hash_hex: String,
}

/// ハッシュ削除のパラメータ
#[derive(Debug, Deserialize)]
struct DeleteMediaHashParams {
    item_uuid: String,
    filename: String,
    time_range: String,
}

/// ハッシュ全削除のパラメータ
#[derive(Debug, Deserialize)]
struct DeleteMediaHashesParams {
    item_uuid: String,
}

/// compute_media_hashのパラメータ
#[derive(Debug, Deserialize)]
struct ComputeMediaHashParams {
    item_uuid: String,
    media_path: String,
    media_type: String,
    duration_sec: Option<i32>,
}

/// compute_media_hashesのパラメータ
#[derive(Debug, Deserialize)]
struct ComputeMediaHashesParams {
    filter: MediaFilter,
    options: Option<QueryOptions>,
    force: bool,
}

/// update-existのパラメータ
#[derive(Debug, Deserialize)]
struct UpdateExistParams {
    #[serde(default)]
    filter: MediaFilter,
    options: Option<QueryOptions>,
    #[serde(default)]
    update_options: UpdateExistOptions,
}

/// check-thumbnail / update-thumbnailのパラメータ
#[derive(Debug, Deserialize)]
struct ThumbnailParams {
    #[serde(default)]
    filter: MediaFilter,
    options: Option<QueryOptions>,
    #[serde(default)]
    thumbnail_options: ThumbnailOptions,
}

/// バックアップのパラメータ
#[derive(Debug, Deserialize)]
struct BackupParams {
    label: Option<String>,
}

/// バックアップセレクターのJSON表現
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum BackupSelectorJson {
    Latest,
    Nth { n: usize },
    ById { id: String },
    /// 既知パスで直接指定（設計 §8 即時復旧・pre-stash 戻し）
    ByPath { path: String },
}

impl BackupSelectorJson {
    fn to_selector(&self) -> BackupSelector {
        match self {
            BackupSelectorJson::Latest => BackupSelector::latest(),
            BackupSelectorJson::Nth { n } => BackupSelector::nth(*n),
            BackupSelectorJson::ById { id } => BackupSelector::by_id(id),
            BackupSelectorJson::ByPath { path } => BackupSelector::by_path(path),
        }
    }
}

/// 差分詳細度のJSON表現
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum DiffDetailJson {
    SummaryOnly,
    Limited { n: usize },
    Full,
}

impl DiffDetailJson {
    fn to_detail(&self) -> kijuku_db::diff::DiffDetail {
        match self {
            Self::SummaryOnly => kijuku_db::diff::DiffDetail::SummaryOnly,
            Self::Limited { n } => kijuku_db::diff::DiffDetail::Limited { n: *n },
            Self::Full => kijuku_db::diff::DiffDetail::Full,
        }
    }
}

/// 差分取得オプションのJSON表現
#[derive(Debug, Deserialize, Default)]
struct DiffOptionsJson {
    detail: Option<DiffDetailJson>,
}

impl DiffOptionsJson {
    fn to_options(&self) -> kijuku_db::diff::DiffOptions {
        kijuku_db::diff::DiffOptions {
            detail: self.detail.as_ref().map(|d| d.to_detail()),
        }
    }
}

/// バックアップ差分のパラメータ
#[derive(Debug, Deserialize, Default)]
struct DiffBackupParams {
    selector: Option<BackupSelectorJson>,
    options: Option<DiffOptionsJson>,
}

/// prod/stg 差分のパラメータ（stdin・設計 §4.4・TASK-53）
#[derive(Debug, Deserialize, Default)]
struct DiffProdStgParams {
    /// prod DB パス（省略時は KIJUKU_DB_PATH / デフォルト）
    #[serde(default, rename = "prodDbPath")]
    prod_db_path: Option<String>,
    options: Option<DiffOptionsJson>,
}

/// observe（機械的 promote gate）のパラメータ（stdin・設計 §3.4/§4.4・TASK-54）。
/// `options` は SDK の `ObserveOptions`（camelCase）に直接デシリアライズ。
#[derive(Debug, Deserialize, Default)]
struct ObserveParams {
    /// prod DB パス（省略時は KIJUKU_DB_PATH / デフォルト）
    #[serde(default, rename = "prodDbPath")]
    prod_db_path: Option<String>,
    #[serde(default)]
    options: kijuku_db::diff::ObserveOptions,
}

/// promote（stg→prod 反映）のパラメータ（stdin・設計 §4.5・TASK-58/62）。
/// `ObserveParams` と同形。`backupOpts` で pre-stash 先（`backup_dir`）をカスタマイズ可能
/// （省略時は `enabled:false` 相当・デフォルト `tmp/` のみ・§7.2）。
#[derive(Debug, Deserialize, Default)]
struct PromoteParams {
    /// prod DB パス（省略時は KIJUKU_DB_PATH / デフォルト）
    #[serde(default, rename = "prodDbPath")]
    prod_db_path: Option<String>,
    #[serde(default)]
    options: kijuku_db::diff::ObserveOptions,
    /// pre-stash 先の BackupManager 設定（省略時はデフォルト `tmp/`・§7.2）。
    #[serde(default, rename = "backupOpts")]
    backup_opts: Option<kijuku_db::BackupOptions>,
}

/// バックアップメタ（ラベル）操作のパラメータ
#[derive(Debug, Deserialize)]
struct BackupLabelParams {
    id: String,
    label: Option<String>,
}

/// バックアップメタ（メモ）操作のパラメータ
#[derive(Debug, Deserialize)]
struct BackupNoteParams {
    id: String,
    note: Option<String>,
}

/// バックアップIDのみのパラメータ
#[derive(Debug, Deserialize)]
struct BackupIdParams {
    id: String,
}

/// バックアップ復元のパラメータ
#[derive(Debug, Deserialize)]
struct RestoreParams {
    selector: Option<BackupSelectorJson>,
    /// (b) 操作の dry-run（設計 §9.2/§15-13）。true なら復元で変化する内容（prod 現状 vs バックアップの
    /// 差分）を返し prod は変更しない。prod 直接実行時の gate の一部。
    #[serde(default, rename = "dryRun")]
    dry_run: bool,
}

/// sync（prod→stg フル複製）のパラメータ（設計 §4.2）。
///
/// `from`/`to` で prod/stg パスを明示指定可能（リモート SSH 経由で両パスを渡す用途）。
/// いずれかが欠落した場合は環境変数/デフォルトから解決する。
#[derive(Debug, Default, Deserialize)]
struct SyncParams {
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

/// バックエンド種別
#[derive(Clone, Debug, clap::ValueEnum, PartialEq, Eq)]
enum BackendKind {
    /// ローカル SQLite
    Local,
    /// Cloudflare D1（REST）
    D1,
}

/// CLI が操作するバックエンド（Local / D1）
enum Backend {
    Local(KijukuDB),
    D1(D1KijukuDB),
}

impl Backend {
    /// 純 DB 操作用の trait オブジェクト
    fn as_backend(&self) -> &dyn KijukuBackend {
        match self {
            Backend::Local(d) => d,
            Backend::D1(d) => d,
        }
    }

    /// Local 固有操作（&self）。D1 は None。
    fn as_local(&self) -> Option<&KijukuDB> {
        match self {
            Backend::Local(d) => Some(d),
            Backend::D1(_) => None,
        }
    }

    /// Local 固有操作（&mut self）。D1 は None。
    fn as_local_mut(&mut self) -> Option<&mut KijukuDB> {
        match self {
            Backend::Local(d) => Some(d),
            Backend::D1(_) => None,
        }
    }
}

fn not_supported(op: &str) -> CommandResponse {
    CommandResponse::error(format!(
        "操作 '{}' は現在のバックエンドではサポートされていません",
        op
    ))
}

/// ctx と backend 種別から Backend を構築
fn build_backend(ctx: &CliContext) -> Result<Backend, CommandResponse> {
    match ctx.backend {
        BackendKind::Local => {
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
            .map_err(|e| CommandResponse::error(format!("データベースのオープンに失敗: {}", e)))?;
            // stg 書込セッション（!readonly）は排他ロックを取得（設計 §15-11）。
            // 別セッションが stg 編集中なら StgBusy で拒否。
            if !ctx.readonly {
                db.acquire_stg_lock().map_err(|e| {
                    CommandResponse::error(format!(
                        "stg の排他ロック取得に失敗しました（別セッションが使用中の可能性）: {}",
                        e
                    ))
                })?;
            }
            Ok(Backend::Local(db))
        }
        BackendKind::D1 => {
            let account_id = std::env::var("D1_ACCOUNT_ID").map_err(|_| {
                CommandResponse::error("D1_ACCOUNT_ID 環境変数が設定されていません".to_string())
            })?;
            let database_id = std::env::var("D1_DATABASE_ID").map_err(|_| {
                CommandResponse::error("D1_DATABASE_ID 環境変数が設定されていません".to_string())
            })?;
            let db = D1KijukuDB::from_wrangler(D1Config {
                account_id,
                database_id,
            })
            .map_err(|e| {
                CommandResponse::error(format!(
                    "D1 接続に失敗: {}（wrangler login 済みか確認してください）",
                    e
                ))
            })?;
            Ok(Backend::D1(db))
        }
    }
}

/// CLIのグローバルコンテキスト
struct CliContext {
    db_path: String,
    verbose: bool,
    backend: BackendKind,
    media_root: Option<String>,
    /// 操作対象（prod/stg・read-source 折り畳み済み・設計 §13）。remote 接続時にリモート CLI へ伝達。
    target: Target,
    /// prod 読込経路なら true（readonly open・設計 §5.1）
    readonly: bool,
    /// prod 読込経路（readonly）なら false（migrate skip・設計 §5.1）
    should_migrate: bool,
}

/// BackupInfoをJSON Valueに変換
fn backup_info_to_json(info: &BackupInfo) -> serde_json::Value {
    let scope = match info.scope {
        BackupScope::Auto => "auto",
        BackupScope::Manual => "manual",
        BackupScope::Tmp => "tmp",
    };
    let kind = match &info.kind {
        BackupKind::Full => serde_json::json!({"type": "full"}),
        BackupKind::Diff { base_id } => serde_json::json!({"type": "diff", "baseId": base_id}),
    };
    let created_at = info
        .created_at
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    use kijuku_db::backup::LabelSource;
    let label_source = match &info.label_source {
        LabelSource::Filename => "filename",
        LabelSource::Sidecar => "sidecar",
    };
    serde_json::json!({
        "id": info.id,
        "name": info.name,
        "path": info.path.to_string_lossy(),
        "createdAt": created_at,
        "scope": scope,
        "kind": kind,
        "label": info.label,
        "labelSource": label_source,
        "note": info.note,
    })
}

use clap::Subcommand;

/// SDK利用ガイドを表示
fn show_docs(doc_type: &str) {
    let (doc_subpath, doc_name) = match doc_type {
        "overview" | "sdk" => ("usage/sdk/README.md", "SDK選択ガイド"),
        "ts" | "typescript" => ("usage/sdk/ts/README.md", "TypeScript SDKガイド"),
        "rust" => ("usage/sdk/rust/README.md", "Rust SDKガイド"),
        "api" => ("api.md", "API仕様書"),
        _ => {
            eprintln!("エラー: 不明なドキュメントタイプ: {}", doc_type);
            eprintln!();
            eprintln!("利用可能なドキュメント:");
            eprintln!("  kijuku-cli docs [overview|sdk]  - SDK選択ガイド（デフォルト）");
            eprintln!("  kijuku-cli docs ts              - TypeScript SDKガイド");
            eprintln!("  kijuku-cli docs rust            - Rust SDKガイド");
            eprintln!("  kijuku-cli docs api             - API仕様書");
            return;
        }
    };

    // バイナリに埋め込まれたドキュメントから読み込む
    let content = DOCS_DIR
        .get_file(doc_subpath)
        .and_then(|f| f.contents_utf8());

    match content {
        Some(doc_content) => {
            println!("# {}", doc_name);
            println!();
            println!("{}", doc_content);
        }
        None => {
            eprintln!("ドキュメントファイルが見つかりません: {}", doc_subpath);
            eprintln!();
            eprintln!("オンラインドキュメント:");
            eprintln!(
                "  https://github.com/TomoTom0/kijuku_db/blob/main/docs/{}",
                doc_subpath
            );
        }
    }
}

#[derive(Parser)]
#[command(name = "kijuku-cli", version)]
#[command(about = "きじゅくDB CLI", long_about = None)]
struct Cli {
    /// データベースファイルのパス（未指定時は --target に応じ stg/prod デフォルト・設計 §13）
    #[arg(long)]
    db: Option<String>,

    /// 実行したSQLをstderrに出力する
    #[arg(long)]
    verbose: bool,

    /// バックエンド（local / d1）。d1 は D1_ACCOUNT_ID / D1_DATABASE_ID 環境変数が必要
    #[arg(long, value_enum, default_value_t = BackendKind::Local)]
    backend: BackendKind,

    /// 操作対象（prod / stg・未指定時は stg）。prod は readonly + migrate skip（設計 §13）
    #[arg(long, value_parser = ["prod", "stg"])]
    target: Option<String>,

    /// 読込先（prod / stg・未指定時は target に従う）。prod 指定で prod RO 読込専用セッション（設計 §3.5・方式A）
    #[arg(long, value_parser = ["prod", "stg"])]
    read_source: Option<String>,

    /// ファイル操作 API（cp/mv/sync/trash/upload/download）のサンドボックス境界（media root のパス）
    #[arg(long)]
    media_root: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Web GUIサーバーを起動
    Server {
        /// サーバーのポート番号（デフォルト: 40001）
        #[arg(long, default_value = "40001")]
        port: u16,

        /// 認証パスワード（省略時は自動生成）
        #[arg(long)]
        password: Option<String>,
    },
    /// SDK利用ガイドを表示
    Docs {
        /// ドキュメントタイプ (overview|ts|rust|api)
        #[arg(value_name = "TYPE", default_value = "overview")]
        doc_type: String,
    },
    /// フィルタで絞り込んだメディアのflag_existをファイル存在状態に基づいて更新する
    UpdateExist {
        /// DBを更新せず結果を出力のみ
        #[arg(long)]
        dry_run: bool,
        /// MediaFilterのJSON文字列
        #[arg(long, default_value = "{}")]
        filter: String,
    },
    /// サムネイルの状態をチェックする
    CheckThumbnail {
        /// MediaFilterのJSON文字列
        #[arg(long, default_value = "{}")]
        filter: String,
    },
    /// サムネイルを生成・更新する
    UpdateThumbnail {
        /// DBを更新せず結果を出力のみ
        #[arg(long)]
        dry_run: bool,
        /// 既存サムネイルを強制再生成する
        #[arg(long)]
        force: bool,
        /// MediaFilterのJSON文字列
        #[arg(long, default_value = "{}")]
        filter: String,
    },
    /// バックアップを作成
    Backup {
        /// バックアップのラベル（省略可）
        #[arg(long)]
        label: Option<String>,
        /// SSH RPC のタイムアウト（ms）。省略時は DB サイズから適応的に算出（TS parity）。ローカル DB では無視される。
        #[arg(long, value_name = "MS")]
        timeout_ms: Option<u32>,
    },
    /// バックアップ一覧を表示
    ListBackups,
    /// pre-stash（即時復旧用ロールバックファイル）一覧を表示（設計 §8）
    ListPreStashes,
    /// バックアップから復元
    Restore {
        /// N番目のバックアップから復元（0が最新、省略時は最新）
        #[arg(long)]
        nth: Option<usize>,
        /// バックアップID（タイムスタンプ）を指定して復元（nth より優先）
        #[arg(long)]
        id: Option<String>,
        /// SSH RPC のタイムアウト（ms）。省略時は DB サイズから適応的に算出（TS parity）。ローカル DB では無視される。
        #[arg(long, value_name = "MS")]
        timeout_ms: Option<u32>,
    },
    /// バックアップと現在DBの差分を表示（復元判断用）
    DiffBackup {
        /// N番目のバックアップと比較（0が最新、省略時は最新）
        #[arg(long)]
        nth: Option<usize>,
        /// バックアップID（タイムスタンプ）を指定（nth より優先）
        #[arg(long)]
        id: Option<String>,
        /// 詳細度: summary（件数のみ）/ limited=<N> / full
        #[arg(long, default_value = "summary")]
        detail: String,
        /// SSH RPC のタイムアウト（ms）。省略時は DB サイズから適応的に算出（TS parity）。ローカル DB では無視される。
        #[arg(long, value_name = "MS")]
        timeout_ms: Option<u32>,
    },
    /// prod と stg（現在DB）の差分を表示（promote 判断用・設計 §4.4）
    DiffProdStg {
        /// prod DB パス（省略時は KIJUKU_DB_PATH / デフォルト）
        #[arg(long)]
        prod: Option<String>,
        /// 詳細度: summary / limited=<N> / full
        #[arg(long, default_value = "limited=20")]
        detail: String,
        /// テーブル別件数・分布（ProdStgDiffSummary）を表示
        #[arg(long)]
        summarize: bool,
        /// LLM explanation prompt を stdout に出力（他の出力を抑制）
        #[arg(long)]
        prompt: bool,
        /// SSH RPC のタイムアウト（ms）。省略時は DB サイズから適応的に算出（TS parity）。ローカル DB では無視される。
        #[arg(long, value_name = "MS")]
        timeout_ms: Option<u32>,
    },
    /// stg と prod を比較し機械的 promote gate を評価（promote 可否・設計 §3.4/§4.4）
    ///
    /// integrity/FK/schema_version/件数差分上限 の機械 gate を実行し、全合格で promote 可能と判定。
    /// 閾値は `--max-added/--max-removed/--max-changed` で上書き可能（デフォルト 5000/1000/5000）。
    Observe {
        /// prod DB パス（省略時は KIJUKU_DB_PATH / デフォルト）
        #[arg(long)]
        prod: Option<String>,
        /// 差分の詳細度: summary / limited=<N> / full（observe 本体は件数で判定するため summary で十分）
        #[arg(long, default_value = "summary")]
        detail: String,
        /// promote で prod に追加される行数の上限（`totals.added`）
        #[arg(long)]
        max_added: Option<usize>,
        /// promote で prod から削除される行数の上限（`totals.removed`）
        #[arg(long)]
        max_removed: Option<usize>,
        /// promote で prod が上書きされる行数の上限（`totals.changed`）
        #[arg(long)]
        max_changed: Option<usize>,
        /// 機械可読 JSON で出力（他の出力を抑制）
        #[arg(long)]
        json: bool,
        /// SSH RPC のタイムアウト（ms）。省略時は DB サイズから適応的に算出（TS parity）。ローカル DB では無視される。
        #[arg(long, value_name = "MS")]
        timeout_ms: Option<u32>,
    },
    /// バックアップにラベルを付与（事後）
    SetBackupLabel {
        #[arg(long)]
        id: String,
        #[arg(long)]
        label: Option<String>,
    },
    /// バックアップにメモを付与（事後）
    SetBackupNote {
        #[arg(long)]
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// prod(RO) → stg(RW) のフル複製（sync・設計 §4.2）。
    ///
    /// 環境変数 `KIJUKU_DB_PATH`(prod) / `KIJUKU_STG_DB_PATH`(stg)（未設定時は
    /// デフォルト）から両パスを解決する。`--from`/`--to` で明示上書き可能。
    /// stg 排他（接続中プロセスがないこと）は呼出側の責任（設計 §15-11 P1）。
    SyncDb {
        /// prod パス（省略時は KIJUKU_DB_PATH / デフォルト）
        #[arg(long)]
        from: Option<String>,
        /// stg パス（省略時は KIJUKU_STG_DB_PATH / デフォルト）
        #[arg(long)]
        to: Option<String>,
        /// SSH RPC のタイムアウト（ms）。省略時は DB サイズから適応的に算出（TS parity）。ローカル DB では無視される。
        #[arg(long, value_name = "MS")]
        timeout_ms: Option<u32>,
    },
    /// stg を破棄して prod から再 sync（discard・設計 §4.6）。
    ///
    /// 処理は `sync-db` と同一（prod→stg の Online Backup フル複製・既存 stg は上書き破棄）。
    /// 操作名のみ監査ログ（§10）で区別するため独立サブコマンド。書込セッション中断・
    /// observe gate 不合格時に stg を捨てて再構築する経路（prod は一切触らない）。
    /// 引数の意味・解決方法は `sync-db` に準ずる。
    DiscardDb {
        /// prod パス（省略時は KIJUKU_DB_PATH / デフォルト）
        #[arg(long)]
        from: Option<String>,
        /// stg パス（省略時は KIJUKU_STG_DB_PATH / デフォルト）
        #[arg(long)]
        to: Option<String>,
        /// SSH RPC のタイムアウト（ms）。省略時は DB サイズから適応的に算出（TS parity）。ローカル DB では無視される。
        #[arg(long, value_name = "MS")]
        timeout_ms: Option<u32>,
    },
    /// コンテンツハッシュ操作
    Hash {
        #[command(subcommand)]
        hash_command: HashCommands,
    },
    /// ファイル操作（cp/mv/sync）。media root 配下の dry-run ファースト操作
    File {
        #[command(subcommand)]
        file_command: FileCommands,
    },
    /// trash（論理削除）操作: 移動・一覧・復元・物理削除
    Trash {
        #[command(subcommand)]
        trash_command: TrashCommands,
    },
    /// ローカルDB（--db）の全データを D1 へバルクロード（移行）し、件数・内容一致を検証する
    ///
    /// `--backend d1` が必要。source は `--db` のローカル SQLite、dest は D1。
    BulkLoad {
        /// source の media 読み出しページサイズ（省略時 500）
        #[arg(long)]
        chunk_size: Option<usize>,
        /// 転送せず、既存の dest に対する検証のみ行う
        #[arg(long)]
        verify_only: bool,
    },
    /// JSON/CSV/TSV ファイルからメディアを一括インポート（タグ・追加属性も処理）
    ///
    /// 拡張子で形式を自動判定（`.json` / `.csv` / `.tsv`）。各レコードの `media_type`・`title` は
    /// 必須。`tags` 列（カンマ区切り）はタグ関連付け、`--additional-columns` で指定した列は
    /// `media_attributes` へ string 型で格納される（TS `cli.ts:runImport` 相当）。
    Import {
        /// 入力ファイルパス（.json / .csv / .tsv）
        #[arg(value_name = "FILE")]
        file: String,
        /// media_attributes に保存する追加カラム（カンマ区切り）
        #[arg(long)]
        additional_columns: Option<String>,
    },
}

#[derive(Subcommand)]
enum HashCommands {
    /// ハッシュを計算・登録
    Compute {
        /// 特定作品のUUID
        #[arg(long)]
        uuid: Option<String>,
        /// ハッシュ未計算の全作品を計算
        #[arg(long)]
        all: bool,
        /// 既存ハッシュがあっても再計算
        #[arg(long)]
        force: bool,
        /// MediaFilterのJSON文字列
        #[arg(long, default_value = "{}")]
        filter: String,
    },
    /// 特定作品のハッシュ一覧を表示
    List {
        /// 作品のUUID
        #[arg(long)]
        uuid: String,
    },
    /// SHA256ハッシュ値で検索
    Find {
        /// SHA256ハッシュ値（HEX文字列）
        #[arg(long)]
        hash: String,
    },
    /// 重複ハッシュを検出
    Duplicates,
}

#[derive(Subcommand)]
enum FileCommands {
    /// ファイル/ディレクトリを複製（dry-run ファースト。上書きは trash 経由）
    Cp {
        /// 複製元（media root からの相対パス）
        src: String,
        /// 複製先（media root からの相対パス）
        dst: String,
        /// 実際に変更を適用する（省略時は dry-run で計画のみ出力）
        #[arg(long)]
        apply: bool,
        /// DB の Media.path を追従させる（当面はフラグのみ受付）
        #[arg(long)]
        update_db: bool,
    },
    /// ファイル/ディレクトリを移動（dry-run ファースト。上書きは trash 経由）
    Mv {
        /// 移動元（media root からの相対パス）
        src: String,
        /// 移動先（media root からの相対パス）
        dst: String,
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        update_db: bool,
    },
    /// ディレクトリを同期（safe モード。余分・上書きは trash 経由）
    Sync {
        /// 同期元ディレクトリ（media root からの相対パス）
        src: String,
        /// 同期先ディレクトリ（media root からの相対パス）
        dst: String,
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        update_db: bool,
    },
}

#[derive(Subcommand)]
enum TrashCommands {
    /// ファイル/ディレクトリを trash へ移動（論理削除。物理削除は purge）
    Move {
        /// 対象パス（media root からの相対パス）
        target_rel: String,
        /// trash 行きの原因（delete / overwrite / sync_extra）。省略時は delete
        #[arg(long, value_enum, default_value_t = ClapTrashOperation::Delete)]
        operation: ClapTrashOperation,
        /// 任意の理由メモ
        #[arg(long)]
        reason: Option<String>,
    },
    /// trash エントリ一覧
    List,
    /// trash から元の位置へ復元（衝突時はエラー）
    Restore {
        /// trash エントリ ID
        id: String,
    },
    /// trash エントリを物理削除（dry-run ファースト）
    Purge {
        /// 削除対象 ID（複数指定可）。省略時は全エントリ
        #[arg(long = "id", value_name = "ID")]
        ids: Vec<String>,
        /// 物理削除せず結果を出力のみ
        #[arg(long)]
        dry_run: bool,
    },
}

/// TrashOperation の clap ValueEnum ラッパ（SDK 型に clap を混ぜないための thin 変換）。
/// serde 表現（snake_case）と一致させるため SyncExtra は #[value(name = "sync_extra")]。
#[derive(Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
enum ClapTrashOperation {
    Delete,
    Overwrite,
    #[value(name = "sync_extra")]
    SyncExtra,
}

impl From<ClapTrashOperation> for TrashOperation {
    fn from(op: ClapTrashOperation) -> Self {
        match op {
            ClapTrashOperation::Delete => TrashOperation::Delete,
            ClapTrashOperation::Overwrite => TrashOperation::Overwrite,
            ClapTrashOperation::SyncExtra => TrashOperation::SyncExtra,
        }
    }
}

async fn handle_server(ctx: &CliContext, port: u16, password: Option<String>) {
    match kijuku_db::KijukuDB::open_with_options(&ctx.db_path, DBOptions { verbose: ctx.verbose, ..Default::default() }) {
        Ok(db) => {
            if let Err(e) = db.migrate() {
                eprintln!("マイグレーションエラー: {}", e);
                return;
            }

            let options = kijuku_db::ServerOptions {
                port,
                password,
            };

            kijuku_db::start_server(db, options).await;
        }
        Err(e) => {
            eprintln!("データベースを開けませんでした: {}", e);
        }
    }
}

fn handle_backup_subcommand(ctx: &CliContext, label: Option<String>, timeout_ms: Option<u32>) {
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    match client.backup(label.as_deref(), timeout_ms) {
        Ok(Some(path)) => println!("{}", path),
        Ok(None) => eprintln!("バックアップマネージャーが設定されていません"),
        Err(e) => eprintln!("バックアップに失敗: {}", e),
    }
}

/// `import` サブコマンドハンドラ（TS `cli.ts:runImport` 相当）。
///
/// JSON/CSV/TSV ファイルを読み込み `ImportRecord` 群へ変換し、`DbClient::import_media` で
/// 一括登録（media + タグ関連付け + 追加属性）。結果は `CommandResponse` の1行JSONで出力する。
/// prod 読込経路（readonly）では書込不可のため事前に拒否する（設計 §5.1）。
fn handle_import_subcommand(
    ctx: &CliContext,
    file: String,
    additional_columns: Option<String>,
) {
    if ctx.readonly {
        output_response(&CommandResponse::error(
            "prod (readonly) では import できません。stg（既定）へ import してください".to_string(),
        ));
        return;
    }

    // additional_columns をカンマ区切りで分解（前後空白削除・空要素除外）
    let additional_cols: Vec<String> = additional_columns
        .map(|s| {
            s.split(',')
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let records = match parse_records(&file, &additional_cols) {
        Ok(r) => r,
        Err(e) => {
            output_response(&CommandResponse::error(e));
            return;
        }
    };

    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            output_response(&CommandResponse::error(e));
            return;
        }
    };
    let result = client.import_media(&records);
    output_response(&CommandResponse::from_result(result, "importエラー: "));
}

/// 入力ファイルを解析し `ImportRecord` 群を構築する。拡張子で .json/.csv/.tsv を自動判定し、
/// いずれも `Vec<HashMap<String,String>>` に正規化してから `build_import_record` へ渡す。
/// UTF-8 BOM が先頭にあれば除去する。
fn parse_records(file: &str, additional_cols: &[String]) -> Result<Vec<ImportRecord>, String> {
    let path = Path::new(file);
    if !path.exists() {
        return Err(format!("ファイルが見つかりません: {}", file));
    }
    let raw = fs::read_to_string(file).map_err(|e| format!("ファイルの読み込みに失敗: {}", e))?;
    let content = raw.strip_prefix('\u{feff}').unwrap_or(&raw);

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    let rows: Vec<HashMap<String, String>> = match ext.as_deref() {
        Some("json") => parse_json_records(content)?,
        Some("csv") => parse_delimited_records(content, b',')?,
        Some("tsv") => parse_delimited_records(content, b'\t')?,
        _ => {
            return Err(
                "サポートされていないファイル形式です（.json/.csv/.tsvのみ）".to_string(),
            )
        }
    };

    rows.into_iter()
        .map(|row| build_import_record(row, additional_cols))
        .collect()
}

/// JSON 配列を `Vec<HashMap<String,String>>` へ正規化。
/// `Null` は除外（TS `!== void 0` に合わせる）。`Bool` は "true"/"false"、数値は `to_string`、
/// 文字列はそのまま。配列/オブジェクト値は `to_string` される（追加カラムでのみ影響・通常スカラー）。
fn parse_json_records(content: &str) -> Result<Vec<HashMap<String, String>>, String> {
    let arr: Vec<serde_json::Value> =
        serde_json::from_str(content).map_err(|e| format!("JSONのパースに失敗: {}", e))?;
    arr.into_iter()
        .map(|v| {
            v.as_object()
                .ok_or_else(|| "JSONレコードがオブジェクトではありません".to_string())
                .map(json_value_to_map)
        })
        .collect()
}

/// JSON オブジェクト1件を文字列マップへ変換（Null はスキップ）。
fn json_value_to_map(obj: &serde_json::Map<String, serde_json::Value>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, val) in obj {
        if val.is_null() {
            continue;
        }
        let s = match val {
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        map.insert(k.clone(), s);
    }
    map
}

/// CSV/TSV（区切り文字指定）を `Vec<HashMap<String,String>>` へ正規化。
/// 先頭行をヘッダとし、各フィールドの前後空白を除去（`csv::Trim::All`）。
fn parse_delimited_records(
    content: &str,
    delimiter: u8,
) -> Result<Vec<HashMap<String, String>>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_reader(content.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| format!("ヘッダの読み込みに失敗: {}", e))?
        .iter()
        .map(|h| h.to_string())
        .collect::<Vec<_>>();

    let mut rows = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|e| format!("レコードの読み込みに失敗: {}", e))?;
        let mut map = HashMap::new();
        for (i, field) in record.iter().enumerate() {
            if let Some(key) = headers.get(i) {
                map.insert(key.clone(), field.to_string());
            }
        }
        rows.push(map);
    }
    Ok(rows)
}

/// 1レコード（文字列マップ）から `ImportRecord` を構築。
/// `title`/`media_type` は必須。`flag_exist` は "true"/"false"（大文字小文字無視）を bool へ。
/// 数値フィールドは空文字・省略を None 扱い。`tags` はカンマ区切りで分割、
/// `additional_cols` に列挙した列は `(key, value)` で属性として取り出す。
fn build_import_record(
    row: HashMap<String, String>,
    additional_cols: &[String],
) -> Result<ImportRecord, String> {
    let title = row
        .get("title")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or("title は必須です")?;
    let media_type_str = row
        .get("media_type")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or("media_type は必須です")?;
    let media_type = MediaType::from_str(&media_type_str).ok_or_else(|| {
        format!(
            "不正な media_type です: {}（comic/video/music のいずれか）",
            media_type_str
        )
    })?;

    let flag_exist = row.get("flag_exist").map(|s| s.to_uppercase() == "TRUE");

    let media = MediaInput {
        title,
        media_type,
        uuid: row.get("uuid").cloned(),
        title_id: row.get("title_id").cloned(),
        path: row.get("path").cloned(),
        thumbnail_path: row.get("thumbnail_path").cloned(),
        artist: row.get("artist").cloned(),
        artist_id: row.get("artist_id").cloned(),
        description: row.get("description").cloned(),
        file_size: parse_opt_int(row.get("file_size"), "file_size")?,
        duration_sec: parse_opt_int(row.get("duration_sec"), "duration_sec")?,
        page_count: parse_opt_int(row.get("page_count"), "page_count")?,
        series: row.get("series").cloned(),
        volume_number: None, // 手動設定は無視（volume_text から自動計算）
        volume_text: row.get("volume_text").cloned(),
        volume_title: row.get("volume_title").cloned(),
        magazine: row.get("magazine").cloned(),
        magazine_id: row.get("magazine_id").cloned(),
        language: row.get("language").cloned(),
        source: row.get("source").cloned(),
        external_id: row.get("external_id").cloned(),
        artist_en: row.get("artist_en").cloned(),
        title_en: row.get("title_en").cloned(),
        chapters: row.get("chapters").cloned(),
        extension: row.get("extension").cloned(),
        flag_exist,
        title_pron: row.get("title_pron").cloned(),
        artist_pron: row.get("artist_pron").cloned(),
        series_pron: row.get("series_pron").cloned(),
    };

    let tags = row
        .get("tags")
        .map(|s| {
            s.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let attributes = additional_cols
        .iter()
        .filter_map(|c| row.get(c).map(|v| (c.clone(), v.clone())))
        .collect();

    Ok(ImportRecord {
        media,
        tags,
        attributes,
    })
}

/// 文字列の Option を整数へパース（空文字・省略は None）。パース失敗はエラー。
fn parse_opt_int<T: std::str::FromStr>(
    v: Option<&String>,
    field: &str,
) -> Result<Option<T>, String>
where
    T::Err: std::fmt::Display,
{
    match v {
        Some(s) if !s.is_empty() => s
            .parse::<T>()
            .map(Some)
            .map_err(|e| format!("{} のパースに失敗 ({}): {}", field, s, e)),
        _ => Ok(None),
    }
}

fn handle_list_backups_subcommand(ctx: &CliContext) {
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    match client.list_backups() {
        Ok(backups) => {
            if backups.is_empty() {
                println!("バックアップはありません");
                return;
            }
            for (i, info) in backups.iter().enumerate() {
                let scope = match info.scope {
                    BackupScope::Auto => "auto",
                    BackupScope::Manual => "manual",
                    BackupScope::Tmp => "tmp",
                };
                let label = info.label.as_deref().unwrap_or("-");
                println!("[{}] {} scope={} label={}", i, info.name, scope, label);
            }
        }
        Err(e) => eprintln!("バックアップ一覧の取得に失敗: {}", e),
    }
}

/// pre-stash（即時復旧用ロールバックファイル）一覧を表示（設計 §8）。
/// promote/(b)操作が返す pre_stash_path を失った場合の発見経路。path は byPath restore に直接渡せる。
fn handle_list_pre_stashes_subcommand(ctx: &CliContext) {
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    match client.list_pre_stashes() {
        Ok(stashes) => {
            if stashes.is_empty() {
                println!("pre-stash（即時復旧用ロールバックファイル）はありません");
                return;
            }
            for (i, info) in stashes.iter().enumerate() {
                // path は byPath restore に直接渡す戻し先（設計 §8 即時復旧）。
                println!("[{}] {} path={}", i, info.name, info.path.display());
            }
        }
        Err(e) => eprintln!("pre-stash 一覧の取得に失敗: {}", e),
    }
}

fn handle_restore_subcommand(ctx: &CliContext, nth: Option<usize>, id: Option<String>, timeout_ms: Option<u32>) {
    let mut client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    let selector = if let Some(id) = id {
        BackupSelector::by_id(id)
    } else {
        match nth {
            Some(n) => BackupSelector::nth(n),
            None => BackupSelector::latest(),
        }
    };
    match client.restore(&selector, timeout_ms) {
        Ok(path) => println!("{}", path),
        Err(e) => eprintln!("復元に失敗: {}", e),
    }
}

/// sync 用に prod/stg 両パスを解決する（設計 §4.2）。
///
/// `from`/`to` の両方が指定されていればそれらを使い、欠落分は環境変数
/// （`KIJUKU_DB_PATH` / `KIJUKU_STG_DB_PATH`）またはデフォルトで補完する。
fn resolve_sync_paths(from: Option<String>, to: Option<String>) -> (String, String) {
    match (from, to) {
        (Some(f), Some(t)) => (f, t),
        (f, t) => {
            let (default_prod, default_stg) = resolve_prod_and_stg_paths(&SystemEnv);
            (f.unwrap_or(default_prod), t.unwrap_or(default_stg))
        }
    }
}

/// `sync-db` サブコマンド: prod(RO)→stg(RW) のフル複製（設計 §4.2）。
///
/// ローカル: stg 接続を開かず `KijukuDB::replicate_db` でファイルコピー（`--from`/`--to` または
/// 環境変数で両パス解決）。リモート（`--db host:path`）: `RemoteKijukuDB::sync` でリモート側の
/// prod/stg パス設定によりサーバ側コピー（`--from`/`--to` は無視・警告）。
fn handle_sync_subcommand(ctx: &CliContext, from: Option<String>, to: Option<String>, timeout_ms: Option<u32>) {
    let parsed = parse_db_path(&ctx.db_path);
    if parsed.is_remote {
        if from.is_some() || to.is_some() {
            eprintln!(
                "警告: リモート sync では --from/--to は無視されます（リモート側 prod/stg パス設定を使用）"
            );
        }
        let config = remote_config_from_ctx(ctx, &parsed);
        let remote = RemoteKijukuDB::new(config);
        match remote.sync(timeout_ms) {
            Ok(r) => println!("sync 完了: {} -> {}", r.prod_path, r.stg_path),
            Err(e) => eprintln!("sync に失敗: {}", e),
        }
    } else {
        let (prod, stg) = resolve_sync_paths(from, to);
        match KijukuDB::replicate_db(Path::new(&prod), Path::new(&stg)) {
            Ok(()) => println!("sync 完了: {} -> {}", prod, stg),
            Err(e) => eprintln!("sync に失敗: {}", e),
        }
    }
}

/// discard（stg 破棄・再 sync・設計 §4.6）。処理は `sync-db` と同一（prod→stg の `replicate_db`・
/// 既存 stg は上書き破棄）。リモートは `RemoteKijukuDB::discard`（operation:"discard"）を呼ぶ。
/// ローカル: stg 接続を開かず `KijukuDB::replicate_db` でファイルコピー。リモート: サーバ側コピー。
fn handle_discard_subcommand(ctx: &CliContext, from: Option<String>, to: Option<String>, timeout_ms: Option<u32>) {
    let parsed = parse_db_path(&ctx.db_path);
    if parsed.is_remote {
        if from.is_some() || to.is_some() {
            eprintln!(
                "警告: リモート discard では --from/--to は無視されます（リモート側 prod/stg パス設定を使用）"
            );
        }
        let config = remote_config_from_ctx(ctx, &parsed);
        let remote = RemoteKijukuDB::new(config);
        match remote.discard(timeout_ms) {
            Ok(r) => println!("discard 完了: {} -> {}", r.prod_path, r.stg_path),
            Err(e) => eprintln!("discard に失敗: {}", e),
        }
    } else {
        let (prod, stg) = resolve_sync_paths(from, to);
        match KijukuDB::replicate_db(Path::new(&prod), Path::new(&stg)) {
            Ok(()) => println!("discard 完了: {} -> {}", prod, stg),
            Err(e) => eprintln!("discard に失敗: {}", e),
        }
    }
}

fn handle_diff_backup_subcommand(
    ctx: &CliContext,
    nth: Option<usize>,
    id: Option<String>,
    detail: String,
    timeout_ms: Option<u32>,
) {
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    let selector = if let Some(id) = id {
        BackupSelector::by_id(id)
    } else {
        match nth {
            Some(n) => BackupSelector::nth(n),
            None => BackupSelector::latest(),
        }
    };
    let detail = match parse_diff_detail(&detail) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    let options = kijuku_db::diff::DiffOptions {
        detail: Some(detail),
    };
    match client.diff_with_backup(&selector, &options, timeout_ms) {
        Ok(diff) => print_backup_diff_summary(&diff),
        Err(e) => eprintln!("差分の取得に失敗: {}", e),
    }
}

/// `diff-prod-stg` サブコマンド: prod(RO) と stg(現在DB) の差分（promote 判断用・設計 §4.4）。
///
/// stg 編集視点（added=stg新規=promoteでprod追加 等）で表示。`--prompt` は LLM explanation
/// prompt を stdout に出力（コピペ可能・他出力抑制）。`--summarize` はテーブル別分布を追加表示。
fn handle_diff_prod_stg_subcommand(
    ctx: &CliContext,
    prod: Option<String>,
    detail: String,
    summarize: bool,
    prompt: bool,
    timeout_ms: Option<u32>,
) {
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    let detail_enum = match parse_diff_detail(&detail) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    let options = kijuku_db::diff::DiffOptions {
        detail: Some(detail_enum),
    };
    let (prod_path, _stg) = resolve_sync_paths(prod, None);
    match client.diff_with_prod(Some(&prod_path), &options, timeout_ms) {
        Ok(diff) => {
            let summary = kijuku_db::diff::summarize_diff(&diff);
            if prompt {
                let p = kijuku_db::diff::build_diff_explanation_prompt(
                    &diff,
                    &summary,
                    &kijuku_db::diff::DiffExplanationPromptOptions::default(),
                );
                print!("{}", p);
            } else {
                print_prod_stg_diff_summary(&diff, &summary, summarize);
            }
        }
        Err(e) => eprintln!("prod/stg 差分の取得に失敗: {}", e),
    }
}

/// `observe` サブコマンド: 機械的 promote gate を評価し結果を表示（設計 §3.4/§4.4・TASK-54）。
fn handle_observe_subcommand(
    ctx: &CliContext,
    prod: Option<String>,
    detail: String,
    max_added: Option<usize>,
    max_removed: Option<usize>,
    max_changed: Option<usize>,
    json: bool,
    timeout_ms: Option<u32>,
) {
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    let detail_enum = match parse_diff_detail(&detail) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    let mut options = kijuku_db::diff::ObserveOptions {
        diff_options: kijuku_db::diff::DiffOptions {
            detail: Some(detail_enum),
        },
        gate_config: kijuku_db::diff::GateConfig::default(),
    };
    if let Some(m) = max_added {
        options.gate_config.max_added = m;
    }
    if let Some(m) = max_removed {
        options.gate_config.max_removed = m;
    }
    if let Some(m) = max_changed {
        options.gate_config.max_changed = m;
    }
    let (prod_path, _stg) = resolve_sync_paths(prod, None);
    match client.observe(Some(&prod_path), &options, timeout_ms) {
        Ok(result) => {
            if json {
                match serde_json::to_string_pretty(&result) {
                    Ok(s) => print!("{}", s),
                    Err(e) => eprintln!("シリアライズエラー: {}", e),
                }
            } else {
                print_observe_result(&result);
            }
        }
        Err(e) => eprintln!("observe（promote gate）の評価に失敗: {}", e),
    }
}

/// observe 結果の人間向け表示
fn print_observe_result(result: &kijuku_db::diff::ObserveResult) {
    if result.passed {
        println!("PROMOTE GATE: PASS（全検査合格・promote 可能）");
    } else {
        println!("PROMOTE GATE: FAIL（不合格の検査あり・promote 不可）");
    }
    println!(
        "  schema_version: prod={} stg={}",
        result.prod_schema_version, result.stg_schema_version
    );
    let t = &result.summary.totals;
    println!("  差分合計: +{} -{} ~{}", t.added, t.removed, t.changed);
    println!("  gate 検査:");
    for c in &result.checks {
        let mark = if c.passed { "OK" } else { "NG" };
        println!("    [{}] {}: {}", mark, c.name, c.detail);
    }
}

/// `--detail` 文字列を DiffDetail に変換（summary / limited=N / full）
/// 無効値はエラーを返し、デフォルトへのサイレントフォールバックを防ぐ。
fn parse_diff_detail(s: &str) -> Result<kijuku_db::diff::DiffDetail, String> {
    match s {
        "summary" => Ok(kijuku_db::diff::DiffDetail::SummaryOnly),
        "full" => Ok(kijuku_db::diff::DiffDetail::Full),
        _ => {
            if let Some(n) = s.strip_prefix("limited=") {
                n.parse::<usize>()
                    .map(|n| kijuku_db::diff::DiffDetail::Limited { n })
                    .map_err(|_| format!("無効な limited 値です: {} (数値を指定してください)", n))
            } else {
                Err(format!(
                    "無効な --detail 値です: {} (summary | limited=N | full のいずれかを指定してください)",
                    s
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests_parse_diff_detail {
    use super::*;

    #[test]
    fn parses_summary() {
        assert!(matches!(
            parse_diff_detail("summary"),
            Ok(kijuku_db::diff::DiffDetail::SummaryOnly)
        ));
    }

    #[test]
    fn parses_full() {
        assert!(matches!(
            parse_diff_detail("full"),
            Ok(kijuku_db::diff::DiffDetail::Full)
        ));
    }

    #[test]
    fn parses_limited_n() {
        assert!(matches!(
            parse_diff_detail("limited=50"),
            Ok(kijuku_db::diff::DiffDetail::Limited { n: 50 })
        ));
    }

    #[test]
    fn rejects_unknown_value() {
        assert!(parse_diff_detail("invalid").is_err());
    }

    #[test]
    fn rejects_non_numeric_limited() {
        assert!(parse_diff_detail("limited=abc").is_err());
    }
}

#[cfg(test)]
mod tests_classify_operation {
    use super::*;

    #[test]
    fn classifies_b_restricted_ops() {
        // (b) stg 制限操作（設計 §9.2・破壊的/全床上書き）。
        for op in &["mediaMv", "purgeTrash", "restore"] {
            assert_eq!(
                classify_operation(op),
                OpClass::StgRestricted,
                "{} は (b) StgRestricted のべき",
                op
            );
        }
    }

    #[test]
    fn classifies_a_permitted_ops() {
        // (a) stg 許可操作（設計 §9.1）。代表操作で (a) 帰着を検証。
        for op in &[
            "createMedia",
            "updateMedia",
            "deleteMedia",
            "bulkCreateMedia",
            "createTag",
            "addTagToMedia",
            "setMediaAttribute",
            "addMediaHash",
            "computeMediaHash",
            "updateExist",
            "updateThumbnail",
            "backup",
            "setBackupLabel",
            "mediaCp",
            "mediaSync",
            "moveToTrash",
            "restoreFromTrash",
            "promote",
            // migrate は P3 まで現状維持（StgPermitted・TODO-P3/TASK-45）
            "migrate",
        ] {
            assert_eq!(
                classify_operation(op),
                OpClass::StgPermitted,
                "{} は (a) StgPermitted のべき",
                op
            );
        }
    }

    #[test]
    fn classifies_read_ops() {
        // 読込操作は Read。
        for op in &[
            "getMedia",
            "findMedia",
            "getAllTags",
            "getMediaAttributes",
            "getMediaHash",
            "listBackups",
            "diffBackup",
            "diffProdStg",
            "observe",
            "getSchemaVersion",
            "listTrash",
        ] {
            assert_eq!(classify_operation(op), OpClass::Read, "{} は Read のべき", op);
        }
    }

    #[test]
    fn unknown_op_is_read() {
        // 未知操作は Read（既存挙動: execute_command が最終的に「不明な操作」エラー）。
        assert_eq!(classify_operation("__unknown__"), OpClass::Read);
    }
}

fn print_backup_diff_summary(diff: &kijuku_db::diff::BackupDiff) {
    let s = &diff.summary;
    println!(
        "media:      +{} -{} ~{}",
        s.media.added, s.media.removed, s.media.changed
    );
    println!(
        "tags:       +{} -{} ~{}",
        s.tags.added, s.tags.removed, s.tags.changed
    );
    println!(
        "media_tags: +{} -{}",
        s.media_tags.added, s.media_tags.removed
    );
    println!(
        "attributes: +{} -{} ~{}",
        s.attributes.added, s.attributes.removed, s.attributes.changed
    );
    println!(
        "hashes:     +{} -{} ~{}",
        s.hashes.added, s.hashes.removed, s.hashes.changed
    );
    if !diff.media.added.is_empty() {
        println!("  [media added]");
        for m in diff.media.added.iter().take(10) {
            println!("    + id={} {}", m.id, m.title);
        }
    }
    if !diff.media.removed.is_empty() {
        println!("  [media removed]");
        for m in diff.media.removed.iter().take(10) {
            println!("    - id={} {}", m.id, m.title);
        }
    }
}

/// prod/stg 差分の人間可読サマリ（stg 編集視点・設計 §4.4・TASK-53）。
///
/// `show_distribution` が true ならテーブル別の偏り（分布）も表示する。
fn print_prod_stg_diff_summary(
    diff: &kijuku_db::diff::BackupDiff,
    summary: &kijuku_db::diff::ProdStgDiffSummary,
    show_distribution: bool,
) {
    let c = &summary.counts;
    println!("prod/stg 差分（stg 編集視点: promote で prod に反映される内容）:");
    println!(
        "  media:      +{} -{} ~{}  (added=stg新規 / removed=prod削除 / changed=上書き)",
        c.media.added, c.media.removed, c.media.changed
    );
    println!("  tags:       +{} -{} ~{}", c.tags.added, c.tags.removed, c.tags.changed);
    println!("  media_tags: +{} -{}", c.media_tags.added, c.media_tags.removed);
    println!(
        "  attributes: +{} -{} ~{}",
        c.attributes.added, c.attributes.removed, c.attributes.changed
    );
    println!("  hashes:     +{} -{} ~{}", c.hashes.added, c.hashes.removed, c.hashes.changed);
    println!(
        "  合計:       +{} -{} ~{}",
        summary.totals.added, summary.totals.removed, summary.totals.changed
    );

    if !diff.media.added.is_empty() {
        println!("  [media added: stg 新規 = promote で prod に追加]");
        for m in diff.media.added.iter().take(10) {
            println!("    + id={} {}", m.id, m.title);
        }
    }
    if !diff.media.removed.is_empty() {
        println!("  [media removed: prod のみ = promote で prod から削除]");
        for m in diff.media.removed.iter().take(10) {
            println!("    - id={} {}", m.id, m.title);
        }
    }
    if !diff.media.changed.is_empty() {
        println!("  [media changed: 両方で異なる = promote で prod が上書き]");
        for ch in diff.media.changed.iter().take(10) {
            println!("    ~ id={} {} -> {}", ch.current.id, ch.current.title, ch.backup.title);
        }
    }

    if show_distribution {
        println!("  [分布（テーブル別の偏り）]");
        for (k, v) in &summary.distribution.media.by_type {
            println!("    media type={}: +{} -{} ~{}", k, v.added, v.removed, v.changed);
        }
        for (k, v) in &summary.distribution.media.by_artist_top {
            println!("    media artist={}: +{} -{} ~{}", k, v.added, v.removed, v.changed);
        }
        for (k, v) in &summary.distribution.media.by_flag_exist {
            println!("    media flag_exist={}: +{} -{} ~{}", k, v.added, v.removed, v.changed);
        }
        for (k, v) in &summary.distribution.attributes.by_key {
            println!("    attr key={}: +{} -{} ~{}", k, v.added, v.removed, v.changed);
        }
        for (k, v) in &summary.distribution.media_tags.by_tag_top {
            println!("    media_tags tag_id={}: +{} -{}", k, v.added, v.removed);
        }
        for (k, v) in &summary.distribution.hashes.by_filename_top {
            println!("    hashes filename={}: +{} -{} ~{}", k, v.added, v.removed, v.changed);
        }
    }
}

/// 操作の階層化分類（設計 §9・TASK-59 P2-C4）。
///
/// - `Read`: DB 変更なし（get*/find*/list*/diff*/observe 等）。prod RO 読込・stg 両方で許可。
/// - `StgPermitted`((a)): stg で許可・promote で prod へ反映（CRUD/tag/attr/hash/upload・
///   cp/sync/trash系可逆操作）。prod 直接実行は拒否（stg 経由・promote が唯一の prod 反映経路）。
/// - `StgRestricted`((b)): stg では環境が制限（拒否）。prod 直接でのみ dry-run + trash + pre-stash
///   のシステム gate を強制して実行（設計 §3.2/§5.2/§9.2）。
///
/// `migrate` は設計 §15-5 で「stg 経由詳細→P3」とされ TASK-45（P3「破壊操作(b)扱い」）の対象のため
/// 本タスクでは `StgPermitted`（現状維持: 接続時 auto-migrate + pre_migrate_snapshot）。正式 (b)
/// gate 化は P3（TODO-P3）。`promote`/`sync` は専用 gate/経路を持つ特殊操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpClass {
    Read,
    StgPermitted,
    StgRestricted,
}

/// 操作名 → 階層化分類（設計 §9）。分類は破壊度基準: 可逆/追加的操作=(a)、不可逆/全床=(b)。
fn classify_operation(op: &str) -> OpClass {
    // (b) stg 制限操作（設計 §9.2・prod 直接 gate 必須）。
    if matches!(op, "mediaMv" | "purgeTrash" | "restore") {
        return OpClass::StgRestricted;
    }
    // (a) stg 許可操作（設計 §9.1）= readonly で拒否すべき書込。promote は stg 起点の専用 gate 操作。
    if matches!(
        op,
        "migrate"
            | "createMedia"
            | "updateMedia"
            | "deleteMedia"
            | "bulkCreateMedia"
            | "bulkDeleteMedia"
            | "bulkUpdateMedia"
            | "createTag"
            | "addTagToMedia"
            | "removeTagFromMedia"
            | "setMediaAttribute"
            | "deleteMediaAttribute"
            | "deleteAllMediaAttributes"
            | "updateExist"
            | "checkThumbnail"
            | "updateThumbnail"
            | "addMediaHash"
            | "addMediaHashes"
            | "deleteMediaHash"
            | "deleteMediaHashes"
            | "computeMediaHash"
            | "computeMediaHashes"
            | "backup"
            | "promote"
            | "setBackupLabel"
            | "setBackupNote"
            | "mediaCp"
            | "mediaSync"
            | "moveToTrash"
            | "restoreFromTrash"
    ) {
        return OpClass::StgPermitted;
    }
    OpClass::Read
}

async fn handle_stdin(ctx: &CliContext) {
    // 標準入力からJSONコマンドを読み取る
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        let response = CommandResponse::error(format!("標準入力の読み取りに失敗: {}", e));
        output_response(&response);
        return;
    }

    // JSONをパース
    let request: CommandRequest = match serde_json::from_str(&input) {
        Ok(req) => req,
        Err(e) => {
            let response = CommandResponse::error(format!("JSONパースエラー: {}", e));
            output_response(&response);
            return;
        }
    };

    // sync/discard は prod→stg ファイルコピーで stg を未接続前提とするため、backend（stg 接続）を
    // 開かずに処理する（接続中の stg 上書きによる WAL 破損を回避・設計 §4.2/§5.3）。
    // discard は stg 破棄・再 sync（§4.6）で処理は sync と同一。操作名のみ監査（§10）で区別。
    if request.operation == "sync" || request.operation == "discard" {
        let response = handle_sync(&request.params).await;
        output_response(&response);
        return;
    }

    // 操作階層化 gate（設計 §9・TASK-59 P2-C4）。分類に基づき stg/prod で実行可否を振り分け。
    let class = classify_operation(&request.operation);
    if ctx.readonly {
        // prod 読込経路（readonly・設計 §5.1）。
        match class {
            OpClass::StgRestricted => {
                // (b) 操作の prod 直接実行経路（設計 §3.2/§5.2/§9.2）。
                // readonly backend を使わず ProdRwScope で prod RW を内部取得し dry-run+trash+pre-stash gate を強制。
                let response = handle_b_operation_prod(ctx, &request).await;
                output_response(&response);
                return;
            }
            OpClass::StgPermitted => {
                // (a) 操作の prod 直接実行は拒否。prod 書込は promote((a)反映)/(b)gate 経路のみ。
                let response = CommandResponse::error(format!(
                    "読込専用セッション（--target prod readonly）では書込操作 '{}' は実行できません。(a) 操作は stg セッション（--target stg）で実行し promote で prod へ反映してください（設計 §3.2/§5.1）",
                    request.operation
                ));
                output_response(&response);
                return;
            }
            OpClass::Read => {} // Read は下部の readonly backend で処理。
        }
    } else if class == OpClass::StgRestricted {
        // stg 書込セッションで (b) は制限（拒否）・prod 直接 gate 経路へ誘導（設計 §9.2）。
        let response = CommandResponse::error(format!(
            "(b) 制限操作 '{}' は stg では実行できません。prod 直接 `--target prod` で dry-run+trash+pre-stash gate 付きで実行してください（設計 §9.2）",
            request.operation
        ));
        output_response(&response);
        return;
    }

    // バックエンドを構築（Local / D1）
    let mut backend = match build_backend(ctx) {
        Ok(b) => b,
        Err(resp) => {
            output_response(&resp);
            return;
        }
    };

    // マイグレーションを実行（Local は schema.sql、D1 は schema.d1.sql が自動選択される）。
    // prod 読込経路（readonly）では skip（設計 §5.1）。
    if ctx.should_migrate {
        if let Err(e) = backend.as_backend().migrate().await {
            let response = CommandResponse::error(format!("マイグレーションに失敗: {}", e));
            output_response(&response);
            return;
        }
    }

    // コマンドを実行
    let response = execute_command(&mut backend, &request).await;
    output_response(&response);
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    // target / db_path 解決（CLI 引数 > 環境変数 > デフォルト stg・設計 §13）
    let resolution = resolve_target(
        cli.target.as_deref().and_then(Target::parse),
        cli.db.as_deref(),
        cli.read_source.as_deref().and_then(Target::parse),
        &SystemEnv,
    );
    let ctx = CliContext {
        db_path: resolution.db_path,
        verbose: cli.verbose,
        backend: cli.backend.clone(),
        media_root: cli.media_root.clone(),
        target: resolution.target,
        readonly: resolution.readonly,
        should_migrate: resolution.should_migrate,
    };

    // remote（--db host:path）は local DB を直接開くモードでは利用不可（TASK-64 Gap e）。
    // stdin プロトコル(サブコマンド無し)/server はローカル DB を開く RPC サーバモード、
    // bulk-load はローカル SQLite を source として開くため、いずれも host:path は無意味。
    // docs は DB を触らないため対象外。リモート RPC のサーバ側は常にローカル --db で起動する。
    let is_remote_db = parse_db_path(&ctx.db_path).is_remote;
    if is_remote_db
        && matches!(
            &cli.command,
            None | Some(Commands::Server { .. }) | Some(Commands::BulkLoad { .. })
        )
    {
        eprintln!(
            "エラー: --db host:path（リモート）では stdin プロトコル/server/bulk-load は利用できません（これらはローカルDBを直接開くモードです）"
        );
        std::process::exit(1);
    }

    // バックエンド種別とサブコマンドの整合性チェック
    match (&ctx.backend, &cli.command) {
        // D1 バックエンド: stdin プロトコル・Docs・BulkLoad のみ対応
        (BackendKind::D1, None)
        | (BackendKind::D1, Some(Commands::Docs { .. }))
        | (BackendKind::D1, Some(Commands::BulkLoad { .. })) => {}
        (BackendKind::D1, _) => {
            eprintln!(
                "エラー: --backend d1 では docs・bulk-load・stdin プロトコルのみ利用可能です（file/trash を含む他のサブコマンドは local バックエンド専用）"
            );
            std::process::exit(1);
        }
        // Local バックエンド: bulk-load は不可（D1 宛先が必要）
        (BackendKind::Local, Some(Commands::BulkLoad { .. })) => {
            eprintln!(
                "エラー: bulk-load には --backend d1 が必要です（ローカル→D1 への移行ツール）"
            );
            std::process::exit(1);
        }
        _ => {}
    }

    match &cli.command {
        Some(Commands::Docs { doc_type }) => {
            show_docs(doc_type);
        }
        Some(Commands::Server { port, password }) => {
            handle_server(&ctx, *port, password.clone()).await;
        }
        Some(Commands::UpdateExist { dry_run, filter }) => {
            handle_update_exist_subcommand(&ctx, *dry_run, filter);
        }
        Some(Commands::CheckThumbnail { filter }) => {
            handle_check_thumbnail_subcommand(&ctx, filter);
        }
        Some(Commands::UpdateThumbnail { dry_run, force, filter }) => {
            handle_update_thumbnail_subcommand(&ctx, *dry_run, *force, filter);
        }
        Some(Commands::Backup { label, timeout_ms }) => {
            handle_backup_subcommand(&ctx, label.clone(), *timeout_ms);
        }
        Some(Commands::ListBackups) => {
            handle_list_backups_subcommand(&ctx);
        }
        Some(Commands::ListPreStashes) => {
            handle_list_pre_stashes_subcommand(&ctx);
        }
        Some(Commands::Restore { nth, id, timeout_ms }) => {
            handle_restore_subcommand(&ctx, *nth, id.clone(), *timeout_ms);
        }
        Some(Commands::DiffBackup { nth, id, detail, timeout_ms }) => {
            handle_diff_backup_subcommand(&ctx, *nth, id.clone(), detail.clone(), *timeout_ms);
        }
        Some(Commands::DiffProdStg { prod, detail, summarize, prompt, timeout_ms }) => {
            handle_diff_prod_stg_subcommand(
                &ctx,
                prod.clone(),
                detail.clone(),
                *summarize,
                *prompt,
                *timeout_ms,
            );
        }
        Some(Commands::Observe {
            prod,
            detail,
            max_added,
            max_removed,
            max_changed,
            json,
            timeout_ms,
        }) => {
            handle_observe_subcommand(
                &ctx,
                prod.clone(),
                detail.clone(),
                *max_added,
                *max_removed,
                *max_changed,
                *json,
                *timeout_ms,
            );
        }
        Some(Commands::SetBackupLabel { id, label }) => {
            handle_set_backup_label_subcommand(&ctx, id.clone(), label.clone());
        }
        Some(Commands::SetBackupNote { id, note }) => {
            handle_set_backup_note_subcommand(&ctx, id.clone(), note.clone());
        }
        Some(Commands::SyncDb { from, to, timeout_ms }) => {
            handle_sync_subcommand(&ctx, from.clone(), to.clone(), *timeout_ms);
        }
        Some(Commands::DiscardDb { from, to, timeout_ms }) => {
            handle_discard_subcommand(&ctx, from.clone(), to.clone(), *timeout_ms);
        }
        Some(Commands::Hash { hash_command }) => {
            handle_hash_subcommand(&ctx, hash_command);
        }
        Some(Commands::BulkLoad { chunk_size, verify_only }) => {
            handle_bulk_load_subcommand(&ctx, *chunk_size, *verify_only).await;
        }
        Some(Commands::File { file_command }) => {
            handle_file_subcommand(&ctx, file_command);
        }
        Some(Commands::Trash { trash_command }) => {
            handle_trash_subcommand(&ctx, trash_command);
        }
        Some(Commands::Import {
            file,
            additional_columns,
        }) => {
            handle_import_subcommand(&ctx, file.clone(), additional_columns.clone());
        }
        None => {
            handle_stdin(&ctx).await;
        }
    }
}

async fn handle_backup(db: &mut KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: BackupParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    let result = if let Some(label) = &params.label {
        db.backup_with_label(label)
    } else {
        db.backup()
    };
    match result {
        Ok(Some(path)) => CommandResponse::success(serde_json::json!({"path": path})),
        Ok(None) => CommandResponse::error("バックアップマネージャーが設定されていません".to_string()),
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

async fn handle_list_backups(db: &KijukuDB) -> CommandResponse {
    match db.list_backups() {
        Ok(backups) => {
            let json_backups: Vec<serde_json::Value> =
                backups.iter().map(backup_info_to_json).collect();
            CommandResponse::success(serde_json::Value::Array(json_backups))
        }
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

/// pre-stash（即時復旧用ロールバックファイル）一覧（設計 §8）。backup_info_to_json を再利用。
async fn handle_list_pre_stashes(db: &KijukuDB) -> CommandResponse {
    match db.list_pre_stashes() {
        Ok(stashes) => {
            let json_stashes: Vec<serde_json::Value> =
                stashes.iter().map(backup_info_to_json).collect();
            CommandResponse::success(serde_json::Value::Array(json_stashes))
        }
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

async fn handle_restore(db: &mut KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: RestoreParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    let selector = params
        .selector
        .as_ref()
        .map(|s| s.to_selector())
        .unwrap_or_else(BackupSelector::latest);
    // dry-run（(b) gate・設計 §9.2/§15-13）: 復元で変化する差分（prod 現状 vs バックアップ）を返し prod 不変。
    if params.dry_run {
        return match db.diff_with_backup(&selector, &kijuku_db::diff::DiffOptions::default()) {
            Ok(diff) => match serde_json::to_value(&diff) {
                Ok(v) => CommandResponse::success(serde_json::json!({"dryRun": true, "diff": v})),
                Err(e) => CommandResponse::error(format!("シリアライズエラー: {}", e)),
            },
            Err(e) => CommandResponse::error(e.to_string()),
        };
    }
    match db.restore(&selector) {
        Ok(path) => CommandResponse::success(serde_json::json!({"path": path.to_string_lossy()})),
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

/// (b) 管理操作の prod 直接実行経路（設計 §3.2/§5.2/§9.2・TASK-59 P2-C4）。
///
/// `--target prod`（readonly）で (b) 操作（mediaMv/purgeTrash/restore）が起票された際の経路。
/// readonly backend を使わず `ProdRwScope` で prod の排他ロック + pre-stash を取得した上で
/// prod RW の `KijukuDB` を一時オープン（migrate skip・恒久接続でない）し、dry-run + trash +
/// pre-stash の gate を強制して実行する。
/// - FS 系（mediaMv/purgeTrash）: dry-run ファースト・上書き/削除は trash 経由（file_ops/trash に既存）。
/// - restore（DB 層）: ProdRwScope の pre-stash が §8 即時巻き戻しを担保。dryRun で差分プレビュー。
async fn handle_b_operation_prod(ctx: &CliContext, request: &CommandRequest) -> CommandResponse {
    // ProdRwScope: prod 排他ロック + pre-stash 強制（設計 §5.2/§7.2）。別セッションが promote 中なら ProdBusy。
    let _scope = match kijuku_db::prod_rw::ProdRwScope::acquire(Path::new(&ctx.db_path), None) {
        Ok(s) => s,
        Err(e) => {
            return CommandResponse::error(format!(
                "(b)操作の prod RW スコープ取得に失敗しました（別セッションが promote/(b) 実行中の可能性）: {}",
                e
            ))
        }
    };

    // prod RW の KijukuDB を一時オープン（設計 §5.1 で prod は migrate skip・readonly=false）。
    let mut db = match KijukuDB::open_with_options(
        &ctx.db_path,
        DBOptions {
            backup: Some(BackupOptions::default()),
            verbose: ctx.verbose,
            readonly: false,
            media_root: ctx.media_root.clone(),
            ..Default::default()
        },
    ) {
        Ok(db) => db,
        Err(e) => return CommandResponse::error(format!("prod のオープンに失敗: {}", e)),
    };

    // TODO-P4(TASK-46): (b)操作の監査ログ記録（操作・対象・pre-stash path・実行者）をここに追加。
    // 既存ハンドラへディスパッチ（gate は各操作に内包）。
    match request.operation.as_str() {
        "restore" => handle_restore(&mut db, &request.params).await,
        "mediaMv" => handle_media_mv(&db, &request.params).await,
        "purgeTrash" => handle_purge_trash(&db, &request.params).await,
        other => CommandResponse::error(format!("(b)操作 '{}' は prod 直接経路で未サポート", other)),
    }
}

/// `sync` 操作（prod→stg フル複製・設計 §4.2）。stg 接続を開かずファイルコピーする。
///
/// params の `from`/`to` で prod/stg パスを明示（リモート SSH 経由で両パスを渡す）。
/// 欠落時は環境変数/デフォルトから解決する。
async fn handle_sync(params: &serde_json::Value) -> CommandResponse {
    let params: SyncParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    let (prod, stg) = resolve_sync_paths(params.from, params.to);
    match KijukuDB::replicate_db(Path::new(&prod), Path::new(&stg)) {
        Ok(()) => CommandResponse::success(serde_json::json!({
            "prodPath": prod,
            "stgPath": stg,
        })),
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

async fn handle_diff_backup(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: DiffBackupParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    let selector = params
        .selector
        .as_ref()
        .map(|s| s.to_selector())
        .unwrap_or_else(BackupSelector::latest);
    let options = params.options.map(|o| o.to_options()).unwrap_or_default();
    match db.diff_with_backup(&selector, &options) {
        Ok(diff) => match serde_json::to_value(&diff) {
            Ok(v) => CommandResponse::success(v),
            Err(e) => CommandResponse::error(format!("シリアライズエラー: {}", e)),
        },
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

/// prod/stg 差分取得（stdin operation `diffProdStg`・設計 §4.4・TASK-53）。
/// リモート CLI は self=stg 起動し、prod パスは params の `prodDbPath` で別途受け取る。
async fn handle_diff_prod_stg(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: DiffProdStgParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    let (prod_path, _stg) = resolve_sync_paths(params.prod_db_path, None);
    let options = params.options.map(|o| o.to_options()).unwrap_or_default();
    match db.diff_with_prod(Path::new(&prod_path), &options) {
        Ok(diff) => match serde_json::to_value(&diff) {
            Ok(v) => CommandResponse::success(v),
            Err(e) => CommandResponse::error(format!("シリアライズエラー: {}", e)),
        },
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

/// observe（機械的 promote gate）を stdin operation として実行（設計 §3.4/§4.4・TASK-54）。
async fn handle_observe(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: ObserveParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    let (prod_path, _stg) = resolve_sync_paths(params.prod_db_path, None);
    match db.observe(Path::new(&prod_path), &params.options) {
        Ok(result) => match serde_json::to_value(&result) {
            Ok(v) => CommandResponse::success(v),
            Err(e) => CommandResponse::error(format!("シリアライズエラー: {}", e)),
        },
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

/// promote（stg→prod 反映）を stdin operation として実行（設計 §4.5・TASK-58/62）。
/// `backup_opts` で pre-stash 先をカスタマイズ（省略時はデフォルト `tmp/`・§7.2）。
/// gate 不合格の `PromoteGateFailed` は `e.to_string()` で error 文字列に化ける（observe と同経路）。
async fn handle_promote(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: PromoteParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    let (prod_path, _stg) = resolve_sync_paths(params.prod_db_path, None);
    match db.promote(Path::new(&prod_path), &params.options, params.backup_opts) {
        Ok(outcome) => match serde_json::to_value(&outcome) {
            Ok(v) => CommandResponse::success(v),
            Err(e) => CommandResponse::error(format!("シリアライズエラー: {}", e)),
        },
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

fn handle_set_backup_label_subcommand(
    ctx: &CliContext,
    id: String,
    label: Option<String>,
) {
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    match client.set_backup_label(&id, label.as_deref()) {
        Ok(_) => println!("ラベルを設定しました（id={}）", id),
        Err(e) => eprintln!("ラベル設定に失敗: {}", e),
    }
}

fn handle_set_backup_note_subcommand(
    ctx: &CliContext,
    id: String,
    note: Option<String>,
) {
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    match client.set_backup_note(&id, note.as_deref()) {
        Ok(_) => println!("メモを設定しました（id={}）", id),
        Err(e) => eprintln!("メモ設定に失敗: {}", e),
    }
}

async fn handle_set_backup_label(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: BackupLabelParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.set_backup_label(&params.id, params.label.as_deref()) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

async fn handle_set_backup_note(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: BackupNoteParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.set_backup_note(&params.id, params.note.as_deref()) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

async fn handle_get_backup_meta(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: BackupIdParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.get_backup_meta(&params.id) {
        Ok(meta) => CommandResponse::success(serde_json::to_value(&meta).unwrap_or_default()),
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

async fn execute_command(backend: &mut Backend, request: &CommandRequest) -> CommandResponse {
    match request.operation.as_str() {
        "migrate" => handle_migrate(backend.as_backend()).await,
        "getSchemaVersion" => handle_get_schema_version(backend.as_backend()).await,
        "getServerVersion" => handle_get_server_version().await,
        "getTables" => handle_get_tables(backend.as_backend()).await,
        "getTableInfo" => handle_get_table_info(backend.as_backend(), &request.params).await,
        "createMedia" => handle_create_media(backend.as_backend(), &request.params).await,
        "getMedia" => handle_get_media(backend.as_backend(), &request.params).await,
        "updateMedia" => handle_update_media(backend.as_backend(), &request.params).await,
        "deleteMedia" => handle_delete_media(backend.as_backend(), &request.params).await,
        "findMedia" => handle_find_media(backend.as_backend(), &request.params).await,
        "getDistinctValues" => handle_get_distinct_values(backend.as_backend(), &request.params).await,
        "bulkCreateMedia" => handle_bulk_create_media(backend.as_backend(), &request.params).await,
        "bulkDeleteMedia" => handle_bulk_delete_media(backend.as_backend(), &request.params).await,
        "bulkUpdateMedia" => handle_bulk_update_media(backend.as_backend(), &request.params).await,
        "createTag" => handle_create_tag(backend.as_backend(), &request.params).await,
        "getTagByName" => handle_get_tag_by_name(backend.as_backend(), &request.params).await,
        "getAllTags" => handle_get_all_tags(backend.as_backend()).await,
        "addTagToMedia" => handle_add_tag_to_media(backend.as_backend(), &request.params).await,
        "removeTagFromMedia" => handle_remove_tag_from_media(backend.as_backend(), &request.params).await,
        "getMediaTags" => handle_get_media_tags(backend.as_backend(), &request.params).await,
        "getMediaTagsBulk" => handle_get_media_tags_bulk(backend.as_backend(), &request.params).await,
        "getTagUsageStats" => handle_get_tag_usage_stats(backend.as_backend()).await,
        "findUnusedTags" => handle_find_unused_tags(backend.as_backend()).await,
        "setMediaAttribute" => handle_set_media_attribute(backend.as_backend(), &request.params).await,
        "getMediaAttribute" => handle_get_media_attribute(backend.as_backend(), &request.params).await,
        "getMediaAttributes" => handle_get_media_attributes(backend.as_backend(), &request.params).await,
        "deleteMediaAttribute" => handle_delete_media_attribute(backend.as_backend(), &request.params).await,
        "deleteAllMediaAttributes" => handle_delete_all_media_attributes(backend.as_backend(), &request.params).await,
        "updateExist" => match backend.as_local() {
            Some(local) => handle_update_exist(local, &request.params).await,
            None => not_supported("updateExist"),
        },
        "checkThumbnail" => match backend.as_local() {
            Some(local) => handle_check_thumbnail(local, &request.params).await,
            None => not_supported("checkThumbnail"),
        },
        "updateThumbnail" => match backend.as_local() {
            Some(local) => handle_update_thumbnail(local, &request.params).await,
            None => not_supported("updateThumbnail"),
        },
        "addMediaHash" => handle_add_media_hash(backend.as_backend(), &request.params).await,
        "addMediaHashes" => handle_add_media_hashes(backend.as_backend(), &request.params).await,
        "getMediaHashes" => handle_get_media_hashes(backend.as_backend(), &request.params).await,
        "getMediaHash" => handle_get_media_hash(backend.as_backend(), &request.params).await,
        "findByContentHash" => handle_find_by_content_hash(backend.as_backend(), &request.params).await,
        "deleteMediaHash" => handle_delete_media_hash(backend.as_backend(), &request.params).await,
        "deleteMediaHashes" => handle_delete_media_hashes(backend.as_backend(), &request.params).await,
        "findDuplicateHashes" => handle_find_duplicate_hashes(backend.as_backend()).await,
        "computeMediaHash" => match backend.as_local() {
            Some(local) => handle_compute_media_hash(local, &request.params).await,
            None => not_supported("computeMediaHash"),
        },
        "computeMediaHashes" => match backend.as_local() {
            Some(local) => handle_compute_media_hashes(local, &request.params).await,
            None => not_supported("computeMediaHashes"),
        },
        "backup" => match backend.as_local_mut() {
            Some(local) => handle_backup(local, &request.params).await,
            None => not_supported("backup"),
        },
        "listBackups" => match backend.as_local() {
            Some(local) => handle_list_backups(local).await,
            None => not_supported("listBackups"),
        },
        "listPreStashes" => match backend.as_local() {
            Some(local) => handle_list_pre_stashes(local).await,
            None => not_supported("listPreStashes"),
        },
        "restore" => match backend.as_local_mut() {
            Some(local) => handle_restore(local, &request.params).await,
            None => not_supported("restore"),
        },
        "diffBackup" => match backend.as_local() {
            Some(local) => handle_diff_backup(local, &request.params).await,
            None => not_supported("diffBackup"),
        },
        "diffProdStg" => match backend.as_local() {
            Some(local) => handle_diff_prod_stg(local, &request.params).await,
            None => not_supported("diffProdStg"),
        },
        "observe" => match backend.as_local() {
            Some(local) => handle_observe(local, &request.params).await,
            None => not_supported("observe"),
        },
        "promote" => match backend.as_local() {
            Some(local) => handle_promote(local, &request.params).await,
            None => not_supported("promote"),
        },
        "setBackupLabel" => match backend.as_local() {
            Some(local) => handle_set_backup_label(local, &request.params).await,
            None => not_supported("setBackupLabel"),
        },
        "setBackupNote" => match backend.as_local() {
            Some(local) => handle_set_backup_note(local, &request.params).await,
            None => not_supported("setBackupNote"),
        },
        "getBackupMeta" => match backend.as_local() {
            Some(local) => handle_get_backup_meta(local, &request.params).await,
            None => not_supported("getBackupMeta"),
        },
        "mediaCp" => match backend.as_local() {
            Some(local) => handle_media_cp(local, &request.params).await,
            None => not_supported("mediaCp"),
        },
        "mediaMv" => match backend.as_local() {
            Some(local) => handle_media_mv(local, &request.params).await,
            None => not_supported("mediaMv"),
        },
        "mediaSync" => match backend.as_local() {
            Some(local) => handle_media_sync(local, &request.params).await,
            None => not_supported("mediaSync"),
        },
        "moveToTrash" => match backend.as_local() {
            Some(local) => handle_move_to_trash(local, &request.params).await,
            None => not_supported("moveToTrash"),
        },
        "listTrash" => match backend.as_local() {
            Some(local) => handle_list_trash(local).await,
            None => not_supported("listTrash"),
        },
        "restoreFromTrash" => match backend.as_local() {
            Some(local) => handle_restore_from_trash(local, &request.params).await,
            None => not_supported("restoreFromTrash"),
        },
        "purgeTrash" => match backend.as_local() {
            Some(local) => handle_purge_trash(local, &request.params).await,
            None => not_supported("purgeTrash"),
        },
        _ => CommandResponse::error(format!("不明な操作: {}", request.operation)),
    }
}

async fn handle_migrate(db: &dyn KijukuBackend) -> CommandResponse {
    match db.migrate().await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("マイグレーションエラー: {}", e)),
    }
}

async fn handle_media_cp(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let p: MediaCpParams = match deserialize_params(params) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    CommandResponse::from_result(db.media_cp(&p.src, &p.dst, &p.options), "media_cpエラー: ")
}

async fn handle_media_mv(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let p: MediaMvParams = match deserialize_params(params) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    CommandResponse::from_result(db.media_mv(&p.src, &p.dst, &p.options), "media_mvエラー: ")
}

async fn handle_media_sync(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let p: MediaSyncParams = match deserialize_params(params) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    CommandResponse::from_result(
        db.media_sync(&p.src, &p.dst, &p.options),
        "media_syncエラー: ",
    )
}

async fn handle_move_to_trash(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let p: MoveToTrashParams = match deserialize_params(params) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    CommandResponse::from_result(
        db.move_to_trash(&p.target_rel, p.operation, p.reason.as_deref()),
        "move_to_trashエラー: ",
    )
}

async fn handle_list_trash(db: &KijukuDB) -> CommandResponse {
    CommandResponse::from_result(db.list_trash(), "list_trashエラー: ")
}

async fn handle_restore_from_trash(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let p: RestoreFromTrashParams = match deserialize_params(params) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    CommandResponse::from_result(
        db.restore_from_trash(&p.id).map(|p| p.to_string_lossy().to_string()),
        "restore_from_trashエラー: ",
    )
}

async fn handle_purge_trash(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let p: PurgeTrashParams = match deserialize_params(params) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    CommandResponse::from_result(
        db.purge_trash(p.ids.as_deref(), p.dry_run),
        "purge_trashエラー: ",
    )
}

async fn handle_get_schema_version(db: &dyn KijukuBackend) -> CommandResponse {
    match db.get_schema_version().await {
        Ok(version) => CommandResponse::success(serde_json::json!({"version": version})),
        Err(e) => CommandResponse::error(format!("スキーマバージョン取得エラー: {}", e)),
    }
}

/// リモート CLI バイナリ自身のバージョンを返す（TASK-69・自動デプロイのバージョン比較用）。
/// DB アクセス不要・prod/stg 両バックエンドで共通。`env!("CARGO_PKG_VERSION")` はビルド時定数埋め込み。
async fn handle_get_server_version() -> CommandResponse {
    CommandResponse::success(serde_json::json!({"version": env!("CARGO_PKG_VERSION")}))
}

async fn handle_get_tables(db: &dyn KijukuBackend) -> CommandResponse {
    CommandResponse::from_result(db.get_tables().await, "テーブル一覧取得エラー: ")
}

async fn handle_get_table_info(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<GetTableInfoParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    CommandResponse::from_result(db.get_table_info(&params.table_name).await, "テーブル情報取得エラー: ")
}

async fn handle_create_media(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<CreateMediaParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    CommandResponse::from_result(db.create_media(&params.data).await, "メディア作成エラー: ")
}

async fn handle_get_media(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<GetMediaParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    match db.get_media(params.id).await {
        Ok(Some(m)) => match serde_json::to_value(m) {
            Ok(v) => CommandResponse::success(v),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Ok(None) => CommandResponse::success(serde_json::Value::Null),
        Err(e) => CommandResponse::error(format!("メディア取得エラー: {}", e)),
    }
}

async fn handle_update_media(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<UpdateMediaParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    match db.update_media(params.id, &params.data).await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("メディア更新エラー: {}", e)),
    }
}

async fn handle_delete_media(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<DeleteMediaParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    match db.delete_media(params.id).await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("メディア削除エラー: {}", e)),
    }
}

async fn handle_find_media(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<FindMediaParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    CommandResponse::from_result(db.find_media(&params.filter, params.options.as_ref()).await, "メディア検索エラー: ")
}

async fn handle_get_distinct_values(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: GetDistinctValuesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    let fields: Vec<&str> = params.fields.iter().map(|s| s.as_str()).collect();
    match db.get_distinct_values(&fields, &params.filter).await {
        Ok(values) => {
            let data = serde_json::to_value(values).unwrap();
            CommandResponse::success(data)
        }
        Err(e) => CommandResponse::error(format!("distinct値取得エラー: {}", e)),
    }
}

async fn handle_bulk_create_media(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: BulkCreateMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.bulk_create_media(&params.data_list).await {
        Ok(media_list) => match serde_json::to_value(media_list) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("一括作成エラー: {}", e)),
    }
}

async fn handle_bulk_delete_media(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: BulkDeleteMediaParams = match BulkDeleteMediaParams::deserialize(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.bulk_delete_media(&params.ids).await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("一括削除エラー: {}", e)),
    }
}

async fn handle_bulk_update_media(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: BulkUpdateMediaParams = match BulkUpdateMediaParams::deserialize(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.bulk_update_media(&params.updates).await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("一括更新エラー: {}", e)),
    }
}

async fn handle_create_tag(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: CreateTagParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.create_tag(&params.name).await {
        Ok(tag) => match serde_json::to_value(tag) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("タグ作成エラー: {}", e)),
    }
}

async fn handle_get_tag_by_name(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: GetTagByNameParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_tag_by_name(&params.name).await {
        Ok(Some(tag)) => match serde_json::to_value(tag) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Ok(None) => CommandResponse::success(serde_json::Value::Null),
        Err(e) => CommandResponse::error(format!("タグ取得エラー: {}", e)),
    }
}

async fn handle_get_all_tags(db: &dyn KijukuBackend) -> CommandResponse {
    match db.get_all_tags().await {
        Ok(tags) => match serde_json::to_value(tags) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("タグ取得エラー: {}", e)),
    }
}

async fn handle_add_tag_to_media(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: AddTagToMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.add_tag_to_media(params.media_id, params.tag_id).await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("タグ追加エラー: {}", e)),
    }
}

async fn handle_remove_tag_from_media(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: RemoveTagFromMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.remove_tag_from_media(params.media_id, params.tag_id).await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("タグ削除エラー: {}", e)),
    }
}

async fn handle_get_media_tags(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaTagsParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_media_tags(params.media_id).await {
        Ok(tags) => match serde_json::to_value(tags) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("メディアタグ取得エラー: {}", e)),
    }
}

async fn handle_get_media_tags_bulk(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaTagsBulkParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_media_tags_bulk(&params.media_ids).await {
        Ok(tags_map) => match serde_json::to_value(tags_map) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("一括メディアタグ取得エラー: {}", e)),
    }
}

async fn handle_get_tag_usage_stats(db: &dyn KijukuBackend) -> CommandResponse {
    match db.get_tag_usage_stats().await {
        Ok(stats) => match serde_json::to_value(stats) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("タグ使用統計取得エラー: {}", e)),
    }
}

async fn handle_find_unused_tags(db: &dyn KijukuBackend) -> CommandResponse {
    match db.find_unused_tags().await {
        Ok(tags) => match serde_json::to_value(tags) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("未使用タグ取得エラー: {}", e)),
    }
}

async fn handle_set_media_attribute(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: SetMediaAttributeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    let value_type = params.value_type.as_ref().and_then(|vt| match vt.as_str() {
        "integer" => Some(AttributeValueType::Integer),
        "boolean" => Some(AttributeValueType::Boolean),
        _ => Some(AttributeValueType::String),
    });

    match db.set_media_attribute(
        params.media_id,
        &params.key,
        params.value.as_deref(),
        value_type,
    )
    .await
    {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("属性設定エラー: {}", e)),
    }
}

async fn handle_get_media_attribute(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaAttributeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_media_attribute(params.media_id, &params.key).await {
        Ok(Some(attr)) => match serde_json::to_value(attr) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Ok(None) => CommandResponse::success(serde_json::Value::Null),
        Err(e) => CommandResponse::error(format!("属性取得エラー: {}", e)),
    }
}

async fn handle_get_media_attributes(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaAttributesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_media_attributes(params.media_id).await {
        Ok(attrs) => match serde_json::to_value(attrs) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("属性取得エラー: {}", e)),
    }
}

async fn handle_delete_media_attribute(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: DeleteMediaAttributeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.delete_media_attribute(params.media_id, &params.key).await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("属性削除エラー: {}", e)),
    }
}

async fn handle_delete_all_media_attributes(
    db: &dyn KijukuBackend,
    params: &serde_json::Value,
) -> CommandResponse {
    let params: DeleteAllMediaAttributesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.delete_all_media_attributes(params.media_id).await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("全属性削除エラー: {}", e)),
    }
}

async fn handle_update_exist(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: UpdateExistParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.update_exist(&params.filter, params.options.as_ref(), &params.update_options) {
        Ok(result) => match serde_json::to_value(result) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("update-existエラー: {}", e)),
    }
}

fn handle_update_exist_subcommand(ctx: &CliContext, dry_run: bool, filter_json: &str) {
    let filter: MediaFilter = match serde_json::from_str(filter_json) {
        Ok(f) => f,
        Err(e) => {
            let response = CommandResponse::error(format!("filterのJSONパースエラー: {}", e));
            output_response(&response);
            return;
        }
    };

    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            output_response(&CommandResponse::error(e));
            return;
        }
    };

    let update_options = UpdateExistOptions { dry_run };
    match client.update_exist(&filter, None, &update_options) {
        Ok(result) => match serde_json::to_value(result) {
            Ok(data) => output_response(&CommandResponse::success(data)),
            Err(e) => output_response(&CommandResponse::error(format!(
                "レスポンスのシリアライズに失敗: {}",
                e
            ))),
        },
        Err(e) => output_response(&CommandResponse::error(format!("update-existエラー: {}", e))),
    }
}

async fn handle_check_thumbnail(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: ThumbnailParams = match ThumbnailParams::deserialize(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.check_thumbnail(&params.filter, params.options.as_ref()) {
        Ok(result) => match serde_json::to_value(result) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("check-thumbnailエラー: {}", e)),
    }
}

async fn handle_update_thumbnail(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: ThumbnailParams = match ThumbnailParams::deserialize(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.update_thumbnail(&params.filter, params.options.as_ref(), &params.thumbnail_options) {
        Ok(result) => match serde_json::to_value(result) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("update-thumbnailエラー: {}", e)),
    }
}

fn handle_check_thumbnail_subcommand(ctx: &CliContext, filter_json: &str) {
    let filter: MediaFilter = match serde_json::from_str(filter_json) {
        Ok(f) => f,
        Err(e) => {
            let response = CommandResponse::error(format!("filterのJSONパースエラー: {}", e));
            output_response(&response);
            return;
        }
    };
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            output_response(&CommandResponse::error(e));
            return;
        }
    };
    match client.check_thumbnail(&filter, None) {
        Ok(result) => match serde_json::to_value(result) {
            Ok(data) => output_response(&CommandResponse::success(data)),
            Err(e) => output_response(&CommandResponse::error(format!(
                "レスポンスのシリアライズに失敗: {}",
                e
            ))),
        },
        Err(e) => output_response(&CommandResponse::error(format!("check-thumbnailエラー: {}", e))),
    }
}

fn handle_update_thumbnail_subcommand(ctx: &CliContext, dry_run: bool, force: bool, filter_json: &str) {
    let filter: MediaFilter = match serde_json::from_str(filter_json) {
        Ok(f) => f,
        Err(e) => {
            let response = CommandResponse::error(format!("filterのJSONパースエラー: {}", e));
            output_response(&response);
            return;
        }
    };
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            output_response(&CommandResponse::error(e));
            return;
        }
    };
    let thumbnail_options = ThumbnailOptions { dry_run, force };
    match client.update_thumbnail(&filter, None, &thumbnail_options) {
        Ok(result) => match serde_json::to_value(result) {
            Ok(data) => output_response(&CommandResponse::success(data)),
            Err(e) => output_response(&CommandResponse::error(format!(
                "レスポンスのシリアライズに失敗: {}",
                e
            ))),
        },
        Err(e) => {
            output_response(&CommandResponse::error(format!("update-thumbnailエラー: {}", e)))
        }
    }
}

fn media_hash_to_json(hash: &kijuku_db::MediaHash) -> serde_json::Value {
    use kijuku_db::hash::bytes_to_hex;
    serde_json::json!({
        "item_uuid": hash.item_uuid,
        "filename": hash.filename,
        "time_range": hash.time_range,
        "content_hash": bytes_to_hex(&hash.content_hash),
        "alternative_of": hash.alternative_of,
        "embedding": hash.embedding.as_ref().map(|e| bytes_to_hex(e)),
        "created_at": hash.created_at,
        "updated_at": hash.updated_at,
    })
}

async fn handle_add_media_hash(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: AddMediaHashParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.add_media_hash(&params.input).await {
        Ok(hash) => CommandResponse::success(media_hash_to_json(&hash)),
        Err(e) => CommandResponse::error(format!("ハッシュ追加エラー: {}", e)),
    }
}

async fn handle_add_media_hashes(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: AddMediaHashesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.add_media_hashes(&params.inputs).await {
        Ok(hashes) => {
            let json: Vec<_> = hashes.iter().map(media_hash_to_json).collect();
            CommandResponse::success(serde_json::Value::Array(json))
        }
        Err(e) => CommandResponse::error(format!("ハッシュ一括追加エラー: {}", e)),
    }
}

async fn handle_get_media_hashes(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaHashesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.get_media_hashes(&params.item_uuid).await {
        Ok(hashes) => {
            let json: Vec<_> = hashes.iter().map(media_hash_to_json).collect();
            CommandResponse::success(serde_json::Value::Array(json))
        }
        Err(e) => CommandResponse::error(format!("ハッシュ取得エラー: {}", e)),
    }
}

async fn handle_get_media_hash(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaHashParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.get_media_hash(&params.item_uuid, &params.filename, &params.time_range).await {
        Ok(Some(hash)) => CommandResponse::success(media_hash_to_json(&hash)),
        Ok(None) => CommandResponse::success(serde_json::Value::Null),
        Err(e) => CommandResponse::error(format!("ハッシュ取得エラー: {}", e)),
    }
}

async fn handle_find_by_content_hash(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: FindByContentHashParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    let hash_bytes = match kijuku_db::hash::hex_to_bytes(&params.hash_hex) {
        Ok(b) => b,
        Err(e) => return CommandResponse::error(format!("ハッシュ値エラー: {}", e)),
    };
    match db.find_by_content_hash(&hash_bytes).await {
        Ok(hashes) => {
            let json: Vec<_> = hashes.iter().map(media_hash_to_json).collect();
            CommandResponse::success(serde_json::Value::Array(json))
        }
        Err(e) => CommandResponse::error(format!("ハッシュ検索エラー: {}", e)),
    }
}

async fn handle_delete_media_hash(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: DeleteMediaHashParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.delete_media_hash(&params.item_uuid, &params.filename, &params.time_range).await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("ハッシュ削除エラー: {}", e)),
    }
}

async fn handle_delete_media_hashes(db: &dyn KijukuBackend, params: &serde_json::Value) -> CommandResponse {
    let params: DeleteMediaHashesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.delete_media_hashes(&params.item_uuid).await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("ハッシュ全削除エラー: {}", e)),
    }
}

async fn handle_find_duplicate_hashes(db: &dyn KijukuBackend) -> CommandResponse {
    match db.find_duplicate_hashes().await {
        Ok(dupes) => {
            use kijuku_db::hash::bytes_to_hex;
            let json: Vec<_> = dupes.iter().map(|(hash, count)| {
                serde_json::json!({
                    "content_hash": bytes_to_hex(hash),
                    "count": count,
                })
            }).collect();
            CommandResponse::success(serde_json::Value::Array(json))
        }
        Err(e) => CommandResponse::error(format!("重複検出エラー: {}", e)),
    }
}

async fn handle_compute_media_hash(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: ComputeMediaHashParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.compute_media_hash(&params.item_uuid, &params.media_path, &params.media_type, params.duration_sec) {
        Ok(result) => {
            let hashes: Vec<_> = result.hashes.iter().map(media_hash_to_json).collect();
            CommandResponse::success(serde_json::json!({
                "item_uuid": result.item_uuid,
                "hashes": hashes,
                "skipped": result.skipped,
                "skip_reason": result.skip_reason,
            }))
        }
        Err(e) => CommandResponse::error(format!("ハッシュ計算エラー: {}", e)),
    }
}

async fn handle_compute_media_hashes(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: ComputeMediaHashesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.compute_media_hashes(&params.filter, params.options.as_ref(), params.force) {
        Ok(results) => {
            let json: Vec<_> = results.iter().map(|r| {
                let hashes: Vec<_> = r.hashes.iter().map(media_hash_to_json).collect();
                serde_json::json!({
                    "item_uuid": r.item_uuid,
                    "hashes": hashes,
                    "skipped": r.skipped,
                    "skip_reason": r.skip_reason,
                })
            }).collect();
            CommandResponse::success(serde_json::Value::Array(json))
        }
        Err(e) => CommandResponse::error(format!("ハッシュ計算エラー: {}", e)),
    }
}

fn handle_hash_subcommand(ctx: &CliContext, cmd: &HashCommands) {
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            output_response(&CommandResponse::error(e));
            return;
        }
    };
    match cmd {
        HashCommands::Compute { uuid, all, force, filter } => {
            let media_filter: MediaFilter = match serde_json::from_str(filter) {
                Ok(f) => f,
                Err(e) => {
                    output_response(&CommandResponse::error(format!("filterのJSONパースエラー: {}", e)));
                    return;
                }
            };
            if let Some(u) = uuid {
                // 特定UUIDのcompute: メディアを検索してcompute
                match client.find_media(&MediaFilter { id_in: None, ..Default::default() }, None) {
                    Ok(_) => {}
                    Err(e) => {
                        output_response(&CommandResponse::error(format!("メディア検索エラー: {}", e)));
                        return;
                    }
                }
                // UUIDからメディアを取得
                let media_list = client.find_media(&MediaFilter { ..Default::default() }, None).unwrap_or_default();
                let media = media_list.iter().find(|m| m.uuid == *u);
                if let Some(m) = media {
                    if !m.flag_exist || m.path.is_none() {
                        output_response(&CommandResponse::error("対象メディアはpathがないかflag_existがfalseです".to_string()));
                        return;
                    }
                    match client.compute_media_hash(&m.uuid, m.path.as_ref().unwrap(), m.media_type.as_str(), m.duration_sec) {
                        Ok(result) => {
                            let hashes: Vec<_> = result.hashes.iter().map(media_hash_to_json).collect();
                            output_response(&CommandResponse::success(serde_json::json!({
                                "item_uuid": result.item_uuid,
                                "hashes": hashes,
                                "skipped": result.skipped,
                                "skip_reason": result.skip_reason,
                            })));
                        }
                        Err(e) => output_response(&CommandResponse::error(format!("ハッシュ計算エラー: {}", e))),
                    }
                } else {
                    output_response(&CommandResponse::error(format!("メディアが見つかりません (uuid: {})", u)));
                }
            } else if *all {
                match client.compute_media_hashes(&media_filter, None, *force) {
                    Ok(results) => {
                        let json: Vec<_> = results.iter().map(|r| {
                            let hashes: Vec<_> = r.hashes.iter().map(media_hash_to_json).collect();
                            serde_json::json!({
                                "item_uuid": r.item_uuid,
                                "hashes": hashes,
                                "skipped": r.skipped,
                                "skip_reason": r.skip_reason,
                            })
                        }).collect();
                        output_response(&CommandResponse::success(serde_json::Value::Array(json)));
                    }
                    Err(e) => output_response(&CommandResponse::error(format!("ハッシュ計算エラー: {}", e))),
                }
            } else {
                output_response(&CommandResponse::error("--uuid または --all を指定してください".to_string()));
            }
        }
        HashCommands::List { uuid } => {
            match client.get_media_hashes(uuid) {
                Ok(hashes) => {
                    let json: Vec<_> = hashes.iter().map(media_hash_to_json).collect();
                    output_response(&CommandResponse::success(serde_json::Value::Array(json)));
                }
                Err(e) => output_response(&CommandResponse::error(format!("ハッシュ取得エラー: {}", e))),
            }
        }
        HashCommands::Find { hash } => {
            let hash_bytes = match kijuku_db::hash::hex_to_bytes(hash) {
                Ok(b) => b,
                Err(e) => {
                    output_response(&CommandResponse::error(format!("ハッシュ値エラー: {}", e)));
                    return;
                }
            };
            match client.find_by_content_hash(&hash_bytes) {
                Ok(hashes) => {
                    let json: Vec<_> = hashes.iter().map(media_hash_to_json).collect();
                    output_response(&CommandResponse::success(serde_json::Value::Array(json)));
                }
                Err(e) => output_response(&CommandResponse::error(format!("ハッシュ検索エラー: {}", e))),
            }
        }
        HashCommands::Duplicates => {
            match client.find_duplicate_hashes() {
                Ok(dupes) => {
                    use kijuku_db::hash::bytes_to_hex;
                    let json: Vec<_> = dupes.iter().map(|(hash, count)| {
                        serde_json::json!({
                            "content_hash": bytes_to_hex(hash),
                            "count": count,
                        })
                    }).collect();
                    output_response(&CommandResponse::success(serde_json::Value::Array(json)));
                }
                Err(e) => output_response(&CommandResponse::error(format!("重複検出エラー: {}", e))),
            }
        }
    }
}

/// file サブコマンド: cp/mv/sync。SDK の media_cp/media_mv/media_sync の薄いラッパ。
fn handle_file_subcommand(ctx: &CliContext, cmd: &FileCommands) {
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            output_response(&CommandResponse::error(e));
            return;
        }
    };
    match cmd {
        FileCommands::Cp { src, dst, apply, update_db } => {
            let opts = FileOpOptions { apply: *apply, update_db: *update_db };
            output_response(&CommandResponse::from_result(
                client.media_cp(src, dst, &opts),
                "media_cpエラー: ",
            ));
        }
        FileCommands::Mv { src, dst, apply, update_db } => {
            let opts = FileOpOptions { apply: *apply, update_db: *update_db };
            output_response(&CommandResponse::from_result(
                client.media_mv(src, dst, &opts),
                "media_mvエラー: ",
            ));
        }
        FileCommands::Sync { src, dst, apply, update_db } => {
            let opts = FileOpOptions { apply: *apply, update_db: *update_db };
            output_response(&CommandResponse::from_result(
                client.media_sync(src, dst, &opts),
                "media_syncエラー: ",
            ));
        }
    }
}

/// trash サブコマンド: move/list/restore/purge。SDK の trash 系メソッドの薄いラッパ。
fn handle_trash_subcommand(ctx: &CliContext, cmd: &TrashCommands) {
    let client = match open_db_client(ctx) {
        Ok(c) => c,
        Err(e) => {
            output_response(&CommandResponse::error(e));
            return;
        }
    };
    match cmd {
        TrashCommands::Move { target_rel, operation, reason } => {
            output_response(&CommandResponse::from_result(
                client.move_to_trash(target_rel, operation.clone().into(), reason.as_deref()),
                "move_to_trashエラー: ",
            ));
        }
        TrashCommands::List => {
            output_response(&CommandResponse::from_result(
                client.list_trash(),
                "list_trashエラー: ",
            ));
        }
        TrashCommands::Restore { id } => {
            // PathBuf → 文字列化（stdin の handle_restore_from_trash と同一方針）。
            // DbClient.restore_from_trash が既に String を返すため文字列化不要。
            output_response(&CommandResponse::from_result(
                client.restore_from_trash(id),
                "restore_from_trashエラー: ",
            ));
        }
        TrashCommands::Purge { ids, dry_run } => {
            let ids_opt: Option<&[String]> = if ids.is_empty() { None } else { Some(ids) };
            output_response(&CommandResponse::from_result(
                client.purge_trash(ids_opt, *dry_run),
                "purge_trashエラー: ",
            ));
        }
    }
}

/// bulk-load サブコマンド: ローカルDB（`--db`）を source、D1 を dest として
/// 全データ（media/tags/media_tags/attributes/hashes）を完全移行し、件数・内容一致を検証する。
/// SDK の `transfer` / `verify` を呼ぶだけの薄いラッパ。
async fn handle_bulk_load_subcommand(ctx: &CliContext, chunk_size: Option<usize>, verify_only: bool) {
    // source: ローカル SQLite（--db）
    let source = match KijukuDB::open_with_options(
        &ctx.db_path,
        DBOptions {
            verbose: ctx.verbose,
            ..Default::default()
        },
    ) {
        Ok(db) => db,
        Err(e) => {
            output_response(&CommandResponse::error(format!("元DBのオープンに失敗: {}", e)));
            return;
        }
    };
    let src: &dyn KijukuBackend = &source;
    if let Err(e) = src.migrate().await {
        output_response(&CommandResponse::error(format!(
            "元DBのマイグレーションに失敗: {}",
            e
        )));
        return;
    }

    // dest: D1（環境変数 + wrangler キャッシュの OAuth トークン）
    let dest = {
        let account_id = match std::env::var("D1_ACCOUNT_ID") {
            Ok(v) => v,
            Err(_) => {
                output_response(&CommandResponse::error(
                    "D1_ACCOUNT_ID 環境変数が設定されていません".to_string(),
                ));
                return;
            }
        };
        let database_id = match std::env::var("D1_DATABASE_ID") {
            Ok(v) => v,
            Err(_) => {
                output_response(&CommandResponse::error(
                    "D1_DATABASE_ID 環境変数が設定されていません".to_string(),
                ));
                return;
            }
        };
        match D1KijukuDB::from_wrangler(D1Config {
            account_id,
            database_id,
        }) {
            Ok(d) => d,
            Err(e) => {
                output_response(&CommandResponse::error(format!(
                    "D1 接続に失敗: {}（wrangler login 済みか確認してください）",
                    e
                )));
                return;
            }
        }
    };

    let opts = TransferOptions {
        chunk_size: chunk_size.unwrap_or(500),
        verbose: ctx.verbose,
    };

    let result = if verify_only {
        verify(src, &dest).await
    } else {
        transfer(src, &dest, &opts).await
    };

    let response = match result {
        Ok(report) => match serde_json::to_value(&report) {
            Ok(v) => CommandResponse::success(v),
            Err(e) => CommandResponse::error(format!("結果のシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("バルクロードエラー: {}", e)),
    };
    output_response(&response);
}

fn output_response(response: &CommandResponse) {
    match serde_json::to_string(response) {
        Ok(json) => println!("{}", json),
        Err(e) => {
            let err_resp = CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e));
            if let Ok(err_json) = serde_json::to_string(&err_resp) {
                println!("{}", err_json);
            } else {
                println!("{{\"success\":false,\"error\":\"Failed to serialize error response\"}}");
            }
        }
    }
}
