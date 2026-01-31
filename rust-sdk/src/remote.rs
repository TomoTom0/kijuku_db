use crate::{BulkUpdateItem, KijukuError, Media, MediaAttribute, MediaFilter, MediaInput, MediaUpdateInput, QueryOptions, Result, TableColumnInfo, Tag, TagUsageStats};
use serde::{Deserialize, Serialize};
use ssh2::Session;
use ssh2_config::{ParseRule, SshConfig};
use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::env;

/// リモート接続設定
#[derive(Debug, Clone)]
pub struct RemoteConfig {
    /// SSHホスト名
    pub ssh_host: String,
    /// SSHポート
    pub port: Option<u16>,
    /// SSHユーザー名
    pub username: String,
    /// SSH秘密鍵のパス
    pub private_key_path: Option<PathBuf>,
    /// リモートのDBパス
    pub db_path: Option<String>,
    /// リモートのバイナリパス
    pub binary_path: Option<String>,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            ssh_host: String::from("localhost"),
            port: Some(22),
            username: String::new(),
            private_key_path: None,
            db_path: Some(String::from("~/.local/share/kijuku/kijuku.db")),
            binary_path: Some(String::from("~/.local/bin/kijuku-cli")),
        }
    }
}

/// コマンドリクエスト
#[derive(Debug, Serialize)]
struct CommandRequest {
    operation: String,
    params: serde_json::Value,
}

/// コマンドレスポンス
#[derive(Debug, Deserialize)]
struct CommandResponse {
    success: bool,
    data: Option<serde_json::Value>,
    error: Option<String>,
}

/// リモートKijuku DB操作クラス
pub struct RemoteKijukuDB {
    config: RemoteConfig,
}

impl RemoteKijukuDB {
    /// 新しいRemoteKijukuDBインスタンスを作成
    pub fn new(config: RemoteConfig) -> Self {
        Self { config }
    }

    /// SSH設定を読み込む
    fn load_ssh_config(&self) -> Result<(String, u16, String, Option<PathBuf>)> {
        let home = env::var("HOME").map_err(|_| KijukuError::Other("HOME environment variable not set".to_string()))?;
        let ssh_config_path = PathBuf::from(&home).join(".ssh").join("config");

        // SSH configが存在しない場合はデフォルト値を使用
        if !ssh_config_path.exists() {
            return Ok((
                self.config.ssh_host.clone(),
                self.config.port.unwrap_or(22),
                self.config.username.clone(),
                self.config.private_key_path.clone(),
            ));
        }

        let config_content = std::fs::read_to_string(&ssh_config_path)
            .map_err(|e| KijukuError::Other(format!("Failed to read SSH config: {}", e)))?;

        let mut reader = std::io::Cursor::new(config_content.as_bytes());
        let ssh_config = SshConfig::default()
            .parse(&mut reader, ParseRule::ALLOW_UNKNOWN_FIELDS)
            .map_err(|e| KijukuError::Other(format!("Failed to parse SSH config: {}", e)))?;

        // ホスト設定を取得
        let params = ssh_config.query(&self.config.ssh_host);

        // 各フィールドを取得（明示的な設定が優先）
        let hostname = params.host_name.unwrap_or_else(|| self.config.ssh_host.clone());
        let port = self.config.port.or(params.port).unwrap_or(22);
        let username = if !self.config.username.is_empty() {
            self.config.username.clone()
        } else {
            params.user.unwrap_or_else(|| env::var("USER").unwrap_or_default())
        };
        let identity_file = self.config.private_key_path.clone().or_else(|| {
            params.identity_file.and_then(|files| {
                files.first().map(|path| {
                    // ~/ を展開
                    let path_str = path.to_string_lossy();
                    if path_str.starts_with("~/") {
                        PathBuf::from(&home).join(&path_str[2..])
                    } else {
                        path.clone()
                    }
                })
            })
        });

        Ok((hostname, port, username, identity_file))
    }

    /// SSH接続を確立
    fn connect(&self) -> Result<Session> {
        let (hostname, port, username, identity_file) = self.load_ssh_config()?;

        let tcp = TcpStream::connect(format!("{}:{}", hostname, port))
            .map_err(|e| KijukuError::Other(format!("TCP connection failed: {}", e)))?;

        let mut sess = Session::new()
            .map_err(|e| KijukuError::Other(format!("SSH session creation failed: {}", e)))?;

        sess.set_tcp_stream(tcp);
        sess.handshake()
            .map_err(|e| KijukuError::Other(format!("SSH handshake failed: {}", e)))?;

        // 認証
        if let Some(key_path) = &identity_file {
            sess.userauth_pubkey_file(&username, None, key_path, None)
                .map_err(|e| KijukuError::Other(format!("SSH authentication failed: {}", e)))?;
        } else {
            return Err(KijukuError::Other(
                "Private key path is required (set in RemoteConfig or SSH config)".to_string(),
            ));
        }

        if !sess.authenticated() {
            return Err(KijukuError::Other(
                "SSH authentication failed".to_string(),
            ));
        }

        Ok(sess)
    }

    /// リモートでJSONコマンドを実行
    fn execute_remote_command(&self, request: CommandRequest) -> Result<CommandResponse> {
        let remote_db_path = self
            .config
            .db_path
            .as_deref()
            .unwrap_or("~/.local/share/kijuku/kijuku.db");
        let remote_binary_path = self
            .config
            .binary_path
            .as_deref()
            .unwrap_or("~/.local/bin/kijuku-cli");

        let json_input = serde_json::to_string(&request)
            .map_err(|e| KijukuError::Other(format!("Failed to serialize request: {}", e)))?;

        let sess = self.connect()?;
        let mut channel = sess
            .channel_session()
            .map_err(|e| KijukuError::Other(format!("Failed to open channel: {}", e)))?;

        let command = format!("{} --db {}", remote_binary_path, remote_db_path);
        channel
            .exec(&command)
            .map_err(|e| KijukuError::Other(format!("Failed to execute command: {}", e)))?;

        use std::io::Write;
        channel
            .write_all(json_input.as_bytes())
            .map_err(|e| KijukuError::Other(format!("Failed to write to channel stdin: {}", e)))?;
        channel.send_eof().map_err(|e| KijukuError::Other(format!("Failed to send EOF to channel: {}", e)))?;

        let mut output = String::new();
        channel
            .read_to_string(&mut output)
            .map_err(|e| KijukuError::Other(format!("Failed to read command stdout: {}", e)))?;

        let mut stderr = String::new();
        channel.stderr().read_to_string(&mut stderr).map_err(|e| KijukuError::Other(format!("Failed to read command stderr: {}", e)))?;

        channel.wait_close().ok();

        let exit_status = channel.exit_status()
            .map_err(|e| KijukuError::Other(format!("Failed to get exit status: {}", e)))?;

        if exit_status != 0 {
            return Err(KijukuError::Other(format!(
                "Command failed with exit code {}. stdout: {}, stderr: {}",
                exit_status, output, stderr
            )));
        }

        serde_json::from_str(&output)
            .map_err(|e| KijukuError::Other(format!("Failed to parse response from stdout: {}. Raw output: {}", e, output)))
    }

    /// レスポンスのエラーチェック
    fn check_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: CommandResponse,
    ) -> Result<T> {
        if !response.success {
            return Err(KijukuError::Other(
                response.error.unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        serde_json::from_value(response.data.unwrap_or(serde_json::Value::Null))
            .map_err(|e| KijukuError::Other(format!("Failed to deserialize data: {}", e)))
    }

    /// マイグレーションを実行
    pub fn migrate(&self) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "migrate".to_string(),
            params: serde_json::json!({}),
        })?;

        self.check_response::<()>(response)
    }

    /// 現在のスキーマバージョンを取得
    pub fn get_schema_version(&self) -> Result<i64> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getSchemaVersion".to_string(),
            params: serde_json::json!({}),
        })?;

        #[derive(Deserialize)]
        struct VersionResponse {
            version: i64,
        }

        let data: VersionResponse = self.check_response(response)?;
        Ok(data.version)
    }

    /// テーブル一覧を取得
    pub fn get_tables(&self) -> Result<Vec<String>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getTables".to_string(),
            params: serde_json::json!({}),
        })?;

        self.check_response(response)
    }

    /// テーブルのカラム情報を取得
    pub fn get_table_info(&self, table_name: &str) -> Result<Vec<TableColumnInfo>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getTableInfo".to_string(),
            params: serde_json::json!({ "table_name": table_name }),
        })?;

        self.check_response(response)
    }

    /// メディアを作成
    pub fn create_media(&self, data: &MediaInput) -> Result<Media> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "createMedia".to_string(),
            params: serde_json::json!({ "data": data }),
        })?;

        self.check_response(response)
    }

    /// IDでメディアを取得
    pub fn get_media(&self, id: i64) -> Result<Option<Media>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getMedia".to_string(),
            params: serde_json::json!({ "id": id }),
        })?;

        self.check_response(response)
    }

    /// メディアを更新（部分更新）
    ///
    /// 指定されたフィールドのみ更新します。
    pub fn update_media(&self, id: i64, data: &MediaUpdateInput) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "updateMedia".to_string(),
            params: serde_json::json!({ "id": id, "data": data }),
        })?;

        self.check_response(response)
    }

    /// メディアを削除
    pub fn delete_media(&self, id: i64) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "deleteMedia".to_string(),
            params: serde_json::json!({ "id": id }),
        })?;

        self.check_response(response)
    }

    /// メディアを検索
    pub fn find_media(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
    ) -> Result<Vec<Media>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "findMedia".to_string(),
            params: serde_json::json!({ "filter": filter, "options": options }),
        })?;

        self.check_response(response)
    }

    /// 複数のメディアを一括作成
    pub fn bulk_create_media(&self, data_list: &[MediaInput]) -> Result<Vec<Media>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "bulkCreateMedia".to_string(),
            params: serde_json::json!({ "data_list": data_list }),
        })?;

        self.check_response(response)
    }

    /// 複数のメディアを一括削除
    pub fn bulk_delete_media(&self, ids: &[i64]) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "bulkDeleteMedia".to_string(),
            params: serde_json::json!({ "ids": ids }),
        })?;

        #[derive(Deserialize)]
        struct DeleteResponse {
            #[allow(dead_code)]
            deleted: bool,
        }

        let _data: DeleteResponse = self.check_response(response)?;
        Ok(())
    }

    /// 複数のメディアを一括更新
    pub fn bulk_update_media(&self, updates: &[BulkUpdateItem]) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "bulkUpdateMedia".to_string(),
            params: serde_json::json!({ "updates": updates }),
        })?;

        #[derive(Deserialize)]
        struct UpdateResponse {
            #[allow(dead_code)]
            updated: bool,
        }

        let _data: UpdateResponse = self.check_response(response)?;
        Ok(())
    }

    /// タグを作成
    pub fn create_tag(&self, name: &str) -> Result<Tag> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "createTag".to_string(),
            params: serde_json::json!({ "name": name }),
        })?;

        self.check_response(response)
    }

    /// タグ名でタグを取得
    pub fn get_tag_by_name(&self, name: &str) -> Result<Option<Tag>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getTagByName".to_string(),
            params: serde_json::json!({ "name": name }),
        })?;

        self.check_response(response)
    }

    /// 全てのタグを取得
    pub fn get_all_tags(&self) -> Result<Vec<Tag>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getAllTags".to_string(),
            params: serde_json::json!({}),
        })?;

        self.check_response(response)
    }

    /// メディアにタグを追加
    pub fn add_tag_to_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "addTagToMedia".to_string(),
            params: serde_json::json!({ "media_id": media_id, "tag_id": tag_id }),
        })?;

        self.check_response(response)
    }

    /// メディアからタグを削除
    pub fn remove_tag_from_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "removeTagFromMedia".to_string(),
            params: serde_json::json!({ "media_id": media_id, "tag_id": tag_id }),
        })?;

        self.check_response(response)
    }

    /// メディアに関連付けられたタグを取得
    pub fn get_media_tags(&self, media_id: i64) -> Result<Vec<Tag>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getMediaTags".to_string(),
            params: serde_json::json!({ "media_id": media_id }),
        })?;

        self.check_response(response)
    }

    /// タグの使用数統計を取得
    pub fn get_tag_usage_stats(&self) -> Result<Vec<TagUsageStats>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getTagUsageStats".to_string(),
            params: serde_json::json!({}),
        })?;

        self.check_response(response)
    }

    /// 未使用のタグを取得
    pub fn find_unused_tags(&self) -> Result<Vec<Tag>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "findUnusedTags".to_string(),
            params: serde_json::json!({}),
        })?;

        self.check_response(response)
    }

    /// メディアに属性を設定
    pub fn set_media_attribute(
        &self,
        media_id: i64,
        key: &str,
        value: Option<&str>,
        value_type: Option<&str>,
    ) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "setMediaAttribute".to_string(),
            params: serde_json::json!({
                "media_id": media_id,
                "key": key,
                "value": value,
                "value_type": value_type
            }),
        })?;

        self.check_response(response)
    }

    /// メディアの属性を取得
    pub fn get_media_attribute(&self, media_id: i64, key: &str) -> Result<Option<MediaAttribute>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getMediaAttribute".to_string(),
            params: serde_json::json!({ "media_id": media_id, "key": key }),
        })?;

        self.check_response(response)
    }

    /// メディアの全ての属性を取得
    pub fn get_media_attributes(&self, media_id: i64) -> Result<Vec<MediaAttribute>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getMediaAttributes".to_string(),
            params: serde_json::json!({ "media_id": media_id }),
        })?;

        self.check_response(response)
    }

    /// メディアの属性を削除
    pub fn delete_media_attribute(&self, media_id: i64, key: &str) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "deleteMediaAttribute".to_string(),
            params: serde_json::json!({ "media_id": media_id, "key": key }),
        })?;

        self.check_response(response)
    }

    /// メディアの全ての属性を削除
    pub fn delete_all_media_attributes(&self, media_id: i64) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "deleteAllMediaAttributes".to_string(),
            params: serde_json::json!({ "media_id": media_id }),
        })?;

        self.check_response(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_config_default() {
        let config = RemoteConfig::default();
        assert_eq!(config.ssh_host, "localhost");
        assert_eq!(config.port, Some(22));
        assert_eq!(
            config.db_path,
            Some("~/.local/share/kijuku/kijuku.db".to_string())
        );
        assert_eq!(
            config.binary_path,
            Some("~/.local/bin/kijuku-cli".to_string())
        );
    }

    #[test]
    fn test_remote_config_creation() {
        let config = RemoteConfig {
            ssh_host: "example.com".to_string(),
            port: Some(2222),
            username: "testuser".to_string(),
            private_key_path: Some(PathBuf::from("/home/user/.ssh/id_rsa")),
            db_path: Some("/path/to/db.db".to_string()),
            binary_path: Some("/path/to/binary".to_string()),
        };

        assert_eq!(config.ssh_host, "example.com");
        assert_eq!(config.port, Some(2222));
        assert_eq!(config.username, "testuser");
    }
}
