/**
 * kijuku-db 設定ファイル（config.toml）の読み込み・マージ機構
 *
 * 設定ファイルの優先度（高いほど低いを上書き）:
 * 1. デフォルト値
 * 2. `~/.local/config/kijuku-db/config.toml`（グローバル）
 * 3. `{cwd}/kijuku-db-config.toml`（実行場所）
 * 4. `{db_dir}/kijuku-db-config.toml`（DB階層）
 * 5. `--config {path}` 引数（絶対最優先）
 */
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { parse as parseToml } from 'smol-toml';

// ---- TOML デシリアライズ用インターフェース ----

interface ConfigFile {
  backup?: BackupSectionFile;
}

interface BackupSectionFile {
  enabled?: boolean;
  backup_dir?: string;
  auto?: BackupAutoSectionFile;
  tmp?: BackupTmpSectionFile;
  retention?: BackupRetentionSectionFile;
}

interface BackupAutoSectionFile {
  enabled?: boolean;
  interval_ms?: number;
}

interface BackupTmpSectionFile {
  retention_secs?: number;
}

interface BackupRetentionSectionFile {
  tiers?: RetentionTierFile[];
}

interface RetentionTierFile {
  max_age_secs: number;
  keep_interval_secs: number;
}

// ---- 解決済み設定インターフェース ----

export interface RetentionTierConfig {
  maxAgeSecs: number;
  keepIntervalSecs: number;
}

/** マージ済みのバックアップ設定 */
export interface BackupConfig {
  /** バックアップ機能全体の有効/無効 */
  enabled: boolean;
  /** バックアップ保存先ディレクトリ（undefined = {db_dir}/backup/） */
  backupDir?: string;
  /** 自動バックアップの有効/無効 */
  autoEnabled: boolean;
  /** 自動バックアップのトリガー間隔（ミリ秒） */
  intervalMs: number;
  /** tmp/ の保持期間（秒） */
  tmpRetentionSecs: number;
  /** 保持ポリシーのtiers（undefined = デフォルト） */
  retentionTiers?: RetentionTierConfig[];
}

/** デフォルトのバックアップ設定 */
export function defaultBackupConfig(): BackupConfig {
  return {
    enabled: true,
    backupDir: undefined,
    autoEnabled: true,
    intervalMs: 3_600_000,
    tmpRetentionSecs: 604_800, // 7日
    retentionTiers: undefined,
  };
}

/** マージ済みの設定全体 */
export interface KijukuConfig {
  backup: BackupConfig;
}

/** デフォルトの設定全体 */
export function defaultKijukuConfig(): KijukuConfig {
  return {
    backup: defaultBackupConfig(),
  };
}

// ---- マージロジック ----

function mergeConfig(base: KijukuConfig, overlay: ConfigFile): void {
  const b = base.backup;
  const o = overlay.backup ?? {};

  if (o.enabled !== undefined) b.enabled = o.enabled;
  if (o.backup_dir !== undefined) b.backupDir = o.backup_dir;
  if (o.auto?.enabled !== undefined) b.autoEnabled = o.auto.enabled;
  if (o.auto?.interval_ms !== undefined) b.intervalMs = o.auto.interval_ms;
  if (o.tmp?.retention_secs !== undefined) b.tmpRetentionSecs = o.tmp.retention_secs;
  if (o.retention?.tiers !== undefined) {
    b.retentionTiers = o.retention.tiers.map((t) => ({
      maxAgeSecs: t.max_age_secs,
      keepIntervalSecs: t.keep_interval_secs,
    }));
  }
}

function readConfigFile(filePath: string): ConfigFile | null {
  try {
    const content = fs.readFileSync(filePath, 'utf-8');
    return parseToml(content) as ConfigFile;
  } catch {
    return null;
  }
}

/** グローバル設定ファイルのパスを返す */
export function globalConfigPath(): string {
  return path.join(os.homedir(), '.local', 'config', 'kijuku-db', 'config.toml');
}

/**
 * 複数の設定ファイルを優先度順にマージして返す
 *
 * @param dbPath DBファイルのパス
 * @param cwd 実行場所のパス（省略時は process.cwd()）
 * @param extraConfigPath 明示指定の設定ファイルパス（最優先）
 */
export function loadConfig(
  dbPath: string,
  cwd?: string,
  extraConfigPath?: string,
): KijukuConfig {
  const config = defaultKijukuConfig();
  const loaded = new Set<string>();

  const tryLoad = (filePath: string): void => {
    const resolved = path.resolve(filePath);
    if (loaded.has(resolved)) return;
    const fileConfig = readConfigFile(resolved);
    if (fileConfig !== null) {
      loaded.add(resolved);
      mergeConfig(config, fileConfig);
    }
  };

  // 優先度 2: グローバル
  tryLoad(globalConfigPath());

  // 優先度 3: cwd
  const cwdPath = cwd ?? process.cwd();
  tryLoad(path.join(cwdPath, 'kijuku-db-config.toml'));

  // 優先度 4: DB階層
  const dbDir = path.dirname(path.resolve(dbPath));
  tryLoad(path.join(dbDir, 'kijuku-db-config.toml'));

  // 優先度 5: 明示指定
  if (extraConfigPath !== undefined) {
    tryLoad(extraConfigPath);
  }

  return config;
}

// ============================================================
// 本番DB保護: target 解決（設計 §13・TASK-42 P0 Step3）
// ============================================================

/** 操作対象 DB。デフォルトは `stg`（設計 §13.2「デフォルト stg・prod は明示フラグ」）。 */
export const TARGET_VALUES = ['prod', 'stg'] as const;
export type Target = (typeof TARGET_VALUES)[number];

/** target 解決の結果。 */
export interface TargetResolution {
  target: Target;
  /** 解決済み DB ファイルパス。 */
  dbPath: string;
  /** prod なら true（readonly open・設計 §5.1）。 */
  readonly: boolean;
  /** prod なら false（migrate skip・設計 §5.1）。 */
  shouldMigrate: boolean;
}

const DEFAULT_PROD_DB = 'kijuku.db';
const DEFAULT_STG_DB = 'kijuku.stg.db';

/** 文字列から target へ変換（大文字小文字無視・不正値は undefined）。 */
export function parseTarget(s: string | undefined): Target | undefined {
  if (s === undefined) return undefined;
  switch (s.trim().toLowerCase()) {
    case 'prod':
      return 'prod';
    case 'stg':
    case 'stage':
    case 'staging':
      return 'stg';
    default:
      return undefined;
  }
}

/** 環境変数読込の抽象化（テストでモック可能にするため）。 */
export type EnvGetter = (key: string) => string | undefined;

/** `process.env` を読む本番用実装。 */
export const systemEnv: EnvGetter = (key) => process.env[key];

/**
 * target と dbPath を解決する純粋関数（設計 §13）。
 *
 * 優先順位（CLI 引数が常に勝つ）:
 * - read-source: `cliReadSource` > `KIJUKU_READ_SOURCE`(env) > 未指定
 * - target: `readSource`（指定なら優先・target に折り畳む） > `cliTarget` > `KIJUKU_TARGET`(env) > デフォルト `stg`
 * - dbPath: `cliDb` > target 別 env（prod=`KIJUKU_DB_PATH` / stg=`KIJUKU_STG_DB_PATH`）
 *   > target 別デフォルト（prod=`kijuku.db` / stg=`kijuku.stg.db`）
 *
 * read-source は target に折り畳む（方式A: read-source=prod は prod RO 読込専用セッション・
 * 設計 §3.5/§6.2 を「インスタンス使い分け」で実現）。readonly / shouldMigrate は target から導出。
 */
export function resolveTarget(
  cliTarget: Target | undefined,
  cliDb: string | undefined,
  cliReadSource: Target | undefined,
  env: EnvGetter,
): TargetResolution {
  // read-source 解決（設計 §3.5/§13）。
  const readSource = cliReadSource ?? parseTarget(env('KIJUKU_READ_SOURCE'));
  // target 解決: read-source（指定なら優先・target に折り畳む） > --target > KIJUKU_TARGET > デフォルト stg。
  const target = readSource ?? cliTarget ?? parseTarget(env('KIJUKU_TARGET')) ?? 'stg';

  const dbPath =
    cliDb ??
    (target === 'prod' ? env('KIJUKU_DB_PATH') : env('KIJUKU_STG_DB_PATH')) ??
    (target === 'prod' ? DEFAULT_PROD_DB : DEFAULT_STG_DB);

  const readonly = target === 'prod';
  return { target, dbPath, readonly, shouldMigrate: !readonly };
}
