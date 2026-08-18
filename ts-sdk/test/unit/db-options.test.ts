/**
 * DBOptions / open 挙動のテスト（本番DB保護 P0: WAL + readonly skip・migrate 拒否）
 *
 * 設計 docs/design/db-protection.md §5.1（prod readonly + no-migrate）・§5.3（WAL）。
 * Rust の db_options_test.rs と同等の挙動をカバーする。
 */
import { describe, it, expect, afterEach } from 'vitest';
import { mkdtempSync, rmSync, readdirSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import Database from 'better-sqlite3';
import { KijukuDB } from '../../src/index.js';

let dirs: string[] = [];
afterEach(() => {
  for (const d of dirs) rmSync(d, { recursive: true, force: true });
  dirs = [];
});
function tempDir(): string {
  const d = mkdtempSync(path.join(tmpdir(), 'kijuku-dbopts-'));
  dirs.push(d);
  return d;
}

/** 別接続で journal_mode を読む（KijukuDB が保持する接続と共存可能・WAL は複数読者を許す）。 */
function readJournalMode(dbPath: string): string {
  const c = new Database(dbPath, { readonly: true });
  const mode = c.pragma('journal_mode', { simple: true }) as string;
  c.close();
  return String(mode).toLowerCase();
}

describe('KijukuDB open options (WAL / readonly)', () => {
  it('RW 接続では WAL が有効になる（設計 §5.3）', () => {
    const dir = tempDir();
    const dbPath = path.join(dir, 'rw.db');
    const db = new KijukuDB(dbPath, { backup: null });
    // スキーマ初期化で WAL 副産物が確実に作られるよう軽い書込
    db.migrate();
    db.close?.();
    expect(readJournalMode(dbPath)).toBe('wal');
  });

  it('readonly 接続では WAL を設定せず、migrate は拒否される（設計 §5.1・§5.3）', () => {
    const dir = tempDir();
    const dbPath = path.join(dir, 'ro.db');

    // 事前に DELETE モード（WAL 未設定）のファイルを作成
    {
      const c = new Database(dbPath);
      c.pragma('journal_mode = DELETE');
      c.exec('CREATE TABLE _t(x)');
      expect(String(c.pragma('journal_mode', { simple: true })).toLowerCase()).toBe('delete');
      c.close();
    }

    // readonly で開く（backup も null で副作用を抑える）
    const db = new KijukuDB(dbPath, { readonly: true, backup: null });

    // WAL は設定されず delete のまま
    expect(readJournalMode(dbPath)).toBe('delete');

    // migrate は readonly で拒否される
    expect(() => db.migrate()).toThrow(/readonly/);
    db.close?.();
  });

  it('migrate 実行前に tmp/ へ pre_migrate snapshot を作る（設計 §7.3）', () => {
    const dir = tempDir();
    const dbPath = path.join(dir, 'mig.db');
    // デフォルト（backup 有効）で開く
    const db = new KijukuDB(dbPath);
    db.migrate();
    db.close();

    const tmpDir = path.join(dir, 'backup', 'tmp');
    expect(existsSync(tmpDir)).toBe(true);
    const snaps = readdirSync(tmpDir).filter((f) => f.endsWith('-pre_migrate.db'));
    expect(snaps.length).toBe(1);
  });
});

describe('file-less DB のバックアップ出力先管理（TASK-95）', () => {
  it(':memory: は既定でバックアップ無効（BackupManager を生成せず cwd に backup/ を作らない）', () => {
    const before = existsSync('backup');
    const db = new KijukuDB(':memory:');
    expect(db.getBackupManager()).toBeUndefined();
    db.close();
    expect(existsSync('backup')).toBe(before);
  });

  it(':memory: で backup を明示した場合、backupDir 未指定だとエラー（cwd 暗黙解決の拒否）', () => {
    expect(() => new KijukuDB(':memory:', { backup: {} })).toThrow(/backupDir must be specified explicitly/);
  });

  it(':memory: + 明示 backupDir は許可される', () => {
    const dir = tempDir();
    const db = new KijukuDB(':memory:', { backup: { backupDir: dir } });
    expect(db.getBackupManager()).toBeDefined();
    db.close();
  });
});
