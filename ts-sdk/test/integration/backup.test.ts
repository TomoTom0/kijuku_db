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

    it('restore() は (b) 制限操作として stg(Local) で拒否される（設計 §9.2・TASK-59）', async () => {
      // データを作成してバックアップ
      db.createMedia({ title: '復元テスト', media_type: 'comic' });
      await db.backup();

      // restore は (b) 制限操作: TS Local（stg・writable）では常に拒否される。
      // prod 直接実行は Remote（Rust CLI の --target prod）経由。
      const { BackupSelector } = await import('../../src/backup.js');
      expect(() => db.restore(BackupSelector.latest())).toThrow(/制限操作.*restore/);
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

    it('backup を明示的に無効化(null)した場合、バックアップマネージャーが作成されない', () => {
      db.close();
      if (fs.existsSync(testDbPath)) {
        fs.unlinkSync(testDbPath);
      }

      const dbWithoutBackup = new KijukuDB(testDbPath, { backup: null });
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

    it('差分バックアップを read-only で開いてクエリできる（.diff open + ById）', async () => {
      const manager = db.getBackupManager()!;
      const { BackupSelector } = await import('../../src/backup.js');

      // フルバックアップ（空のDB）
      await manager.backupAuto();
      // データ追加
      db.createMedia({ title: 'Diff Target', media_type: 'video' });
      // 差分バックアップ
      await manager.backupAuto();

      const backups = db.listBackups();
      const diff = backups.find((b) => b.kind.type === 'diff');
      expect(diff).toBeDefined();

      // 差分バックアップを ID 指定で read-only open しクエリできる
      const media = db.findMediaFromBackup({}, undefined, BackupSelector.byId(diff!.id));
      expect(media.length).toBe(1);
      expect(media[0].title).toBe('Diff Target');

      // 一時フルDBはコールバック終了後に削除される
      const tmpDir = path.join(backupDir, 'tmp');
      if (fs.existsSync(tmpDir)) {
        const tempFiles = fs.readdirSync(tmpDir).filter((f) => f.includes('restore_temp_'));
        expect(tempFiles.length, `temp files remaining: ${JSON.stringify(tempFiles)}`).toBe(0);
      }
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

  describe('バックアップ差分（diffWithBackup）', () => {
    it('backup直後は差分なし', async () => {
      const { BackupSelector } = await import('../../src/backup.js');
      db.createMedia({ title: 'A', media_type: 'video' });
      await db.backupWithLabel('b1');
      const diff = db.diffWithBackup(BackupSelector.latest());
      expect(diff.summary.media.added).toBe(0);
      expect(diff.summary.media.removed).toBe(0);
      expect(diff.summary.media.changed).toBe(0);
    });

    it('backup後に追加したmediaは removed（復元で失われる）', async () => {
      const { BackupSelector } = await import('../../src/backup.js');
      // 空DBでバックアップ
      await db.backupWithLabel('empty');
      // backup後に2件追加（現在にのみ存在 → removed）
      db.createMedia({ title: 'A', media_type: 'video' });
      db.createMedia({ title: 'B', media_type: 'comic' });

      const diff = db.diffWithBackup(BackupSelector.latest());
      expect(diff.summary.media.removed).toBe(2);
      expect(diff.summary.media.added).toBe(0);
    });

    it('backup時のみのmediaは added（復元で復活）', async () => {
      const { BackupSelector } = await import('../../src/backup.js');
      const a = db.createMedia({ title: 'A', media_type: 'video' });
      await db.backupWithLabel('b1');
      // backup後にAを削除（backupにのみ残る → added）
      db.deleteMedia(a.id);

      const diff = db.diffWithBackup(BackupSelector.latest());
      expect(diff.summary.media.added).toBe(1);
      expect(diff.summary.media.removed).toBe(0);
    });
  });

  describe('事後ラベル/メモ（機能3）', () => {
    it('ラベルとメモを付与してlistで見える（サイドカー優先）', async () => {
      db.createMedia({ title: 'A', media_type: 'video' });
      await db.backupWithLabel('initial'); // ファイル名ラベル
      const id = db.listBackups()[0].id;

      db.setBackupLabel(id, 'important');
      db.setBackupNote(id, '作業前の状態');

      const info = db.listBackups().find((b) => b.id === id)!;
      expect(info.label).toBe('important');
      expect(info.labelSource).toBe('sidecar');
      expect(info.note).toBe('作業前の状態');
    });

    it('存在しないidはエラー', () => {
      expect(() => db.setBackupLabel('20990101000000-000', 'x')).toThrow();
    });
  });

  describe('pre-stash 一覧（設計 §8 即時復旧）', () => {
    /** pre-stash 名か（migrate が作る pre_migrate 等・3種） */
    const isPreStash = (name: string) =>
      name.endsWith('-pre_migrate.db') ||
      name.endsWith('-pre_restore.db') ||
      name.endsWith('-pre_promote.db');

    it('listPreStashes は pre-stash のみを返し listBackups は除外する', () => {
      const manager = db.getBackupManager()!;
      const tmpDir = path.join(backupDir, 'tmp');
      fs.mkdirSync(tmpDir, { recursive: true });
      const stem = path.basename(testDbPath, '.db');

      // 3種の pre-stash（タイムスタンプ違い）と通常 backup を tmp/ に追加配置。
      // ※ db.migrate() が pre_migrate を1件既に作成済み（これも正しく検出される）。
      const added = [
        `${stem}.20260724000000-000-pre_promote.db`,
        `${stem}.20260724000005-000-pre_restore.db`,
        `${stem}.20260724000010-000-pre_migrate.db`,
      ];
      for (const name of added) fs.writeFileSync(path.join(tmpDir, name), 'x');
      // 通常 backup（listPreStashes には含まれない・listBackups 対象）。
      const normalName = `${stem}.20260724000020-000.db`;
      fs.writeFileSync(path.join(tmpDir, normalName), 'd');

      const stashes = manager.listPreStashes();
      const stashNames = stashes.map((s) => s.name);

      // 追加した3件は全て検出される。
      for (const name of added) expect(stashNames).toContain(name);
      // 通常 backup は含まれない。全件 pre-stash 名のみ。
      expect(stashNames).not.toContain(normalName);
      expect(stashes.every((s) => isPreStash(s.name))).toBe(true);
      // path は tmp/ 配下で byPath restore に直接渡せる・kind は full・ラベルなし。
      for (const s of stashes) {
        expect(s.path).toContain(path.join(backupDir, 'tmp'));
        expect(s.kind).toEqual({ type: 'full' });
        expect(s.label).toBeUndefined();
      }
      // id 降順（全件で単調非増加）。
      for (let i = 1; i < stashes.length; i++) {
        expect(stashes[i - 1].id >= stashes[i].id).toBe(true);
      }
      // 追加した3件の相対順序は pre_migrate(新) > pre_restore > pre_promote(旧)。
      const idx = (name: string) => stashNames.indexOf(name);
      expect(idx(`${stem}.20260724000010-000-pre_migrate.db`)).toBeLessThan(
        idx(`${stem}.20260724000005-000-pre_restore.db`),
      );
      expect(idx(`${stem}.20260724000005-000-pre_restore.db`)).toBeLessThan(
        idx(`${stem}.20260724000000-000-pre_promote.db`),
      );

      // listBackups は pre-stash を全て除外し通常 backup を含む。
      const backups = db.listBackups();
      const backupNames = backups.map((b) => b.name);
      expect(backupNames).toContain(normalName);
      expect(backups.every((b) => !isPreStash(b.name))).toBe(true);
    });

    it('pre-stash が無い（通常 backup のみ）場合は空配列', () => {
      const manager = db.getBackupManager()!;
      const tmpDir = path.join(backupDir, 'tmp');
      // migrate 等が作った既存 pre-stash を全て削除して空状態を作る。
      if (fs.existsSync(tmpDir)) {
        for (const f of fs.readdirSync(tmpDir)) fs.unlinkSync(path.join(tmpDir, f));
      }
      expect(manager.listPreStashes()).toEqual([]);

      // 通常 backup のみ配置しても pre-stash は増えない。
      fs.mkdirSync(tmpDir, { recursive: true });
      const stem = path.basename(testDbPath, '.db');
      fs.writeFileSync(path.join(tmpDir, `${stem}.20260724000000-000.db`), 'x');
      expect(manager.listPreStashes()).toEqual([]);
    });
  });

  describe('retention: manual 保護（設計 §7.4）', () => {
    it('フォールバックモード（maxBackups）でも manual は削除対象外', async () => {
      const tmpDir = os.tmpdir();
      const p = path.join(tmpDir, `retm-${Date.now()}.db`);
      const bdir = path.join(tmpDir, `retmbak-${Date.now()}`);
      const db2 = new KijukuDB(p, {
        backup: { backupDir: bdir, enabled: true, maxBackups: 1 },
      });
      db2.migrate();
      try {
        // manual backup を3つ作成（各 backup() で cleanupOldBackups がトリガされる）
        for (let i = 0; i < 3; i++) {
          await db2.backup();
        }
        const manuals = db2.listBackups().filter((b) => b.scope === 'manual');
        expect(manuals.length).toBe(3);
      } finally {
        db2.close();
        if (fs.existsSync(p)) fs.unlinkSync(p);
        if (fs.existsSync(bdir)) fs.rmSync(bdir, { recursive: true });
      }
    });
  });
});
