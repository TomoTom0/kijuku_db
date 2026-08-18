use crate::{AttributeValueType, AuditLogFilter, AuditRecord, BackupInfo, BulkUpdateItem, CheckThumbnailResult, KijukuBackend, KijukuError, Media, MediaAttribute, MediaFilter, MediaHash, MediaHashInput, MediaInput, MediaUpdateInput, QueryOptions, Result, TableColumnInfo, Tag, TagUsageStats, ThumbnailOptions, UpdateExistOptions, UpdateExistResult, UpdateThumbnailResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ssh2::Session;
use ssh2_config::{ParseRule, SshConfig};
use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::env;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use parking_lot::Mutex;

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

/// プールされた SSH セッション（TASK-70・Session 再利用）。
/// `binary_ensured` は `ensure_remote_binary`（毎 RPC 先頭の `getServerVersion` ラウンドトリップ）が
/// 完了したことを示し、以降の RPC ではスキップしてレイテンシを削減する。
struct PooledSession {
    sess: Session,
    binary_ensured: bool,
}

/// リモートKijuku DB操作クラス
///
/// SSH `Session` は `Arc<Mutex<Option<PooledSession>>>` でキャッシュ・再利用される（TASK-70）。
/// 初回 RPC で TCP+handshake+認証を済ませた Session を確立し、以降の連続 RPC はそれを使い回す。
/// セッション系エラー（`KijukuError::Ssh`）時は slot を無効化し、次回 RPC で再接続する。
///
/// **直列化のトレードオフ**: `with_session` は RPC 全期間ロックを保持する。同一インスタンスの
/// clone 間で RPC を並行発行しても直列化される（ssh2 は同一 Session 上の channel を内部 Mutex で
/// 直列化する仕様上、そもそも直列化は必然）。真の並行には複数 Session が必要（本タスクの範囲外）。
#[derive(Clone)]
pub struct RemoteKijukuDB {
    config: RemoteConfig,
    session: Arc<Mutex<Option<PooledSession>>>,
    /// 新規 SSH 接続を確立した回数（診断用・テストで再利用を検証）。
    connect_count: Arc<AtomicU64>,
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

/// リモート prod DB のデフォルトパス（`resolve_remote_db_path`・`long_op_target` で使用）。
const DEFAULT_REMOTE_PROD_DB: &str = "~/.local/share/kijuku/kijuku.db";
/// リモート stg DB のデフォルトパス。
const DEFAULT_REMOTE_STG_DB: &str = "~/.local/share/kijuku/kijuku.stg.db";

/// `RemoteConfig` から target に応じたリモート DB パスを解決する（設計 §4.1・§13）。
///
/// - prod: `db_path`（未設定時は prod デフォルト）
/// - stg: `stg_db_path`、未設定なら `db_path` から導出、それも無ければ stg デフォルト
fn resolve_remote_db_path(config: &RemoteConfig) -> String {
    // read-source を target に折り畳む（方式A・設計 §3.5/§13）。
    match config.effective_target() {
        Target::Prod => config
            .db_path
            .clone()
            .unwrap_or_else(|| DEFAULT_REMOTE_PROD_DB.to_string()),
        Target::Stg => config
            .stg_db_path
            .clone()
            .or_else(|| config.db_path.as_deref().map(derive_stg_db_path))
            .unwrap_or_else(|| DEFAULT_REMOTE_STG_DB.to_string()),
    }
}

/// `RemoteConfig` から sync/discard の `from`（prod）/`to`（stg）パスを解決する（TS `runProdToStg` と同一規約）。
///
/// - from: `db_path`（未設定時は prod デフォルト）
/// - to: `stg_db_path`、未設定なら `db_path` から導出、それも無ければ stg デフォルト
///
/// サーバ側 `resolve_sync_paths` はデフォルト補完しない（TASK-94）ため、null を送らないよう必ず具象化する。
fn resolve_sync_paths_from_config(config: &RemoteConfig) -> (String, String) {
    let from = config
        .db_path
        .clone()
        .unwrap_or_else(|| DEFAULT_REMOTE_PROD_DB.to_string());
    let to = config
        .stg_db_path
        .clone()
        .or_else(|| config.db_path.as_deref().map(derive_stg_db_path))
        .unwrap_or_else(|| DEFAULT_REMOTE_STG_DB.to_string());
    (from, to)
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

/// デフォルト RPC タイムアウト（短操作・TS `executeRemoteCommand` デフォルトと同一）。
const DEFAULT_RPC_TIMEOUT_MS: u32 = 30_000;
/// ファイル転送（upload/download）のタイムアウト（TS parity・固定 120s）。
const FILE_TRANSFER_TIMEOUT_MS: u32 = 120_000;

/// 適応的タイムアウトを適用する長時間操作の operation 名（TS parity・calcBackupTimeoutMs 適用対象）。
const LONG_RUNNING_OPS: &[&str] = &[
    "backup",
    "restore",
    "sync",
    "discard",
    "diffBackup",
    "diffProdStg",
    "observe",
    "promote",
];

/// `operation` が長時間操作（適応的タイムアウト適用対象）か。
fn is_long_running_op(op: &str) -> bool {
    LONG_RUNNING_OPS.contains(&op)
}

/// 長操作の適応的タイムアウト算出に用いる DB パスと `calc_backup_timeout_ms` 結果への乗数（TS parity）。
///
/// 戻り値: `(db_path, multiplier)`。長操作でない場合は `None`。
/// - restore: 現在 DB の退避 + 復元の 2 段階で **2 倍**（TS `calcBackupTimeoutMs(dbSize) * 2`・remote.ts L1162）
/// - sync/discard: prod パス（`config.db_path ?? DEFAULT_REMOTE_PROD_DB`・remote.ts L1188）。target に依存しない
/// - 上記以外（backup/diffBackup/diffProdStg/observe/promote）: `resolve_remote_db_path` と同一・乗数 1
fn long_op_target(op: &str, config: &RemoteConfig) -> Option<(String, u32)> {
    match op {
        "restore" => Some((resolve_remote_db_path(config), 2)),
        "sync" | "discard" => Some((
            config
                .db_path
                .clone()
                .unwrap_or_else(|| DEFAULT_REMOTE_PROD_DB.to_string()),
            1,
        )),
        _ if is_long_running_op(op) => Some((resolve_remote_db_path(config), 1)),
        _ => None,
    }
}

/// DB サイズに基づいてバックアップ系長操作のタイムアウトを計算（ms・TS parity）。
///
/// TS `calcBackupTimeoutMs`（ts-sdk/src/remote.ts）と同一ロジック・定数。
/// rusqlite バックアップ: 750,000 ページ/バッチ・10s スリープ、HDD 想定 50MB/s、MARGIN=2、最低 60s。
/// 計算は f64 で行い（TS と厳密一致）、最後に u32 へ変換（u32::MAX で飽和）。
fn calc_backup_timeout_ms(db_size_bytes: u64) -> u32 {
    const PAGE_SIZE: f64 = 4096.0;
    const HDD_BYTES_PER_MS: f64 = (50.0 * 1024.0 * 1024.0) / 1000.0;
    const BATCH_PAGES: f64 = 750_000.0;
    const SLEEP_PER_BATCH_MS: f64 = 10_000.0;
    const MARGIN: f64 = 2.0;
    const MIN_TIMEOUT_MS: f64 = 60_000.0;

    let size = db_size_bytes as f64;
    let copy_time_ms = size / HDD_BYTES_PER_MS;
    let num_batches = (size / (BATCH_PAGES * PAGE_SIZE)).ceil();
    let sleep_time_ms = num_batches * SLEEP_PER_BATCH_MS;
    let total = (copy_time_ms + sleep_time_ms) * MARGIN;
    let result = total.max(MIN_TIMEOUT_MS);
    if result.is_finite() && result < u32::MAX as f64 {
        result as u32
    } else {
        u32::MAX
    }
}

/// `MAJOR.MINOR.PATCH` 形式のセマンティックバージョンを `[u64; 3]` にパース（TASK-69・TS parity）。
/// 3要素未満（例: `"0.2"`）は末尾を 0 補間（`[0, 2, 0]`）。4要素以上・非数値は `Err`。
/// kijuku-cli の `CARGO_PKG_VERSION` は厳密 `MAJOR.MINOR.PATCH` 形式を前提（pre-release 非対応・
/// パース失敗時は呼び出し元でフェイルセーフとして「デプロイ必要」扱い）。
fn parse_semver(s: &str) -> Result<[u64; 3]> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() > 3 {
        return Err(KijukuError::Other(format!(
            "invalid semver (too many parts): {}",
            s
        )));
    }
    let mut nums = [0u64; 3];
    for (i, part) in parts.iter().enumerate() {
        nums[i] = part.parse::<u64>().map_err(|_| {
            KijukuError::Other(format!("invalid semver (non-numeric part): {}", s))
        })?;
    }
    Ok(nums)
}

/// ローカル（クライアント）CLI バイナリよりリモート CLI バイナリが古い場合にデプロイが必要か
/// （TASK-69・TS `needsDeploy` parity）。`local > remote`（厳密大なり・`[u64;3]` 辞書式比較）のみ `true`。
/// - `local == remote` → skip / `local < remote` → ダウングレード保護で skip
/// - `remote` が `None`（取得失敗/バイナリ未存在）またはパース失敗 → `true`（フェイルセーフ・自動回復）
fn needs_deploy(local: &str, remote: Option<&str>) -> bool {
    let local = match parse_semver(local) {
        Ok(v) => v,
        Err(_) => return true,
    };
    match remote.and_then(|r| parse_semver(r).ok()) {
        Some(r) => local > r,
        None => true,
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
        Self {
            config,
            session: Arc::new(Mutex::new(None)),
            connect_count: Arc::new(AtomicU64::new(0)),
        }
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

    /// SSH接続を確立（キャッシュ済み Session の再構築用・都度 TCP+handshake+認証）。
    /// 戻り値の `Session` は `with_session` 経由でプールにキャッシュされる。
    fn connect(&self) -> Result<Session> {
        let (hostname, port, username, identity_file) = self.load_ssh_config()?;

        let tcp = TcpStream::connect(format!("{}:{}", hostname, port))
            .map_err(|e| KijukuError::Ssh(format!("TCP connection failed: {}", e)))?;

        let mut sess = Session::new()
            .map_err(|e| KijukuError::Ssh(format!("SSH session creation failed: {}", e)))?;

        sess.set_tcp_stream(tcp);
        sess.handshake()
            .map_err(|e| KijukuError::Ssh(format!("SSH handshake failed: {}", e)))?;

        // 認証
        if let Some(key_path) = &identity_file {
            sess.userauth_pubkey_file(&username, None, key_path, None)
                .map_err(|e| KijukuError::Ssh(format!("SSH authentication failed: {}", e)))?;
        } else {
            return Err(KijukuError::Ssh(
                "Private key path is required (set in RemoteConfig or SSH config)".to_string(),
            ));
        }

        if !sess.authenticated() {
            return Err(KijukuError::Ssh("SSH authentication failed".to_string()));
        }

        // keepalive 設定（TASK-70）。
        // 注意: ssh2 0.9.6 の set_keepalive は interval 設定のみで定期送信はドライブされない
        // （別途 keepalive_send() のポーリング呼び出しが必要）。アイドル中の TCP drop 検知は
        // サーバ側 ClientAliveInterval + set_timeout + セッション系エラー時の slot 無効化でフェイルセーフを担保。
        sess.set_keepalive(true, 30);

        Ok(sess)
    }

    /// セッション健全性に関わるエラーか（slot 無効化の判定用・TASK-70）。
    /// `Ssh` のみがセッション破壊を示し、アプリケーションエラー（`Other`/`Validation` 等）は Session を保持する。
    fn is_session_error(e: &KijukuError) -> bool {
        matches!(e, KijukuError::Ssh(_))
    }

    /// プールされた Session 上で `f` を実行（TASK-70）。
    /// 初回は `connect()` で確立してキャッシュ、以降は再利用。`f` が `Ssh` エラーを返した場合は
    /// slot を無効化し、次回 RPC で再接続する（フェイルセーフ）。
    fn with_session<R>(&self, f: impl FnOnce(&mut PooledSession) -> Result<R>) -> Result<R> {
        let mut guard = self.session.lock();
        if guard.is_none() {
            *guard = Some(PooledSession {
                sess: self.connect()?,
                binary_ensured: false,
            });
            self.connect_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let pooled = guard.as_mut().expect("session just established or cached");
        let result = f(pooled);
        if let Err(e) = &result {
            if Self::is_session_error(e) {
                *guard = None;
            }
        }
        result
    }

    /// プールされた SSH Session を切断し、slot を無効化（TASK-70）。
    /// SSH BYE パケットを送る（TCP ソケット自体は Session drop で閉じる）。SDK 長期利用者が明示的に呼ぶ用途で、
    /// 以降の RPC は再接続される。`Drop` は実装しない（`Arc<Mutex>` と相性が悪く、プロセス終了でソケットは閉じる）。
    pub fn disconnect(&self) -> Result<()> {
        let mut guard = self.session.lock();
        if let Some(pooled) = guard.take() {
            let _ = pooled.sess.disconnect(None, "bye", None);
        }
        Ok(())
    }

    /// 新規 SSH 接続を確立した回数（診断用・TASK-70）。連続 RPC で 1 のままなら Session 再利用を示す。
    pub fn connect_count(&self) -> u64 {
        self.connect_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// リモート DB ファイルのサイズを取得（適応的タイムアウト算出用・TS `getRemoteFileSize` parity）。
    /// `stat -c %s <db> 2>/dev/null || echo 0`。失敗時は None（呼び出し元で 0 扱い→デフォルト 60s）。
    fn remote_db_size(&self, sess: &Session, db_path: &str) -> Option<u64> {
        let cmd = format!(
            "stat -c %s {} 2>/dev/null || echo 0",
            escape_for_remote_shell(db_path)
        );
        let mut channel = sess.channel_session().ok()?;
        channel.exec(&cmd).ok()?;
        channel.send_eof().ok()?;
        let mut output = String::new();
        channel.read_to_string(&mut output).ok()?;
        channel.wait_close().ok();
        output.trim().parse::<u64>().ok()
    }

    /// リモートでJSONコマンドを実行（短操作用・タイムアウトは DEFAULT_RPC_TIMEOUT_MS 固定）。
    /// 長操作（backup/restore/sync 等）は `execute_remote_command_timed` で `timeout_ms` を明示する。
    fn execute_remote_command(&self, request: CommandRequest) -> Result<CommandResponse> {
        self.execute_remote_command_timed(request, None)
    }

    /// `execute_remote_command` のタイムアウト指定版（TS parity）。
    /// - `Some(ms)` → 呼び出し元が上書き
    /// - `None` かつ長操作 → DB サイズから適応的に算出（restore は退避+復元で ×2）
    /// - `None` かつ短操作 → DEFAULT_RPC_TIMEOUT_MS (30s)
    ///
    /// SSH `Session` はプールから取得（TASK-70・初回のみ接続・以降再利用・セッション系エラーで無効化）。
    fn execute_remote_command_timed(
        &self,
        request: CommandRequest,
        timeout_ms: Option<u32>,
    ) -> Result<CommandResponse> {
        self.with_session(|pooled| self.execute_on_session(pooled, request, timeout_ms))
    }

    /// `execute_remote_command_timed` の本体。プールされた Session 上で RPC を実行。
    /// `ensure_remote_binary`（`getServerVersion` ラウンドトリップ）は `binary_ensured` で初回のみガードし、
    /// 連続 RPC のオーバーヘッドを削減する。
    fn execute_on_session(
        &self,
        pooled: &mut PooledSession,
        request: CommandRequest,
        timeout_ms: Option<u32>,
    ) -> Result<CommandResponse> {
        let remote_db_path = resolve_remote_db_path(&self.config);
        let remote_binary_path = self
            .config
            .binary_path
            .as_deref()
            .unwrap_or("~/.local/bin/kijuku-cli");

        let json_input = serde_json::to_string(&request)
            .map_err(|e| KijukuError::Other(format!("Failed to serialize request: {}", e)))?;

        // stat コマンド自体のハング対策（TS `getRemoteFileSize` が execCommand 30s デフォルトであることと parity）。
        // 設定しないと remote_db_size 内の channel_read が無限待ちになる。
        pooled.sess.set_timeout(DEFAULT_RPC_TIMEOUT_MS);

        // バージョンベース自動デプロイ（TASK-69）。リモート CLI が古い/未存在なら最新へ更新。
        // ensure 内の getServerVersion は sess を使い回し execute_remote_command_timed を経由しないため再帰しない。
        // 初回のみ実行し、以降は binary_ensured でスキップ（連続 RPC の getServerVersion オーバーヘッドを削減）。
        if !pooled.binary_ensured {
            self.ensure_remote_binary(&pooled.sess)?;
            pooled.binary_ensured = true;
        }

        // 適応的タイムアウト解決（TS parity）。
        //   Some(ms) → 呼び出し元が上書き
        //   None かつ長操作 → DB サイズから算出（restore は退避+復元で ×2）
        //   None かつ短操作 → デフォルト 30s
        let effective_timeout = match timeout_ms {
            Some(ms) => ms,
            None => match long_op_target(&request.operation, &self.config) {
                Some((db_path, multiplier)) => {
                    let size = self.remote_db_size(&pooled.sess, &db_path).unwrap_or(0);
                    calc_backup_timeout_ms(size).saturating_mul(multiplier)
                }
                None => DEFAULT_RPC_TIMEOUT_MS,
            },
        };

        let sess = &pooled.sess;
        sess.set_timeout(effective_timeout);

        let mut channel = sess
            .channel_session()
            .map_err(|e| KijukuError::Ssh(format!("Failed to open channel: {}", e)))?;

        let command = build_remote_command(
            remote_binary_path,
            &remote_db_path,
            self.config.effective_target(),
            self.config.media_root.as_deref(),
        );
        channel
            .exec(&command)
            .map_err(|e| KijukuError::Ssh(format!("Failed to execute command: {}", e)))?;

        use std::io::Write;
        channel
            .write_all(json_input.as_bytes())
            .map_err(|e| KijukuError::Ssh(format!("Failed to write to channel stdin: {}", e)))?;
        channel.send_eof().map_err(|e| KijukuError::Ssh(format!("Failed to send EOF to channel: {}", e)))?;

        let mut output = String::new();
        channel
            .read_to_string(&mut output)
            .map_err(|e| KijukuError::Ssh(format!("Failed to read command stdout (timeout={}ms): {}", effective_timeout, e)))?;

        let mut stderr = String::new();
        channel.stderr().read_to_string(&mut stderr).map_err(|e| KijukuError::Ssh(format!("Failed to read command stderr: {}", e)))?;

        channel.wait_close().ok();

        let exit_status = channel.exit_status()
            .map_err(|e| KijukuError::Ssh(format!("Failed to get exit status: {}", e)))?;

        if exit_status != 0 {
            return Err(KijukuError::Other(format!(
                "Command failed with exit code {}. stdout: {}, stderr: {}",
                exit_status, output, stderr
            )));
        }

        serde_json::from_str(&output)
            .map_err(|e| KijukuError::Other(format!("Failed to parse response from stdout: {}. Raw output: {}", e, output)))
    }

    // --- リモートバイナリ自動デプロイ（TASK-69・バージョン比較ベース） ---

    /// sess 上で単純なシェルコマンドを実行（exit 0 必須・mkdir/chmod/ln 等・TASK-69）。
    /// `escape_for_remote_shell` で `~` → `$HOME` 展開済みのコマンド文字列を渡すこと。
    fn run_remote(sess: &Session, cmd: &str) -> Result<()> {
        let mut channel = sess
            .channel_session()
            .map_err(|e| KijukuError::Other(format!("Failed to open channel: {}", e)))?;
        channel
            .exec(cmd)
            .map_err(|e| KijukuError::Other(format!("Failed to exec command: {}", e)))?;
        channel.send_eof().ok();
        let mut output = String::new();
        channel.read_to_string(&mut output).ok();
        channel.wait_close().ok();
        let exit_status = channel
            .exit_status()
            .map_err(|e| KijukuError::Other(format!("Failed to get exit status: {}", e)))?;
        if exit_status != 0 {
            let mut stderr = String::new();
            channel.stderr().read_to_string(&mut stderr).ok();
            return Err(KijukuError::Other(format!(
                "Command failed with exit code {}. cmd: {}, stdout: {}, stderr: {}",
                exit_status, cmd, output, stderr
            )));
        }
        Ok(())
    }

    /// リモートの `$HOME` 絶対パスを取得（TASK-69・SFTP 転送先解決用）。
    /// libssh2 の SFTP は `~` を展開しないため転送先は絶対パスが必要（TS ssh2 の `~` 展開とは異なる）。
    fn remote_home(sess: &Session) -> Result<String> {
        let mut channel = sess
            .channel_session()
            .map_err(|e| KijukuError::Other(format!("Failed to open channel: {}", e)))?;
        channel
            .exec("echo $HOME")
            .map_err(|e| KijukuError::Other(format!("Failed to exec echo $HOME: {}", e)))?;
        channel.send_eof().ok();
        let mut output = String::new();
        channel
            .read_to_string(&mut output)
            .map_err(|e| KijukuError::Other(format!("Failed to read $HOME: {}", e)))?;
        channel.wait_close().ok();
        let home = output.trim().to_string();
        if home.is_empty() || !home.starts_with('/') {
            return Err(KijukuError::Other(format!("invalid remote $HOME: {:?}", home)));
        }
        Ok(home)
    }

    /// デプロイ元のローカル CLI バイナリパス（TS `resolve(homedir(), '.local', 'bin', 'kijuku-cli')` parity）。
    fn local_binary_path() -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .map_err(|_| KijukuError::Other("HOME env not set".to_string()))?;
        Ok(PathBuf::from(home).join(".local").join("bin").join("kijuku-cli"))
    }

    /// リモート CLI バイナリのバージョンを取得（TASK-69・`getServerVersion` operation）。
    /// `execute_remote_command_timed` と同じ channel パターンだが sess を使い回し（ensure を bypass・
    /// 無限再帰回避）。失敗時は `Ok(None)`（呼び出し元 `ensure_remote_binary` が None を「デプロイ必要」
    /// と解釈・古いバイナリからの自動回復）。
    fn get_server_version(&self, sess: &Session) -> Result<Option<String>> {
        let remote_db_path = resolve_remote_db_path(&self.config);
        let remote_binary_path = self
            .config
            .binary_path
            .as_deref()
            .unwrap_or("~/.local/bin/kijuku-cli");
        let command = build_remote_command(
            remote_binary_path,
            &remote_db_path,
            self.config.effective_target(),
            self.config.media_root.as_deref(),
        );
        let json_input = r#"{"operation":"getServerVersion","params":{}}"#;

        let mut channel = match sess.channel_session() {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };
        if channel.exec(&command).is_err() {
            return Ok(None);
        }
        use std::io::Write;
        if channel.write_all(json_input.as_bytes()).is_err() {
            return Ok(None);
        }
        if channel.send_eof().is_err() {
            return Ok(None);
        }
        let mut output = String::new();
        if channel.read_to_string(&mut output).is_err() {
            return Ok(None);
        }
        channel.wait_close().ok();

        let response: CommandResponse = match serde_json::from_str(output.trim()) {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };
        if !response.success {
            return Ok(None);
        }
        Ok(response
            .data
            .as_ref()
            .and_then(|v| v.get("version"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()))
    }

    /// media_root サンドボックスを回避し絶対パスへ SFTP 書き込み（TASK-69・バイナリデプロイ用）。
    /// `upload` は `resolve_remote_within_root` で media_root 配下に強制するためバイナリ配置には使えない。
    /// `remote_abs` は絶対パス（`~` 未展開・`remote_home` で解決済み）。
    fn upload_to_absolute_path(
        &self,
        sess: &Session,
        local_path: &std::path::Path,
        remote_abs: &str,
    ) -> Result<()> {
        let sftp = sess
            .sftp()
            .map_err(|e| KijukuError::Other(format!("SFTP セッション確立失敗: {}", e)))?;
        let mut local = std::fs::File::open(local_path)
            .map_err(|e| KijukuError::Other(format!("ローカルバイナリのオープン失敗: {}", e)))?;
        let mut remote = sftp
            .create(std::path::Path::new(remote_abs))
            .map_err(|e| KijukuError::Other(format!("リモートバイナリの作成失敗: {}", e)))?;
        std::io::copy(&mut local, &mut remote)
            .map_err(|e| KijukuError::Other(format!("バイナリ転送失敗: {}", e)))?;
        Ok(())
    }

    /// ローカル CLI バイナリをリモートへデプロイ（TASK-69・`scripts/dev/deploy-local.sh` L53-67 移植）。
    /// 実体 `~/.local/kijuku-db/bin/kijuku-cli` + symlink `~/.local/bin/kijuku-cli` 構成。
    /// 1 セッション内で完結（TASK-70 接続プール化を見据え sess 受け取り）。
    fn deploy_binary_to_remote(&self, sess: &Session) -> Result<()> {
        let home = Self::remote_home(sess)?;
        let remote_real_abs = format!("{}/.local/kijuku-db/bin/kijuku-cli", home);
        let local = Self::local_binary_path()?;

        // (1) mkdir -p 実体Dir + symlinkDir（シェル経由で ~ → $HOME 展開）
        Self::run_remote(
            sess,
            &format!(
                "mkdir -p {} {}",
                escape_for_remote_shell("~/.local/kijuku-db/bin"),
                escape_for_remote_shell("~/.local/bin")
            ),
        )?;
        // (2) SFTP 転送（絶対パス・ファイル転送タイムアウト 120s）
        sess.set_timeout(FILE_TRANSFER_TIMEOUT_MS);
        self.upload_to_absolute_path(sess, &local, &remote_real_abs)?;
        sess.set_timeout(DEFAULT_RPC_TIMEOUT_MS);
        // (3) chmod +x 実体
        Self::run_remote(
            sess,
            &format!(
                "chmod +x {}",
                escape_for_remote_shell("~/.local/kijuku-db/bin/kijuku-cli")
            ),
        )?;
        // (4) ln -sf 実体 → symlink（冪等・上書き）
        Self::run_remote(
            sess,
            &format!(
                "ln -sf {} {}",
                escape_for_remote_shell("~/.local/kijuku-db/bin/kijuku-cli"),
                escape_for_remote_shell("~/.local/bin/kijuku-cli")
            ),
        )?;
        Ok(())
    }

    /// リモート CLI バイナリが古い場合に自動デプロイ（TASK-69・`execute_remote_command_timed` 先頭で毎 RPC 呼ばれる）。
    /// getServerVersion → `needs_deploy`（ローカル > リモート判定）→ 必要ならデプロイ。
    /// リモートバージョン取得失敗（古いバイナリ・未存在）はデプロイで自動回復。
    fn ensure_remote_binary(&self, sess: &Session) -> Result<()> {
        let remote_version = self.get_server_version(sess)?;
        if needs_deploy(env!("CARGO_PKG_VERSION"), remote_version.as_deref()) {
            self.deploy_binary_to_remote(sess)?;
        }
        Ok(())
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
        self.with_session(|pooled| {
            // ファイル転送のタイムアウト（TS parity・固定 120s）。
            pooled.sess.set_timeout(FILE_TRANSFER_TIMEOUT_MS);
            let sftp = pooled
                .sess
                .sftp()
                .map_err(|e| KijukuError::Ssh(format!("SFTP セッション確立失敗: {}", e)))?;
            if let Some(parent) = remote_abs.parent() {
                Self::ensure_remote_dir(&sftp, parent);
            }
            let mut local = std::fs::File::open(local_path)?;
            let mut remote = sftp
                .create(&remote_abs)
                .map_err(|e| KijukuError::Other(format!("リモートファイル作成失敗: {}", e)))?;
            std::io::copy(&mut local, &mut remote)?;
            Ok(())
        })
    }

    /// リモートの `remote_rel`（media root 相対）をローカルの `local_path` へダウンロード（SFTP・ストリーミング）。
    pub fn download(&self, remote_rel: &str, local_path: &std::path::Path) -> Result<()> {
        let remote_abs = self.resolve_remote_within_root(remote_rel)?;
        self.with_session(|pooled| {
            // ファイル転送のタイムアウト（TS parity・固定 120s）。
            pooled.sess.set_timeout(FILE_TRANSFER_TIMEOUT_MS);
            let sftp = pooled
                .sess
                .sftp()
                .map_err(|e| KijukuError::Ssh(format!("SFTP セッション確立失敗: {}", e)))?;
            let mut remote = sftp
                .open(&remote_abs)
                .map_err(|e| KijukuError::Other(format!("リモートファイルオープン失敗: {}", e)))?;
            if let Some(parent) = local_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut local = std::fs::File::create(local_path)?;
            std::io::copy(&mut remote, &mut local)?;
            Ok(())
        })
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
    pub fn backup(&self, label: Option<&str>, timeout_ms: Option<u32>) -> Result<Option<String>> {
        let response = self.execute_remote_command_timed(CommandRequest {
            operation: "backup".to_string(),
            params: serde_json::json!({ "label": label }),
        }, timeout_ms)?;

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

    /// 監査ログを取得（設計 §10・TASK-46）。サーバ側（prod の `backup/meta/audit.log`）を読む。
    pub fn list_audit_logs(&self, filter: &AuditLogFilter) -> Result<Vec<AuditRecord>> {
        let response = self.execute_remote_command(CommandRequest {
            operation: "listAuditLogs".to_string(),
            params: serde_json::to_value(filter).unwrap_or_default(),
        })?;
        let logs: Vec<AuditRecord> = self.check_response(response)?;
        Ok(logs)
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
    pub fn restore(&self, selector: &serde_json::Value, timeout_ms: Option<u32>) -> Result<String> {
        let response = self.execute_remote_command_timed(CommandRequest {
            operation: "restore".to_string(),
            params: serde_json::json!({ "selector": selector }),
        }, timeout_ms)?;

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
        timeout_ms: Option<u32>,
    ) -> Result<crate::diff::BackupDiff> {
        let response = self.execute_remote_command_timed(CommandRequest {
            operation: "diffBackup".to_string(),
            params: serde_json::json!({
                "selector": selector,
                "options": serde_json::to_value(options)
                    .map_err(|e| KijukuError::Other(e.to_string()))?,
            }),
        }, timeout_ms)?;
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
        timeout_ms: Option<u32>,
    ) -> Result<crate::diff::BackupDiff> {
        let response = self.execute_remote_command_timed(CommandRequest {
            operation: "diffProdStg".to_string(),
            params: serde_json::json!({
                "prodDbPath": prod_db_path,
                "options": serde_json::to_value(options)
                    .map_err(|e| KijukuError::Other(e.to_string()))?,
            }),
        }, timeout_ms)?;
        self.check_response(response)
    }

    /// prod(RO) と stg の整合性を観測し gate を評価（設計 §3.4・promote 判断の客観根拠）。
    /// `diff_with_prod` と同様、リモート CLI は self=stg 起動を想定し `prod_db_path` で prod を指定。
    /// `prod_db_path` が None の場合は CLI 側で prod target のデフォルトパスを解決する。
    pub fn observe(
        &self,
        prod_db_path: Option<&std::path::Path>,
        options: &crate::diff::ObserveOptions,
        timeout_ms: Option<u32>,
    ) -> Result<crate::diff::ObserveResult> {
        let response = self.execute_remote_command_timed(CommandRequest {
            operation: "observe".to_string(),
            params: serde_json::json!({
                "prodDbPath": prod_db_path,
                "options": serde_json::to_value(options)
                    .map_err(|e| KijukuError::Other(e.to_string()))?,
            }),
        }, timeout_ms)?;
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
        timeout_ms: Option<u32>,
    ) -> Result<crate::PromoteOutcome> {
        let response = self.execute_remote_command_timed(CommandRequest {
            operation: "promote".to_string(),
            params: serde_json::json!({
                "prodDbPath": prod_db_path,
                "options": serde_json::to_value(options)
                    .map_err(|e| KijukuError::Other(e.to_string()))?,
                "backupOpts": backup_opts,
            }),
        }, timeout_ms)?;
        self.check_response(response)
    }

    /// prod→stg コピー操作の共通基盤（sync/discard・設計 §4.2/§4.6）。`operation` に "sync" または
    /// "discard" を渡す。`from`/`to` は `resolve_sync_paths_from_config` で必ず具象パスに解決して
    /// 明示渡す（TS parity）。サーバ側 `resolve_sync_paths` は TASK-94 でデフォルト補完しなく
    /// なったため、null を送ると環境変数設定がない限り失敗する。
    fn run_prod_to_stg(&self, operation: &str, timeout_ms: Option<u32>) -> Result<SyncResult> {
        let (from, to) = resolve_sync_paths_from_config(&self.config);
        let response = self.execute_remote_command_timed(CommandRequest {
            operation: operation.to_string(),
            params: serde_json::json!({ "from": from, "to": to }),
        }, timeout_ms)?;
        self.check_response(response)
    }

    /// prod→stg の初期同期（設計 §4.2/§5.3・書込セッション開始時）。Online Backup 1 パスのコピー。
    pub fn sync(&self, timeout_ms: Option<u32>) -> Result<SyncResult> {
        self.run_prod_to_stg("sync", timeout_ms)
    }

    /// stg 破棄・再 sync（設計 §4.6・書込セッション中断/gate 不合格時）。処理は `sync` と同一
    /// （prod→stg の Online Backup コピー・既存 stg は上書き破棄・prod は一切触らない）。
    /// 操作名のみ監査ログ（§10）で区別するため独立メソッド。
    pub fn discard(&self, timeout_ms: Option<u32>) -> Result<SyncResult> {
        self.run_prod_to_stg("discard", timeout_ms)
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
/// `RemoteKijukuDB` は `Clone` 可能で、clone 間で `Arc<Mutex<Option<PooledSession>>>` を共有（TASK-70）。
/// 初回 RPC で SSH Session を確立してキャッシュし、以降の連続 RPC は再利用（再 handshake 省略）。
/// セッション系エラー（`KijukuError::Ssh`）時は slot 無効化で次回再接続する。
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

    /// sync/discard の from/to は必ず具象パスに解決される（サーバ側 resolve_sync_paths は
    /// TASK-94 でデフォルト補完しないため・TS runProdToStg と同一規約・PR#61 レビュー指摘）。
    #[test]
    fn resolve_sync_paths_from_config_always_concrete() {
        // db_path 指定: from=db_path、to=導出 stg
        let explicit = RemoteConfig {
            db_path: Some("~/.local/share/kijuku/kijuku.db".to_string()),
            stg_db_path: None,
            ..Default::default()
        };
        assert_eq!(
            resolve_sync_paths_from_config(&explicit),
            (
                "~/.local/share/kijuku/kijuku.db".to_string(),
                "~/.local/share/kijuku/kijuku.stg.db".to_string(),
            )
        );

        // stg_db_path 指定: to は導出でなく明示値
        let stg_explicit = RemoteConfig {
            db_path: Some("~/.local/share/kijuku/kijuku.db".to_string()),
            stg_db_path: Some("/custom/stg.db".to_string()),
            ..Default::default()
        };
        assert_eq!(
            resolve_sync_paths_from_config(&stg_explicit),
            (
                "~/.local/share/kijuku/kijuku.db".to_string(),
                "/custom/stg.db".to_string(),
            )
        );

        // 両方未指定: リモートデフォルトで補完（null を送らない）
        let all_default = RemoteConfig {
            db_path: None,
            stg_db_path: None,
            ..Default::default()
        };
        assert_eq!(
            resolve_sync_paths_from_config(&all_default),
            (
                "~/.local/share/kijuku/kijuku.db".to_string(),
                "~/.local/share/kijuku/kijuku.stg.db".to_string(),
            )
        );
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

    // --- 適応的タイムアウト（TASK-67・TS parity） ---

    #[test]
    fn calc_backup_timeout_ms_enforces_60s_minimum() {
        // size=0・極小サイズは最低 60s（TS parity: Math.max(..., 60_000)）
        assert_eq!(calc_backup_timeout_ms(0), 60_000);
        assert_eq!(calc_backup_timeout_ms(1), 60_000);
    }

    #[test]
    fn calc_backup_timeout_ms_matches_ts_formula() {
        // TS calcBackupTimeoutMs（ts-sdk/src/remote.ts）と厳密一致を既知値で検証。
        // 1 GiB: copyTime=20480, numBatches=ceil(1)=1, sleep=10000, total=(20480+10000)*2=60960
        assert_eq!(calc_backup_timeout_ms(1024 * 1024 * 1024), 60_960);
    }

    #[test]
    fn calc_backup_timeout_ms_saturates_at_u32_max() {
        // f64 で計算後 u32::MAX を超える場合は飽和（オーバーフローしない）
        assert_eq!(calc_backup_timeout_ms(u64::MAX), u32::MAX);
    }

    #[test]
    fn is_long_running_op_covers_eight_ops() {
        // 長操作8種（TS parity・calcBackupTimeoutMs 適用対象）
        for op in &[
            "backup",
            "restore",
            "sync",
            "discard",
            "diffBackup",
            "diffProdStg",
            "observe",
            "promote",
        ] {
            assert!(is_long_running_op(op), "{} should be long-running", op);
        }
        // 短操作・ファイル転送・未定義は対象外
        assert!(!is_long_running_op("upload"));
        assert!(!is_long_running_op("download"));
        assert!(!is_long_running_op("listBackups"));
        assert!(!is_long_running_op(""));
    }

    #[test]
    fn long_op_target_restore_doubles_timeout() {
        // restore は現在DB退避 + 復元の2段階 → multiplier=2（TS remote.ts L1162）
        let config = RemoteConfig {
            target: Target::Prod,
            db_path: Some("/db/prod.db".to_string()),
            ..Default::default()
        };
        assert_eq!(
            long_op_target("restore", &config),
            Some(("/db/prod.db".to_string(), 2))
        );
    }

    #[test]
    fn long_op_target_sync_discard_uses_prod_path() {
        // sync/discard は prod パス（target に依存しない）・乗数1（TS remote.ts L1188/L1195）
        let stg_config = RemoteConfig {
            target: Target::Stg,
            db_path: Some("/db/prod.db".to_string()),
            stg_db_path: Some("/db/stg.db".to_string()),
            ..Default::default()
        };
        assert_eq!(
            long_op_target("sync", &stg_config),
            Some(("/db/prod.db".to_string(), 1))
        );
        assert_eq!(
            long_op_target("discard", &stg_config),
            Some(("/db/prod.db".to_string(), 1))
        );
        // db_path 未設定時はデフォルト prod パス
        let no_db = RemoteConfig {
            target: Target::Prod,
            db_path: None,
            ..Default::default()
        };
        assert_eq!(
            long_op_target("sync", &no_db),
            Some((DEFAULT_REMOTE_PROD_DB.to_string(), 1))
        );
    }

    #[test]
    fn long_op_target_default_uses_resolved_path() {
        // backup/diffBackup/diffProdStg/observe/promote は resolve_remote_db_path と同一・乗数1
        let prod_config = RemoteConfig {
            target: Target::Prod,
            db_path: Some("/db/prod.db".to_string()),
            ..Default::default()
        };
        for op in &["backup", "diffBackup", "diffProdStg", "observe", "promote"] {
            assert_eq!(
                long_op_target(op, &prod_config),
                Some((resolve_remote_db_path(&prod_config), 1)),
                "{}",
                op
            );
        }
        // stg ターゲットなら stg パスに解決される（derive_stg_db_path）
        let stg_config = RemoteConfig {
            target: Target::Stg,
            db_path: Some("/db/prod.db".to_string()),
            ..Default::default()
        };
        assert_eq!(
            long_op_target("backup", &stg_config),
            Some((resolve_remote_db_path(&stg_config), 1))
        );
    }

    #[test]
    fn long_op_target_short_ops_return_none() {
        // 短操作・未定義は適応的タイムアウト対象外（DEFAULT_RPC_TIMEOUT_MS 固定）
        let config = RemoteConfig::default();
        assert_eq!(long_op_target("upload", &config), None);
        assert_eq!(long_op_target("listBackups", &config), None);
        assert_eq!(long_op_target("", &config), None);
    }

    // --- バージョン比較（TASK-69・TS parity） ---

    #[test]
    fn parse_semver_basic() {
        assert_eq!(parse_semver("0.2.2").unwrap(), [0, 2, 2]);
        // 短縮形は 0 補間
        assert_eq!(parse_semver("0.2").unwrap(), [0, 2, 0]);
        assert_eq!(parse_semver("1").unwrap(), [1, 0, 0]);
        // 4要素以上は Err
        assert!(parse_semver("0.2.2.1").is_err());
        // 非数値は Err
        assert!(parse_semver("0.x.2").is_err());
        assert!(parse_semver("0.2.-1").is_err());
    }

    #[test]
    fn needs_deploy_logic() {
        // リモート未取得 → デプロイ（自動回復）
        assert!(needs_deploy("0.2.2", None));
        // upgrade → デプロイ
        assert!(needs_deploy("0.2.2", Some("0.2.1")));
        assert!(needs_deploy("1.0.0", Some("0.2.2")));
        // equal → skip
        assert!(!needs_deploy("0.2.2", Some("0.2.2")));
        // downgrade 保護（ローカルが古い → skip）
        assert!(!needs_deploy("0.2.1", Some("0.2.2")));
        assert!(!needs_deploy("0.2.2", Some("1.0.0")));
        // リモートパース失敗 → フェイルセーフでデプロイ
        assert!(needs_deploy("0.2.2", Some("garbage")));
        assert!(needs_deploy("0.2.2", Some("0.2.2.1")));
        // リモート短縮形 → 0 補間で比較（0.2.0 < 0.2.2 → デプロイ）
        assert!(needs_deploy("0.2.2", Some("0.2")));
    }

    // --- 接続プール（TASK-70） ---

    #[test]
    fn is_session_error_classifies_correctly() {
        // Ssh はセッション系（slot 無効化対象）
        assert!(RemoteKijukuDB::is_session_error(&KijukuError::Ssh(
            "channel failed".to_string()
        )));
        // アプリケーションエラーは Session を保持（slot 無効化しない）
        assert!(!RemoteKijukuDB::is_session_error(&KijukuError::Other(
            "exit code 1".to_string()
        )));
        assert!(!RemoteKijukuDB::is_session_error(&KijukuError::Validation(
            "bad input".to_string()
        )));
        assert!(!RemoteKijukuDB::is_session_error(&KijukuError::NotFound(
            "missing".to_string()
        )));
    }
}
