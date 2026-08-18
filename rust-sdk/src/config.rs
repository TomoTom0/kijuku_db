/// kijuku-db 設定ファイル（config.toml）の読み込み・マージ機構
///
/// 設定ファイルの優先度（高いほど低いを上書き）:
/// 1. デフォルト値（コード組み込み）
/// 2. `~/.local/config/kijuku-db/config.toml`（グローバル）
/// 3. `{cwd}/kijuku-db-config.toml`（実行場所）
/// 4. `{db_dir}/kijuku-db-config.toml`（DB階層）
/// 5. `--config {path}` 引数（絶対最優先）
use crate::{KijukuError, Result};
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

// ============================================================
// 本番DB保護: target 解決（設計 §13・TASK-42 P0 Step3）
// ============================================================

/// 操作対象 DB。デフォルトは `Stg`（設計 §13.2「デフォルト stg・prod は明示フラグ」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// 本番 DB。readonly + migrate skip（設計 §5.1）。
    Prod,
    /// ステージング DB。RW + migrate 実行（LLM の通常作業環境）。
    Stg,
}

impl Default for Target {
    fn default() -> Self {
        Target::Stg
    }
}

impl Target {
    pub fn as_str(&self) -> &'static str {
        match self {
            Target::Prod => "prod",
            Target::Stg => "stg",
        }
    }

    /// 文字列から target へ変換（大文字小文字無視・不正値は None）。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "prod" => Some(Target::Prod),
            "stg" | "stage" | "staging" => Some(Target::Stg),
            _ => None,
        }
    }
}

/// target 解決の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetResolution {
    pub target: Target,
    /// 解決済み DB ファイルパス。
    pub db_path: String,
    /// prod なら true（readonly open・設計 §5.1）。
    pub readonly: bool,
    /// prod なら false（migrate skip・設計 §5.1）。
    pub should_migrate: bool,
}

/// 環境変数読込の抽象化（テストでモック可能にするため）。
pub trait EnvProvider {
    fn get(&self, key: &str) -> Option<String>;
}

/// `std::env::var` を読む本番用実装。
pub struct SystemEnv;

impl EnvProvider for SystemEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// 既定のDBパス解決は持たない: DB配置は利用者の明示指定（`--db` または
/// `KIJUKU_DB_PATH` / `KIJUKU_STG_DB_PATH`）のみとし、未指定はエラー（TASK-94）。
/// 暗黙の既定パス（cwd相対・固定絶対パスの双方）で勝手にファイルを作らせない。

/// target と db_path を解決する純粋関数（設計 §13）。
///
/// 優先順位（CLI 引数が常に勝つ）:
/// - read-source: `cli_read_source` > `KIJUKU_READ_SOURCE`(env) > 未指定
/// - target: `read_source`（Some なら優先・target に折り畳む） > `cli_target` > `KIJUKU_TARGET`(env)
///   > デフォルト `Stg`
/// - db_path: `cli_db` > target 別 env（prod=`KIJUKU_DB_PATH` / stg=`KIJUKU_STG_DB_PATH`）。
///   いずれも無い場合はエラー（DB配置は明示指定必須・暗黙の既定パスは持たない・TASK-94）
///
/// read-source は target に折り畳む（方式A: `read-source=prod` は prod RO 読込専用セッション・
/// 設計 §3.5/§6.2 を「インスタンス使い分け」で実現）。readonly / should_migrate は target から
/// 導出（prod→readonly=true・migrate skip）。
pub fn resolve_target(
    cli_target: Option<Target>,
    cli_db: Option<&str>,
    cli_read_source: Option<Target>,
    env: &dyn EnvProvider,
) -> Result<TargetResolution> {
    // read-source 解決（設計 §3.5/§13）。
    let read_source = cli_read_source
        .or_else(|| env.get("KIJUKU_READ_SOURCE").and_then(|s| Target::parse(&s)));

    // target 解決: read-source（Some なら優先・target に折り畳む） > --target > KIJUKU_TARGET > デフォルト Stg。
    let target = read_source
        .or_else(|| {
            cli_target
                .or_else(|| env.get("KIJUKU_TARGET").and_then(|s| Target::parse(&s)))
        })
        .unwrap_or_default();

    // db_path 解決: --db > target 別 env。未指定はエラー（DB配置は明示指定必須・暗黙の既定パスなし・TASK-94）。
    let db_path = match cli_db
        .map(|s| s.to_string())
        .or_else(|| match target {
            Target::Prod => env.get("KIJUKU_DB_PATH"),
            Target::Stg => env.get("KIJUKU_STG_DB_PATH"),
        })
        .filter(|p| !p.trim().is_empty())
    {
        Some(p) => p,
        None => {
            return Err(KijukuError::Validation(
                "db path is required: specify --db or KIJUKU_DB_PATH / KIJUKU_STG_DB_PATH (no implicit default path)".into(),
            ))
        }
    };

    let readonly = matches!(target, Target::Prod);
    Ok(TargetResolution {
        target,
        db_path,
        readonly,
        should_migrate: !readonly,
    })
}

/// prod と stg の両 DB パスを環境変数から解決する（sync 用・設計 §4.2）。
///
/// sync は prod(RO)→stg(RW) のファイルコピーで両パスを要するが、`resolve_target` は
/// 単一 target しか解決しないため、Prod と Stg をそれぞれ解決する。
/// `cli_db` / `KIJUKU_TARGET` は無視し、それぞれ `KIJUKU_DB_PATH` / `KIJUKU_STG_DB_PATH`
/// を用いる。未設定の側はエラー（`--from` / `--to` の明示指定へ誘導・TASK-94）。
pub fn resolve_prod_and_stg_paths(env: &dyn EnvProvider) -> Result<(String, String)> {
    let prod = env
        .get("KIJUKU_DB_PATH")
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| {
            KijukuError::Validation(
                "prod db path is required: specify --from (or KIJUKU_DB_PATH)".into(),
            )
        })?;
    let stg = env
        .get("KIJUKU_STG_DB_PATH")
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| {
            KijukuError::Validation(
                "stg db path is required: specify --to (or KIJUKU_STG_DB_PATH)".into(),
            )
        })?;
    Ok((prod, stg))
}

/// `--db` 文字列の解析結果（設計 §13・TS `cli.ts:parseDbPath` と同等）。
/// `host:path` 形式をリモート SSH 接続、それ以外をローカルパスとみなす。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDbPath {
    /// リモート（`host:path`）なら true。
    pub is_remote: bool,
    /// リモート時の SSH ホスト（~/.ssh/config のエイリアス可）。ローカル時は None。
    pub ssh_host: Option<String>,
    /// DB パス。リモート時はホスト上のパス、ローカル時はローカルパス。
    pub path: String,
}

/// `--db` 文字列を解析しローカル/リモートを判別する（設計 §13）。
///
/// 最初のコロンで `host` 部と `path` 部に分割し、ホスト部が2文字以上かつパス部が
/// 空でなければリモート扱い。ホスト部1文字（Windows ドライブレター `C:` 等）はローカル。
/// TS `cli.ts:parseDbPath` の正規表現 `^([^:]+):(.+)$` + `length > 1` と同等。
pub fn parse_db_path(db_path: &str) -> ParsedDbPath {
    if let Some(idx) = db_path.find(':') {
        let host = &db_path[..idx];
        let path = &db_path[idx + 1..];
        if host.len() > 1 && !path.is_empty() {
            return ParsedDbPath {
                is_remote: true,
                ssh_host: Some(host.to_string()),
                path: path.to_string(),
            };
        }
    }
    ParsedDbPath {
        is_remote: false,
        ssh_host: None,
        path: db_path.to_string(),
    }
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
    fn test_parse_db_path_remote_host_path() {
        let p = parse_db_path("nas:/data/kijuku.db");
        assert!(p.is_remote);
        assert_eq!(p.ssh_host.as_deref(), Some("nas"));
        assert_eq!(p.path, "/data/kijuku.db");
    }

    #[test]
    fn test_parse_db_path_remote_user_at_host() {
        let p = parse_db_path("tomo@nas:/data/kijuku.db");
        assert!(p.is_remote);
        assert_eq!(p.ssh_host.as_deref(), Some("tomo@nas"));
        assert_eq!(p.path, "/data/kijuku.db");
    }

    #[test]
    fn test_parse_db_path_remote_relative_path() {
        // ホスト部2文字以上・パス部非空ならリモート（TS と同じ挙動）。
        let p = parse_db_path("nas:data/kijuku.db");
        assert!(p.is_remote);
        assert_eq!(p.ssh_host.as_deref(), Some("nas"));
        assert_eq!(p.path, "data/kijuku.db");
    }

    #[test]
    fn test_parse_db_path_local_windows_drive_letter() {
        // ホスト部1文字（Windows ドライブレター）はローカル扱い。
        let p = parse_db_path("C:\\Users\\tomo\\kijuku.db");
        assert!(!p.is_remote);
        assert!(p.ssh_host.is_none());
        assert_eq!(p.path, "C:\\Users\\tomo\\kijuku.db");
    }

    #[test]
    fn test_parse_db_path_local_absolute() {
        let p = parse_db_path("/home/tomo/kijuku.db");
        assert!(!p.is_remote);
        assert!(p.ssh_host.is_none());
        assert_eq!(p.path, "/home/tomo/kijuku.db");
    }

    #[test]
    fn test_parse_db_path_local_relative_no_colon() {
        let p = parse_db_path("kijuku.db");
        assert!(!p.is_remote);
        assert!(p.ssh_host.is_none());
        assert_eq!(p.path, "kijuku.db");
    }

    #[test]
    fn test_parse_db_path_empty_path_after_colon_is_local() {
        // `host:` のようにパス部が空ならローカル扱い（TS `.+` と同等）。
        let p = parse_db_path("nas:");
        assert!(!p.is_remote);
        assert_eq!(p.path, "nas:");
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

    // ---- target 解決器のテスト（設計 §13・TASK-42 P0 Step3）----

    use std::collections::HashMap;

    struct MockEnv {
        vars: HashMap<String, String>,
    }
    impl MockEnv {
        fn empty() -> Self {
            Self {
                vars: HashMap::new(),
            }
        }
        fn with(mut self, k: &str, v: &str) -> Self {
            self.vars.insert(k.to_string(), v.to_string());
            self
        }
    }
    impl EnvProvider for MockEnv {
        fn get(&self, key: &str) -> Option<String> {
            self.vars.get(key).cloned()
        }
    }

    #[test]
    fn test_resolve_target_requires_explicit_db_path() {
        // --db・環境変数とも未指定はエラー（DB配置は明示指定必須・暗黙の既定パスなし・TASK-94）
        let err = match resolve_target(None, None, None, &MockEnv::empty()) {
            Err(e) => e,
            Ok(_) => panic!("missing db path must be rejected"),
        };
        assert!(
            err.to_string().contains("db path is required"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn test_resolve_target_default_target_is_stg() {
        // target 既定は stg（db パスは env で明示）
        let r = resolve_target(
            None,
            None,
            None,
            &MockEnv::empty().with("KIJUKU_STG_DB_PATH", "/env/stg.db"),
        )
        .unwrap();
        assert_eq!(r.target, Target::Stg);
        assert_eq!(r.db_path, "/env/stg.db");
        assert!(!r.readonly);
        assert!(r.should_migrate);
    }

    #[test]
    fn test_resolve_target_cli_beats_env() {
        // CLI --target prod が KIJUKU_TARGET=stg に勝つ
        let r = resolve_target(
            Some(Target::Prod),
            None,
            None,
            &MockEnv::empty()
                .with("KIJUKU_TARGET", "stg")
                .with("KIJUKU_DB_PATH", "/env/prod.db"),
        )
        .unwrap();
        assert_eq!(r.target, Target::Prod);
        assert!(r.readonly);
        assert!(!r.should_migrate);
        assert_eq!(r.db_path, "/env/prod.db");
    }

    #[test]
    fn test_resolve_target_env_beats_default() {
        let r = resolve_target(
            None,
            None,
            None,
            &MockEnv::empty()
                .with("KIJUKU_TARGET", "prod")
                .with("KIJUKU_DB_PATH", "/env/prod.db"),
        )
        .unwrap();
        assert_eq!(r.target, Target::Prod);
        assert!(r.readonly);
    }

    #[test]
    fn test_resolve_target_invalid_env_falls_back_to_stg() {
        let r = resolve_target(
            None,
            None,
            None,
            &MockEnv::empty()
                .with("KIJUKU_TARGET", "bogus")
                .with("KIJUKU_STG_DB_PATH", "/env/stg.db"),
        )
        .unwrap();
        assert_eq!(r.target, Target::Stg);
        assert_eq!(r.db_path, "/env/stg.db");
    }

    #[test]
    fn test_resolve_target_db_cli_beats_all() {
        // --db が target 別 env に勝つ（target=prod でも --db を尊重）
        let r = resolve_target(
            Some(Target::Prod),
            Some("/custom/path.db"),
            None,
            &MockEnv::empty().with("KIJUKU_DB_PATH", "/env/prod.db"),
        )
        .unwrap();
        assert_eq!(r.db_path, "/custom/path.db");
        assert!(r.readonly); // readonly は target から導出（--db には依存しない）
    }

    #[test]
    fn test_resolve_target_db_env_per_target() {
        // prod + KIJUKU_DB_PATH
        let r = resolve_target(
            Some(Target::Prod),
            None,
            None,
            &MockEnv::empty().with("KIJUKU_DB_PATH", "/env/prod.db"),
        )
        .unwrap();
        assert_eq!(r.db_path, "/env/prod.db");
        // stg + KIJUKU_STG_DB_PATH
        let r = resolve_target(
            Some(Target::Stg),
            None,
            None,
            &MockEnv::empty().with("KIJUKU_STG_DB_PATH", "/env/stg.db"),
        )
        .unwrap();
        assert_eq!(r.db_path, "/env/stg.db");
    }

    #[test]
    fn test_resolve_target_stg_ignores_prod_env() {
        // target=stg のとき KIJUKU_DB_PATH(prod用) は無視して stg 用 env を使う
        let r = resolve_target(
            Some(Target::Stg),
            None,
            None,
            &MockEnv::empty()
                .with("KIJUKU_DB_PATH", "/env/prod.db")
                .with("KIJUKU_STG_DB_PATH", "/env/stg.db"),
        )
        .unwrap();
        assert_eq!(r.db_path, "/env/stg.db");
    }

    #[test]
    fn test_resolve_target_stg_missing_db_errors() {
        // target=stg で stg 用 env が無くても prod 用 env では補完しない（エラー）
        assert!(resolve_target(
            Some(Target::Stg),
            None,
            None,
            &MockEnv::empty().with("KIJUKU_DB_PATH", "/env/prod.db"),
        )
        .is_err());
    }

    #[test]
    fn test_resolve_target_read_source_prod_overrides_target() {
        // --read-source prod が --target stg に勝つ（read-source は target に折り畳む・方式A）
        let r = resolve_target(
            Some(Target::Stg),
            None,
            Some(Target::Prod),
            &MockEnv::empty().with("KIJUKU_DB_PATH", "/env/prod.db"),
        )
        .unwrap();
        assert_eq!(r.target, Target::Prod);
        assert!(r.readonly);
        assert!(!r.should_migrate);
        assert_eq!(r.db_path, "/env/prod.db");
    }

    #[test]
    fn test_resolve_target_read_source_env() {
        // KIJUKU_READ_SOURCE=prod が未指定 target に勝つ
        let r = resolve_target(
            None,
            None,
            None,
            &MockEnv::empty()
                .with("KIJUKU_READ_SOURCE", "prod")
                .with("KIJUKU_DB_PATH", "/env/prod.db"),
        )
        .unwrap();
        assert_eq!(r.target, Target::Prod);
        assert!(r.readonly);
    }

    #[test]
    fn test_resolve_target_read_source_cli_beats_env() {
        // --read-source stg が KIJUKU_READ_SOURCE=prod に勝つ
        let r = resolve_target(
            None,
            None,
            Some(Target::Stg),
            &MockEnv::empty()
                .with("KIJUKU_READ_SOURCE", "prod")
                .with("KIJUKU_STG_DB_PATH", "/env/stg.db"),
        )
        .unwrap();
        assert_eq!(r.target, Target::Stg);
        assert!(!r.readonly);
    }

    #[test]
    fn test_resolve_target_read_source_beats_target_env() {
        // read-source が KIJUKU_TARGET より優先
        let r = resolve_target(
            None,
            None,
            None,
            &MockEnv::empty()
                .with("KIJUKU_TARGET", "stg")
                .with("KIJUKU_READ_SOURCE", "prod")
                .with("KIJUKU_DB_PATH", "/env/prod.db"),
        )
        .unwrap();
        assert_eq!(r.target, Target::Prod);
        assert!(r.readonly);
    }

    #[test]
    fn test_resolve_target_read_source_none_falls_back_to_target() {
        // read-source 未指定時は target に従う（従来通り）
        let r = resolve_target(
            Some(Target::Prod),
            None,
            None,
            &MockEnv::empty().with("KIJUKU_DB_PATH", "/env/prod.db"),
        )
        .unwrap();
        assert_eq!(r.target, Target::Prod);
        assert!(r.readonly);
    }

    #[test]
    fn test_resolve_target_read_source_invalid_env_falls_back() {
        // KIJUKU_READ_SOURCE 不正値は無視して target（デフォルト stg）に従う
        let r = resolve_target(
            None,
            None,
            None,
            &MockEnv::empty()
                .with("KIJUKU_READ_SOURCE", "bogus")
                .with("KIJUKU_STG_DB_PATH", "/env/stg.db"),
        )
        .unwrap();
        assert_eq!(r.target, Target::Stg);
        assert!(!r.readonly);
    }

    #[test]
    fn test_target_parse_variants() {
        assert_eq!(Target::parse("prod"), Some(Target::Prod));
        assert_eq!(Target::parse("PROD"), Some(Target::Prod));
        assert_eq!(Target::parse("stg"), Some(Target::Stg));
        assert_eq!(Target::parse("staging"), Some(Target::Stg));
        assert_eq!(Target::parse("  prod "), Some(Target::Prod));
        assert_eq!(Target::parse("nope"), None);
    }
}
