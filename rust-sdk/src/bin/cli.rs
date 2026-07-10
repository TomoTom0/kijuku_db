//! Kijuku DB CLI
//!
//! JSON形式の入出力でリモート操作を可能にするCLIツール
//! Version: 0.2.1
//!
//! NOTE: CLI のハンドラ群は Phase 3 で async 化（KijukuBackend 経由）する予定。
//! それまで KijukuDB の deprecated 同期メソッドを使用するため、移行完了まで一時的に許容する。

// ハンドラの async 化（Phase 3）までの間、deprecated 同期 API の使用を一時的に許容
#![allow(deprecated)]

use clap::Parser;
use include_dir::{include_dir, Dir};
use kijuku_db::{
    AttributeValueType, BackupInfo, BackupKind, BackupOptions, BackupScope, BackupSelector,
    BulkUpdateItem, D1Config, D1KijukuDB, DBOptions, KijukuBackend, KijukuDB, MediaFilter,
    MediaInput, MediaHashInput, MediaUpdateInput, QueryOptions, ThumbnailOptions,
    TransferOptions, UpdateExistOptions, transfer, verify,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

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
}

impl BackupSelectorJson {
    fn to_selector(&self) -> BackupSelector {
        match self {
            BackupSelectorJson::Latest => BackupSelector::latest(),
            BackupSelectorJson::Nth { n } => BackupSelector::nth(*n),
            BackupSelectorJson::ById { id } => BackupSelector::by_id(id),
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
            let db = KijukuDB::open_with_options(
                &ctx.db_path,
                DBOptions {
                    backup: Some(BackupOptions::default()),
                    verbose: ctx.verbose,
                    ..Default::default()
                },
            )
            .map_err(|e| CommandResponse::error(format!("データベースのオープンに失敗: {}", e)))?;
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
    /// データベースファイルのパス
    #[arg(long, default_value = "kijuku.db")]
    db: String,

    /// 実行したSQLをstderrに出力する
    #[arg(long)]
    verbose: bool,

    /// バックエンド（local / d1）。d1 は D1_ACCOUNT_ID / D1_DATABASE_ID 環境変数が必要
    #[arg(long, value_enum, default_value_t = BackendKind::Local)]
    backend: BackendKind,

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
    },
    /// バックアップ一覧を表示
    ListBackups,
    /// バックアップから復元
    Restore {
        /// N番目のバックアップから復元（0が最新、省略時は最新）
        #[arg(long)]
        nth: Option<usize>,
        /// バックアップID（タイムスタンプ）を指定して復元（nth より優先）
        #[arg(long)]
        id: Option<String>,
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
    /// コンテンツハッシュ操作
    Hash {
        #[command(subcommand)]
        hash_command: HashCommands,
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

fn open_db_with_backup(ctx: &CliContext) -> Option<KijukuDB> {
    match KijukuDB::open_with_options(
        &ctx.db_path,
        DBOptions {
            backup: Some(BackupOptions::default()),
            verbose: ctx.verbose,
            ..Default::default()
        },
    ) {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("データベースのオープンに失敗: {}", e);
            None
        }
    }
}

fn open_and_migrate_db(ctx: &CliContext) -> Option<KijukuDB> {
    let db = open_db_with_backup(ctx)?;
    if let Err(e) = db.migrate() {
        eprintln!("マイグレーションに失敗: {}", e);
        return None;
    }
    Some(db)
}

fn handle_backup_subcommand(ctx: &CliContext, label: Option<String>) {
    let db = match open_and_migrate_db(ctx) {
        Some(db) => db,
        None => return,
    };
    let result = if let Some(ref l) = label {
        db.backup_with_label(l)
    } else {
        db.backup()
    };
    match result {
        Ok(Some(path)) => println!("{}", path),
        Ok(None) => eprintln!("バックアップマネージャーが設定されていません"),
        Err(e) => eprintln!("バックアップに失敗: {}", e),
    }
}

fn handle_list_backups_subcommand(ctx: &CliContext) {
    let db = match open_and_migrate_db(ctx) {
        Some(db) => db,
        None => return,
    };
    match db.list_backups() {
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

fn handle_restore_subcommand(ctx: &CliContext, nth: Option<usize>, id: Option<String>) {
    let mut db = match open_and_migrate_db(ctx) {
        Some(db) => db,
        None => return,
    };
    let selector = if let Some(id) = id {
        BackupSelector::by_id(id)
    } else {
        match nth {
            Some(n) => BackupSelector::nth(n),
            None => BackupSelector::latest(),
        }
    };
    match db.restore(&selector) {
        Ok(path) => println!("{}", path.to_string_lossy()),
        Err(e) => eprintln!("復元に失敗: {}", e),
    }
}

fn handle_diff_backup_subcommand(
    ctx: &CliContext,
    nth: Option<usize>,
    id: Option<String>,
    detail: String,
) {
    let db = match open_and_migrate_db(ctx) {
        Some(db) => db,
        None => return,
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
    match db.diff_with_backup(&selector, &options) {
        Ok(diff) => print_backup_diff_summary(&diff),
        Err(e) => eprintln!("差分の取得に失敗: {}", e),
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

    // バックエンドを構築（Local / D1）
    let mut backend = match build_backend(ctx) {
        Ok(b) => b,
        Err(resp) => {
            output_response(&resp);
            return;
        }
    };

    // マイグレーションを実行（Local は schema.sql、D1 は schema.d1.sql が自動選択される）
    if let Err(e) = backend.as_backend().migrate().await {
        let response = CommandResponse::error(format!("マイグレーションに失敗: {}", e));
        output_response(&response);
        return;
    }

    // コマンドを実行
    let response = execute_command(&mut backend, &request).await;
    output_response(&response);
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let ctx = CliContext {
        db_path: cli.db.clone(),
        verbose: cli.verbose,
        backend: cli.backend.clone(),
    };

    // バックエンド種別とサブコマンドの整合性チェック
    match (&ctx.backend, &cli.command) {
        // D1 バックエンド: stdin プロトコル・Docs・BulkLoad のみ対応
        (BackendKind::D1, None)
        | (BackendKind::D1, Some(Commands::Docs { .. }))
        | (BackendKind::D1, Some(Commands::BulkLoad { .. })) => {}
        (BackendKind::D1, _) => {
            eprintln!(
                "エラー: --backend d1 では stdin プロトコル・docs・bulk-load のみ利用可能です"
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
        Some(Commands::Backup { label }) => {
            handle_backup_subcommand(&ctx, label.clone());
        }
        Some(Commands::ListBackups) => {
            handle_list_backups_subcommand(&ctx);
        }
        Some(Commands::Restore { nth, id }) => {
            handle_restore_subcommand(&ctx, *nth, id.clone());
        }
        Some(Commands::DiffBackup { nth, id, detail }) => {
            handle_diff_backup_subcommand(&ctx, *nth, id.clone(), detail.clone());
        }
        Some(Commands::SetBackupLabel { id, label }) => {
            handle_set_backup_label_subcommand(&ctx, id.clone(), label.clone());
        }
        Some(Commands::SetBackupNote { id, note }) => {
            handle_set_backup_note_subcommand(&ctx, id.clone(), note.clone());
        }
        Some(Commands::Hash { hash_command }) => {
            handle_hash_subcommand(&ctx, hash_command);
        }
        Some(Commands::BulkLoad { chunk_size, verify_only }) => {
            handle_bulk_load_subcommand(&ctx, *chunk_size, *verify_only).await;
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
    match db.restore(&selector) {
        Ok(path) => CommandResponse::success(serde_json::json!({"path": path.to_string_lossy()})),
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

fn handle_set_backup_label_subcommand(
    ctx: &CliContext,
    id: String,
    label: Option<String>,
) {
    let db = match open_and_migrate_db(ctx) {
        Some(db) => db,
        None => return,
    };
    match db.set_backup_label(&id, label.as_deref()) {
        Ok(_) => println!("ラベルを設定しました（id={}）", id),
        Err(e) => eprintln!("ラベル設定に失敗: {}", e),
    }
}

fn handle_set_backup_note_subcommand(
    ctx: &CliContext,
    id: String,
    note: Option<String>,
) {
    let db = match open_and_migrate_db(ctx) {
        Some(db) => db,
        None => return,
    };
    match db.set_backup_note(&id, note.as_deref()) {
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
        "restore" => match backend.as_local_mut() {
            Some(local) => handle_restore(local, &request.params).await,
            None => not_supported("restore"),
        },
        "diffBackup" => match backend.as_local() {
            Some(local) => handle_diff_backup(local, &request.params).await,
            None => not_supported("diffBackup"),
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
        _ => CommandResponse::error(format!("不明な操作: {}", request.operation)),
    }
}

async fn handle_migrate(db: &dyn KijukuBackend) -> CommandResponse {
    match db.migrate().await {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("マイグレーションエラー: {}", e)),
    }
}

async fn handle_get_schema_version(db: &dyn KijukuBackend) -> CommandResponse {
    match db.get_schema_version().await {
        Ok(version) => CommandResponse::success(serde_json::json!({"version": version})),
        Err(e) => CommandResponse::error(format!("スキーマバージョン取得エラー: {}", e)),
    }
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

    let db = match KijukuDB::open_with_options(&ctx.db_path, DBOptions { verbose: ctx.verbose, ..Default::default() }) {
        Ok(db) => db,
        Err(e) => {
            let response = CommandResponse::error(format!("データベースのオープンに失敗: {}", e));
            output_response(&response);
            return;
        }
    };

    if let Err(e) = db.migrate() {
        let response = CommandResponse::error(format!("マイグレーションに失敗: {}", e));
        output_response(&response);
        return;
    }

    let update_options = UpdateExistOptions { dry_run };
    match db.update_exist(&filter, None, &update_options) {
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

fn open_and_migrate_db_for_json_output(ctx: &CliContext) -> Option<KijukuDB> {
    let db = match KijukuDB::open_with_options(&ctx.db_path, DBOptions { verbose: ctx.verbose, ..Default::default() }) {
        Ok(db) => db,
        Err(e) => {
            output_response(&CommandResponse::error(format!("データベースのオープンに失敗: {}", e)));
            return None;
        }
    };
    if let Err(e) = db.migrate() {
        output_response(&CommandResponse::error(format!("マイグレーションに失敗: {}", e)));
        return None;
    }
    Some(db)
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
    let db = match open_and_migrate_db_for_json_output(ctx) {
        Some(db) => db,
        None => return,
    };
    match db.check_thumbnail(&filter, None) {
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
    let db = match open_and_migrate_db_for_json_output(ctx) {
        Some(db) => db,
        None => return,
    };
    let thumbnail_options = ThumbnailOptions { dry_run, force };
    match db.update_thumbnail(&filter, None, &thumbnail_options) {
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
    let db = match open_and_migrate_db_for_json_output(ctx) {
        Some(db) => db,
        None => return,
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
                match db.find_media(&MediaFilter { id_in: None, ..Default::default() }, None) {
                    Ok(_) => {}
                    Err(e) => {
                        output_response(&CommandResponse::error(format!("メディア検索エラー: {}", e)));
                        return;
                    }
                }
                // UUIDからメディアを取得
                let media_list = db.find_media(&MediaFilter { ..Default::default() }, None).unwrap_or_default();
                let media = media_list.iter().find(|m| m.uuid == *u);
                if let Some(m) = media {
                    if !m.flag_exist || m.path.is_none() {
                        output_response(&CommandResponse::error("対象メディアはpathがないかflag_existがfalseです".to_string()));
                        return;
                    }
                    match db.compute_media_hash(&m.uuid, m.path.as_ref().unwrap(), m.media_type.as_str(), m.duration_sec) {
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
                match db.compute_media_hashes(&media_filter, None, *force) {
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
            match db.get_media_hashes(uuid) {
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
            match db.find_by_content_hash(&hash_bytes) {
                Ok(hashes) => {
                    let json: Vec<_> = hashes.iter().map(media_hash_to_json).collect();
                    output_response(&CommandResponse::success(serde_json::Value::Array(json)));
                }
                Err(e) => output_response(&CommandResponse::error(format!("ハッシュ検索エラー: {}", e))),
            }
        }
        HashCommands::Duplicates => {
            match db.find_duplicate_hashes() {
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
