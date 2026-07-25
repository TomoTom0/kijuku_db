/// バックアップ機能
///
/// ディレクトリ構成:
/// ```text
/// backup/
///   auto/   - 自動バックアップ（フル .db / 差分 .diff）
///   manual/ - 手動バックアップ（常にフル .db）
///   tmp/    - restore前自動退避 (.db)
///   meta/
///     auto-records.csv
/// ```
///
/// ファイル名規則:
/// - `{stem}.{timestamp}.db`            (フル)
/// - `{stem}.{timestamp}.diff`          (差分)
/// - `{stem}.{timestamp}-{label}.db`    (ラベル付き手動)
/// - `{stem}.{timestamp}-pre_restore.db` (tmp退避)
///
/// timestamp = `YYYYMMDDHHMMSS-mmm` (18文字固定)
use crate::{KijukuError, Result};
use chrono::{Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const HOUR_SECS: u64 = 3_600;
const DAY_SECS: u64 = 86_400;
const WEEK_SECS: u64 = 604_800;
const MONTH_SECS: u64 = 2_592_000;
const QUARTER_SECS: u64 = 7_776_000;
const YEAR_SECS: u64 = 31_536_000;

// ============================================================
// 型定義
// ============================================================

/// バックアップのスコープ（保存先ディレクトリ）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupScope {
    /// 自動バックアップ (backup/auto/)
    Auto,
    /// 手動バックアップ (backup/manual/)
    Manual,
    /// restore前自動退避 (backup/tmp/)
    Tmp,
}

/// バックアップの種別（フル / 差分）
#[derive(Debug, Clone)]
pub enum BackupKind {
    /// フルバックアップ（.db）
    Full,
    /// 差分バックアップ（.diff）。基底フルのIDを持つ
    Diff { base_id: String },
}

/// ラベルの由来
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelSource {
    /// ファイル名に埋め込まれた作成時ラベル
    Filename,
    /// サイドカー（backup-meta.json）の事後付与ラベル
    Sidecar,
}

/// バックアップ情報
#[derive(Debug, Clone)]
pub struct BackupInfo {
    /// バックアップID（タイムスタンプ文字列、18文字）
    pub id: String,
    /// ファイル名
    pub name: String,
    /// ファイルパス
    pub path: PathBuf,
    /// 作成日時
    pub created_at: SystemTime,
    /// スコープ（auto / manual / tmp）
    pub scope: BackupScope,
    /// 種別（フル / 差分）
    pub kind: BackupKind,
    /// ラベル（サイドカー優先、なければファイル名由来）
    pub label: Option<String>,
    /// ラベルの由来
    pub label_source: LabelSource,
    /// メモ（サイドカー backup-meta.json 由来）
    pub note: Option<String>,
}

impl BackupInfo {
    /// 手動バックアップかどうか
    pub fn is_manual(&self) -> bool {
        self.scope == BackupScope::Manual
    }
}

/// auto-records.csv のステータス
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoRecordStatus {
    Kept,
    Pruned,
}

/// auto-records.csv の1レコード
#[derive(Debug, Clone)]
pub struct AutoRecord {
    pub id: String,
    pub created_at: String,
    pub tier: String,
    pub kind: String,
    pub base_id: String,
    pub size_bytes: u64,
    pub status: AutoRecordStatus,
    pub pruned_at: String,
}

/// バックアップの事後メタ（ラベル/メモ）。サイドカー backup-meta.json の1エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetaEntry {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub updated_at: String,
}

/// サイドカー backup-meta.json の全体
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackupMetaStore {
    pub entries: std::collections::BTreeMap<String, BackupMetaEntry>,
}

/// 保持ポリシーの1段階
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionTier {
    pub max_age_secs: u64,
    pub keep_interval_secs: u64,
}

/// 自動バックアップの粗密保持ポリシー
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPolicy {
    pub tiers: Vec<RetentionTier>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            tiers: vec![
                RetentionTier { max_age_secs: HOUR_SECS,    keep_interval_secs: 0 },
                RetentionTier { max_age_secs: DAY_SECS,     keep_interval_secs: HOUR_SECS },
                RetentionTier { max_age_secs: WEEK_SECS,    keep_interval_secs: DAY_SECS },
                RetentionTier { max_age_secs: MONTH_SECS,   keep_interval_secs: WEEK_SECS },
                RetentionTier { max_age_secs: QUARTER_SECS, keep_interval_secs: MONTH_SECS },
                RetentionTier { max_age_secs: YEAR_SECS,    keep_interval_secs: QUARTER_SECS },
                RetentionTier { max_age_secs: u64::MAX,     keep_interval_secs: YEAR_SECS },
            ],
        }
    }
}

/// バックアップ進捗情報
#[derive(Debug, Clone)]
pub struct BackupProgress {
    pub total_pages: i32,
    pub remaining_pages: i32,
}

/// バックアップ設定オプション
///
/// 全フィールド `Option` かつ `#[serde(default)]` で部分指定を許容（CLI/remote wire・TASK-62）。
/// `onProgress` のようなコールバックは持たない（Rust 側は純粋なシリアライズ対象）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BackupOptions {
    /// バックアップ保存先ディレクトリ
    pub backup_dir: Option<String>,
    /// 自動バックアップのトリガー間隔（ミリ秒）
    pub interval_ms: Option<u64>,
    /// バックアップ機能全体の有効/無効
    pub enabled: Option<bool>,
    /// 自動バックアップの有効/無効
    pub auto_enabled: Option<bool>,
    /// tmp/ の保持期間（秒）
    pub tmp_retention_secs: Option<u64>,
    /// 最大バックアップ数（retention_policy が None の場合のみ有効）
    pub max_backups: Option<usize>,
    /// 最大保持日数（retention_policy が None の場合のみ有効）
    pub max_age_days: Option<u64>,
    /// 保持ポリシー
    pub retention_policy: Option<RetentionPolicy>,
    /// DBロック時に1回の試行で待機する最大時間（ミリ秒）
    pub busy_timeout_ms: Option<u64>,
    /// DBロック時のリトライ間隔（ミリ秒）のリスト。長さがリトライ回数を決定する
    pub retry_intervals_ms: Option<Vec<u64>>,
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            backup_dir: None,
            interval_ms: Some(3_600_000),
            enabled: Some(true),
            auto_enabled: Some(true),
            tmp_retention_secs: Some(604_800),
            max_backups: None,
            max_age_days: None,
            retention_policy: None,
            busy_timeout_ms: Some(5_000),
            retry_intervals_ms: Some(vec![5_000, 10_000, 30_000, 60_000]),
        }
    }
}

/// バックアップ選択条件
#[derive(Debug, Clone)]
pub struct BackupSelector {
    pub(crate) kind: BackupSelectorKind,
    /// スコープフィルタ（None = 全スコープ対象）
    pub scope_filter: Option<BackupScope>,
}

#[derive(Debug, Clone)]
pub(crate) enum BackupSelectorKind {
    Latest,
    Nth(usize),
    Before(SystemTime),
    After(SystemTime),
    ClosestTo(SystemTime),
    /// ID（タイムスタンプ文字列 YYYYMMDDHHMMSS-mmm）で直接指定
    ById(String),
    /// 既知パスで直接指定（設計 §8 即時復旧・pre-stash 等、list から除外されるファイル用）
    ByPath(PathBuf),
}

impl BackupSelector {
    pub fn latest() -> Self {
        Self { kind: BackupSelectorKind::Latest, scope_filter: None }
    }
    pub fn nth(n: usize) -> Self {
        Self { kind: BackupSelectorKind::Nth(n), scope_filter: None }
    }
    pub fn before(time: SystemTime) -> Self {
        Self { kind: BackupSelectorKind::Before(time), scope_filter: None }
    }
    pub fn after(time: SystemTime) -> Self {
        Self { kind: BackupSelectorKind::After(time), scope_filter: None }
    }
    pub fn closest_to(time: SystemTime) -> Self {
        Self { kind: BackupSelectorKind::ClosestTo(time), scope_filter: None }
    }

    /// ID（タイムスタンプ文字列）でバックアップを直接指定する
    pub fn by_id(id: impl Into<String>) -> Self {
        Self { kind: BackupSelectorKind::ById(id.into()), scope_filter: None }
    }

    /// 既知パスでバックアップファイルを直接指定する（設計 §8 即時復旧・pre-stash 戻し）。
    ///
    /// `list_backups` から除外される pre-stash（`...-pre_promote.db` 等）を restore/diff する経路。
    /// promote/(b)操作が返す `pre_stash_path` を restore に回す即時復旧ループ（§8）で使用する。
    /// backup_dir 配下の `.db` ファイルのみ許可（`select_backup` で検証）。
    pub fn by_path(path: impl Into<PathBuf>) -> Self {
        Self { kind: BackupSelectorKind::ByPath(path.into()), scope_filter: None }
    }

    /// スコープを限定する（TASK-150）
    pub fn scope(mut self, scope: BackupScope) -> Self {
        self.scope_filter = Some(scope);
        self
    }

    /// stdin プロトコル wire 形式（`{"type":...}`・サーバ側 `BackupSelectorJson` と対称）へ直列化。
    ///
    /// `RemoteKijukuDB` の restore / diff_with_backup が selector を JSON として送るために使用。
    /// TS `RemoteBackupSelector`（`{type:"latest"|"nth"|"byId"|"byPath"}`）と同形式。
    /// - `scope_filter` は wire に載せない（サーバ側 stdin プロトコルが scope 非対応）。
    /// - `Before` / `After` / `ClosestTo` は非対応（サーバ側 `BackupSelectorJson` にバリアントがない）。
    pub fn to_wire_value(&self) -> Result<serde_json::Value> {
        use serde_json::json;
        let value = match &self.kind {
            BackupSelectorKind::Latest => json!({ "type": "latest" }),
            BackupSelectorKind::Nth(n) => json!({ "type": "nth", "n": n }),
            BackupSelectorKind::ById(id) => json!({ "type": "byId", "id": id }),
            BackupSelectorKind::ByPath(p) => {
                json!({ "type": "byPath", "path": p.to_string_lossy() })
            }
            BackupSelectorKind::Before(_)
            | BackupSelectorKind::After(_)
            | BackupSelectorKind::ClosestTo(_) => {
                return Err(KijukuError::Other(
                    "このセレクタ種別（Before/After/ClosestTo）は stdin プロトコルでサポートされていません"
                        .to_string(),
                ));
            }
        };
        Ok(value)
    }
}

impl Default for BackupSelector {
    fn default() -> Self {
        Self::latest()
    }
}

// ============================================================
// BackupManager
// ============================================================

pub struct BackupManager {
    last_backup_time: Arc<Mutex<Option<u64>>>,
    last_operation_time: Arc<Mutex<Option<u64>>>,
    backup_dir: PathBuf,
    interval_ms: u64,
    enabled: bool,
    auto_enabled: bool,
    tmp_retention_secs: u64,
    db_path: PathBuf,
    db_stem: String,
    max_backups: Option<usize>,
    max_age_days: Option<u64>,
    retention_policy: Option<RetentionPolicy>,
    busy_timeout_ms: u64,
    retry_intervals_ms: Vec<u64>,
}

impl BackupManager {
    pub fn new<P: AsRef<Path>>(db_path: P, options: BackupOptions) -> Result<Self> {
        let db_path_buf = db_path.as_ref().to_path_buf();
        let db_stem = db_path_buf
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("database")
            .to_string();

        let backup_dir = match options.backup_dir {
            Some(dir) => PathBuf::from(dir),
            None => {
                let parent = db_path_buf.parent().unwrap_or(Path::new("."));
                parent.join("backup")
            }
        };

        let interval_ms = options.interval_ms.unwrap_or(3_600_000);
        let enabled = options.enabled.unwrap_or(true);
        let auto_enabled = options.auto_enabled.unwrap_or(true);
        let tmp_retention_secs = options.tmp_retention_secs.unwrap_or(604_800);
        let busy_timeout_ms = options.busy_timeout_ms.unwrap_or(5_000);
        let retry_intervals_ms = options
            .retry_intervals_ms
            .unwrap_or_else(|| vec![5_000, 10_000, 30_000, 60_000]);

        if enabled {
            fs::create_dir_all(backup_dir.join("auto"))?;
            fs::create_dir_all(backup_dir.join("manual"))?;
            fs::create_dir_all(backup_dir.join("meta"))?;
        }

        Ok(Self {
            last_backup_time: Arc::new(Mutex::new(None)),
            last_operation_time: Arc::new(Mutex::new(None)),
            backup_dir,
            interval_ms,
            enabled,
            auto_enabled,
            tmp_retention_secs,
            db_path: db_path_buf,
            db_stem,
            max_backups: options.max_backups,
            max_age_days: options.max_age_days,
            retention_policy: options.retention_policy,
            busy_timeout_ms,
            retry_intervals_ms,
        })
    }

    /// 操作を記録し、必要に応じて自動バックアップを実行
    pub fn record_operation(&self) -> Result<()> {
        if !self.enabled || !self.auto_enabled {
            return Ok(());
        }

        let now = now_ms()?;
        *lock_mutex(&self.last_operation_time)? = Some(now);

        let should_backup = {
            let last_backup = lock_mutex(&self.last_backup_time)?;
            match *last_backup {
                None => true,
                Some(last) => now.saturating_sub(last) >= self.interval_ms,
            }
        };

        if should_backup {
            self.backup_auto()?;
        }

        Ok(())
    }

    /// 自動バックアップを実行（内部用）
    pub(crate) fn backup_auto(&self) -> Result<String> {
        let timestamp = current_timestamp_str();
        let auto_dir = self.backup_dir.join("auto");
        fs::create_dir_all(&auto_dir)?;

        // 今日のフルバックアップを探す（差分の基底）
        let today_full_opt = self.find_today_full_backup()?;

        let (backup_path, is_diff, base_id_opt) = if let Some(ref base) = today_full_opt {
            // 差分バックアップを作成
            match self.create_diff_backup_file(&timestamp, base) {
                Ok(path) => (path, true, Some(base.id.clone())),
                Err(_) => {
                    // 差分作成失敗時はフルにフォールバック
                    let filename = format!("{}.{}.db", self.db_stem, timestamp);
                    let path = auto_dir.join(&filename);
                    self.copy_db_to(&path)?;
                    (path, false, None)
                }
            }
        } else {
            // フルバックアップを作成（今日の最初 = daily）
            let filename = format!("{}.{}.db", self.db_stem, timestamp);
            let path = auto_dir.join(&filename);
            self.copy_db_to(&path)?;
            (path, false, None)
        };

        *lock_mutex(&self.last_backup_time)? = Some(now_ms()?);

        // records.csv に追記
        let size_bytes = fs::metadata(&backup_path).map(|m| m.len()).unwrap_or(0);
        let tier = if is_diff {
            let elapsed_secs = today_full_opt
                .as_ref()
                .and_then(|b| b.created_at.elapsed().ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if elapsed_secs < HOUR_SECS { "recent" } else { "hourly" }
        } else {
            "daily"
        };
        let record = AutoRecord {
            id: timestamp,
            created_at: iso8601_now(),
            tier: tier.to_string(),
            kind: if is_diff { "diff".to_string() } else { "full".to_string() },
            base_id: base_id_opt.unwrap_or_default(),
            size_bytes,
            status: AutoRecordStatus::Kept,
            pruned_at: String::new(),
        };
        let _ = self.append_auto_record(&record); // エラーは無視（記録失敗はバックアップ失敗にしない）

        self.cleanup_old_backups()?;
        Ok(backup_path.to_string_lossy().to_string())
    }

    /// 手動バックアップを実行（常にフル）
    pub fn backup(&self, label: Option<&str>) -> Result<String> {
        if !self.enabled {
            return Err(KijukuError::Other("Backup is disabled".to_string()));
        }

        let timestamp = current_timestamp_str();
        let manual_dir = self.backup_dir.join("manual");
        fs::create_dir_all(&manual_dir)?;

        let filename = match label {
            Some(l) => {
                let sanitized: String = l
                    .chars()
                    .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
                    .collect();
                format!("{}.{}-{}.db", self.db_stem, timestamp, sanitized)
            }
            None => format!("{}.{}.db", self.db_stem, timestamp),
        };

        let backup_path = manual_dir.join(&filename);
        self.copy_db_to(&backup_path)?;

        *lock_mutex(&self.last_backup_time)? = Some(now_ms()?);
        self.cleanup_old_backups()?;

        Ok(backup_path.to_string_lossy().to_string())
    }

    /// バックアップを復元する
    ///
    /// 復元前に現在の状態を tmp/ へ自動退避する（TASK-149）
    pub fn restore(
        &self,
        conn: &mut rusqlite::Connection,
        selector: &BackupSelector,
    ) -> Result<PathBuf> {
        let backup_info = self
            .select_backup(selector)?
            .ok_or_else(|| KijukuError::Other("No backup found matching selector".to_string()))?;

        // 1. 現在のDBを tmp/ へ退避（enabled に関わらず常時退避・設計 §7.2）。
        //    Backup の conn 借用をブロックスコープに閉じ、後続の復元借用と衝突させない。
        {
            let timestamp = current_timestamp_str();
            let tmp_dir = self.backup_dir.join("tmp");
            fs::create_dir_all(&tmp_dir)?;
            let pre_restore_path =
                tmp_dir.join(format!("{}.{}-pre_restore.db", self.db_stem, timestamp));
            let mut dst = rusqlite::Connection::open(&pre_restore_path)?;
            let bk = rusqlite::backup::Backup::new(conn, &mut dst)?;
            bk.run_to_completion(1000, std::time::Duration::from_millis(50), None)?;
        }

        // 2. バックアップを復元
        match &backup_info.kind {
            BackupKind::Full => {
                let src =
                    rusqlite::Connection::open_with_flags(&backup_info.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
                let bk = rusqlite::backup::Backup::new(&src, conn)?;
                bk.run_to_completion(1000, std::time::Duration::from_millis(50), None)?;
            }
            BackupKind::Diff { base_id } => {
                let base = self
                    .find_backup_by_id_in_scope(base_id, BackupScope::Auto)?
                    .ok_or_else(|| KijukuError::Other(format!("Base backup {} not found", base_id)))?;

                // 差分を適用して一時ファイルに復元
                let temp_path = self.temp_full_path_for_diff(base_id);
                fs::create_dir_all(temp_path.parent().unwrap())?;
                apply_diff_to_file(&base.path, &backup_info.path, &temp_path)?;

                let src = rusqlite::Connection::open_with_flags(&temp_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
                let bk = rusqlite::backup::Backup::new(&src, conn)?;
                bk.run_to_completion(1000, std::time::Duration::from_millis(50), None)?;
                let _ = fs::remove_file(&temp_path);
                let _ = fs::remove_file(format!("{}-wal", temp_path.display()));
                let _ = fs::remove_file(format!("{}-shm", temp_path.display()));
            }
        }

        Ok(backup_info.path)
    }

    /// migrate 実行前に現在の DB を `backup/tmp/` へ退避する（設計 §7.3・TASK-42 P0）。
    ///
    /// `enabled=false` の場合は no-op（`Ok(None)`）。migrate でスキーマ破損が起きた場合の
    /// 即時巻き戻し（§8 即時復旧）の前提。stg/admin 経路の migrate でのみ意味を持ち、
    /// prod 読込経路では migrate 自体が呼ばれない（§5.1）ため snapshot も走らない。
    /// 保持期間等の詳細は P2（§15-14）。
    pub fn create_pre_migrate_snapshot(&self) -> Result<Option<String>> {
        // enabled に関わらず常時退避（設計 §7.3・migrate 前 snapshot 必須）
        let timestamp = current_timestamp_str();
        let tmp_dir = self.backup_dir.join("tmp");
        fs::create_dir_all(&tmp_dir)?;
        let pre_migrate_path = tmp_dir.join(format!(
            "{}.{}-pre_migrate.db",
            self.db_stem, timestamp
        ));
        self.copy_db_to(&pre_migrate_path)?;
        Ok(Some(pre_migrate_path.to_string_lossy().to_string()))
    }

    /// promote 実行前に prod を退避する（設計 §7.2/§4.5）。
    /// enabled に関わらず常時退避。リスト除外フィルタでユーザー向け一覧から非表示。
    pub fn create_pre_promote_snapshot(&self) -> Result<Option<String>> {
        let timestamp = current_timestamp_str();
        let tmp_dir = self.backup_dir.join("tmp");
        fs::create_dir_all(&tmp_dir)?;
        let pre_promote_path = tmp_dir.join(format!(
            "{}.{}-pre_promote.db",
            self.db_stem, timestamp
        ));
        self.copy_db_to(&pre_promote_path)?;
        Ok(Some(pre_promote_path.to_string_lossy().to_string()))
    }

    /// バックアップ一覧を取得（新しい順）
    ///
    /// auto / manual / tmp の全スコープを横断
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        self.list_backups_filtered(None)
    }

    /// バックアップにラベルを付与（事後）。None でクリア
    pub fn set_backup_label(&self, id: &str, label: Option<&str>) -> Result<()> {
        self.update_backup_meta(id, |e| {
            e.label = label.map(|s| s.to_string());
        })
    }

    /// バックアップにメモを付与。None でクリア（最大4096文字）
    pub fn set_backup_note(&self, id: &str, note: Option<&str>) -> Result<()> {
        let note = match note {
            Some(n) => {
                if n.chars().count() > 4096 {
                    return Err(KijukuError::Other(
                        "noteが長すぎます（最大4096文字）".to_string(),
                    ));
                }
                Some(n.to_string())
            }
            None => None,
        };
        self.update_backup_meta(id, |e| {
            e.note = note;
        })
    }

    /// バックアップの事後メタを取得
    pub fn get_backup_meta(&self, id: &str) -> Result<Option<BackupMetaEntry>> {
        Ok(self.read_backup_meta_store()?.entries.get(id).cloned())
    }

    /// スコープを指定してバックアップ一覧を取得（TASK-150）
    pub fn list_backups_in_scope(&self, scope: BackupScope) -> Result<Vec<BackupInfo>> {
        self.list_backups_filtered(Some(&scope))
    }

    fn list_backups_filtered(&self, scope_filter: Option<&BackupScope>) -> Result<Vec<BackupInfo>> {
        let meta = self.read_backup_meta_store().unwrap_or_default();
        let mut backups = Vec::new();

        let scopes: &[BackupScope] = match scope_filter {
            Some(s) => std::slice::from_ref(s),
            None => &[BackupScope::Auto, BackupScope::Manual, BackupScope::Tmp],
        };

        for scope in scopes {
            let subdir = match scope {
                BackupScope::Auto => self.backup_dir.join("auto"),
                BackupScope::Manual => self.backup_dir.join("manual"),
                BackupScope::Tmp => self.backup_dir.join("tmp"),
            };
            if !subdir.exists() {
                continue;
            }

            for entry in fs::read_dir(&subdir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().to_string();

                // pre-stash（pre_migrate/pre_restore/pre_promote）は運用用ロールバックファイルで
                // ユーザー向け backup 一覧から除外する（設計 §7.2/§7.3・§8）。
                // §8 復旧は list_pre_stashes() / byPath 経由で発見・戻しする。
                if is_pre_stash_name(&name) {
                    continue;
                }

                if let Some(parsed) = parse_backup_filename(&name, &self.db_stem) {
                    // tmp と manual は .db のみ
                    if *scope != BackupScope::Auto && parsed.extension != "db" {
                        continue;
                    }

                    let path = entry.path();
                    let metadata = fs::metadata(&path)?;
                    let kind = if parsed.extension == "diff" {
                        let base_id = read_diff_base_id(&path)
                            .unwrap_or_else(|_| "unknown".to_string());
                        BackupKind::Diff { base_id }
                    } else {
                        BackupKind::Full
                    };

                    let (label, label_source, note) = match meta.entries.get(&parsed.id) {
                        Some(e) if e.label.is_some() => {
                            (e.label.clone(), LabelSource::Sidecar, e.note.clone())
                        }
                        e => (
                            parsed.label.clone(),
                            LabelSource::Filename,
                            e.and_then(|x| x.note.clone()),
                        ),
                    };
                    backups.push(BackupInfo {
                        id: parsed.id,
                        name,
                        path,
                        created_at: metadata.modified()?,
                        scope: scope.clone(),
                        kind,
                        label,
                        label_source,
                        note,
                    });
                }
            }
        }

        // 作成順のソートは mtime ではなくファイル名タイムスタンプ(id)で行う。
        // mtime は粒度が粗く同ミリ秒作成でフル/差分が同値になり、latest 選択が
        // 非決定で古いフルを選ぶ不具合の原因となるため(TASK-17 と同一原因)。
        backups.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(backups)
    }

    /// 条件に一致するバックアップを選択
    pub fn select_backup(&self, selector: &BackupSelector) -> Result<Option<BackupInfo>> {
        // ByPath は list_backups が pre-stash を除外するため専用経路（§8 即時復旧・既知パス）。
        // scope_filter は無視（明示パス優先）。
        if let BackupSelectorKind::ByPath(ref p) = selector.kind {
            return self.select_backup_by_path(p);
        }
        let backups = match &selector.scope_filter {
            Some(scope) => self.list_backups_in_scope(scope.clone())?,
            None => self.list_backups()?,
        };

        if backups.is_empty() {
            return Ok(None);
        }

        Ok(match &selector.kind {
            BackupSelectorKind::Latest => backups.into_iter().next(),
            BackupSelectorKind::Nth(n) => backups.into_iter().nth(*n),
            BackupSelectorKind::Before(time) => {
                backups.into_iter().find(|b| b.created_at < *time)
            }
            BackupSelectorKind::After(time) => {
                backups.into_iter().filter(|b| b.created_at > *time).last()
            }
            BackupSelectorKind::ClosestTo(time) => {
                backups.into_iter().min_by_key(|b| {
                    let diff = if b.created_at > *time {
                        b.created_at.duration_since(*time).unwrap_or_default()
                    } else {
                        time.duration_since(b.created_at).unwrap_or_default()
                    };
                    diff.as_millis()
                })
            }
            // ID（タイムスタンプ文字列）で検索。.db/.diff 両方を対象とするため
            // list 結果から一致する id を探す（find_backup_by_id_in_scope は基底フル
            // .db のみを返すため、差分バックアップの直接選択には使えない）。
            BackupSelectorKind::ById(id) => backups.into_iter().find(|b| &b.id == id),
            // ByPath は select_backup 先頭で early-return するためここには来ない。
            BackupSelectorKind::ByPath(_) => unreachable!("ByPath handled by early-return"),
        })
    }

    /// 既知のバックアップファイルパスから BackupInfo を合成（設計 §8 即時復旧・既知パス経由）。
    ///
    /// `list_backups` が pre-stash（pre_promote/pre_restore/pre_migrate）を一覧から除外するため、
    /// ByPath selector と `list_pre_stashes` は list を経由せず指定パスから直接 BackupInfo を合成
    /// する。backup_dir 配下の `.db` ファイルのみ許可（path traversal・外部ファイル参照を拒否）。
    /// restore/diff_with_backup/with_backup_db が消費するのは `kind`（Full）と `path` のみで、
    /// 他フィールドは情報用。
    fn build_backup_info_from_path(&self, path: &Path) -> Result<BackupInfo> {
        // canonicalize で実在確認＋絶対パス化。不在は NotFound。traversal(`..`) も解決後に
        // backup_dir 配下チェックで弾かれる。
        let path_c = path
            .canonicalize()
            .map_err(|_| KijukuError::NotFound(format!("backup path not found: {}", path.display())))?;

        // 拡張子は .db のみ（pre-stash は常にフル .db）。
        if path_c.extension().and_then(|e| e.to_str()) != Some("db") {
            return Err(KijukuError::Validation(format!(
                "ByPath selector requires a .db file: {}",
                path.display()
            )));
        }

        // backup_dir 配下のみ許可（path traversal・外部ファイル参照の拒否）。
        let backup_dir_c = self.backup_dir.canonicalize().map_err(|_| {
            KijukuError::Validation(format!(
                "backup_dir not accessible: {}",
                self.backup_dir.display()
            ))
        })?;
        if !path_c.starts_with(&backup_dir_c) {
            return Err(KijukuError::Validation(format!(
                "ByPath selector path must be under backup_dir {}: {}",
                backup_dir_c.display(),
                path.display()
            )));
        }

        let name = path_c
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // id は parse_backup_filename から（pre-stash 命名規則に合致）。失敗時はファイル名をフォールバック。
        let id = parse_backup_filename(&name, &self.db_stem)
            .map(|p| p.id)
            .unwrap_or_else(|| name.clone());

        // scope は親ディレクトリ名から推導（pre-stash は tmp）。判定不能時は Tmp。
        let scope = path_c
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| match n.to_string_lossy().as_ref() {
                "auto" => Some(BackupScope::Auto),
                "manual" => Some(BackupScope::Manual),
                "tmp" => Some(BackupScope::Tmp),
                _ => None,
            })
            .unwrap_or(BackupScope::Tmp);

        let created_at = fs::metadata(&path_c)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        Ok(BackupInfo {
            id,
            name,
            path: path_c,
            created_at,
            scope,
            kind: BackupKind::Full,
            // pre-stash の "pre_promote" 等はユーザー向けラベルではないため None で上書き。
            label: None,
            label_source: LabelSource::Filename,
            note: None,
        })
    }

    /// ByPath selector のための BackupInfo 合成（設計 §8 即時復旧）。`build_backup_info_from_path`
    /// の thin wrapper（select_backup が Some/None を扱うため Option で包む）。
    fn select_backup_by_path(&self, path: &Path) -> Result<Option<BackupInfo>> {
        Ok(Some(self.build_backup_info_from_path(path)?))
    }

    /// pre-stash（即時復旧用ロールバックファイル）一覧を取得（設計 §8）。
    ///
    /// `tmp/` 配下の `*-pre_{migrate,restore,promote}.db` を返す。promote/(b)操作が返す
    /// `pre_stash_path` を呼出側が失った場合の発見経路。戻り値の `path` はそのまま
    /// `BackupSelector::by_path` で `restore` に渡せる（backup_dir 配下のため）。
    /// `list_backups` と同じく id（タイムスタンプ）降順（mtime の非決定回避・TASK-17 同原因）。
    /// 読めないファイル（走査中の削除等）は飛ばし、一覧全体は失敗させない（復旧用途のため）。
    pub fn list_pre_stashes(&self) -> Result<Vec<BackupInfo>> {
        let tmp_dir = self.backup_dir.join("tmp");
        if !tmp_dir.exists() {
            return Ok(Vec::new());
        }

        let mut stashes = Vec::new();
        for entry in fs::read_dir(&tmp_dir)? {
            let Ok(entry) = entry else { continue };
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_pre_stash_name(&name) {
                continue;
            }
            // 復旧用途: 1ファイルの読み込み失敗（走査中削除等）で一覧全体を落とさない。
            if let Ok(info) = self.build_backup_info_from_path(&entry.path()) {
                stashes.push(info);
            }
        }

        stashes.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(stashes)
    }

    /// 条件に一致するバックアップのパスを取得
    pub fn get_backup_path(&self, selector: &BackupSelector) -> Result<Option<PathBuf>> {
        Ok(self.select_backup(selector)?.map(|b| b.path))
    }

    /// 最後のバックアップ時刻を取得
    pub fn get_last_backup_time(&self) -> Option<SystemTime> {
        let last_backup = self.last_backup_time.lock().ok()?;
        last_backup.map(|ms| UNIX_EPOCH + std::time::Duration::from_millis(ms))
    }

    /// 最後の操作時刻を取得
    pub fn get_last_operation_time(&self) -> Option<SystemTime> {
        let last_op = self.last_operation_time.lock().ok()?;
        last_op.map(|ms| UNIX_EPOCH + std::time::Duration::from_millis(ms))
    }

    /// 次回バックアップまでの残り時間（ミリ秒）を取得
    pub fn get_time_until_next_backup(&self) -> Option<u64> {
        let last_backup = self.last_backup_time.lock().ok()?;
        match *last_backup {
            None => Some(0),
            Some(last) => {
                let now = now_ms().ok()?;
                let elapsed = now.saturating_sub(last);
                if elapsed < self.interval_ms {
                    Some(self.interval_ms - elapsed)
                } else {
                    Some(0)
                }
            }
        }
    }

    /// 古いバックアップファイルを削除（TASK-149 の tmp 自動削除を含む）
    pub fn cleanup_old_backups(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        self.cleanup_auto_backups()?;
        self.cleanup_tmp_backups()?;
        self.cleanup_backup_meta()?;

        Ok(())
    }

    // ---- private helpers ----

    fn copy_db_to(&self, dst_path: &Path) -> Result<()> {
        match self.try_copy_db_to(dst_path) {
            Ok(()) => return Ok(()),
            Err(e) if !is_busy_error(&e) => return Err(e),
            Err(e) => {
                let mut last_err = e;
                for &interval_ms in &self.retry_intervals_ms {
                    std::thread::sleep(std::time::Duration::from_millis(interval_ms));
                    match self.try_copy_db_to(dst_path) {
                        Ok(()) => return Ok(()),
                        Err(e) if is_busy_error(&e) => last_err = e,
                        Err(e) => return Err(e),
                    }
                }
                Err(last_err)
            }
        }
    }

    fn try_copy_db_to(&self, dst_path: &Path) -> Result<()> {
        let mut dst_conn = rusqlite::Connection::open(dst_path)?;
        let src_conn = rusqlite::Connection::open_with_flags(
            &self.db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        src_conn.busy_timeout(std::time::Duration::from_millis(self.busy_timeout_ms))?;
        let bk = rusqlite::backup::Backup::new(&src_conn, &mut dst_conn)?;
        bk.run_to_completion(1000, std::time::Duration::from_millis(100), None)?;
        Ok(())
    }

    /// 任意 src→dst の Online Backup コピー（設計 §4.5・sync/promote 基盤）。
    /// BackupManager インスタンスに依存しない関連関数。src は読み取り専用で開く。
    /// busy エラー時は固定間隔でリトライ。既存 `copy_db_to` と同等だが src をパラメータ化。
    pub fn copy_db_online(src: &Path, dst: &Path) -> Result<()> {
        const RETRY_INTERVALS_MS: [u64; 5] = [50, 100, 200, 500, 1000];
        match Self::try_copy_db_online(src, dst) {
            Ok(()) => Ok(()),
            Err(e) if !is_busy_error(&e) => Err(e),
            Err(e) => {
                let mut last_err = e;
                for &interval_ms in &RETRY_INTERVALS_MS {
                    std::thread::sleep(std::time::Duration::from_millis(interval_ms));
                    match Self::try_copy_db_online(src, dst) {
                        Ok(()) => return Ok(()),
                        Err(e) if is_busy_error(&e) => last_err = e,
                        Err(e) => return Err(e),
                    }
                }
                Err(last_err)
            }
        }
    }

    fn try_copy_db_online(src: &Path, dst: &Path) -> Result<()> {
        let mut dst_conn = rusqlite::Connection::open(dst)?;
        let src_conn = rusqlite::Connection::open_with_flags(
            src,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        src_conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        let bk = rusqlite::backup::Backup::new(&src_conn, &mut dst_conn)?;
        bk.run_to_completion(1000, std::time::Duration::from_millis(100), None)?;
        Ok(())
    }

    /// 今日のフルバックアップを探す（差分の基底候補）
    fn find_today_full_backup(&self) -> Result<Option<BackupInfo>> {
        let auto_dir = self.backup_dir.join("auto");
        if !auto_dir.exists() {
            return Ok(None);
        }

        let today = Utc::now().date_naive();
        let mut candidates = Vec::new();

        for entry in fs::read_dir(&auto_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            if let Some(parsed) = parse_backup_filename(&name, &self.db_stem) {
                if parsed.extension != "db" {
                    continue;
                }
                if let Some(date) = timestamp_to_date(&parsed.id) {
                    if date == today {
                        let path = entry.path();
                        let metadata = fs::metadata(&path)?;
                        candidates.push(BackupInfo {
                            id: parsed.id,
                            name,
                            path,
                            created_at: metadata.modified()?,
                            scope: BackupScope::Auto,
                            kind: BackupKind::Full,
                            label: None,
                            label_source: LabelSource::Filename,
                            note: None,
                        });
                    }
                }
            }
        }

        candidates.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(candidates.into_iter().next())
    }

    /// IDでバックアップを検索（指定スコープ内）
    pub(crate) fn find_backup_by_id_in_scope(&self, id: &str, scope: BackupScope) -> Result<Option<BackupInfo>> {
        let subdir = match &scope {
            BackupScope::Auto => self.backup_dir.join("auto"),
            BackupScope::Manual => self.backup_dir.join("manual"),
            BackupScope::Tmp => self.backup_dir.join("tmp"),
        };
        if !subdir.exists() {
            return Ok(None);
        }

        for entry in fs::read_dir(&subdir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(parsed) = parse_backup_filename(&name, &self.db_stem) {
                // フルバックアップ(.db)のみを基底候補とする。フル({stem}.{ts}.db)と
                // 差分({stem}.{ts}.diff)が同一タイムスタンプになる場合があり、拡張子で
                // 区別しないと差分ファイル(.diff = SQLite DB ではない)が誤って基底に選ばれ、
                // 復元時に SQLITE_NOTADB で失敗する（read_dir 順序依存で非決定に発生）。
                if parsed.id == id && parsed.extension == "db" {
                    let path = entry.path();
                    let metadata = fs::metadata(&path)?;
                    return Ok(Some(BackupInfo {
                        id: parsed.id,
                        name,
                        path,
                        created_at: metadata.modified()?,
                        scope,
                        kind: BackupKind::Full,
                        label: parsed.label,
                        label_source: LabelSource::Filename,
                        note: None,
                    }));
                }
            }
        }

        Ok(None)
    }

    /// 差分backup復元・参照用の一時フルDBパス（pid付きでプロセス間衝突回避）
    pub(crate) fn temp_full_path_for_diff(&self, base_id: &str) -> PathBuf {
        self.backup_dir
            .join("tmp")
            .join(format!("{}.restore_temp_{}_{}.db", self.db_stem, base_id, std::process::id()))
    }

    /// 差分バックアップファイルを作成（TASK-147）
    fn create_diff_backup_file(&self, timestamp: &str, base: &BackupInfo) -> Result<PathBuf> {
        let auto_dir = self.backup_dir.join("auto");

        // 現在のDBの一時フルコピーを作成
        let temp_path = auto_dir.join(format!("{}.{}.tmp_full.db", self.db_stem, timestamp));
        self.copy_db_to(&temp_path)?;

        // 差分ファイルを作成
        let diff_path = auto_dir.join(format!("{}.{}.diff", self.db_stem, timestamp));
        let result = create_diff_file(&base.id, &base.path, &temp_path, &diff_path);

        // 一時ファイルを削除
        let _ = fs::remove_file(&temp_path);

        result?;
        Ok(diff_path)
    }

    /// 自動バックアップのクリーンアップ（retention_policy による間引き）
    fn cleanup_auto_backups(&self) -> Result<()> {
        let now = SystemTime::now();
        let mut to_delete: Vec<PathBuf> = Vec::new();

        if let Some(ref policy) = self.retention_policy {
            // retention_policy による間引き（auto バックアップのみ対象）
            let auto_backups = self.list_backups_in_scope(BackupScope::Auto)?;
            let candidates = prune_auto_backups_by_policy(&auto_backups, policy, now);

            // 基底フル削除制約チェック（TASK-147）
            let records = self.read_auto_records().unwrap_or_default();

            for path in candidates {
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    to_delete.push(path);
                    continue;
                };
                let Some(parsed) = parse_backup_filename(name, &self.db_stem) else {
                    to_delete.push(path);
                    continue;
                };
                if parsed.extension != "db" {
                    // .diff ファイルは直接削除
                    to_delete.push(path);
                    continue;
                }

                // このフルを参照しているkept差分があるか確認
                let has_kept_diff = records.iter().any(|r| {
                    r.base_id == parsed.id && r.status == AutoRecordStatus::Kept
                });

                if !has_kept_diff {
                    to_delete.push(path);
                }
            }
        } else {
            // max_age_days / max_backups による削除（auto バックアップのみ対象・manual は対象外＝手動削除のみ §7.4）
            let all_backups = self.list_backups()?;
            let auto_only: Vec<&BackupInfo> = all_backups
                .iter()
                .filter(|b| b.scope == BackupScope::Auto)
                .collect();

            if let Some(max_age_days) = self.max_age_days {
                let max_age = std::time::Duration::from_secs(max_age_days * DAY_SECS);
                for b in &auto_only {
                    if let Ok(age) = now.duration_since(b.created_at) {
                        if age > max_age {
                            to_delete.push(b.path.clone());
                        }
                    }
                }
            }

            if let Some(max_backups) = self.max_backups {
                if auto_only.len() > max_backups {
                    for b in auto_only.iter().skip(max_backups) {
                        if !to_delete.contains(&b.path) {
                            to_delete.push(b.path.clone());
                        }
                    }
                }
            }
        }

        for path in &to_delete {
            let _ = fs::remove_file(path);
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(parsed) = parse_backup_filename(name, &self.db_stem) {
                    let _ = self.update_auto_record_pruned(&parsed.id);
                }
            }
        }

        Ok(())
    }

    /// tmp/ の期限切れファイルを削除（TASK-149）
    fn cleanup_tmp_backups(&self) -> Result<()> {
        let tmp_dir = self.backup_dir.join("tmp");
        if !tmp_dir.exists() {
            return Ok(());
        }

        let now = SystemTime::now();
        let retention = std::time::Duration::from_secs(self.tmp_retention_secs);

        for entry in fs::read_dir(&tmp_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Ok(metadata) = fs::metadata(&path) {
                if let Ok(created) = metadata.modified() {
                    if let Ok(age) = now.duration_since(created) {
                        if age > retention {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // ---- auto-records.csv (TASK-148) ----

    fn meta_path(&self) -> PathBuf {
        self.backup_dir.join("meta").join("backup-meta.json")
    }

    fn read_backup_meta_store(&self) -> Result<BackupMetaStore> {
        let path = self.meta_path();
        if !path.exists() {
            return Ok(BackupMetaStore::default());
        }
        let content = fs::read_to_string(&path)?;
        if content.trim().is_empty() {
            return Ok(BackupMetaStore::default());
        }
        let store: BackupMetaStore = serde_json::from_str(&content)
            .map_err(|e| KijukuError::Other(e.to_string()))?;
        Ok(store)
    }

    fn write_backup_meta_store(&self, store: &BackupMetaStore) -> Result<()> {
        let path = self.meta_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json =
            serde_json::to_string_pretty(store).map_err(|e| KijukuError::Other(e.to_string()))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn update_backup_meta<F: FnOnce(&mut BackupMetaEntry)>(&self, id: &str, f: F) -> Result<()> {
        let exists = self.list_backups()?.iter().any(|b| b.id == id);
        if !exists {
            return Err(KijukuError::Other(format!("Backup {} not found", id)));
        }
        let mut store = self.read_backup_meta_store()?;
        let entry = store.entries.entry(id.to_string()).or_insert(BackupMetaEntry {
            id: id.to_string(),
            label: None,
            note: None,
            updated_at: iso8601_now(),
        });
        f(entry);
        entry.updated_at = iso8601_now();
        self.write_backup_meta_store(&store)
    }

    /// 実在しないバックアップのメタエントリを掃除
    fn cleanup_backup_meta(&self) -> Result<()> {
        let store = self.read_backup_meta_store().unwrap_or_default();
        if store.entries.is_empty() {
            return Ok(());
        }
        let existing: std::collections::HashSet<String> =
            self.list_backups()?.into_iter().map(|b| b.id).collect();
        let original_len = store.entries.len();
        let surviving: std::collections::BTreeMap<_, _> = store
            .entries
            .into_iter()
            .filter(|(id, _)| existing.contains(id))
            .collect();
        if surviving.len() != original_len {
            self.write_backup_meta_store(&BackupMetaStore { entries: surviving })?;
        }
        Ok(())
    }

    fn records_path(&self) -> PathBuf {
        self.backup_dir.join("meta").join("auto-records.csv")
    }

    fn read_auto_records(&self) -> Result<Vec<AutoRecord>> {
        let path = self.records_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path)?;
        let mut records = Vec::new();
        for line in content.lines().skip(1) {
            // skip header
            if let Some(record) = parse_csv_record(line) {
                records.push(record);
            }
        }
        Ok(records)
    }

    fn append_auto_record(&self, record: &AutoRecord) -> Result<()> {
        let path = self.records_path();
        fs::create_dir_all(path.parent().unwrap())?;

        let needs_header = !path.exists()
            || fs::metadata(&path).map(|m| m.len() == 0).unwrap_or(true);

        let mut file = fs::OpenOptions::new().create(true).append(true).open(&path)?;

        if needs_header {
            writeln!(
                file,
                "id,created_at,tier,type,base_id,size_bytes,status,pruned_at"
            )?;
        }

        writeln!(
            file,
            "{},{},{},{},{},{},{},{}",
            record.id,
            record.created_at,
            record.tier,
            record.kind,
            record.base_id,
            record.size_bytes,
            if record.status == AutoRecordStatus::Kept { "kept" } else { "pruned" },
            record.pruned_at,
        )?;

        Ok(())
    }

    fn update_auto_record_pruned(&self, id: &str) -> Result<()> {
        let path = self.records_path();
        if !path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&path)?;
        let pruned_at = iso8601_now();
        let updated: String = content
            .lines()
            .map(|line| {
                if line.starts_with(&format!("{},", id)) {
                    // status と pruned_at を更新
                    let parts: Vec<&str> = line.splitn(8, ',').collect();
                    if parts.len() >= 8 {
                        format!(
                            "{},{},{},{},{},{},pruned,{}",
                            parts[0], parts[1], parts[2], parts[3],
                            parts[4], parts[5], pruned_at
                        )
                    } else {
                        line.to_string()
                    }
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        fs::write(&path, updated + "\n")?;
        Ok(())
    }
}

// ============================================================
// 差分バックアップ実装 (TASK-147)
// ============================================================

/// SQLite DB ファイルのページサイズを取得
fn get_sqlite_page_size(path: &Path) -> io::Result<u32> {
    let mut file = File::open(path)?;
    let mut buf = [0u8; 18];
    file.read_exact(&mut buf)?;
    // ページサイズはオフセット16の2バイト（ビッグエンディアン）
    let raw = u16::from_be_bytes([buf[16], buf[17]]) as u32;
    Ok(if raw == 1 { 65_536 } else { raw })
}

/// 差分ファイルを作成する
///
/// # 差分ファイル形式
/// ```text
/// Header (30 bytes):
///   base_id       18 bytes  基底フルのID
///   page_size      4 bytes  big-endian
///   total_pages    4 bytes  big-endian
///   changed_pages  4 bytes  big-endian
///
/// For each changed page:
///   page_number    4 bytes  big-endian (1-indexed)
///   page_data     (page_size bytes)
/// ```
fn create_diff_file(
    base_id: &str,
    base_path: &Path,
    current_path: &Path,
    diff_path: &Path,
) -> Result<()> {
    let page_size = get_sqlite_page_size(base_path)
        .map_err(|e| KijukuError::Other(format!("Failed to read page size: {}", e)))?
        as usize;

    let current_page_count = (fs::metadata(current_path)?.len() as usize / page_size) as u32;
    let base_page_count = (fs::metadata(base_path)?.len() as usize / page_size) as u32;

    let mut out = File::create(diff_path)?;
    // ヘッダのプレースホルダ（30バイト）を書き出し、後で上書き
    out.write_all(&[0u8; 30])?;

    let mut base_file = File::open(base_path)?;
    let mut current_file = File::open(current_path)?;
    let mut base_buf = vec![0u8; page_size];
    let mut current_buf = vec![0u8; page_size];
    let mut changed_count: u32 = 0;

    for page_num in 1..=current_page_count {
        current_file.read_exact(&mut current_buf)?;
        if page_num <= base_page_count {
            base_file.read_exact(&mut base_buf)?;
            if current_buf != base_buf {
                out.write_all(&page_num.to_be_bytes())?;
                out.write_all(&current_buf)?;
                changed_count += 1;
            }
        } else {
            // 基底に存在しないページは常に差分として記録
            out.write_all(&page_num.to_be_bytes())?;
            out.write_all(&current_buf)?;
            changed_count += 1;
        }
    }

    // 先頭に戻って実際のヘッダを書き込む
    out.seek(std::io::SeekFrom::Start(0))?;
    let mut base_id_bytes = [0u8; 18];
    let copy_len = base_id.len().min(18);
    base_id_bytes[..copy_len].copy_from_slice(&base_id.as_bytes()[..copy_len]);
    out.write_all(&base_id_bytes)?;
    out.write_all(&(page_size as u32).to_be_bytes())?;
    out.write_all(&current_page_count.to_be_bytes())?;
    out.write_all(&changed_count.to_be_bytes())?;

    Ok(())
}

/// 差分ファイルのヘッダから base_id を読む
fn read_diff_base_id(diff_path: &Path) -> Result<String> {
    let mut file = File::open(diff_path)?;
    let mut base_id_bytes = [0u8; 18];
    file.read_exact(&mut base_id_bytes)?;
    let s = std::str::from_utf8(&base_id_bytes)
        .unwrap_or("")
        .trim_end_matches('\0')
        .to_string();
    Ok(s)
}

/// 差分を基底フルに適用して output_path に書き出す
///
/// 基底DBと差分ファイルをページ単位でストリーミングしながらマージするため、
/// ファイルサイズに依存しない一定量のメモリのみを使用する。
pub(crate) fn apply_diff_to_file(base_path: &Path, diff_path: &Path, output_path: &Path) -> Result<()> {
    let mut diff_file = File::open(diff_path)?;
    let mut header = [0u8; 30]; // 18 + 4 + 4 + 4
    diff_file
        .read_exact(&mut header)
        .map_err(|e| KijukuError::Other(format!("Invalid diff file: {}", e)))?;

    let page_size = u32::from_be_bytes([header[18], header[19], header[20], header[21]]) as usize;
    let total_pages = u32::from_be_bytes([header[22], header[23], header[24], header[25]]) as usize;
    let changed_count =
        u32::from_be_bytes([header[26], header[27], header[28], header[29]]) as usize;

    let base_page_count = fs::metadata(base_path)?.len() as usize / page_size;

    // 一時ファイルに書き込み、完了後にアトミックにリネーム
    let temp_path = output_path.with_extension("db.tmp");

    let write_result = write_diff_to_temp(
        base_path,
        &mut diff_file,
        &temp_path,
        page_size,
        total_pages,
        changed_count,
        base_page_count,
    );

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
        return write_result;
    }

    fs::rename(&temp_path, output_path)?;
    Ok(())
}

fn write_diff_to_temp(
    base_path: &Path,
    diff_file: &mut File,
    temp_path: &Path,
    page_size: usize,
    total_pages: usize,
    changed_count: usize,
    base_page_count: usize,
) -> Result<()> {
    let mut base_file = File::open(base_path)?;
    let mut out = File::create(temp_path)?;
    let mut base_buf = vec![0u8; page_size];
    let mut patch_buf = vec![0u8; page_size];
    let zero_buf = vec![0u8; page_size];

    // 差分エントリはpage_num昇順で記録されているため、ストリーミングマージが可能
    let mut patches_remaining = changed_count;
    let mut next_patch_num: Option<usize> = None;
    if patches_remaining > 0 {
        let mut num_bytes = [0u8; 4];
        diff_file
            .read_exact(&mut num_bytes)
            .map_err(|e| KijukuError::Other(format!("Diff read error: {}", e)))?;
        diff_file
            .read_exact(&mut patch_buf)
            .map_err(|e| KijukuError::Other(format!("Diff page read error: {}", e)))?;
        patches_remaining -= 1;
        next_patch_num = Some(u32::from_be_bytes(num_bytes) as usize);
    }

    for page_num in 1..=total_pages {
        if next_patch_num == Some(page_num) {
            out.write_all(&patch_buf)?;
            if page_num <= base_page_count {
                base_file.read_exact(&mut base_buf)?; // 基底ページを読み捨て
            }
            next_patch_num = if patches_remaining > 0 {
                let mut num_bytes = [0u8; 4];
                diff_file
                    .read_exact(&mut num_bytes)
                    .map_err(|e| KijukuError::Other(format!("Diff read error: {}", e)))?;
                diff_file
                    .read_exact(&mut patch_buf)
                    .map_err(|e| KijukuError::Other(format!("Diff page read error: {}", e)))?;
                patches_remaining -= 1;
                Some(u32::from_be_bytes(num_bytes) as usize)
            } else {
                None
            };
        } else if page_num <= base_page_count {
            base_file.read_exact(&mut base_buf)?;
            out.write_all(&base_buf)?;
        } else {
            out.write_all(&zero_buf)?;
        }
    }

    out.sync_all()?;
    drop(out);
    Ok(())
}

// ============================================================
// 保持ポリシーによる間引き
// ============================================================

fn prune_auto_backups_by_policy(
    backups: &[BackupInfo],
    policy: &RetentionPolicy,
    now: SystemTime,
) -> Vec<PathBuf> {
    let mut to_delete = Vec::new();
    // 自動バックアップ（フル・差分両方）を対象
    let auto_backups: Vec<&BackupInfo> = backups.iter().filter(|b| b.scope == BackupScope::Auto).collect();

    let mut min_age_secs = 0u64;

    for tier in &policy.tiers {
        if tier.keep_interval_secs == 0 {
            min_age_secs = tier.max_age_secs;
            continue;
        }

        let tier_backups: Vec<&&BackupInfo> = auto_backups
            .iter()
            .filter(|b| {
                let age = now.duration_since(b.created_at).unwrap_or_default().as_secs();
                age >= min_age_secs && age < tier.max_age_secs
            })
            .collect();

        if tier_backups.is_empty() {
            min_age_secs = tier.max_age_secs;
            continue;
        }

        let mut buckets: HashMap<u64, Vec<&&BackupInfo>> = HashMap::new();
        for backup in &tier_backups {
            let age = now.duration_since(backup.created_at).unwrap_or_default().as_secs();
            let bucket = age / tier.keep_interval_secs;
            buckets.entry(bucket).or_default().push(backup);
        }

        for bucket_backups in buckets.values() {
            let mut sorted = bucket_backups.clone();
            sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            for backup in sorted.into_iter().skip(1) {
                to_delete.push(backup.path.clone());
            }
        }

        min_age_secs = tier.max_age_secs;
    }

    to_delete
}

// ============================================================
// ファイル名のパース
// ============================================================

struct ParsedFilename {
    id: String,
    extension: String,
    label: Option<String>,
}

/// pre-stash（pre_migrate/pre_restore/pre_promote）ロールバックファイルか（設計 §7.2/§7.3・§8）。
/// `list_backups` はこれをユーザー向け一覧から除外し、`list_pre_stashes` はこれを抽出する。
fn is_pre_stash_name(name: &str) -> bool {
    name.ends_with("-pre_migrate.db")
        || name.ends_with("-pre_restore.db")
        || name.ends_with("-pre_promote.db")
}

/// ファイル名から ID・拡張子・ラベルを解析
///
/// 対応形式:
/// - `{stem}.{timestamp}.db`            (フル)
/// - `{stem}.{timestamp}.diff`          (差分)
/// - `{stem}.{timestamp}-{label}.db`    (ラベル付き)
///
/// timestamp は `YYYYMMDDHHMMSS-mmm` の 18 文字固定。
fn parse_backup_filename(name: &str, stem: &str) -> Option<ParsedFilename> {
    let prefix = format!("{}.", stem);
    if !name.starts_with(&prefix) {
        return None;
    }

    let rest = &name[prefix.len()..]; // "20260323131045-789-label.db" etc.
    if rest.len() < 19 {
        // 18 chars timestamp + at least ".db" (3 chars) = 21 min, but ".diff" is longer
        return None;
    }

    // タイムスタンプ部分の検証 (18文字: 14桁 + '-' + 3桁)
    let ts = &rest[..18];
    if !ts[..14].chars().all(|c| c.is_ascii_digit())
        || ts.chars().nth(14) != Some('-')
        || !ts[15..].chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }

    let after_ts = &rest[18..]; // ".db", ".diff", "-label.db"

    if after_ts == ".db" {
        return Some(ParsedFilename {
            id: ts.to_string(),
            extension: "db".to_string(),
            label: None,
        });
    }

    if after_ts == ".diff" {
        return Some(ParsedFilename {
            id: ts.to_string(),
            extension: "diff".to_string(),
            label: None,
        });
    }

    // ラベルなし .tmp_full.db (一時ファイル) はスキップ
    if after_ts.contains(".tmp_full.") {
        return None;
    }

    if let Some(label_and_ext) = after_ts.strip_prefix('-') {
        if let Some(label) = label_and_ext.strip_suffix(".db") {
            return Some(ParsedFilename {
                id: ts.to_string(),
                extension: "db".to_string(),
                label: Some(label.to_string()),
            });
        }
    }

    None
}

// ============================================================
// auto-records.csv のパース
// ============================================================

fn parse_csv_record(line: &str) -> Option<AutoRecord> {
    let parts: Vec<&str> = line.splitn(8, ',').collect();
    if parts.len() < 8 {
        return None;
    }
    Some(AutoRecord {
        id: parts[0].to_string(),
        created_at: parts[1].to_string(),
        tier: parts[2].to_string(),
        kind: parts[3].to_string(),
        base_id: parts[4].to_string(),
        size_bytes: parts[5].parse().unwrap_or(0),
        status: if parts[6] == "kept" {
            AutoRecordStatus::Kept
        } else {
            AutoRecordStatus::Pruned
        },
        pruned_at: parts[7].to_string(),
    })
}

// ============================================================
// ユーティリティ
// ============================================================

fn is_busy_error(err: &KijukuError) -> bool {
    match err {
        KijukuError::Database(rusqlite::Error::SqliteFailure(sqlite_err, _)) => {
            sqlite_err.code == rusqlite::ffi::ErrorCode::DatabaseBusy
                || sqlite_err.code == rusqlite::ffi::ErrorCode::DatabaseLocked
        }
        _ => false,
    }
}

fn now_ms() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .map_err(|e| KijukuError::Other(e.to_string()))
}

fn lock_mutex<T>(mutex: &Arc<Mutex<T>>) -> Result<std::sync::MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|e| KijukuError::Other(format!("Mutex lock failed: {}", e)))
}

fn current_timestamp_str() -> String {
    let now = Utc::now();
    format!(
        "{}{:02}{:02}{:02}{:02}{:02}-{:03}",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.timestamp_subsec_millis(),
    )
}

fn iso8601_now() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn timestamp_to_date(ts: &str) -> Option<NaiveDate> {
    if ts.len() < 8 {
        return None;
    }
    let year: i32 = ts[0..4].parse().ok()?;
    let month: u32 = ts[4..6].parse().ok()?;
    let day: u32 = ts[6..8].parse().ok()?;
    NaiveDate::from_ymd_opt(year, month, day)
}

// chrono::Timelike trait を使うために
use chrono::Timelike;

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    fn create_test_db(dir: &std::path::Path) -> PathBuf {
        let db_path = dir.join("test.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT)", [])
            .unwrap();
        conn.execute("INSERT INTO test (data) VALUES (?1)", ["test data"])
            .unwrap();
        db_path
    }

    #[test]
    fn test_backup_options_default() {
        let options = BackupOptions::default();
        assert_eq!(options.interval_ms, Some(3_600_000));
        assert_eq!(options.enabled, Some(true));
        assert_eq!(options.auto_enabled, Some(true));
        assert!(options.retention_policy.is_none());
        assert_eq!(options.busy_timeout_ms, Some(5_000));
        assert_eq!(
            options.retry_intervals_ms,
            Some(vec![5_000, 10_000, 30_000, 60_000])
        );
    }

    #[test]
    fn test_backup_retry_options_custom() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            busy_timeout_ms: Some(1_000),
            retry_intervals_ms: Some(vec![500, 1_000]),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();
        assert_eq!(manager.busy_timeout_ms, 1_000);
        assert_eq!(manager.retry_intervals_ms, vec![500, 1_000]);
    }

    #[test]
    fn test_backup_retry_options_default_fallback() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            busy_timeout_ms: None,
            retry_intervals_ms: None,
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();
        assert_eq!(manager.busy_timeout_ms, 5_000);
        assert_eq!(manager.retry_intervals_ms, vec![5_000, 10_000, 30_000, 60_000]);
    }

    #[test]
    fn test_retention_policy_default() {
        let policy = RetentionPolicy::default();
        assert_eq!(policy.tiers.len(), 7);
        assert_eq!(policy.tiers[0].max_age_secs, HOUR_SECS);
        assert_eq!(policy.tiers[0].keep_interval_secs, 0);
        assert_eq!(policy.tiers[6].keep_interval_secs, YEAR_SECS);
    }

    #[test]
    fn test_backup_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            interval_ms: Some(1000),
            enabled: Some(true),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options);
        assert!(manager.is_ok());
    }

    #[test]
    fn test_manual_backup_no_label() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();
        let backup_path = manager.backup(None).unwrap();

        let path = PathBuf::from(&backup_path);
        assert!(path.exists());
        // manual/ ディレクトリ下にある
        assert!(backup_path.contains("/manual/"));
        assert!(backup_path.ends_with(".db"));
    }

    #[test]
    fn test_manual_backup_with_label() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();
        let backup_path = manager.backup(Some("before_import")).unwrap();

        assert!(PathBuf::from(&backup_path).exists());
        assert!(backup_path.contains("/manual/"));
        assert!(backup_path.contains("-before_import.db"));
    }

    #[test]
    fn test_list_backups_scope() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            interval_ms: Some(1),
            enabled: Some(true),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();

        manager.backup_auto().unwrap();
        thread::sleep(Duration::from_millis(5));
        manager.backup(None).unwrap();
        thread::sleep(Duration::from_millis(5));
        manager.backup(Some("my_label")).unwrap();

        let backups = manager.list_backups().unwrap();
        assert_eq!(backups.len(), 3); // 1 auto + 2 manual

        // scope確認
        let auto_count = backups.iter().filter(|b| b.scope == BackupScope::Auto).count();
        let manual_count = backups.iter().filter(|b| b.scope == BackupScope::Manual).count();
        assert_eq!(auto_count, 1);
        assert_eq!(manual_count, 2);

        // ラベル確認
        let labeled = backups.iter().find(|b| b.label.is_some()).unwrap();
        assert_eq!(labeled.label, Some("my_label".to_string()));
    }

    #[test]
    fn test_list_backups_sorted_newest_first() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            interval_ms: Some(1),
            enabled: Some(true),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();

        manager.backup(None).unwrap();
        thread::sleep(Duration::from_millis(10));
        manager.backup(None).unwrap();

        let backups = manager.list_backups().unwrap();
        assert_eq!(backups.len(), 2);
        assert!(backups[0].created_at >= backups[1].created_at);
    }

    #[test]
    fn test_restore() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();
        manager.backup(Some("before_delete")).unwrap();

        let mut conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("DELETE FROM test", []).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);

        manager.restore(&mut conn, &BackupSelector::latest()).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        // tmp/ に pre_restore ファイルが作成されていることを確認
        let tmp_dir = backup_dir.join("tmp");
        assert!(tmp_dir.exists());
        let tmp_files: Vec<_> = fs::read_dir(&tmp_dir).unwrap().collect();
        assert!(!tmp_files.is_empty());
    }

    #[test]
    fn test_get_last_backup_time() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            interval_ms: Some(1000),
            enabled: Some(true),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();
        assert!(manager.get_last_backup_time().is_none());
        manager.backup_auto().unwrap();
        assert!(manager.get_last_backup_time().is_some());
    }

    #[test]
    fn test_parse_backup_filename_full() {
        let parsed = parse_backup_filename("test.20260323120530-456.db", "test").unwrap();
        assert_eq!(parsed.id, "20260323120530-456");
        assert_eq!(parsed.extension, "db");
        assert_eq!(parsed.label, None);
    }

    #[test]
    fn test_parse_backup_filename_diff() {
        let parsed = parse_backup_filename("test.20260323120530-456.diff", "test").unwrap();
        assert_eq!(parsed.id, "20260323120530-456");
        assert_eq!(parsed.extension, "diff");
        assert_eq!(parsed.label, None);
    }

    #[test]
    fn test_parse_backup_filename_with_label() {
        let parsed =
            parse_backup_filename("test.20260323120530-456-before_import.db", "test").unwrap();
        assert_eq!(parsed.id, "20260323120530-456");
        assert_eq!(parsed.extension, "db");
        assert_eq!(parsed.label, Some("before_import".to_string()));
    }

    #[test]
    fn test_parse_backup_filename_invalid() {
        assert!(parse_backup_filename("test.invalid.db", "test").is_none());
        assert!(parse_backup_filename("other.20260323120530-456.db", "test").is_none());
    }

    #[test]
    fn test_retention_policy_prune() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            interval_ms: Some(1),
            enabled: Some(true),
            retention_policy: Some(RetentionPolicy::default()),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();

        for _ in 0..3 {
            manager.backup_auto().unwrap();
            thread::sleep(Duration::from_millis(5));
        }
        manager.backup(Some("keep_me")).unwrap();

        let backups = manager.list_backups().unwrap();
        // recent tier は全保持なので削除なし
        assert_eq!(backups.len(), 4);
        assert!(backups.iter().any(|b| b.scope == BackupScope::Manual));
    }

    #[test]
    fn test_cleanup_max_backups_fallback() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            interval_ms: Some(1),
            enabled: Some(true),
            max_backups: Some(2),
            retention_policy: None,
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();

        for _ in 0..4 {
            manager.backup(None).unwrap();
            thread::sleep(Duration::from_millis(5));
        }

        let backups = manager.list_backups().unwrap();
        // フォールバックモード（retention_policy=None + max_backups）の削除対象は auto のみ。
        // manual は削除されず全件残る（設計 §7.4・manual は対象外＝手動削除のみ）。
        assert_eq!(backups.len(), 4, "manual は max_backups の削除対象外（§7.4）");
    }

    #[test]
    fn test_auto_records_csv() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            interval_ms: Some(1),
            enabled: Some(true),
            auto_enabled: Some(true),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();
        manager.backup_auto().unwrap();

        let records = manager.read_auto_records().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kind, "full");
        assert_eq!(records[0].status, AutoRecordStatus::Kept);
    }

    #[test]
    fn test_diff_create_and_apply() {
        let temp_dir = TempDir::new().unwrap();

        // ベースDBを作成
        let base_db = temp_dir.path().join("base.db");
        let conn = rusqlite::Connection::open(&base_db).unwrap();
        conn.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)", []).unwrap();
        conn.execute("INSERT INTO t VALUES (1, 'hello')", []).unwrap();
        drop(conn);

        // 変更後DBを作成
        let current_db = temp_dir.path().join("current.db");
        fs::copy(&base_db, &current_db).unwrap();
        let conn = rusqlite::Connection::open(&current_db).unwrap();
        conn.execute("INSERT INTO t VALUES (2, 'world')", []).unwrap();
        drop(conn);

        // 差分ファイルを作成
        let diff_path = temp_dir.path().join("test.diff");
        create_diff_file("base_id_test", &base_db, &current_db, &diff_path).unwrap();

        assert!(diff_path.exists());
        assert!(diff_path.metadata().unwrap().len() > 0);

        // base_id を読む
        let read_id = read_diff_base_id(&diff_path).unwrap();
        assert_eq!(read_id, "base_id_test");

        // 差分を適用して復元
        let restored_db = temp_dir.path().join("restored.db");
        apply_diff_to_file(&base_db, &diff_path, &restored_db).unwrap();

        let conn = rusqlite::Connection::open(&restored_db).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_restore_from_diff_backup() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();

        // 1. フルバックアップを作成（1行のDB）
        manager.backup_auto().unwrap();

        // 2. DBにデータを追加（2行に）
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute("INSERT INTO test (data) VALUES (?1)", ["added_row"])
                .unwrap();
        }

        // 3. 差分バックアップを作成（同日なのでdiff）
        manager.backup_auto().unwrap();

        // 差分バックアップが存在することを確認
        let backups = manager.list_backups().unwrap();
        let has_diff = backups
            .iter()
            .any(|b| matches!(&b.kind, BackupKind::Diff { .. }));
        assert!(has_diff, "差分バックアップが作成されているべき");

        // 4. DBを削除して空にする
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("DELETE FROM test", []).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);

        // 5. 差分バックアップ（最新のauto）から復元
        let selector = BackupSelector::latest().scope(BackupScope::Auto);
        manager.restore(&mut conn, &selector).unwrap();

        // 6. 差分適用後の状態（2行）に復元されていることを確認
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2, "フル+差分適用後の2レコードが復元されているべき");
    }

    #[test]
    fn test_find_backup_by_id_prefers_full_over_diff() {
        // 同一タイムスタンプのフル(.db)と差分(.diff)が共存する場合、
        // find_backup_by_id_in_scope は基底としてフル(.db)を返すべき。
        // 旧実装は拡張子を区別せず、read_dir 順序で非決定に差分(.diff = SQLite DB ではない)
        // を返し、復元時に SQLITE_NOTADB で失敗していた。
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");
        let auto_dir = backup_dir.join("auto");
        fs::create_dir_all(&auto_dir).unwrap();

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };
        let manager = BackupManager::new(&db_path, options).unwrap();

        // 同一タイムスタンプのフル(.db)と差分(.diff)を手動で配置
        let ts = "20260622120000-123";
        fs::write(auto_dir.join(format!("test.{}.db", ts)), b"dummy full").unwrap();
        fs::write(auto_dir.join(format!("test.{}.diff", ts)), b"dummy diff").unwrap();

        let found = manager
            .find_backup_by_id_in_scope(ts, BackupScope::Auto)
            .unwrap();
        assert!(found.is_some(), "基底フルが見つかるべき");
        let found = found.unwrap();
        assert!(
            found.path.to_string_lossy().ends_with(".db"),
            "フル(.db)が基底に選ばれるべき: {:?}",
            found.path
        );
        assert!(matches!(found.kind, BackupKind::Full), "kind は Full のべき");
    }

    /// ByPath selector で pre-stash（list 除外ファイル）を直接選択できる（設計 §8 即時復旧）。
    #[test]
    fn test_select_backup_by_path_pre_stash() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");
        let tmp_dir = backup_dir.join("tmp");
        fs::create_dir_all(&tmp_dir).unwrap();

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };
        let manager = BackupManager::new(&db_path, options).unwrap();

        // tmp/ に pre_promote を配置（list_backups は除外するが ByPath は直接合成）
        let pre = tmp_dir.join("test.20260724000000-000-pre_promote.db");
        fs::write(&pre, b"dummy pre-stash").unwrap();

        let info = manager
            .select_backup(&BackupSelector::by_path(&pre))
            .expect("by_path ok")
            .expect("Some BackupInfo");
        assert!(matches!(info.kind, BackupKind::Full), "pre-stash は Full");
        assert_eq!(info.scope, BackupScope::Tmp);
        assert_eq!(info.label, None, "pre_promote ラベルは None で上書き");
        assert!(info.path.ends_with("test.20260724000000-000-pre_promote.db"));
    }

    /// list_pre_stashes は pre-stash のみを返し list_backups は除外したまま（設計 §8）。
    /// id（タイムスタンプ）降順。戻り値の path は tmp/ 配下で byPath restore に直接渡せる。
    #[test]
    fn test_list_pre_stashes() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");
        let tmp_dir = backup_dir.join("tmp");
        fs::create_dir_all(&tmp_dir).unwrap();

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };
        let manager = BackupManager::new(&db_path, options).unwrap();

        // 3種の pre-stash（タイムスタンプ違い）と通常 backup を tmp/ に配置。
        fs::write(tmp_dir.join("test.20260724000000-000-pre_promote.db"), b"a").unwrap();
        fs::write(tmp_dir.join("test.20260724000005-000-pre_restore.db"), b"b").unwrap();
        fs::write(tmp_dir.join("test.20260724000010-000-pre_migrate.db"), b"c").unwrap();
        // 通常 backup（list_backups 対象・list_pre_stashes には含まれない）。
        fs::write(tmp_dir.join("test.20260724000020-000.db"), b"d").unwrap();

        let stashes = manager.list_pre_stashes().expect("list_pre_stashes ok");
        // id 降順（最新の pre_migrate が先）。
        let names: Vec<String> = stashes.iter().map(|s| s.name.clone()).collect();
        assert_eq!(
            names,
            vec![
                "test.20260724000010-000-pre_migrate.db".to_string(),
                "test.20260724000005-000-pre_restore.db".to_string(),
                "test.20260724000000-000-pre_promote.db".to_string(),
            ],
            "id 降順・pre-stash 3件のみ"
        );
        // path は tmp/ 配下（canonicalize 済み・byPath restore に直接渡せる）。
        let tmp_c = tmp_dir.canonicalize().unwrap();
        for s in &stashes {
            assert!(s.path.starts_with(&tmp_c), "tmp 配下: {}", s.path.display());
            assert!(matches!(s.kind, BackupKind::Full));
            assert_eq!(s.label, None, "pre-stash ラベルは None");
        }

        // list_backups は pre-stash を除外し通常 backup のみ。
        let backups = manager.list_backups().expect("list_backups ok");
        assert_eq!(backups.len(), 1, "通常 backup のみ: {backups:?}");
        assert!(backups[0].name.ends_with("-000.db"));
    }

    /// tmp/ が無い、または pre-stash が無い場合は空 Vec。
    #[test]
    fn test_list_pre_stashes_empty() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        // tmp/ を作らない。
        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };
        let manager = BackupManager::new(&db_path, options).unwrap();
        assert!(manager.list_pre_stashes().unwrap().is_empty(), "tmp/ なしは空");

        // tmp/ はあるが pre-stash なし（通常 backup のみ）。
        let tmp_dir = backup_dir.join("tmp");
        fs::create_dir_all(&tmp_dir).unwrap();
        fs::write(tmp_dir.join("test.20260724000000-000.db"), b"x").unwrap();
        assert!(
            manager.list_pre_stashes().unwrap().is_empty(),
            "pre-stash なしは空"
        );
    }

    #[test]
    fn test_select_backup_by_path_rejects_outside_backup_dir() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };
        let manager = BackupManager::new(&db_path, options).unwrap();

        let outside = temp_dir.path().join("outside.db");
        fs::write(&outside, b"dummy").unwrap();

        let err = manager
            .select_backup(&BackupSelector::by_path(&outside))
            .unwrap_err();
        assert!(matches!(err, KijukuError::Validation(_)), "got: {err:?}");
    }

    #[test]
    fn test_select_backup_by_path_rejects_non_db_extension() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");
        let tmp_dir = backup_dir.join("tmp");
        fs::create_dir_all(&tmp_dir).unwrap();

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };
        let manager = BackupManager::new(&db_path, options).unwrap();

        let txt = tmp_dir.join("test.20260724000000-000.txt");
        fs::write(&txt, b"dummy").unwrap();

        let err = manager
            .select_backup(&BackupSelector::by_path(&txt))
            .unwrap_err();
        assert!(matches!(err, KijukuError::Validation(_)), "got: {err:?}");
    }

    #[test]
    fn test_select_backup_by_path_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };
        let manager = BackupManager::new(&db_path, options).unwrap();

        let missing = backup_dir.join("tmp").join("nonexistent.db");
        let err = manager
            .select_backup(&BackupSelector::by_path(&missing))
            .unwrap_err();
        assert!(matches!(err, KijukuError::NotFound(_)), "got: {err:?}");
    }

    #[test]
    fn test_parse_backup_filename_pre_promote_label() {
        let parsed = parse_backup_filename("test.20260724000000-000-pre_promote.db", "test");
        let parsed = parsed.expect("pre_promote parses as labeled .db");
        assert_eq!(parsed.id, "20260724000000-000");
        assert_eq!(parsed.extension, "db");
        assert_eq!(parsed.label.as_deref(), Some("pre_promote"));
    }

    #[test]
    fn test_diff_with_pages_beyond_base() {
        let temp_dir = TempDir::new().unwrap();

        // ベースDB（最小サイズ）
        let base_db = temp_dir.path().join("base_new_pages.db");
        {
            let conn = rusqlite::Connection::open(&base_db).unwrap();
            conn.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)", [])
                .unwrap();
        }

        // 現在DB（大量データ挿入でページ数がベースを超える）
        let current_db = temp_dir.path().join("current_new_pages.db");
        fs::copy(&base_db, &current_db).unwrap();
        {
            let conn = rusqlite::Connection::open(&current_db).unwrap();
            for i in 0..200i32 {
                conn.execute(
                    "INSERT INTO t VALUES (?1, ?2)",
                    rusqlite::params![i, "x".repeat(100)],
                )
                .unwrap();
            }
        }

        // current がベースより大きいことを確認
        assert!(
            fs::metadata(&current_db).unwrap().len() > fs::metadata(&base_db).unwrap().len(),
            "currentはbaseより大きいべき"
        );

        // 差分ファイルを作成・適用
        let diff_path = temp_dir.path().join("new_pages.diff");
        create_diff_file("new_pages_test", &base_db, &current_db, &diff_path).unwrap();

        let restored_db = temp_dir.path().join("restored_new_pages.db");
        apply_diff_to_file(&base_db, &diff_path, &restored_db).unwrap();

        let conn = rusqlite::Connection::open(&restored_db).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 200);
    }

    #[test]
    fn test_backup_selector_scope() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = create_test_db(temp_dir.path());
        let backup_dir = temp_dir.path().join("backups");

        let options = BackupOptions {
            backup_dir: Some(backup_dir.to_string_lossy().to_string()),
            enabled: Some(true),
            ..Default::default()
        };

        let manager = BackupManager::new(&db_path, options).unwrap();
        manager.backup_auto().unwrap();
        manager.backup(None).unwrap();

        // auto のみ
        let selector = BackupSelector::latest().scope(BackupScope::Auto);
        let result = manager.select_backup(&selector).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().scope, BackupScope::Auto);

        // manual のみ
        let selector = BackupSelector::latest().scope(BackupScope::Manual);
        let result = manager.select_backup(&selector).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().scope, BackupScope::Manual);
    }

    #[test]
    fn test_backup_selector_to_wire_value_latest() {
        let v = BackupSelector::latest().to_wire_value().unwrap();
        assert_eq!(v, serde_json::json!({ "type": "latest" }));
    }

    #[test]
    fn test_backup_selector_to_wire_value_nth() {
        let v = BackupSelector::nth(3).to_wire_value().unwrap();
        assert_eq!(v, serde_json::json!({ "type": "nth", "n": 3 }));
    }

    #[test]
    fn test_backup_selector_to_wire_value_by_id() {
        let v = BackupSelector::by_id("20260724160000-000")
            .to_wire_value()
            .unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "type": "byId", "id": "20260724160000-000" })
        );
    }

    #[test]
    fn test_backup_selector_to_wire_value_by_path() {
        let v = BackupSelector::by_path("/tmp/x.db").to_wire_value().unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "type": "byPath", "path": "/tmp/x.db" })
        );
    }

    #[test]
    fn test_backup_selector_to_wire_value_time_based_unsupported() {
        // Before/After/ClosestTo は stdin プロトコル非対応（サーバ側 BackupSelectorJson にバリアントがない）。
        let now = std::time::SystemTime::now();
        assert!(BackupSelector::before(now).to_wire_value().is_err());
        assert!(BackupSelector::after(now).to_wire_value().is_err());
        assert!(BackupSelector::closest_to(now).to_wire_value().is_err());
    }

    #[test]
    fn test_backup_selector_to_wire_value_scope_not_serialized() {
        // scope_filter を付けても wire には載らない（サーバ側プロトコルが scope 非対応）。
        let v = BackupSelector::latest()
            .scope(BackupScope::Auto)
            .to_wire_value()
            .unwrap();
        assert_eq!(v, serde_json::json!({ "type": "latest" }));
    }
}
