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
      fs.rmSync(backupDir, { recursive: true });
    }
  });

  describe('バックアップディレクトリの作成', () => {
    it('バックアップサブディレクトリが自動的に作成される', () => {
      expect(fs.existsSync(path.join(backupDir, 'auto'))).toBe(true);
      expect(fs.existsSync(path.join(backupDir, 'manual'))).toBe(true);
      expect(fs.existsSync(path.join(backupDir, 'meta'))).toBe(true);
    });
  });

  describe('手動バックアップ', () => {
    it('backup()で手動バックアップを実行できる', async () => {
      const backupPath = await db.backup();

      expect(backupPath).toBeDefined();
      expect(fs.existsSync(backupPath!)).toBe(true);
      // manual/ ディレクトリ下にある
      expect(backupPath!).toContain(path.join(backupDir, 'manual'));
    });

    it('バックアップ一覧を取得できる', async () => {
      await db.backup();
      await db.backup();

      const backups = db.listBackups();
      expect(backups.length).toBe(2);
      // 新しいファイル名形式: {db_stem}.{timestamp}.db
      expect(backups[0].name).toMatch(/^test-backup-\d+\.\d{14}-\d{3}\.db$/);
      expect(backups[0].createdAt).toBeInstanceOf(Date);
      expect(backups[0].scope).toBe('manual');
      expect(backups[0].kind.type).toBe('full');
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

    it('ラベル付きバックアップを作成できる', async () => {
      const backupPath = await db.backupWithLabel('before_import');
      expect(backupPath).toBeDefined();
      expect(backupPath!).toContain('-before_import.db');

      const backups = db.listBackups();
      expect(backups[0].label).toBe('before_import');
    });

    it('restore()でバックアップから復元できる', async () => {
      // データを作成してバックアップ
      db.createMedia({ title: '復元テスト', media_type: 'comic' });
      await db.backup();

      // データを追加
      db.createMedia({ title: '追加データ', media_type: 'video' });
      expect(db.findMedia({}).length).toBe(2);

      // バックアップ時点に復元
      const { BackupSelector } = await import('../../src/backup.js');
      db.restore(BackupSelector.latest());

      // 復元後は1件に戻っている
      const afterRestore = db.findMedia({});
      expect(afterRestore.length).toBe(1);
      expect(afterRestore[0].title).toBe('復元テスト');
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

  describe('スコープフィルタ（TASK-150）', () => {
    it('auto/ スコープを指定してバックアップを選択できる', async () => {
      const manager = db.getBackupManager()!;
      await manager.backupAuto();
      await db.backup();

      const { BackupSelector } = await import('../../src/backup.js');
      const autoSelector = BackupSelector.latest().scope('auto');
      const result = manager.selectBackup(autoSelector);
      expect(result).not.toBeNull();
      expect(result!.scope).toBe('auto');
    });

    it('manual/ スコープを指定してバックアップを選択できる', async () => {
      const manager = db.getBackupManager()!;
      await manager.backupAuto();
      await db.backup();

      const { BackupSelector } = await import('../../src/backup.js');
      const manualSelector = BackupSelector.latest().scope('manual');
      const result = manager.selectBackup(manualSelector);
      expect(result).not.toBeNull();
      expect(result!.scope).toBe('manual');
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

  describe('差分バックアップ', () => {
    it('自動バックアップで差分ファイルが作成される', async () => {
      const manager = db.getBackupManager()!;

      // フルバックアップ
      await manager.backupAuto();

      // データを追加
      db.createMedia({ title: 'Media for diff', media_type: 'video' });

      // 差分バックアップ（同日なのでdiff）
      await manager.backupAuto();

      const backups = db.listBackups();
      const hasDiff = backups.some((b) => b.kind.type === 'diff');
      expect(hasDiff).toBe(true);
    });

    it('差分バックアップから復元できる', async () => {
      const manager = db.getBackupManager()!;
      const { BackupSelector } = await import('../../src/backup.js');

      // フルバックアップ（空のDB）
      await manager.backupAuto();

      // データを追加
      db.createMedia({ title: 'Media A', media_type: 'video' });
      db.createMedia({ title: 'Media B', media_type: 'comic' });

      // 差分バックアップ（2件のデータ状態）
      await manager.backupAuto();

      const backupsAfterDiff = db.listBackups();
      const hasDiff = backupsAfterDiff.some((b) => b.kind.type === 'diff');
      expect(hasDiff).toBe(true);

      // データをさらに追加
      db.createMedia({ title: 'Media C', media_type: 'music' });

      // 差分バックアップ（最新）から復元
      const selector = BackupSelector.latest().scope('auto');
      manager.restore(selector);

      // 復元後は新しい接続で検証
      const Database = (await import('better-sqlite3')).default;
      const verifyDb = new Database(testDbPath);
      verifyDb.pragma('foreign_keys = ON');
      const count = (
        verifyDb.prepare('SELECT COUNT(*) as cnt FROM media').get() as { cnt: number }
      ).cnt;
      verifyDb.close();

      // 差分バックアップ時点の2件が復元されているべき
      expect(count).toBe(2);
    });

    it('フル/差分のmtimeが同値でも latest は作成順(id)で差分を選ぶ（TASK-17 回帰）', async () => {
      // ファイル mtime の粒度が粗く同ミリ秒作成でフル/差分が同 mtime になると、
      // mtime ベースのソートは非決定になり latest が古いフル(空)を選ぶ不具合があった。
      // 作成順はファイル名タイムスタンプ(id)で決定論的に判定されることを検証する。
      const manager = db.getBackupManager()!;
      const { BackupSelector } = await import('../../src/backup.js');

      await manager.backupAuto(); // フル(空)
      db.createMedia({ title: 'A', media_type: 'video' });
      db.createMedia({ title: 'B', media_type: 'comic' });
      await manager.backupAuto(); // 差分(2件)

      // フル/差分両バックアップの mtime を同一化してタイ条件を決定論的に再現
      const sameTime = new Date();
      for (const b of db.listBackups().filter((x) => x.scope === 'auto')) {
        fs.utimesSync(b.path, sameTime, sameTime);
      }

      const selected = manager.selectBackup(BackupSelector.latest().scope('auto'));
      expect(selected).not.toBeNull();
      // フル(空状態)ではなく、新しい差分(2件)が選ばれるべき
      expect(selected!.kind.type).toBe('diff');
    });
  });

  describe('auto-records.csv (TASK-148)', () => {
    it('自動バックアップ取得時に records.csv が更新される', async () => {
      const manager = db.getBackupManager()!;
      await manager.backupAuto();

      const metaDir = path.join(backupDir, 'meta');
      const csvPath = path.join(metaDir, 'auto-records.csv');
      expect(fs.existsSync(csvPath)).toBe(true);

      const content = fs.readFileSync(csvPath, 'utf-8');
      expect(content).toContain('id,created_at,tier,type,base_id,size_bytes,status,pruned_at');
      expect(content).toContain('daily');
      expect(content).toContain('full');
    });
  });
});
