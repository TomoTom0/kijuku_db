/**
 * BackupManager の単体テスト
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB } from '../../src/index.js';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

describe('BackupManager', () => {
  let testDbPath: string;
  let backupDir: string;
  let db: KijukuDB;

  beforeEach(() => {
    const tmpDir = os.tmpdir();
    testDbPath = path.join(tmpDir, `test-backup-${Date.now()}.db`);
    backupDir = path.join(tmpDir, `backup-${Date.now()}`);

    db = new KijukuDB(testDbPath, {
      backup: {
        backupDir,
        intervalMs: 1000,
        enabled: true,
      },
    });

    db.migrate();
  });

  afterEach(() => {
    db.close();

    if (fs.existsSync(testDbPath)) {
      fs.unlinkSync(testDbPath);
    }

    if (fs.existsSync(backupDir)) {
      const files = fs.readdirSync(backupDir);
      for (const file of files) {
        fs.unlinkSync(path.join(backupDir, file));
      }
      fs.rmdirSync(backupDir);
    }
  });

  describe('バックアップディレクトリの作成', () => {
    it('バックアップディレクトリが自動的に作成される', () => {
      expect(fs.existsSync(backupDir)).toBe(true);
    });
  });

  describe('手動バックアップ', () => {
    it('backup()で手動バックアップを実行できる', async () => {
      const backupPath = await db.backup();

      expect(backupPath).toBeDefined();
      expect(fs.existsSync(backupPath!)).toBe(true);
      expect(path.dirname(backupPath!)).toBe(backupDir);
    });

    it('バックアップ一覧を取得できる', async () => {
      await db.backup();
      await db.backup();

      const backups = db.listBackups();
      expect(backups.length).toBe(2);
      // 新しいファイル名形式: {db_stem}.backup-{yyyymmddhhmmss-mmm}.db
      expect(backups[0].name).toMatch(/^test-backup-\d+\.backup-\d{14}-\d{3}\.db$/);
      expect(backups[0].createdAt).toBeInstanceOf(Date);
    });

    it('バックアップファイルが新しい順にソートされる', async () => {
      await db.backup();
      await new Promise((resolve) => setTimeout(resolve, 100));
      await db.backup();

      const backups = db.listBackups();
      expect(backups.length).toBe(2);
      expect(backups[0].createdAt.getTime()).toBeGreaterThanOrEqual(
        backups[1].createdAt.getTime()
      );
    });
  });

  describe('自動バックアップ', () => {
    it('時間間隔に基づいて自動バックアップが実行される', async () => {
      const backupManager = db.getBackupManager();
      expect(backupManager).toBeDefined();

      db.createMedia({
        title: 'Test Media 1',
        media_type: 'comic',
      });

      await new Promise((resolve) => setTimeout(resolve, 200));

      let backups = db.listBackups();
      expect(backups.length).toBe(1);

      await new Promise((resolve) => setTimeout(resolve, 1100));

      db.createMedia({
        title: 'Test Media 2',
        media_type: 'comic',
      });

      await new Promise((resolve) => setTimeout(resolve, 200));

      backups = db.listBackups();
      expect(backups.length).toBe(2);
    });

    it('時間間隔内では自動バックアップが実行されない', async () => {
      db.createMedia({
        title: 'Test Media 1',
        media_type: 'comic',
      });

      await new Promise((resolve) => setTimeout(resolve, 100));

      let backups = db.listBackups();
      expect(backups.length).toBe(1);

      db.createMedia({
        title: 'Test Media 2',
        media_type: 'comic',
      });

      await new Promise((resolve) => setTimeout(resolve, 100));

      backups = db.listBackups();
      expect(backups.length).toBe(1);
    });
  });

  describe('バックアップ無効化', () => {
    it('enabled: falseの場合、バックアップが無効化される', async () => {
      db.close();
      if (fs.existsSync(testDbPath)) {
        fs.unlinkSync(testDbPath);
      }

      const dbWithoutBackup = new KijukuDB(testDbPath, {
        backup: {
          backupDir,
          enabled: false,
        },
      });

      dbWithoutBackup.migrate();

      dbWithoutBackup.createMedia({
        title: 'Test Media',
        media_type: 'comic',
      });

      await new Promise((resolve) => setTimeout(resolve, 100));

      const backups = dbWithoutBackup.listBackups();
      expect(backups.length).toBe(0);

      dbWithoutBackup.close();
    });

    it('backupオプションが指定されていない場合、バックアップマネージャーが作成されない', () => {
      db.close();
      if (fs.existsSync(testDbPath)) {
        fs.unlinkSync(testDbPath);
      }

      const dbWithoutBackup = new KijukuDB(testDbPath);
      dbWithoutBackup.migrate();

      const backupManager = dbWithoutBackup.getBackupManager();
      expect(backupManager).toBeUndefined();

      dbWithoutBackup.close();
    });
  });

  describe('バックアップマネージャー情報取得', () => {
    it('最後のバックアップ時刻を取得できる', async () => {
      const backupManager = db.getBackupManager();
      expect(backupManager).toBeDefined();

      expect(backupManager!.getLastBackupTime()).toBeNull();

      await db.backup();

      const lastBackupTime = backupManager!.getLastBackupTime();
      expect(lastBackupTime).toBeInstanceOf(Date);
    });

    it('次回バックアップまでの残り時間を取得できる', async () => {
      const backupManager = db.getBackupManager();
      expect(backupManager).toBeDefined();

      expect(backupManager!.getTimeUntilNextBackup()).toBe(0);

      await db.backup();

      const remaining = backupManager!.getTimeUntilNextBackup();
      expect(remaining).toBeGreaterThan(0);
      expect(remaining).toBeLessThanOrEqual(1000);
    });
  });
});
