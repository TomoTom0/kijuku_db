/// kijuku-db 設定ファイル（config.toml）の読み込み・マージ機構
///
/// 設定ファイルの優先度（高いほど低いを上書き）:
/// 1. デフォルト値（コード組み込み）
/// 2. `~/.local/config/kijuku-db/config.toml`（グローバル）
/// 3. `{cwd}/kijuku-db-config.toml`（実行場所）
/// 4. `{db_dir}/kijuku-db-config.toml`（DB階層）
/// 5. `--config {path}` 引数（絶対最優先）
use serde::Deserialize;
use std::path::{Path, PathBuf};

// ---- TOML デシリアライズ用構造体 ----

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    pub backup: BackupSectionFile,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct BackupSectionFile {
    pub enabled: Option<bool>,
    pub backup_dir: Option<String>,
    pub auto: BackupAutoSectionFile,
    pub tmp: BackupTmpSectionFile,
    pub retention: BackupRetentionSectionFile,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct BackupAutoSectionFile {
    pub enabled: Option<bool>,
    pub interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct BackupTmpSectionFile {
    pub retention_secs: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct BackupRetentionSectionFile {
    pub tiers: Option<Vec<RetentionTierFile>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetentionTierFile {
    pub max_age_secs: u64,
    pub keep_interval_secs: u64,
}

// ---- 解決済み設定構造体 ----

/// マージ済みのバックアップ設定
#[derive(Debug, Clone)]
pub struct BackupConfig {
    /// バックアップ機能全体の有効/無効
    pub enabled: bool,
    /// バックアップ保存先ディレクトリ（None = {db_dir}/backup/）
    pub backup_dir: Option<String>,
    /// 自動バックアップの有効/無効
    pub auto_enabled: bool,
    /// 自動バックアップのトリガー間隔（ミリ秒）
    pub interval_ms: u64,
    /// tmp/ の保持期間（秒）
    pub tmp_retention_secs: u64,
    /// 保持ポリシーのtiers（None = デフォルト）
    pub retention_tiers: Option<Vec<RetentionTierConfig>>,
}

#[derive(Debug, Clone)]
pub struct RetentionTierConfig {
    pub max_age_secs: u64,
    pub keep_interval_secs: u64,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backup_dir: None,
            auto_enabled: true,
            interval_ms: 3_600_000,
            tmp_retention_secs: 604_800, // 7日
            retention_tiers: None,
        }
    }
}

/// マージ済みの設定全体
#[derive(Debug, Clone, Default)]
pub struct KijukuConfig {
    pub backup: BackupConfig,
}

// ---- マージロジック ----

fn merge_config(base: &mut KijukuConfig, overlay: ConfigFile) {
    let b = &mut base.backup;
    let o = overlay.backup;
    if let Some(v) = o.enabled {
        b.enabled = v;
    }
    if let Some(v) = o.backup_dir {
        b.backup_dir = Some(v);
    }
    if let Some(v) = o.auto.enabled {
        b.auto_enabled = v;
    }
    if let Some(v) = o.auto.interval_ms {
        b.interval_ms = v;
    }
    if let Some(v) = o.tmp.retention_secs {
        b.tmp_retention_secs = v;
    }
    if let Some(tiers) = o.retention.tiers {
        b.retention_tiers = Some(
            tiers
                .into_iter()
                .map(|t| RetentionTierConfig {
                    max_age_secs: t.max_age_secs,
                    keep_interval_secs: t.keep_interval_secs,
                })
                .collect(),
        );
    }
}

fn read_config_file(path: &Path) -> Option<ConfigFile> {
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

/// グローバル設定ファイルのパスを返す（~/.local/config/kijuku-db/config.toml）
pub fn global_config_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".local/config/kijuku-db/config.toml"))
}

/// 複数の設定ファイルを優先度順にマージして返す
///
/// # 引数
/// - `db_path`: DBファイルのパス（DB階層の設定を探すために使用）
/// - `cwd`: 実行場所のパス（None の場合は "." を使用）
/// - `extra_config_path`: 明示指定の設定ファイルパス（最優先）
pub fn load_config(
    db_path: &Path,
    cwd: Option<&Path>,
    extra_config_path: Option<&Path>,
) -> KijukuConfig {
    let mut config = KijukuConfig::default();
    let mut loaded: Vec<PathBuf> = Vec::new();

    macro_rules! try_load {
        ($path:expr) => {
            let p: PathBuf = $path;
            if !loaded.contains(&p) {
                if let Some(file_cfg) = read_config_file(&p) {
                    loaded.push(p.clone());
                    merge_config(&mut config, file_cfg);
                } else {
                    // ファイルが存在しない or パースエラーはスキップ
                    let _ = p;
                }
            }
        };
    }

    // 優先度 2: グローバル
    if let Some(global) = global_config_path() {
        try_load!(global);
    }

    // 優先度 3: cwd
    let cwd_path = cwd.unwrap_or(Path::new("."));
    try_load!(cwd_path.join("kijuku-db-config.toml"));

    // 優先度 4: DB階層
    if let Some(db_dir) = db_path.parent() {
        try_load!(db_dir.join("kijuku-db-config.toml"));
    }

    // 優先度 5: 明示指定
    if let Some(extra) = extra_config_path {
        try_load!(extra.to_path_buf());
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_toml(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_default_config() {
        let config = KijukuConfig::default();
        assert!(config.backup.enabled);
        assert!(config.backup.auto_enabled);
        assert_eq!(config.backup.interval_ms, 3_600_000);
        assert_eq!(config.backup.tmp_retention_secs, 604_800);
        assert!(config.backup.backup_dir.is_none());
        assert!(config.backup.retention_tiers.is_none());
    }

    #[test]
    fn test_load_config_single_file() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        write_toml(
            dir.path(),
            "kijuku-db-config.toml",
            r#"
[backup]
enabled = false
backup_dir = "/tmp/mybackup"

[backup.auto]
interval_ms = 1800000
"#,
        );

        let config = load_config(&db_path, Some(dir.path()), None);
        assert!(!config.backup.enabled);
        assert_eq!(config.backup.backup_dir, Some("/tmp/mybackup".to_string()));
        assert_eq!(config.backup.interval_ms, 1_800_000);
    }

    #[test]
    fn test_load_config_merge_priority() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        // cwd config: interval_ms = 1800000
        write_toml(
            dir1.path(),
            "kijuku-db-config.toml",
            "[backup.auto]\ninterval_ms = 1800000\n",
        );

        // db config: interval_ms = 900000 (higher priority)
        write_toml(
            dir2.path(),
            "kijuku-db-config.toml",
            "[backup.auto]\ninterval_ms = 900000\n",
        );

        let db_path = dir2.path().join("test.db");
        let config = load_config(&db_path, Some(dir1.path()), None);
        // db hierarchy wins over cwd
        assert_eq!(config.backup.interval_ms, 900_000);
    }

    #[test]
    fn test_load_config_extra_path_highest_priority() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");

        write_toml(
            dir.path(),
            "kijuku-db-config.toml",
            "[backup.auto]\ninterval_ms = 1800000\n",
        );

        let extra_path = write_toml(
            dir.path(),
            "extra-config.toml",
            "[backup.auto]\ninterval_ms = 600000\n",
        );

        let config = load_config(&db_path, Some(dir.path()), Some(&extra_path));
        assert_eq!(config.backup.interval_ms, 600_000);
    }

    #[test]
    fn test_load_config_no_files() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let config = load_config(&db_path, Some(dir.path()), None);
        // Should return defaults
        assert!(config.backup.enabled);
        assert_eq!(config.backup.interval_ms, 3_600_000);
    }

    #[test]
    fn test_load_config_retention_tiers() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        write_toml(
            dir.path(),
            "kijuku-db-config.toml",
            r#"
[[backup.retention.tiers]]
max_age_secs = 3600
keep_interval_secs = 0

[[backup.retention.tiers]]
max_age_secs = 86400
keep_interval_secs = 3600
"#,
        );

        let config = load_config(&db_path, Some(dir.path()), None);
        let tiers = config.backup.retention_tiers.unwrap();
        assert_eq!(tiers.len(), 2);
        assert_eq!(tiers[0].max_age_secs, 3600);
        assert_eq!(tiers[0].keep_interval_secs, 0);
        assert_eq!(tiers[1].max_age_secs, 86400);
        assert_eq!(tiers[1].keep_interval_secs, 3600);
    }
}
