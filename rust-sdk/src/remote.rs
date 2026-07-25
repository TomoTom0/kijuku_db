use crate::{AttributeValueType, BackupInfo, BulkUpdateItem, CheckThumbnailResult, KijukuBackend, KijukuError, Media, MediaAttribute, MediaFilter, MediaHash, MediaHashInput, MediaInput, MediaUpdateInput, QueryOptions, Result, TableColumnInfo, Tag, TagUsageStats, ThumbnailOptions, UpdateExistOptions, UpdateExistResult, UpdateThumbnailResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ssh2::Session;
use ssh2_config::{ParseRule, SshConfig};
use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::env;

use crate::config::Target;

/// リモート接続設定
#[derive(Debug, Clone)]
pub struct RemoteConfig {
    /// SSHホスト名
    pub ssh_host: String,
    /// SSHポート
    pub port: Option<u16>,
    /// SSHユーザー名
    pub username: Option<String>,
    /// SSH秘密鍵のパス
    pub private_key_path: Option<PathBuf>,
    /// リモートの prod DB パス
    pub db_path: Option<String>,
    /// リモートの stg DB パス（None 時は db_path から `<stem>.stg.db` を導出・設計 §4.1）
    pub stg_db_path: Option<String>,
    /// リモートのバイナリパス
    pub binary_path: Option<String>,
    /// リモートホスト上の media root（ファイル操作APIのサンドボックス境界）。
    /// 未設定の場合、リモートでのファイル操作APIはエラーで拒否される。
    pub media_root: Option<String>,
    /// 操作対象 DB（未設定 = デフォルト stg・設計 §13）。prod は readonly + migrate skip。
    pub target: Target,
    /// 読込先 DB（未指定時は target に従う・設計 §3.5）。prod 指定で prod RO 読込専用セッション。
    /// `resolve_target` と同じ方式Aで target に折り畳む（read_source が Some なら優先）。
    pub read_source: Option<Target>,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            ssh_host: String::from("localhost"),
            port: Some(22),
            username: None,
            private_key_path: None,
            db_path: Some(String::from("~/.local/share/kijuku/kijuku.db")),
            stg_db_path: None,
            binary_path: Some(String::from("~/.local/bin/kijuku-cli")),
            media_root: None,
            target: Target::default(),
            read_source: None,
        }
    }
}

impl RemoteConfig {
    /// read-source を target に折り畳んだ実効 target（方式A・設計 §3.5/§13）。
    /// read_source が Some ならそちらを優先（prod RO 読込専用セッション）。
    pub fn effective_target(&self) -> Target {
        self.read_source.unwrap_or(self.target)
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
#[derive(Clone)]
pub struct RemoteKijukuDB {
    config: RemoteConfig,
}

/// リモートシェルコマンドに埋め込むパスを安全にクォートする。
/// `~` / `~/...` は `$HOME` に展開し、それ以外はシングルクォートで囲む（`'` はエスケープ）。
fn escape_for_remote_shell(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/") {
        return format!("\"$HOME\"/'{}'", rest.replace('\'', "'\\''"));
    }
    if s == "~" {
        return "\"$HOME\"".to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// リモートの prod DB パスから stg DB パスを導出する（設計 §4.1）。
///
/// `<stem>.db` → `<stem>.stg.db`（拡張子の前に `.stg` を挿入）。
/// 拡張子無しは `<path>.stg.db`。`~` 含むリモートパスもそのまま処理（rfind で最後の `.` を使用）。
fn derive_stg_db_path(prod_path: &str) -> String {
    match prod_path.rfind('.') {
        // ディレクトリ区切りより後ろにある最後の `.` を拡張子区切りとみなす
        Some(dot) if dot > prod_path.rfind('/').unwrap_or(0) => {
            format!("{}.stg{}", &prod_path[..dot], &prod_path[dot..])
        }
        _ => format!("{}.stg.db", prod_path),
    }
}

/// `RemoteConfig` から target に応じたリモート DB パスを解決する（設計 §4.1・§13）。
///
/// - prod: `db_path`（未設定時は prod デフォルト）
/// - stg: `stg_db_path`、未設定なら `db_path` から導出、それも無ければ stg デフォルト
fn resolve_remote_db_path(config: &RemoteConfig) -> String {
    const DEFAULT_PROD: &str = "~/.local/share/kijuku/kijuku.db";
    const DEFAULT_STG: &str = "~/.local/share/kijuku/kijuku.stg.db";
    // read-source を target に折り畳む（方式A・設計 §3.5/§13）。
    match config.effective_target() {
        Target::Prod => config
            .db_path
            .clone()
            .unwrap_or_else(|| DEFAULT_PROD.to_string()),
        Target::Stg => config
            .stg_db_path
            .clone()
            .or_else(|| config.db_path.as_deref().map(derive_stg_db_path))
            .unwrap_or_else(|| DEFAULT_STG.to_string()),
    }
}

/// リモートで実行する CLI コマンド文字列を組み立てる（テスト容易化のため分離）。
///
/// `--target` を常に付与する（設計 §13「CLI が常に勝つ」）。これによりリモート側の
/// `resolve_target` が target から readonly を正しく導出し、prod を RW で開く保護ホールを防ぐ。
fn build_remote_command(
    binary_path: &str,
    db_path: &str,
    target: Target,
    media_root: Option<&str>,
) -> String {
    let base = format!(
        "{} --db {} --target {}",
        escape_for_remote_shell(binary_path),
        escape_for_remote_shell(db_path),
        target.as_str(),
    );
    match media_root {
        Some(root) => format!("{} --media-root {}", base, escape_for_remote_shell(root)),
        None => base,
    }
}

/// prod→stg 同期（sync）の結果。prod/stg の実効パス（設計 §4.2/§5.3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    /// prod DB パス（コピー元）。
    pub prod_path: String,
    /// stg DB パス（コピー先）。
    pub stg_path: String,
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
            let username = self.config.username.clone().ok_or_else(|| {
                KijukuError::Other(format!(
                    "SSH user for '{}' is not specified and no ~/.ssh/config found",
                    self.config.ssh_host
                ))
            })?;
            return Ok((
                self.config.ssh_host.clone(),
                self.config.port.unwrap_or(22),
                username,
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
        let username = self.config.username.clone()
            .or(params.user)
            .or_else(|| env::var("USER").ok())
            .ok_or_else(|| {
                KijukuError::Other(format!(
                    "SSH user for '{}' is not specified. Set it in RemoteConfig, ~/.ssh/config or USER env var",
                    self.config.ssh_host
                ))
            })?;
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
        let remote_db_path = resolve_remote_db_path(&self.config);
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

        let command = build_remote_command(
            remote_binary_path,
            &remote_db_path,
            self.config.effective_target(),
            self.config.media_root.as_deref(),
        );
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

    /// レスポンスのエラーチェック（データあり）
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

    /// レスポンスのエラーチェック（データなし）
    fn check_unit_response(&self, response: CommandResponse) -> Result<()> {
        if !response.success {
            return Err(KijukuError::Other(
                response.error.unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }
        Ok(())
    }

    /// `src` を `dst` へ複製する（リモート CLI に委譲・dry-run ファースト・上書きは trash 経由）。
    pub fn media_cp(
        &self,
        src: &str,
        dst: &str,
        opts: &crate::file_ops::FileOpOptions,
    ) -> Result<crate::file_ops::FileOpResult> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "mediaCp".to_string(),
            params: serde_json::json!({ "src": src, "dst": dst, "options": opts }),
        })?;
        self.check_response(response)
    }

    /// `src` を `dst` へ移動する（リモート CLI に委譲・dry-run ファースト・上書きは trash 経由）。
    pub fn media_mv(
        &self,
        src: &str,
        dst: &str,
        opts: &crate::file_ops::FileOpOptions,
    ) -> Result<crate::file_ops::FileOpResult> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "mediaMv".to_string(),
            params: serde_json::json!({ "src": src, "dst": dst, "options": opts }),
        })?;
        self.check_response(response)
    }

    /// `src`（ディレクトリ）の内容を `dst` へ同期する（リモート CLI に委譲・safe モード）。
    pub fn media_sync(
        &self,
        src: &str,
        dst: &str,
        opts: &crate::file_ops::FileOpOptions,
    ) -> Result<crate::file_ops::FileOpResult> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "mediaSync".to_string(),
            params: serde_json::json!({ "src": src, "dst": dst, "options": opts }),
        })?;
        self.check_response(response)
    }

    /// `target_rel` を trash へ移動する（リモート CLI に委譲・論理削除・物理削除はしない）。
    pub fn move_to_trash(
        &self,
        target_rel: &str,
        operation: crate::trash::TrashOperation,
        reason: Option<&str>,
    ) -> Result<crate::trash::TrashId> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "moveToTrash".to_string(),
            params: serde_json::json!({ "target_rel": target_rel, "operation": operation, "reason": reason }),
        })?;
        self.check_response(response)
    }

    /// trash 内のエントリ一覧を返す（リモート CLI に委譲）。
    pub fn list_trash(&self) -> Result<Vec<crate::trash::TrashEntry>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "listTrash".to_string(),
            params: serde_json::json!({}),
        })?;
        self.check_response(response)
    }

    /// trash から `id` のエントリを復元する（リモート CLI に委譲）。戻り値はリモートホスト上の絶対パス。
    pub fn restore_from_trash(&self, id: &str) -> Result<String> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "restoreFromTrash".to_string(),
            params: serde_json::json!({ "id": id }),
        })?;
        self.check_response(response)
    }

    /// trash 内のエントリを物理削除する（リモート CLI に委譲・dry-run ファースト）。
    pub fn purge_trash(&self, ids: Option<&[String]>, dry_run: bool) -> Result<Vec<String>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "purgeTrash".to_string(),
            params: serde_json::json!({ "ids": ids, "dry_run": dry_run }),
        })?;
        self.check_response(response)
    }

    /// ローカルの `local_path` をリモートの `remote_rel`（media root 相対）へアップロード（SFTP・ストリーミング）。
    pub fn upload(&self, local_path: &std::path::Path, remote_rel: &str) -> Result<()> {
        let remote_abs = self.resolve_remote_within_root(remote_rel)?;
        let sess = self.connect()?;
        let sftp = sess
            .sftp()
            .map_err(|e| KijukuError::Other(format!("SFTP セッション確立失敗: {}", e)))?;
        if let Some(parent) = remote_abs.parent() {
            Self::ensure_remote_dir(&sftp, parent);
        }
        let mut local = std::fs::File::open(local_path)?;
        let mut remote = sftp
            .create(&remote_abs)
            .map_err(|e| KijukuError::Other(format!("リモートファイル作成失敗: {}", e)))?;
        std::io::copy(&mut local, &mut remote)?;
        Ok(())
    }

    /// リモートの `remote_rel`（media root 相対）をローカルの `local_path` へダウンロード（SFTP・ストリーミング）。
    pub fn download(&self, remote_rel: &str, local_path: &std::path::Path) -> Result<()> {
        let remote_abs = self.resolve_remote_within_root(remote_rel)?;
        let sess = self.connect()?;
        let sftp = sess
            .sftp()
            .map_err(|e| KijukuError::Other(format!("SFTP セッション確立失敗: {}", e)))?;
        let mut remote = sftp
            .open(&remote_abs)
            .map_err(|e| KijukuError::Other(format!("リモートファイルオープン失敗: {}", e)))?;
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut local = std::fs::File::create(local_path)?;
        std::io::copy(&mut remote, &mut local)?;
        Ok(())
    }

    /// リモートの `remote_rel` を media root 配下に解決し、保護パス（`.trash` 等）を拒否する。
    /// リモート FS を canonicalize できないため lexical 解決のみ（FS アクセス無し）。
    fn resolve_remote_within_root(&self, remote_rel: &str) -> Result<std::path::PathBuf> {
        let media_root = self.config.media_root.as_ref().ok_or_else(|| {
            KijukuError::Validation(
                "media root is not configured; set RemoteConfig.media_root to use upload/download"
                    .to_string(),
            )
        })?;
        let root = std::path::Path::new(media_root);
        let abs = crate::media_path::resolve_within_root(root, remote_rel)?;
        let root_normalized = crate::media_path::normalize_lexical(root);
        let protected = vec![crate::media_path::trash_dir(&root_normalized)];
        if crate::media_path::is_protected(&abs, &protected) {
            return Err(KijukuError::Validation(format!(
                "remote path is a protected path ({}); refused",
                remote_rel
            )));
        }
        Ok(abs)
    }

    /// リモートディレクトリを再帰作成する（`mkdir -p` 相当・既存は無視）。
    fn ensure_remote_dir(sftp: &ssh2::Sftp, path: &std::path::Path) {
        let mut current = std::path::PathBuf::new();
        for comp in path.components() {
            current.push(comp);
            let _ = sftp.mkdir(&current, 0o755);
        }
    }

    /// マイグレーションを実行
    pub fn migrate(&self) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "migrate".to_string(),
            params: serde_json::json!({}),
        })?;

        self.check_unit_response(response)
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

        self.check_unit_response(response)
    }

    /// メディアを削除
    pub fn delete_media(&self, id: i64) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "deleteMedia".to_string(),
            params: serde_json::json!({ "id": id }),
        })?;

        self.check_unit_response(response)
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

    /// 指定したフィールド群の重複なしの値の組み合わせ一覧を取得する
    pub fn get_distinct_values(
        &self,
        fields: &[&str],
        filter: &MediaFilter,
    ) -> Result<Vec<Vec<Option<String>>>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getDistinctValues".to_string(),
            params: serde_json::json!({ "fields": fields, "filter": filter }),
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

        self.check_unit_response(response)
    }

    /// 複数のメディアを一括更新
    pub fn bulk_update_media(&self, updates: &[BulkUpdateItem]) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "bulkUpdateMedia".to_string(),
            params: serde_json::json!({ "updates": updates }),
        })?;

        self.check_unit_response(response)
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

        self.check_unit_response(response)
    }

    /// メディアからタグを削除
    pub fn remove_tag_from_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "removeTagFromMedia".to_string(),
            params: serde_json::json!({ "media_id": media_id, "tag_id": tag_id }),
        })?;

        self.check_unit_response(response)
    }

    /// メディアに関連付けられたタグを取得
    pub fn get_media_tags(&self, media_id: i64) -> Result<Vec<Tag>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getMediaTags".to_string(),
            params: serde_json::json!({ "media_id": media_id }),
        })?;

        self.check_response(response)
    }

    /// 複数メディアのタグを一括取得（N+1 回避・単発 RPC）。タグなしメディアは結果に含まれない。
    pub fn get_media_tags_bulk(
        &self,
        media_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<Tag>>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getMediaTagsBulk".to_string(),
            params: serde_json::json!({ "media_ids": media_ids }),
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

        self.check_unit_response(response)
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

        self.check_unit_response(response)
    }

    /// メディアの全ての属性を削除
    pub fn delete_all_media_attributes(&self, media_id: i64) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "deleteAllMediaAttributes".to_string(),
            params: serde_json::json!({ "media_id": media_id }),
        })?;

        self.check_unit_response(response)
    }

    /// 手動バックアップを実行
    pub fn backup(&self, label: Option<&str>) -> Result<Option<String>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "backup".to_string(),
            params: serde_json::json!({ "label": label }),
        })?;

        #[derive(Deserialize)]
        struct PathResponse {
            path: Option<String>,
        }

        let data: PathResponse = self.check_response(response)?;
        Ok(data.path)
    }

    /// バックアップ一覧を取得
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "listBackups".to_string(),
            params: serde_json::json!({}),
        })?;
        self.backup_info_list_from_response(response)
    }

    /// pre-stash（promote/(b)操作直前の prod snapshot）一覧を取得（設計 §8・prod 復旧経路）。
    /// wire 形状は listBackups と同一（共に `backup_info_to_json` で直列化）。
    pub fn list_pre_stashes(&self) -> Result<Vec<BackupInfo>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "listPreStashes".to_string(),
            params: serde_json::json!({}),
        })?;
        self.backup_info_list_from_response(response)
    }

    /// listBackups / listPreStashes 共通: BackupInfo 配列レスポンスを `BackupInfo` へ変換。
    fn backup_info_list_from_response(
        &self,
        response: CommandResponse,
    ) -> Result<Vec<BackupInfo>> {
        #[derive(Deserialize)]
        struct BackupInfoRaw {
            id: String,
            name: String,
            path: String,
            #[serde(rename = "createdAt")]
            created_at: f64,
            scope: String,
            kind: BackupKindRaw,
            label: Option<String>,
            #[serde(rename = "labelSource", default)]
            label_source: String,
            note: Option<String>,
        }

        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "camelCase")]
        enum BackupKindRaw {
            Full,
            Diff { base_id: String },
        }

        let items: Vec<BackupInfoRaw> = self.check_response(response)?;
        items
            .into_iter()
            .map(|item| {
                use crate::backup::{BackupKind, BackupScope};
                let scope = match item.scope.as_str() {
                    "auto" => BackupScope::Auto,
                    "manual" => BackupScope::Manual,
                    "tmp" => BackupScope::Tmp,
                    _ => BackupScope::Auto,
                };
                let kind = match item.kind {
                    BackupKindRaw::Full => BackupKind::Full,
                    BackupKindRaw::Diff { base_id } => BackupKind::Diff { base_id },
                };
                Ok(BackupInfo {
                    id: item.id,
                    name: item.name,
                    path: std::path::PathBuf::from(item.path),
                    created_at: std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(item.created_at),
                    scope,
                    kind,
                    label: item.label,
                    label_source: match item.label_source.as_str() {
                        "sidecar" => crate::backup::LabelSource::Sidecar,
                        _ => crate::backup::LabelSource::Filename,
                    },
                    note: item.note,
                })
            })
            .collect::<Result<Vec<_>>>()
    }

    /// バックアップのラベルを設定/解除（設計 §7.5・事後メモ）。
    pub fn set_backup_label(&self, id: &str, label: Option<&str>) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "setBackupLabel".to_string(),
            params: serde_json::json!({ "id": id, "label": label }),
        })?;
        self.check_unit_response(response)
    }

    /// バックアップのノート（メモ）を設定/解除（設計 §7.5）。
    pub fn set_backup_note(&self, id: &str, note: Option<&str>) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "setBackupNote".to_string(),
            params: serde_json::json!({ "id": id, "note": note }),
        })?;
        self.check_unit_response(response)
    }

    /// バックアップのメタ情報（ラベル/ノート等）を取得。未設定時は None。
    pub fn get_backup_meta(&self, id: &str) -> Result<Option<crate::backup::BackupMetaEntry>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getBackupMeta".to_string(),
            params: serde_json::json!({ "id": id }),
        })?;
        self.check_response(response)
    }

    /// バックアップを復元
    pub fn restore(&self, selector: &serde_json::Value) -> Result<String> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "restore".to_string(),
            params: serde_json::json!({ "selector": selector }),
        })?;

        #[derive(Deserialize)]
        struct PathResponse {
            path: String,
        }

        let data: PathResponse = self.check_response(response)?;
        Ok(data.path)
    }

    /// バックアップと現在DBの差分を取得
    pub fn diff_with_backup(
        &self,
        selector: &serde_json::Value,
        options: &crate::diff::DiffOptions,
    ) -> Result<crate::diff::BackupDiff> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "diffBackup".to_string(),
            params: serde_json::json!({
                "selector": selector,
                "options": serde_json::to_value(options)
                    .map_err(|e| KijukuError::Other(e.to_string()))?,
            }),
        })?;
        self.check_response(response)
    }

    /// prod(RO) と現在DB(stg) の差分を取得（promote 判断用・設計 §4.4・TASK-53）。
    /// リモート CLI は self=stg 起動を想定し、`prod_db_path` で prod を別途指定する
    /// （`build_remote_command` は --db を1つしか渡せないため・設計 §4.2/§4.4）。
    /// `prod_db_path` が None の場合は CLI 側で prod target のデフォルトパスを解決する。
    pub fn diff_with_prod(
        &self,
        prod_db_path: Option<&std::path::Path>,
        options: &crate::diff::DiffOptions,
    ) -> Result<crate::diff::BackupDiff> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "diffProdStg".to_string(),
            params: serde_json::json!({
                "prodDbPath": prod_db_path,
                "options": serde_json::to_value(options)
                    .map_err(|e| KijukuError::Other(e.to_string()))?,
            }),
        })?;
        self.check_response(response)
    }

    /// prod(RO) と stg の整合性を観測し gate を評価（設計 §3.4・promote 判断の客観根拠）。
    /// `diff_with_prod` と同様、リモート CLI は self=stg 起動を想定し `prod_db_path` で prod を指定。
    /// `prod_db_path` が None の場合は CLI 側で prod target のデフォルトパスを解決する。
    pub fn observe(
        &self,
        prod_db_path: Option<&std::path::Path>,
        options: &crate::diff::ObserveOptions,
    ) -> Result<crate::diff::ObserveResult> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "observe".to_string(),
            params: serde_json::json!({
                "prodDbPath": prod_db_path,
                "options": serde_json::to_value(options)
                    .map_err(|e| KijukuError::Other(e.to_string()))?,
            }),
        })?;
        self.check_response(response)
    }

    /// stg→prod への反映（promote・設計 §4.5）。observe gate 合格が前提。
    /// 成功時 `PromoteOutcome`（gate 結果 + pre-stash パス）。gate 不合格は prod 未更新で
    /// `KijukuError::Other`（サーバ側 `PromoteGateFailed` が文字列化される・TS と同じ制約）。
    pub fn promote(
        &self,
        prod_db_path: Option<&std::path::Path>,
        options: &crate::diff::ObserveOptions,
        backup_opts: Option<&crate::BackupOptions>,
    ) -> Result<crate::PromoteOutcome> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "promote".to_string(),
            params: serde_json::json!({
                "prodDbPath": prod_db_path,
                "options": serde_json::to_value(options)
                    .map_err(|e| KijukuError::Other(e.to_string()))?,
                "backupOpts": backup_opts,
            }),
        })?;
        self.check_response(response)
    }

    /// prod→stg コピー操作の共通基盤（sync/discard・設計 §4.2/§4.6）。`operation` に "sync" または
    /// "discard" を渡す。`from`/`to` は config（prod `db_path` と導出 stg）から解決し明示渡す（TS parity）。
    /// いずれか未設定時はサーバ側 `resolve_sync_paths` がデフォルト補完する。
    fn run_prod_to_stg(&self, operation: &str) -> Result<SyncResult> {
        let from = self.config.db_path.clone();
        let to = match &self.config.stg_db_path {
            Some(s) => Some(s.clone()),
            None => self.config.db_path.as_ref().map(|p| derive_stg_db_path(p)),
        };
        let response = self.execute_remote_command(CommandRequest {
            operation: operation.to_string(),
            params: serde_json::json!({ "from": from, "to": to }),
        })?;
        self.check_response(response)
    }

    /// prod→stg の初期同期（設計 §4.2/§5.3・書込セッション開始時）。Online Backup 1 パスのコピー。
    pub fn sync(&self) -> Result<SyncResult> {
        self.run_prod_to_stg("sync")
    }

    /// stg 破棄・再 sync（設計 §4.6・書込セッション中断/gate 不合格時）。処理は `sync` と同一
    /// （prod→stg の Online Backup コピー・既存 stg は上書き破棄・prod は一切触らない）。
    /// 操作名のみ監査ログ（§10）で区別するため独立メソッド。
    pub fn discard(&self) -> Result<SyncResult> {
        self.run_prod_to_stg("discard")
    }

    // --- update_exist ---

    /// フィルタで絞り込んだメディアのflag_existをファイル存在状態に基づいて更新する
    pub fn update_exist(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        update_options: &UpdateExistOptions,
    ) -> Result<UpdateExistResult> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "updateExist".to_string(),
            params: serde_json::json!({ "filter": filter, "options": options, "update_options": update_options }),
        })?;
        self.check_response(response)
    }

    // --- hash operations ---

    /// メディアハッシュを追加
    pub fn add_media_hash(&self, input: &MediaHashInput) -> Result<MediaHash> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "addMediaHash".to_string(),
            params: serde_json::json!({ "input": input }),
        })?;
        let remote: MediaHashRemote = self.check_response(response)?;
        remote.to_media_hash()
    }

    /// メディアハッシュを一括追加
    pub fn add_media_hashes(&self, inputs: &[MediaHashInput]) -> Result<Vec<MediaHash>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "addMediaHashes".to_string(),
            params: serde_json::json!({ "inputs": inputs }),
        })?;
        let remotes: Vec<MediaHashRemote> = self.check_response(response)?;
        remotes.into_iter().map(|r| r.to_media_hash()).collect()
    }

    /// 指定UUIDのメディアハッシュ一覧を取得
    pub fn get_media_hashes(&self, item_uuid: &str) -> Result<Vec<MediaHash>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getMediaHashes".to_string(),
            params: serde_json::json!({ "item_uuid": item_uuid }),
        })?;
        let remotes: Vec<MediaHashRemote> = self.check_response(response)?;
        remotes.into_iter().map(|r| r.to_media_hash()).collect()
    }

    /// 指定位置のメディアハッシュを取得
    pub fn get_media_hash(
        &self,
        item_uuid: &str,
        filename: &str,
        time_range: &str,
    ) -> Result<Option<MediaHash>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "getMediaHash".to_string(),
            params: serde_json::json!({ "item_uuid": item_uuid, "filename": filename, "time_range": time_range }),
        })?;
        let remote: Option<MediaHashRemote> = self.check_response(response)?;
        remote.map(|r| r.to_media_hash()).transpose()
    }

    /// content_hashでメディアハッシュを検索
    pub fn find_by_content_hash(&self, hash_bytes: &[u8]) -> Result<Vec<MediaHash>> {
        let hash_hex = crate::hash::bytes_to_hex(hash_bytes);
        let response = self.execute_remote_command(CommandRequest {
            operation: "findByContentHash".to_string(),
            params: serde_json::json!({ "hash_hex": hash_hex }),
        })?;
        let remotes: Vec<MediaHashRemote> = self.check_response(response)?;
        remotes.into_iter().map(|r| r.to_media_hash()).collect()
    }

    /// 指定位置のメディアハッシュを削除
    pub fn delete_media_hash(
        &self,
        item_uuid: &str,
        filename: &str,
        time_range: &str,
    ) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "deleteMediaHash".to_string(),
            params: serde_json::json!({ "item_uuid": item_uuid, "filename": filename, "time_range": time_range }),
        })?;
        self.check_unit_response(response)
    }

    /// 指定UUIDのメディアハッシュを全削除
    pub fn delete_media_hashes(&self, item_uuid: &str) -> Result<()> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "deleteMediaHashes".to_string(),
            params: serde_json::json!({ "item_uuid": item_uuid }),
        })?;
        self.check_unit_response(response)
    }

    /// 重複するcontent_hashを検索
    pub fn find_duplicate_hashes(&self) -> Result<Vec<(Vec<u8>, i64)>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "findDuplicateHashes".to_string(),
            params: serde_json::json!({}),
        })?;
        let items: Vec<DuplicateHashRemote> = self.check_response(response)?;
        items
            .into_iter()
            .map(|item| {
                crate::hash::hex_to_bytes(&item.content_hash)
                    .map(|bytes| (bytes, item.count))
            })
            .collect()
    }

    /// 特定のメディアのハッシュを計算・登録
    pub fn compute_media_hash(
        &self,
        item_uuid: &str,
        media_path: &str,
        media_type: &str,
        duration_sec: Option<i32>,
    ) -> Result<crate::hash::ComputeHashResult> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "computeMediaHash".to_string(),
            params: serde_json::json!({
                "item_uuid": item_uuid,
                "media_path": media_path,
                "media_type": media_type,
                "duration_sec": duration_sec,
            }),
        })?;
        let remote: ComputeHashResultRemote = self.check_response(response)?;
        remote.to_compute_result()
    }

    /// フィルタ条件でメディアを絞り込み、ハッシュを計算・登録
    pub fn compute_media_hashes(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        force: bool,
    ) -> Result<Vec<crate::hash::ComputeHashResult>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "computeMediaHashes".to_string(),
            params: serde_json::json!({ "filter": filter, "options": options, "force": force }),
        })?;
        let remotes: Vec<ComputeHashResultRemote> = self.check_response(response)?;
        remotes.into_iter().map(|r| r.to_compute_result()).collect()
    }

    /// サムネイルの状態をチェック
    pub fn check_thumbnail(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
    ) -> Result<CheckThumbnailResult> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "checkThumbnail".to_string(),
            params: serde_json::json!({ "filter": filter, "options": options, "thumbnail_options": {} }),
        })?;

        self.check_response(response)
    }

    /// サムネイルを更新
    pub fn update_thumbnail(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
        thumbnail_options: &ThumbnailOptions,
    ) -> Result<UpdateThumbnailResult> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "updateThumbnail".to_string(),
            params: serde_json::json!({ "filter": filter, "options": options, "thumbnail_options": thumbnail_options }),
        })?;

        self.check_response(response)
    }
}

/// SSH/ネットワーク I/O を伴う同期 RPC をブロッキングスレッドプールに逃すヘルパ。
/// `RemoteKijukuDB` は `Clone` 可能（`RemoteConfig` のみ保持）で、各 RPC は毎回新規 SSH
/// セッションを張るため、呼び出しごとに複製して `spawn_blocking` に渡す。
async fn spawn_remote<F, T>(this: RemoteKijukuDB, f: F) -> Result<T>
where
    F: FnOnce(RemoteKijukuDB) -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || f(this))
        .await
        .map_err(|e| KijukuError::Other(format!("remote spawn_blocking join error: {}", e)))?
}

#[async_trait]
impl KijukuBackend for RemoteKijukuDB {
    async fn migrate(&self) -> Result<()> {
        spawn_remote(self.clone(), |this| this.migrate()).await
    }

    async fn get_schema_version(&self) -> Result<i64> {
        spawn_remote(self.clone(), |this| this.get_schema_version()).await
    }

    async fn get_tables(&self) -> Result<Vec<String>> {
        spawn_remote(self.clone(), |this| this.get_tables()).await
    }

    async fn get_table_info(&self, table_name: &str) -> Result<Vec<TableColumnInfo>> {
        let table_name = table_name.to_string();
        spawn_remote(self.clone(), move |this| this.get_table_info(&table_name)).await
    }

    async fn create_media(&self, data: &MediaInput) -> Result<Media> {
        let data = data.clone();
        spawn_remote(self.clone(), move |this| this.create_media(&data)).await
    }

    async fn get_media(&self, id: i64) -> Result<Option<Media>> {
        spawn_remote(self.clone(), move |this| this.get_media(id)).await
    }

    async fn update_media(&self, id: i64, input: &MediaUpdateInput) -> Result<()> {
        let input = input.clone();
        spawn_remote(self.clone(), move |this| this.update_media(id, &input)).await
    }

    async fn delete_media(&self, id: i64) -> Result<()> {
        spawn_remote(self.clone(), move |this| this.delete_media(id)).await
    }

    async fn find_media(
        &self,
        filter: &MediaFilter,
        options: Option<&QueryOptions>,
    ) -> Result<Vec<Media>> {
        let filter = filter.clone();
        let options = options.cloned();
        spawn_remote(self.clone(), move |this| {
            this.find_media(&filter, options.as_ref())
        })
        .await
    }

    async fn get_distinct_values(
        &self,
        fields: &[&str],
        filter: &MediaFilter,
    ) -> Result<Vec<Vec<Option<String>>>> {
        let fields: Vec<String> = fields.iter().map(|s| s.to_string()).collect();
        let filter = filter.clone();
        spawn_remote(self.clone(), move |this| {
            let fields_ref: Vec<&str> = fields.iter().map(|s| s.as_str()).collect();
            this.get_distinct_values(&fields_ref, &filter)
        })
        .await
    }

    async fn bulk_create_media(&self, data_list: &[MediaInput]) -> Result<Vec<Media>> {
        let data_list = data_list.to_vec();
        spawn_remote(self.clone(), move |this| this.bulk_create_media(&data_list)).await
    }

    async fn bulk_delete_media(&self, ids: &[i64]) -> Result<()> {
        let ids = ids.to_vec();
        spawn_remote(self.clone(), move |this| this.bulk_delete_media(&ids)).await
    }

    async fn bulk_update_media(&self, updates: &[BulkUpdateItem]) -> Result<()> {
        let updates = updates.to_vec();
        spawn_remote(self.clone(), move |this| this.bulk_update_media(&updates)).await
    }

    async fn create_tag(&self, name: &str) -> Result<Tag> {
        let name = name.to_string();
        spawn_remote(self.clone(), move |this| this.create_tag(&name)).await
    }

    async fn get_tag_by_name(&self, name: &str) -> Result<Option<Tag>> {
        let name = name.to_string();
        spawn_remote(self.clone(), move |this| this.get_tag_by_name(&name)).await
    }

    async fn get_all_tags(&self) -> Result<Vec<Tag>> {
        spawn_remote(self.clone(), |this| this.get_all_tags()).await
    }

    async fn add_tag_to_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        spawn_remote(self.clone(), move |this| this.add_tag_to_media(media_id, tag_id)).await
    }

    async fn remove_tag_from_media(&self, media_id: i64, tag_id: i64) -> Result<()> {
        spawn_remote(self.clone(), move |this| this.remove_tag_from_media(media_id, tag_id)).await
    }

    async fn get_media_tags(&self, media_id: i64) -> Result<Vec<Tag>> {
        spawn_remote(self.clone(), move |this| this.get_media_tags(media_id)).await
    }

    async fn get_media_tags_bulk(
        &self,
        media_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<Tag>>> {
        let media_ids = media_ids.to_vec();
        spawn_remote(self.clone(), move |this| this.get_media_tags_bulk(&media_ids)).await
    }

    async fn get_tag_usage_stats(&self) -> Result<Vec<TagUsageStats>> {
        spawn_remote(self.clone(), |this| this.get_tag_usage_stats()).await
    }

    async fn find_unused_tags(&self) -> Result<Vec<Tag>> {
        spawn_remote(self.clone(), |this| this.find_unused_tags()).await
    }

    async fn set_media_attribute(
        &self,
        media_id: i64,
        key: &str,
        value: Option<&str>,
        value_type: Option<AttributeValueType>,
    ) -> Result<()> {
        let key = key.to_string();
        let value = value.map(|s| s.to_string());
        spawn_remote(self.clone(), move |this| {
            // RemoteKijukuDB::set_media_attribute は value_type を文字列で受け取るため変換
            this.set_media_attribute(media_id, &key, value.as_deref(), value_type.map(|t| t.as_str()))
        })
        .await
    }

    async fn get_media_attribute(&self, media_id: i64, key: &str) -> Result<Option<MediaAttribute>> {
        let key = key.to_string();
        spawn_remote(self.clone(), move |this| this.get_media_attribute(media_id, &key)).await
    }

    async fn get_media_attributes(&self, media_id: i64) -> Result<Vec<MediaAttribute>> {
        spawn_remote(self.clone(), move |this| this.get_media_attributes(media_id)).await
    }

    async fn delete_media_attribute(&self, media_id: i64, key: &str) -> Result<()> {
        let key = key.to_string();
        spawn_remote(self.clone(), move |this| this.delete_media_attribute(media_id, &key)).await
    }

    async fn delete_all_media_attributes(&self, media_id: i64) -> Result<()> {
        spawn_remote(self.clone(), move |this| this.delete_all_media_attributes(media_id)).await
    }

    async fn add_media_hash(&self, input: &MediaHashInput) -> Result<MediaHash> {
        let input = input.clone();
        spawn_remote(self.clone(), move |this| this.add_media_hash(&input)).await
    }

    async fn add_media_hashes(&self, inputs: &[MediaHashInput]) -> Result<Vec<MediaHash>> {
        let inputs = inputs.to_vec();
        spawn_remote(self.clone(), move |this| this.add_media_hashes(&inputs)).await
    }

    async fn get_media_hashes(&self, item_uuid: &str) -> Result<Vec<MediaHash>> {
        let item_uuid = item_uuid.to_string();
        spawn_remote(self.clone(), move |this| this.get_media_hashes(&item_uuid)).await
    }

    async fn get_media_hash(
        &self,
        item_uuid: &str,
        filename: &str,
        time_range: &str,
    ) -> Result<Option<MediaHash>> {
        let item_uuid = item_uuid.to_string();
        let filename = filename.to_string();
        let time_range = time_range.to_string();
        spawn_remote(self.clone(), move |this| {
            this.get_media_hash(&item_uuid, &filename, &time_range)
        })
        .await
    }

    async fn find_by_content_hash(&self, hash: &[u8]) -> Result<Vec<MediaHash>> {
        let hash = hash.to_vec();
        spawn_remote(self.clone(), move |this| this.find_by_content_hash(&hash)).await
    }

    async fn delete_media_hash(
        &self,
        item_uuid: &str,
        filename: &str,
        time_range: &str,
    ) -> Result<()> {
        let item_uuid = item_uuid.to_string();
        let filename = filename.to_string();
        let time_range = time_range.to_string();
        spawn_remote(self.clone(), move |this| {
            this.delete_media_hash(&item_uuid, &filename, &time_range)
        })
        .await
    }

    async fn delete_media_hashes(&self, item_uuid: &str) -> Result<()> {
        let item_uuid = item_uuid.to_string();
        spawn_remote(self.clone(), move |this| this.delete_media_hashes(&item_uuid)).await
    }

    async fn find_duplicate_hashes(&self) -> Result<Vec<(Vec<u8>, i64)>> {
        spawn_remote(self.clone(), |this| this.find_duplicate_hashes()).await
    }
}

/// CLIのmedia_hash_to_json出力をデシリアライズするためのヘルパー
/// CLIはVec<u8>をhex文字列に変換して出力するため、直接MediaHashとしてデシリアライズできない
#[derive(Deserialize)]
struct MediaHashRemote {
    item_uuid: String,
    filename: String,
    time_range: String,
    content_hash: String,
    alternative_of: Option<String>,
    embedding: Option<String>,
    created_at: String,
    updated_at: String,
}

impl MediaHashRemote {
    fn to_media_hash(self) -> Result<MediaHash> {
        Ok(MediaHash {
            item_uuid: self.item_uuid,
            filename: self.filename,
            time_range: self.time_range,
            content_hash: crate::hash::hex_to_bytes(&self.content_hash)?,
            alternative_of: self.alternative_of,
            embedding: self.embedding
                .map(|e| crate::hash::hex_to_bytes(&e))
                .transpose()?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(Deserialize)]
struct DuplicateHashRemote {
    content_hash: String,
    count: i64,
}

#[derive(Deserialize)]
struct ComputeHashResultRemote {
    item_uuid: String,
    hashes: Vec<MediaHashRemote>,
    skipped: bool,
    skip_reason: Option<String>,
}

impl ComputeHashResultRemote {
    fn to_compute_result(self) -> Result<crate::hash::ComputeHashResult> {
        let hashes = self.hashes.into_iter().map(|r| r.to_media_hash()).collect::<Result<Vec<_>>>()?;
        Ok(crate::hash::ComputeHashResult {
            item_uuid: self.item_uuid,
            hashes,
            skipped: self.skipped,
            skip_reason: self.skip_reason,
        })
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
            username: Some("testuser".to_string()),
            private_key_path: Some(PathBuf::from("/home/user/.ssh/id_rsa")),
            db_path: Some("/path/to/db.db".to_string()),
            binary_path: Some("/path/to/binary".to_string()),
            media_root: None,
            ..Default::default()
        };

        assert_eq!(config.ssh_host, "example.com");
        assert_eq!(config.port, Some(2222));
        assert_eq!(config.username, Some("testuser".to_string()));
    }

    #[test]
    fn build_remote_command_with_media_root() {
        let cmd = build_remote_command(
            "~/.local/bin/kijuku-cli",
            "~/.local/share/kijuku/kijuku.db",
            Target::Stg,
            Some("/media/root"),
        );
        assert!(cmd.contains("--media-root"), "cmd: {cmd}");
        // `~` は `$HOME` に展開される
        assert!(cmd.contains("\"$HOME\""), "cmd: {cmd}");
        assert!(cmd.contains("--db"), "cmd: {cmd}");
        // target が常に付与される（設計 §13）
        assert!(cmd.contains("--target stg"), "cmd: {cmd}");
    }

    #[test]
    fn build_remote_command_without_media_root() {
        let cmd = build_remote_command(
            "/usr/bin/kijuku-cli",
            "/var/db/kijuku.db",
            Target::Prod,
            None,
        );
        assert!(!cmd.contains("--media-root"), "cmd: {cmd}");
        assert!(cmd.contains("--db"), "cmd: {cmd}");
        // prod は --target prod で readonly 伝達（設計 §5.1・§13）
        assert!(cmd.contains("--target prod"), "cmd: {cmd}");
    }

    #[test]
    fn build_remote_command_quotes_special_chars() {
        // 空白・シングルクォートを含むパスが安全にクォートされる
        let cmd = build_remote_command(
            "/path/to/binary",
            "/path/with space/db",
            Target::Stg,
            Some("/media/it's"),
        );
        assert!(cmd.contains("'/path/with space/db'"), "cmd: {cmd}");
        // シングルクォートは `'\''` にエスケープされる
        assert!(cmd.contains("'\\''"), "cmd: {cmd}");
    }

    #[test]
    fn derive_stg_db_path_inserts_stg_before_extension() {
        assert_eq!(derive_stg_db_path("~/.local/share/kijuku/kijuku.db"), "~/.local/share/kijuku/kijuku.stg.db");
        assert_eq!(derive_stg_db_path("kijuku.db"), "kijuku.stg.db");
        assert_eq!(derive_stg_db_path("/var/db/my.db"), "/var/db/my.stg.db");
        // 拡張子無し
        assert_eq!(derive_stg_db_path("/var/db/kijulu"), "/var/db/kijulu.stg.db");
        // ディレクトリ名のドットは拡張子と誤認しない
        assert_eq!(derive_stg_db_path("/foo.bar/kijuku.db"), "/foo.bar/kijuku.stg.db");
    }

    #[test]
    fn resolve_remote_db_path_per_target() {
        // prod: db_path をそのまま
        let prod = RemoteConfig {
            target: Target::Prod,
            db_path: Some("~/.local/share/kijuku/kijuku.db".to_string()),
            ..Default::default()
        };
        assert_eq!(resolve_remote_db_path(&prod), "~/.local/share/kijuku/kijuku.db");

        // stg: stg_db_path があればそれを優先
        let stg_explicit = RemoteConfig {
            target: Target::Stg,
            db_path: Some("~/.local/share/kijuku/kijuku.db".to_string()),
            stg_db_path: Some("/custom/stg.db".to_string()),
            ..Default::default()
        };
        assert_eq!(resolve_remote_db_path(&stg_explicit), "/custom/stg.db");

        // stg: stg_db_path 無し → db_path から導出
        let stg_derived = RemoteConfig {
            target: Target::Stg,
            db_path: Some("~/.local/share/kijuku/kijuku.db".to_string()),
            stg_db_path: None,
            ..Default::default()
        };
        assert_eq!(resolve_remote_db_path(&stg_derived), "~/.local/share/kijuku/kijuku.stg.db");

        // stg: 両方無し → stg デフォルト
        let stg_default = RemoteConfig {
            target: Target::Stg,
            db_path: None,
            stg_db_path: None,
            ..Default::default()
        };
        assert_eq!(resolve_remote_db_path(&stg_default), "~/.local/share/kijuku/kijuku.stg.db");
    }

    #[test]
    fn effective_target_prefers_read_source() {
        // read_source 指定時は target より優先（方式A・設計 §3.5/§13）。
        // target=Stg でも read_source=Some(Prod) なら prod RO 読込専用セッション。
        let ro_session = RemoteConfig {
            target: Target::Stg,
            read_source: Some(Target::Prod),
            ..Default::default()
        };
        assert_eq!(ro_session.effective_target(), Target::Prod);

        // 逆方向: target=Prod, read_source=Some(Stg) → Stg
        let flip = RemoteConfig {
            target: Target::Prod,
            read_source: Some(Target::Stg),
            ..Default::default()
        };
        assert_eq!(flip.effective_target(), Target::Stg);

        // read_source 未指定時は target に従う（従来通り）
        let no_read_source = RemoteConfig {
            target: Target::Stg,
            read_source: None,
            ..Default::default()
        };
        assert_eq!(no_read_source.effective_target(), Target::Stg);
    }

    #[test]
    fn resolve_remote_db_path_prefers_read_source() {
        // target=Stg でも read_source=Some(Prod) なら prod DB パスを使用（方式A・read_source 優先）。
        let ro_session = RemoteConfig {
            target: Target::Stg,
            read_source: Some(Target::Prod),
            db_path: Some("~/.local/share/kijuku/kijuku.db".to_string()),
            ..Default::default()
        };
        assert_eq!(
            resolve_remote_db_path(&ro_session),
            "~/.local/share/kijuku/kijuku.db"
        );

        // read_source=Some(Stg) なら stg 導出パスを使用（stg_db_path 未指定 → db_path から導出）
        let stg_read = RemoteConfig {
            target: Target::Prod,
            read_source: Some(Target::Stg),
            db_path: Some("~/.local/share/kijuku/kijuku.db".to_string()),
            stg_db_path: None,
            ..Default::default()
        };
        assert_eq!(
            resolve_remote_db_path(&stg_read),
            "~/.local/share/kijuku/kijuku.stg.db"
        );
    }

    #[test]
    fn resolve_remote_within_root_errors_when_unset() {
        let remote = RemoteKijukuDB::new(RemoteConfig::default());
        assert!(remote.resolve_remote_within_root("a/b.jpg").is_err());
    }

    #[test]
    fn resolve_remote_within_root_rejects_traversal() {
        let config = RemoteConfig {
            ssh_host: "localhost".to_string(),
            media_root: Some("/media".to_string()),
            ..Default::default()
        };
        let remote = RemoteKijukuDB::new(config);
        assert!(remote.resolve_remote_within_root("../escape").is_err());
        assert!(remote.resolve_remote_within_root("/etc/passwd").is_err());
    }

    #[test]
    fn resolve_remote_within_root_rejects_protected() {
        let config = RemoteConfig {
            ssh_host: "localhost".to_string(),
            media_root: Some("/media".to_string()),
            ..Default::default()
        };
        let remote = RemoteKijukuDB::new(config);
        // `.trash` 配下は保護パス
        assert!(remote.resolve_remote_within_root(".trash/xxx").is_err());
        // 正常な相対パスは許可
        assert!(remote.resolve_remote_within_root("a/b.jpg").is_ok());
    }
}
