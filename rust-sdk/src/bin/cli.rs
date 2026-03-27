//! Kijuku DB CLI
//!
//! JSON形式の入出力でリモート操作を可能にするCLIツール
//! Version: 0.1.0

use clap::Parser;
use include_dir::{include_dir, Dir};
use kijuku_db::{
    AttributeValueType, BackupInfo, BackupKind, BackupOptions, BackupScope, BackupSelector,
    BulkUpdateItem, DBOptions, KijukuDB, MediaFilter, MediaInput, MediaUpdateInput, QueryOptions,
    UpdateExistOptions,
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

/// update-existのパラメータ
#[derive(Debug, Deserialize)]
struct UpdateExistParams {
    #[serde(default)]
    filter: MediaFilter,
    options: Option<QueryOptions>,
    #[serde(default)]
    update_options: UpdateExistOptions,
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
}

async fn handle_server(db_path: &str, port: u16, password: Option<String>) {
    match kijuku_db::KijukuDB::open(db_path) {
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

fn open_db_with_backup(db_path: &str) -> Option<KijukuDB> {
    match KijukuDB::open_with_options(
        db_path,
        DBOptions {
            backup: Some(BackupOptions::default()),
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

fn open_and_migrate_db(db_path: &str) -> Option<KijukuDB> {
    let db = open_db_with_backup(db_path)?;
    if let Err(e) = db.migrate() {
        eprintln!("マイグレーションに失敗: {}", e);
        return None;
    }
    Some(db)
}

fn handle_backup_subcommand(db_path: &str, label: Option<String>) {
    let db = match open_and_migrate_db(db_path) {
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

fn handle_list_backups_subcommand(db_path: &str) {
    let db = match open_and_migrate_db(db_path) {
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

fn handle_restore_subcommand(db_path: &str, nth: Option<usize>) {
    let mut db = match open_and_migrate_db(db_path) {
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

fn handle_stdin(db_path: &str) {
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
        db_path,
        DBOptions {
            backup: Some(BackupOptions::default()),
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
    let db_path = &cli.db;

    match &cli.command {
        Some(Commands::Docs { doc_type }) => {
            show_docs(doc_type);
        }
        Some(Commands::Server { port, password }) => {
            handle_server(db_path, *port, password.clone()).await;
        }
        Some(Commands::UpdateExist { dry_run, filter }) => {
            handle_update_exist_subcommand(db_path, *dry_run, filter);
        }
        Some(Commands::Backup { label }) => {
            handle_backup_subcommand(db_path, label.clone());
        }
        Some(Commands::ListBackups) => {
            handle_list_backups_subcommand(db_path);
        }
        Some(Commands::Restore { nth }) => {
            handle_restore_subcommand(db_path, *nth);
        }
        None => {
            handle_stdin(db_path);
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
        "backup" => handle_backup(db, &request.params),
        "listBackups" => handle_list_backups(db),
        "restore" => handle_restore(db, &request.params),
        _ => CommandResponse::error(format!("不明な操作: {}", request.operation)),
    }
}

fn handle_migrate(db: &KijukuDB) -> CommandResponse {
    match db.migrate() {
        Ok(_) => CommandResponse::success(serde_json::json!({"migrated": true})),
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
    match db.get_tables() {
        Ok(tables) => match serde_json::to_value(tables) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("レスポンスのシリアライズに失敗: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("テーブル一覧取得エラー: {}", e)),
    }
}

fn handle_get_table_info(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetTableInfoParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_table_info(&params.table_name) {
        Ok(columns) => match serde_json::to_value(columns) {
            Ok(data) => CommandResponse::success(data),
            Err(e) => CommandResponse::error(format!("テーブル情報取得エラー: {}", e)),
        },
        Err(e) => CommandResponse::error(format!("テーブル情報取得エラー: {}", e)),
    }
}

fn handle_create_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: CreateMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.create_media(&params.data) {
        Ok(media) => {
            let data = serde_json::to_value(media).unwrap();
            CommandResponse::success(data)
        }
        Err(e) => CommandResponse::error(format!("メディア作成エラー: {}", e)),
    }
}

fn handle_get_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_media(params.id) {
        Some(media) => {
            let data = serde_json::to_value(media).unwrap();
            CommandResponse::success(data)
        }
        None => CommandResponse::error(format!("メディアが見つかりません (id: {})", params.id)),
    }
}

fn handle_update_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: UpdateMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.update_media(params.id, &params.data) {
        Ok(_) => CommandResponse::success(serde_json::json!({"updated": true})),
        Err(e) => CommandResponse::error(format!("メディア更新エラー: {}", e)),
    }
}

fn handle_delete_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: DeleteMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.delete_media(params.id) {
        Ok(_) => CommandResponse::success(serde_json::json!({"deleted": true})),
        Err(e) => CommandResponse::error(format!("メディア削除エラー: {}", e)),
    }
}

fn handle_find_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: FindMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.find_media(&params.filter, params.options.as_ref()) {
        Ok(media_list) => {
            let data = serde_json::to_value(media_list).unwrap();
            CommandResponse::success(data)
        }
        Err(e) => CommandResponse::error(format!("メディア検索エラー: {}", e)),
    }
}

fn handle_bulk_create_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: BulkCreateMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.bulk_create_media(&params.data_list) {
        Ok(media_list) => {
            let data = serde_json::to_value(media_list).unwrap();
            CommandResponse::success(data)
        }
        Err(e) => CommandResponse::error(format!("一括作成エラー: {}", e)),
    }
}

fn handle_bulk_delete_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: BulkDeleteMediaParams = match BulkDeleteMediaParams::deserialize(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.bulk_delete_media(&params.ids) {
        Ok(_) => CommandResponse::success(serde_json::json!({"deleted": true})),
        Err(e) => CommandResponse::error(format!("一括削除エラー: {}", e)),
    }
}

fn handle_bulk_update_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: BulkUpdateMediaParams = match BulkUpdateMediaParams::deserialize(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.bulk_update_media(&params.updates) {
        Ok(_) => CommandResponse::success(serde_json::json!({"updated": true})),
        Err(e) => CommandResponse::error(format!("一括更新エラー: {}", e)),
    }
}

fn handle_create_tag(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: CreateTagParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.create_tag(&params.name) {
        Ok(tag) => {
            let data = serde_json::to_value(tag).unwrap();
            CommandResponse::success(data)
        }
        Err(e) => CommandResponse::error(format!("タグ作成エラー: {}", e)),
    }
}

fn handle_get_tag_by_name(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetTagByNameParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_tag_by_name(&params.name) {
        Some(tag) => {
            let data = serde_json::to_value(tag).unwrap();
            CommandResponse::success(data)
        }
        None => CommandResponse::error(format!("タグが見つかりません (name: {})", params.name)),
    }
}

fn handle_get_all_tags(db: &KijukuDB) -> CommandResponse {
    match db.get_all_tags() {
        Ok(tags) => {
            let data = serde_json::to_value(tags).unwrap();
            CommandResponse::success(data)
        }
        Err(e) => CommandResponse::error(format!("タグ取得エラー: {}", e)),
    }
}

fn handle_add_tag_to_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: AddTagToMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.add_tag_to_media(params.media_id, params.tag_id) {
        Ok(_) => CommandResponse::success(serde_json::json!({"added": true})),
        Err(e) => CommandResponse::error(format!("タグ追加エラー: {}", e)),
    }
}

fn handle_remove_tag_from_media(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: RemoveTagFromMediaParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.remove_tag_from_media(params.media_id, params.tag_id) {
        Ok(_) => CommandResponse::success(serde_json::json!({"removed": true})),
        Err(e) => CommandResponse::error(format!("タグ削除エラー: {}", e)),
    }
}

fn handle_get_media_tags(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaTagsParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_media_tags(params.media_id) {
        Ok(tags) => {
            let data = serde_json::to_value(tags).unwrap();
            CommandResponse::success(data)
        }
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
        Ok(_) => CommandResponse::success(serde_json::json!({"set": true})),
        Err(e) => CommandResponse::error(format!("属性設定エラー: {}", e)),
    }
}

fn handle_get_media_attribute(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaAttributeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_media_attribute(params.media_id, &params.key) {
        Ok(Some(attr)) => {
            let data = serde_json::to_value(attr).unwrap();
            CommandResponse::success(data)
        }
        Ok(None) => CommandResponse::error(format!(
            "属性が見つかりません (media_id: {}, key: {})",
            params.media_id, params.key
        )),
        Err(e) => CommandResponse::error(format!("属性取得エラー: {}", e)),
    }
}

fn handle_get_media_attributes(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: GetMediaAttributesParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.get_media_attributes(params.media_id) {
        Ok(attrs) => {
            let data = serde_json::to_value(attrs).unwrap();
            CommandResponse::success(data)
        }
        Err(e) => CommandResponse::error(format!("属性取得エラー: {}", e)),
    }
}

fn handle_delete_media_attribute(db: &KijukuDB, params: &serde_json::Value) -> CommandResponse {
    let params: DeleteMediaAttributeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("パラメータエラー: {}", e)),
    };

    match db.delete_media_attribute(params.media_id, &params.key) {
        Ok(_) => CommandResponse::success(serde_json::json!({"deleted": true})),
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
        Ok(_) => CommandResponse::success(serde_json::json!({"deleted": true})),
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

fn handle_update_exist_subcommand(db_path: &str, dry_run: bool, filter_json: &str) {
    let filter: MediaFilter = match serde_json::from_str(filter_json) {
        Ok(f) => f,
        Err(e) => {
            let response = CommandResponse::error(format!("filterのJSONパースエラー: {}", e));
            output_response(&response);
            return;
        }
    };

    let db = match KijukuDB::open(db_path) {
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
