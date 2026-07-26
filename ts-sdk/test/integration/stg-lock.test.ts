/**
 * stg 排他ロック + sync 元 revision 記録の統合テスト（設計 §15-11・TASK-55）。
 *
 * Rust 側 `rust-sdk/tests/stg_session_test.rs` と同等の検証を TS 側で保証する（SDK parity）。
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import {
  KijukuDB,
  acquireStgLock,
  readStgMeta,
  metaPath,
  StgBusyError,
} from '../../src/index.js';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

describe('stg 排他ロック + revision 記録 (TASK-55)', () => {
  let dir: string;

  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kijuku-stglock-'));
  });

  afterEach(() => {
    fs.rmSync(dir, { recursive: true, force: true });
  });

  function setupProd(n: number): { prod: string; stg: string } {
    const prod = path.join(dir, 'kijuku.db');
    const stg = path.join(dir, 'kijuku.stg.db');
    const prodDb = new KijukuDB(prod, { backup: null });
    prodDb.migrate();
    for (let i = 0; i < n; i++) {
      prodDb.createMedia({ title: `m${i}`, media_type: 'comic' });
    }
    prodDb.close();
    return { prod, stg };
  }

  test('sync 後 <stg>.meta.json に revision が記録される', async () => {
    const { prod, stg } = setupProd(3);
    await KijukuDB.replicateDb(prod, stg);

    expect(fs.existsSync(metaPath(stg))).toBe(true);
    const meta = readStgMeta(stg);
    expect(meta).toBeDefined();
    expect(meta!.syncedFrom.prodPath).toBe(prod);
    expect(meta!.syncedFrom.revision.mediaCount).toBe(3);
    expect(meta!.syncedFrom.revision.mediaMaxId).toBe(3);
    expect(meta!.syncedFrom.revision.schemaVersion).toBe(6);
  });

  test('排他ロック: 2回目の取得は StgBusyError・解放後に再取得可能', () => {
    const stg = path.join(dir, 'kijuku.stg.db');
    const lock1 = acquireStgLock(stg);
    expect(() => acquireStgLock(stg)).toThrow(StgBusyError);
    lock1.release();
    // 解放後は再取得可能
    const lock2 = acquireStgLock(stg);
    expect(lock2.lockPath).toBe(`${stg}.lock`);
    lock2.release();
  });

  test('sync 中（ロック保持中）の sync は StgBusyError', async () => {
    const { prod, stg } = setupProd(2);
    const held = acquireStgLock(stg); // 編集中セッションを模擬
    try {
      await expect(KijukuDB.replicateDb(prod, stg)).rejects.toThrow(StgBusyError);
    } finally {
      held.release();
    }
  });

  test('observe: prod 不変更時 prod_sync_revision gate 合格', async () => {
    const { prod, stg } = setupProd(2);
    await KijukuDB.replicateDb(prod, stg);

    const stgDb = new KijukuDB(stg, { backup: null });
    const result = stgDb.observe(prod);
    const rev = result.checks.find((c) => c.name === 'prod_sync_revision');
    expect(rev).toBeDefined();
    expect(rev!.passed).toBe(true);
    stgDb.close();
  });

  test('observe: prod が sync 後に更新されると gate 不合格・再 sync で復帰', async () => {
    const { prod, stg } = setupProd(2);
    await KijukuDB.replicateDb(prod, stg);

    // prod を sync 後に変更（drift 発生）
    const prodDb = new KijukuDB(prod, { backup: null });
    prodDb.createMedia({ title: 'drift', media_type: 'comic' });
    prodDb.close();

    const stgDb = new KijukuDB(stg, { backup: null });
    let result = stgDb.observe(prod);
    let rev = result.checks.find((c) => c.name === 'prod_sync_revision');
    expect(rev!.passed).toBe(false);
    stgDb.close();

    // 再 sync で revision 更新 -> 合格に復帰
    await KijukuDB.replicateDb(prod, stg);
    const stgDb2 = new KijukuDB(stg, { backup: null });
    result = stgDb2.observe(prod);
    rev = result.checks.find((c) => c.name === 'prod_sync_revision');
    expect(rev!.passed).toBe(true);
    stgDb2.close();
  });
});
