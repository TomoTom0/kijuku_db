//! Kijuku DB CLI
//!
//! JSON形式の入出力でリモート操作を可能にするCLIツール

use clap::Parser;
use kijuku_db::{
    AttributeValueType, KijukuDB, MediaFilter, MediaInput, QueryOptions,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

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

/// メディア更新のパラメータ
#[derive(Debug, Deserialize)]
struct UpdateMediaParams {
    id: i64,
    data: MediaInput,
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

fn main() {
    #[derive(Parser)]
    #[command(name = "kijuku-cli")]
    #[command(about = "きじゅくDB CLI", long_about = None)]
    struct Cli {
        #[arg(long)]
        db: String,
    }

    let cli = Cli::parse();
    let db_path = &cli.db;

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

    // データベースを開く
    let db = match KijukuDB::open(db_path) {
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
    let response = execute_command(&db, &request);
    output_response(&response);
}

fn execute_command(db: &KijukuDB, request: &CommandRequest) -> CommandResponse {
    match request.operation.as_str() {
        "migrate" => handle_migrate(db),
        "getSchemaVersion" => handle_get_schema_version(db),
        "createMedia" => handle_create_media(db, &request.params),
        "getMedia" => handle_get_media(db, &request.params),
        "updateMedia" => handle_update_media(db, &request.params),
        "deleteMedia" => handle_delete_media(db, &request.params),
        "findMedia" => handle_find_media(db, &request.params),
        "bulkCreateMedia" => handle_bulk_create_media(db, &request.params),
        "createTag" => handle_create_tag(db, &request.params),
        "getTagByName" => handle_get_tag_by_name(db, &request.params),
        "getAllTags" => handle_get_all_tags(db),
        "addTagToMedia" => handle_add_tag_to_media(db, &request.params),
        "removeTagFromMedia" => handle_remove_tag_from_media(db, &request.params),
        "getMediaTags" => handle_get_media_tags(db, &request.params),
        "setMediaAttribute" => handle_set_media_attribute(db, &request.params),
        "getMediaAttribute" => handle_get_media_attribute(db, &request.params),
        "getMediaAttributes" => handle_get_media_attributes(db, &request.params),
        "deleteMediaAttribute" => handle_delete_media_attribute(db, &request.params),
        "deleteAllMediaAttributes" => handle_delete_all_media_attributes(db, &request.params),
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
