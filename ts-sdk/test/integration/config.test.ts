/**
 * config.toml 読み込みのテスト
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { loadConfig, defaultBackupConfig, defaultKijukuConfig } from '../../src/config.js';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

describe('defaultBackupConfig', () => {
  it('デフォルト値が正しい', () => {
    const config = defaultBackupConfig();
    expect(config.enabled).toBe(true);
    expect(config.autoEnabled).toBe(true);
    expect(config.intervalMs).toBe(3_600_000);
    expect(config.tmpRetentionSecs).toBe(604_800);
    expect(config.backupDir).toBeUndefined();
    expect(config.retentionTiers).toBeUndefined();
  });
});

describe('loadConfig', () => {
  let tmpDir: string;
  let dbPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'kijuku-config-test-'));
    dbPath = path.join(tmpDir, 'test.db');
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it('設定ファイルがない場合はデフォルト値を返す', () => {
    const config = loadConfig(dbPath, tmpDir);
    const defaults = defaultKijukuConfig();
    expect(config.backup.enabled).toBe(defaults.backup.enabled);
    expect(config.backup.intervalMs).toBe(defaults.backup.intervalMs);
    expect(config.backup.autoEnabled).toBe(defaults.backup.autoEnabled);
  });

  it('DB階層の設定ファイルを読み込む', () => {
    const configPath = path.join(tmpDir, 'kijuku-db-config.toml');
    fs.writeFileSync(configPath, `
[backup]
enabled = false
`);
    const config = loadConfig(dbPath, '/nonexistent-cwd');
    expect(config.backup.enabled).toBe(false);
  });

  it('backup_dir を設定できる', () => {
    const configPath = path.join(tmpDir, 'kijuku-db-config.toml');
    fs.writeFileSync(configPath, `
[backup]
backup_dir = "/custom/backup/path"
`);
    const config = loadConfig(dbPath, '/nonexistent-cwd');
    expect(config.backup.backupDir).toBe('/custom/backup/path');
  });

  it('[backup.auto] セクションを読み込む', () => {
    const configPath = path.join(tmpDir, 'kijuku-db-config.toml');
    fs.writeFileSync(configPath, `
[backup.auto]
enabled = false
interval_ms = 60000
`);
    const config = loadConfig(dbPath, '/nonexistent-cwd');
    expect(config.backup.autoEnabled).toBe(false);
    expect(config.backup.intervalMs).toBe(60000);
  });

  it('[backup.tmp] セクションを読み込む', () => {
    const configPath = path.join(tmpDir, 'kijuku-db-config.toml');
    fs.writeFileSync(configPath, `
[backup.tmp]
retention_secs = 86400
`);
    const config = loadConfig(dbPath, '/nonexistent-cwd');
    expect(config.backup.tmpRetentionSecs).toBe(86400);
  });

  it('[backup.retention.tiers] を読み込む', () => {
    const configPath = path.join(tmpDir, 'kijuku-db-config.toml');
    fs.writeFileSync(configPath, `
[[backup.retention.tiers]]
max_age_secs = 3600
keep_interval_secs = 0

[[backup.retention.tiers]]
max_age_secs = 86400
keep_interval_secs = 3600
`);
    const config = loadConfig(dbPath, '/nonexistent-cwd');
    expect(config.backup.retentionTiers).toHaveLength(2);
    expect(config.backup.retentionTiers![0].maxAgeSecs).toBe(3600);
    expect(config.backup.retentionTiers![0].keepIntervalSecs).toBe(0);
    expect(config.backup.retentionTiers![1].maxAgeSecs).toBe(86400);
    expect(config.backup.retentionTiers![1].keepIntervalSecs).toBe(3600);
  });

  it('extraConfigPath が最優先になる', () => {
    // DB階層のconfig
    const dbConfigPath = path.join(tmpDir, 'kijuku-db-config.toml');
    fs.writeFileSync(dbConfigPath, `
[backup]
enabled = false
`);

    // extraConfigPath
    const extraDir = fs.mkdtempSync(path.join(os.tmpdir(), 'extra-config-'));
    const extraConfigPath = path.join(extraDir, 'override.toml');
    fs.writeFileSync(extraConfigPath, `
[backup]
enabled = true
`);

    try {
      const config = loadConfig(dbPath, '/nonexistent-cwd', extraConfigPath);
      // extraConfigPath が DB階層を上書きするので enabled = true になる
      expect(config.backup.enabled).toBe(true);
    } finally {
      fs.rmSync(extraDir, { recursive: true });
    }
  });

  it('存在しない設定ファイルは無視される', () => {
    const config = loadConfig(dbPath, '/nonexistent-cwd', '/nonexistent/path.toml');
    const defaults = defaultKijukuConfig();
    expect(config.backup.enabled).toBe(defaults.backup.enabled);
  });

  it('同じファイルを二重に読み込まない（deduplicate）', () => {
    const configPath = path.join(tmpDir, 'kijuku-db-config.toml');
    fs.writeFileSync(configPath, `
[backup.auto]
interval_ms = 9999
`);
    // cwd と db_dir が同じ場合（同じファイルを2回指定）
    const config = loadConfig(dbPath, tmpDir);
    // マージされても値は1回分
    expect(config.backup.intervalMs).toBe(9999);
  });
});
