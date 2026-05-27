//! Kijuku DB CLI
//!
//! JSON形式の入出力でリモート操作を可能にするCLIツール
//! Version: 0.1.0

use clap::Parser;
use include_dir::{include_dir, Dir};
use kijuku_db::{
    AttributeValueType, BackupInfo, BackupKind, BackupOptions, BackupScope, BackupSelector,
    BulkUpdateItem, DBOptions, KijukuDB, MediaFilter, MediaInput, MediaHashInput,
    MediaUpdateInput, QueryOptions, ThumbnailOptions, UpdateExistOptions,
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

    fn from_option<T: serde::Serialize>(option: Option<T>) -> Self {
        match option {
            Some(data) => match serde_json::to_value(data) {
                Ok(v) => Self::success(v),
                Err(e) => Self::error(format!("レスポンスのシリアライズに失敗: {}", e)),
            },
            None => Self::success(serde_json::Value::Null),
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
}

impl BackupSelectorJson {
    fn to_selector(&self) -> BackupSelector {
        match self {
            BackupSelectorJson::Latest => BackupSelector::latest(),
            BackupSelectorJson::Nth { n } => BackupSelector::nth(*n),
        }
    }
}

/// バックアップ復元のパラメータ
#[derive(Debug, Deserialize)]
struct RestoreParams {
    selector: Option<BackupSelectorJson>,
}

/// CLIのグローバルコンテキスト
struct CliContext {
    db_path: String,
    verbose: bool,
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
    serde_json::json!({
        "id": info.id,
        "name": info.name,
        "path": info.path.to_string_lossy(),
        "createdAt": created_at,
        "scope": scope,
        "kind": kind,
        "label": info.label,
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
#[command(name = "kijuku-cli")]
#[command(about = "きじゅくDB CLI", long_about = None)]
struct Cli {
    /// データベースファイルのパス
    #[arg(long, default_value = "kijuku.db")]
    db: String,

    /// 実行したSQLをstderrに出力する
    #[arg(long)]
    verbose: bool,

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
    },
    /// コンテンツハッシュ操作
    Hash {
        #[command(subcommand)]
        hash_command: HashCommands,
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

fn handle_restore_subcommand(ctx: &CliContext, nth: Option<usize>) {
    let mut db = match open_and_migrate_db(ctx) {
        Some(db) => db,
        None => return,
    };
    let selector = match nth {
        Some(n) => BackupSelector::nth(n),
        None => BackupSelector::latest(),
    };
    match db.restore(&selector) {
        Ok(path) => println!("{}", path.to_string_lossy()),
        Err(e) => eprintln!("復元に失敗: {}", e),
    }
}

fn handle_stdin(ctx: &CliContext) {
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

    // データベースを開く（バックアップ有効）
    let mut db = match KijukuDB::open_with_options(
        &ctx.db_path,
        DBOptions {
            backup: Some(BackupOptions::default()),
            verbose: ctx.verbose,
            ..Default::default()
        },
    ) {
        Ok(db) => db,
        Err(e) => {
            let response = CommandResponse::error(format!("データベースのオープンに失敗: {}", e));
            output_response(&response);
            return;
        }
    };

    // マイグレーションを実行
    if let Err(e) = db.migrate() {
        let response = CommandResponse::error(format!("マイグレーションに失敗: {}", e));
        output_response(&response);
        return;
    }

    // コマンドを実行
    let response = execute_command(&mut db, &request);
    output_response(&response);
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let ctx = CliContext {
        db_path: cli.db.clone(),
        verbose: cli.verbose,
    };

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
        Some(Commands::Restore { nth }) => {
            handle_restore_subcommand(&ctx, *nth);
        }
        Some(Commands::Hash { hash_command }) => {
            handle_hash_subcommand(&ctx, hash_command);
        }
        None => {
            handle_stdin(&ctx);
        }
    }
}

fn handle_backup(db: &mut KijukuDB, params: &serde_json::Value) -> CommandResponse {
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

fn handle_list_backups(db: &KijukuDB) -> CommandResponse {
    match db.list_backups() {
        Ok(backups) => {
            let json_backups: Vec<serde_json::Value> =
                backups.iter().map(backup_info_to_json).collect();
            CommandResponse::success(serde_json::Value::Array(json_backups))
        }
        Err(e) => CommandResponse::error(e.to_string()),
    }
}

fn handle_restore(db: &mut KijukuDB, params: &serde_json::Value) -> CommandResponse {
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

fn execute_command(db: &mut KijukuDB, request: &CommandRequest) -> CommandResponse {
    match request.operation.as_str() {
        "migrate" => handle_migrate(db),
        "getSchemaVersion" => handle_get_schema_version(db),
        "getTables" => handle_get_tables(db),
        "getTableInfo" => handle_get_table_info(db, &request.params),
        "createMedia" => handle_create_media(db, &request.params),
        "getMedia" => handle_get_media(db, &request.params),
        "updateMedia" => handle_update_media(db, &request.params),
        "deleteMedia" => handle_delete_media(db, &request.params),
        "findMedia" => handle_find_media(db, &request.params),
        "getDistinctValues" => handle_get_distinct_values(db, &request.params),
        "bulkCreateMedia" => handle_bulk_create_media(db, &request.params),
        "bulkDeleteMedia" => handle_bulk_delete_media(db, &request.params),
        "bulkUpdateMedia" => handle_bulk_update_media(db, &request.params),
        "createTag" => handle_create_tag(db, &request.params),
        "getTagByName" => handle_get_tag_by_name(db, &request.params),
        "getAllTags" => handle_get_all_tags(db),
        "addTagToMedia" => handle_add_tag_to_media(db, &request.params),
        "removeTagFromMedia" => handle_remove_tag_from_media(db, &request.params),
        "getMediaTags" => handle_get_media_tags(db, &request.params),
        "getTagUsageStats" => handle_get_tag_usage_stats(db),
        "findUnusedTags" => handle_find_unused_tags(db),
        "setMediaAttribute" => handle_set_media_attribute(db, &request.params),
        "getMediaAttribute" => handle_get_media_attribute(db, &request.params),
        "getMediaAttributes" => handle_get_media_attributes(db, &request.params),
        "deleteMediaAttribute" => handle_delete_media_attribute(db, &request.params),
        "deleteAllMediaAttributes" => handle_delete_all_media_attributes(db, &request.params),
        "updateExist" => handle_update_exist(db, &request.params),
        "checkThumbnail" => handle_check_thumbnail(db, &request.params),
        "updateThumbnail" => handle_update_thumbnail(db, &request.params),
        "addMediaHash" => handle_add_media_hash(db, &request.params),
        "addMediaHashes" => handle_add_media_hashes(db, &request.params),
        "getMediaHashes" => handle_get_media_hashes(db, &request.params),
        "getMediaHash" => handle_get_media_hash(db, &request.params),
        "findByContentHash" => handle_find_by_content_hash(db, &request.params),
        "deleteMediaHash" => handle_delete_media_hash(db, &request.params),
        "deleteMediaHashes" => handle_delete_media_hashes(db, &request.params),
        "findDuplicateHashes" => handle_find_duplicate_hashes(db),
        "computeMediaHash" => handle_compute_media_hash(db, &request.params),
        "computeMediaHashes" => handle_compute_media_hashes(db, &request.params),
        "backup" => handle_backup(db, &request.params),
        "listBackups" => handle_list_backups(db),
        "restore" => handle_restore(db, &request.params),
        _ => CommandResponse::error(format!("不明な操作: {}", request.operation)),
    }
}

fn handle_migrate(db: &KijukuDB) -> CommandResponse {
    match db.migrate() {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("マイグレーションエラー: {}", e)),
    }
}

fn handle_get_schema_version(db: &KijukuDB) -> CommandResponse {
    match db.get_schema_version() {
        Ok(version) => CommandResponse::success(serde_json::json!({"version": version})),
        Err(e) => CommandResponse::error(format!("スキーマバージョン取得エラー: {}", e)),
    }
}

fn handle_get_tables(db: &KijukuDB) -> CommandResponse {
    CommandResponse::from_result(db.get_tables(), "テーブル一覧取得エラー: ")
}

fn handle_get_table_info(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<GetTableInfoParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    CommandResponse::from_result(db.get_table_info(&params.table_name), "テーブル情報取得エラー: ")
}

fn handle_create_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<CreateMediaParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    CommandResponse::from_result(db.create_media(&params.data), "メディア作成エラー: ")
}

fn handle_get_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<GetMediaParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    CommandResponse::from_option(db.get_media(params.id))
}

fn handle_update_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<UpdateMediaParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    match db.update_media(params.id, &params.data) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("メディア更新エラー: {}", e)),
    }
}

fn handle_delete_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<DeleteMediaParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    match db.delete_media(params.id) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("メディア削除エラー: {}", e)),
    }
}

fn handle_find_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params = match deserialize_params::<FindMediaParams>(params) {
        Ok(p) => p, Err(e) => return e,
    };
    CommandResponse::from_result(db.find_media(&params.filter, params.options.as_ref()), "メディア検索エラー: ")
}

fn handle_get_distinct_values(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetDistinctValuesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    let fields: Vec<&str> = params.fields.iter().map(|s| s.as_str()).collect();
    match db.get_distinct_values(&fields, &params.filter) {
        Ok(values) => {
            let data = serde_json::to_value(values).unwrap();
            CommandResponse::success(data)
        }
        Err(e) => CommandResponse::error(format!("distinct値取得エラー: {}", e)),
    }
}

fn handle_bulk_create_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: BulkCreateMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.bulk_create_media(&params.data_list) {
        Ok(media_list) => match serde_json::to_value(media_list) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("一括作成エラー: {}", e)),
    }
}

fn handle_bulk_delete_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: BulkDeleteMediaParams = match BulkDeleteMediaParams::deserialize(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.bulk_delete_media(&params.ids) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("一括削除エラー: {}", e)),
    }
}

fn handle_bulk_update_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: BulkUpdateMediaParams = match BulkUpdateMediaParams::deserialize(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.bulk_update_media(&params.updates) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("一括更新エラー: {}", e)),
    }
}

fn handle_create_tag(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: CreateTagParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.create_tag(&params.name) {
        Ok(tag) => match serde_json::to_value(tag) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("タグ作成エラー: {}", e)),
    }
}

fn handle_get_tag_by_name(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetTagByNameParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_tag_by_name(&params.name) {
        Some(tag) => match serde_json::to_value(tag) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        None => CommandResponse::success(serde_json::Value::Null),
    }
}

fn handle_get_all_tags(db: &KijukuDB) -> CommandResponse {
    match db.get_all_tags() {
        Ok(tags) => match serde_json::to_value(tags) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("タグ取得エラー: {}", e)),
    }
}

fn handle_add_tag_to_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: AddTagToMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.add_tag_to_media(params.media_id, params.tag_id) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("タグ追加エラー: {}", e)),
    }
}

fn handle_remove_tag_from_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: RemoveTagFromMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.remove_tag_from_media(params.media_id, params.tag_id) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("タグ削除エラー: {}", e)),
    }
}

fn handle_get_media_tags(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaTagsParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_media_tags(params.media_id) {
        Ok(tags) => match serde_json::to_value(tags) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("メディアタグ取得エラー: {}", e)),
    }
}

fn handle_get_tag_usage_stats(db: &KijukuDB) -> CommandResponse {
    match db.get_tag_usage_stats() {
        Ok(stats) => match serde_json::to_value(stats) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("タグ使用統計取得エラー: {}", e)),
    }
}

fn handle_find_unused_tags(db: &KijukuDB) -> CommandResponse {
    match db.find_unused_tags() {
        Ok(tags) => match serde_json::to_value(tags) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("未使用タグ取得エラー: {}", e)),
    }
}

fn handle_set_media_attribute(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
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
    ) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("属性設定エラー: {}", e)),
    }
}

fn handle_get_media_attribute(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaAttributeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_media_attribute(params.media_id, &params.key) {
        Ok(Some(attr)) => match serde_json::to_value(attr) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Ok(None) => CommandResponse::success(serde_json::Value::Null),
        Err(e) => CommandResponse::error(format!("属性取得エラー: {}", e)),
    }
}

fn handle_get_media_attributes(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaAttributesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_media_attributes(params.media_id) {
        Ok(attrs) => match serde_json::to_value(attrs) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("属性取得エラー: {}", e)),
    }
}

fn handle_delete_media_attribute(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: DeleteMediaAttributeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.delete_media_attribute(params.media_id, &params.key) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("属性削除エラー: {}", e)),
    }
}

fn handle_delete_all_media_attributes(
    db: &KijukuDB,
    params: &serde_json::Value,
) -> CommandResponse {
    let params: DeleteAllMediaAttributesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.delete_all_media_attributes(params.media_id) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("全属性削除エラー: {}", e)),
    }
}

fn handle_update_exist(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
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

fn handle_check_thumbnail(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
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

fn handle_update_thumbnail(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
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

fn handle_add_media_hash(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: AddMediaHashParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.add_media_hash(&params.input) {
        Ok(hash) => CommandResponse::success(media_hash_to_json(&hash)),
        Err(e) => CommandResponse::error(format!("ハッシュ追加エラー: {}", e)),
    }
}

fn handle_add_media_hashes(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: AddMediaHashesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.add_media_hashes(&params.inputs) {
        Ok(hashes) => {
            let json: Vec<_> = hashes.iter().map(media_hash_to_json).collect();
            CommandResponse::success(serde_json::Value::Array(json))
        }
        Err(e) => CommandResponse::error(format!("ハッシュ一括追加エラー: {}", e)),
    }
}

fn handle_get_media_hashes(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaHashesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.get_media_hashes(&params.item_uuid) {
        Ok(hashes) => {
            let json: Vec<_> = hashes.iter().map(media_hash_to_json).collect();
            CommandResponse::success(serde_json::Value::Array(json))
        }
        Err(e) => CommandResponse::error(format!("ハッシュ取得エラー: {}", e)),
    }
}

fn handle_get_media_hash(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaHashParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.get_media_hash(&params.item_uuid, &params.filename, &params.time_range) {
        Ok(Some(hash)) => CommandResponse::success(media_hash_to_json(&hash)),
        Ok(None) => CommandResponse::success(serde_json::Value::Null),
        Err(e) => CommandResponse::error(format!("ハッシュ取得エラー: {}", e)),
    }
}

fn handle_find_by_content_hash(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: FindByContentHashParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    let hash_bytes = match kijuku_db::hash::hex_to_bytes(&params.hash_hex) {
        Ok(b) => b,
        Err(e) => return CommandResponse::error(format!("ハッシュ値エラー: {}", e)),
    };
    match db.find_by_content_hash(&hash_bytes) {
        Ok(hashes) => {
            let json: Vec<_> = hashes.iter().map(media_hash_to_json).collect();
            CommandResponse::success(serde_json::Value::Array(json))
        }
        Err(e) => CommandResponse::error(format!("ハッシュ検索エラー: {}", e)),
    }
}

fn handle_delete_media_hash(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: DeleteMediaHashParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.delete_media_hash(&params.item_uuid, &params.filename, &params.time_range) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("ハッシュ削除エラー: {}", e)),
    }
}

fn handle_delete_media_hashes(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: DeleteMediaHashesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };
    match db.delete_media_hashes(&params.item_uuid) {
        Ok(_) => CommandResponse::ack(),
        Err(e) => CommandResponse::error(format!("ハッシュ全削除エラー: {}", e)),
    }
}

fn handle_find_duplicate_hashes(db: &KijukuDB) -> CommandResponse {
    match db.find_duplicate_hashes() {
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

fn handle_compute_media_hash(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
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

fn handle_compute_media_hashes(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
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
