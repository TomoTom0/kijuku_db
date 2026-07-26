/**
 * promote（stg→prod 反映）のテスト（設計 §4.5/§6.4/§8・TASK-58）。
 *
 * Rust の `rust-sdk/tests/promote_test.rs` と同等の検証を TS 側で保証する（SDK parity）。
 * - gate 合格で stg→prod が反映され pre-stash（promote 前 prod）が作られる
 * - gate 不合格では prod が一切触られない（PromoteGateFailedError）
 * - stg==prod の同一パスは誤設定として拒否
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB, PromoteGateFailedError } from '../../src/index.js';
import type { ObserveOptions } from '../../src/types.js';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

describe('KijukuDB.promote (stg -> prod)', () => {
  let dir: string;

  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kijuku-promote-'));
  });

  afterEach(() => {
    fs.rmSync(dir, { recursive: true, force: true });
  });

  // prod を作成（migrate + n件）する。ハンドルは閉じる。
  function setupProd(prodPath: string, n: number): void {
    const db = new KijukuDB(prodPath, { backup: null });
    db.migrate();
    for (let i = 0; i < n; i++) {
      db.createMedia({ title: `prod${i}`, media_type: 'comic' });
    }
    db.close();
  }

  // DB ファイルのメディア件数。
  function mediaCount(dbPath: string): number {
    const db = new KijukuDB(dbPath, { backup: null });
    const n = db.findMedia({}).length;
    db.close();
    return n;
  }

  test('gate 合格: stg→prod 反映 + pre-stash（promote 前 prod）作成', async () => {
    const prod = path.join(dir, 'kijuku.db');
    const stg = path.join(dir, 'kijuku.stg.db');

    setupProd(prod, 2);
    await KijukuDB.replicateDb(prod, stg);

    const stgDb = new KijukuDB(stg, { backup: null });
    stgDb.createMedia({ title: 'new', media_type: 'comic' });

    expect(mediaCount(prod)).toBe(2);

    const outcome = await stgDb.promote(prod);
    expect(outcome.observe.passed).toBe(true);

    // prod に stg が反映（3件・"new" 含む）。
    expect(mediaCount(prod)).toBe(3);
    const prodDb = new KijukuDB(prod, { backup: null });
    const titles = prodDb.findMedia({}).map((m) => m.title);
    prodDb.close();
    expect(titles).toContain('new');

    // pre-stash は promote 前 prod（2件）= §8 即時復旧の戻し先。
    expect(outcome.preStashPath).toBeDefined();
    const preStash = outcome.preStashPath as string;
    expect(fs.existsSync(preStash)).toBe(true);
    expect(preStash).toMatch(/-pre_promote\.db$/);
    expect(mediaCount(preStash)).toBe(2);
    stgDb.close();
  });

  test('backupOpts で pre-stash 先（backupDir）をカスタマイズ（設計 §7.2・TASK-62）', async () => {
    const prod = path.join(dir, 'kijuku.db');
    const stg = path.join(dir, 'kijuku.stg.db');
    const customBackupDir = path.join(dir, 'custom-backup');

    setupProd(prod, 2);
    await KijukuDB.replicateDb(prod, stg);

    const stgDb = new KijukuDB(stg, { backup: null });
    stgDb.createMedia({ title: 'new', media_type: 'comic' });

    // pre-stash 先を custom-backup/ に指定（enabled:false で auto/manual/meta dir を作らず tmp/ のみ）。
    const outcome = await stgDb.promote(prod, {}, { backupDir: customBackupDir, enabled: false });
    expect(outcome.observe.passed).toBe(true);
    expect(mediaCount(prod)).toBe(3);

    // pre-stash は backupOpts.backupDir 配下に作られる。
    expect(outcome.preStashPath).toBeDefined();
    const preStash = outcome.preStashPath as string;
    expect(preStash.startsWith(customBackupDir)).toBe(true);
    expect(fs.existsSync(preStash)).toBe(true);
    expect(mediaCount(preStash)).toBe(2);
    stgDb.close();
  });

  test('promote 後: listPreStashes で該当 pre-stash を発見（preStashPath 喪失時の復旧経路・設計 §8）', async () => {
    const prod = path.join(dir, 'kijuku.db');
    const stg = path.join(dir, 'kijuku.stg.db');
    const customBackupDir = path.join(dir, 'custom-backup');

    setupProd(prod, 2);
    await KijukuDB.replicateDb(prod, stg);

    const stgDb = new KijukuDB(stg, { backup: null });
    stgDb.createMedia({ title: 'new', media_type: 'comic' });

    // pre-stash 先を custom-backup/ に指定（enabled:false で auto/manual/meta dir を作らず tmp/ のみ）。
    const outcome = await stgDb.promote(prod, {}, { backupDir: customBackupDir, enabled: false });
    expect(outcome.observe.passed).toBe(true);
    expect(outcome.preStashPath).toBeDefined();
    const pre = outcome.preStashPath as string;
    expect(fs.existsSync(pre)).toBe(true);

    // prod を同じ backupDir で開き直し、pre-stash 一覧から発見できる（preStashPath がなくても復旧可能）。
    const prodDb = new KijukuDB(prod, { backup: { backupDir: customBackupDir } });
    const stashes = prodDb.listPreStashes();

    // pre-stash は promote が作った1件のみ（setupProd は backup 無効で migrate の pre-stash 無し）。
    expect(stashes).toHaveLength(1);
    // 発見した path は promote が作った pre-stash と一致（byPath restore に直接渡せる）。
    expect(fs.realpathSync(stashes[0].path)).toBe(fs.realpathSync(pre));

    stgDb.close();
    prodDb.close();
  });

  test('gate 不合格: prod は一切触られない（maxAdded=0 で追加1件を拒否）', async () => {
    const prod = path.join(dir, 'kijuku.db');
    const stg = path.join(dir, 'kijuku.stg.db');

    setupProd(prod, 1);
    await KijukuDB.replicateDb(prod, stg);
    const stgDb = new KijukuDB(stg, { backup: null });
    stgDb.createMedia({ title: 'new', media_type: 'comic' });

    const opts: ObserveOptions = {
      gateConfig: { maxAdded: 0, maxRemoved: 0, maxChanged: 0, goldenAssertions: [] },
    };
    await expect(stgDb.promote(prod, opts)).rejects.toBeInstanceOf(PromoteGateFailedError);

    // prod は未更新（1件のまま）。
    expect(mediaCount(prod)).toBe(1);
    stgDb.close();
  });

  test('stg==prod の同一パスは誤設定として拒否', async () => {
    const stg = path.join(dir, 'kijuku.stg.db');
    const stgDb = new KijukuDB(stg, { backup: null });
    stgDb.migrate();

    await expect(stgDb.promote(stg)).rejects.toThrow(/same path/);
    stgDb.close();
  });
});
