/**
 * diffWithProd（prod RO / stg 差分）の統合テスト（設計 §4.4・TASK-53）。
 *
 * Rust 側 `rust-sdk/tests/diff_prod_stg_test.rs` と同等のセマンティクス反転検証を
 * TS 側で保証する（SDK parity）。stg 編集視点（added=stg新規=promoteでprod追加 等）。
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB } from '../../src/index.js';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

describe('KijukuDB.diffWithProd (prod RO / stg 差分・TASK-53)', () => {
  let dir: string;

  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kijuku-diffps-'));
  });

  afterEach(() => {
    fs.rmSync(dir, { recursive: true, force: true });
  });

  test('sync 直後はゼロ差分', async () => {
    const prod = path.join(dir, 'kijuku.db');
    const stg = path.join(dir, 'kijuku.stg.db');
    const prodDb = new KijukuDB(prod, { backup: null });
    prodDb.migrate();
    prodDb.createMedia({ title: 'P1', media_type: 'comic' });
    prodDb.close();
    await KijukuDB.replicateDb(prod, stg);

    const stgDb = new KijukuDB(stg, { backup: null });
    const diff = stgDb.diffWithProd(prod);
    expect(diff.summary.media.added).toBe(0);
    expect(diff.summary.media.removed).toBe(0);
    expect(diff.summary.media.changed).toBe(0);
    stgDb.close();
  });

  test('stg 新規追加は added（セマンティクス反転・promote 対象）', async () => {
    const prod = path.join(dir, 'kijuku.db');
    const stg = path.join(dir, 'kijuku.stg.db');
    const prodDb = new KijukuDB(prod, { backup: null });
    prodDb.migrate();
    prodDb.createMedia({ title: 'P1', media_type: 'comic' });
    prodDb.close();
    await KijukuDB.replicateDb(prod, stg);

    const stgDb = new KijukuDB(stg, { backup: null });
    stgDb.createMedia({ title: 'stg新規', media_type: 'comic' });
    const diff = stgDb.diffWithProd(prod);
    expect(diff.summary.media.added).toBe(1);
    expect(diff.summary.media.removed).toBe(0);
    expect(diff.summary.media.changed).toBe(0);
    stgDb.close();
  });

  test('stg で変更/削除/新規が changed/removed/added になる（完全反転検証）', async () => {
    const prod = path.join(dir, 'kijuku.db');
    const stg = path.join(dir, 'kijuku.stg.db');
    const prodDb = new KijukuDB(prod, { backup: null });
    prodDb.migrate();
    const a = prodDb.createMedia({ title: 'A', media_type: 'comic' });
    prodDb.createMedia({ title: 'B', media_type: 'comic' });
    prodDb.close();
    await KijukuDB.replicateDb(prod, stg);

    const stgDb = new KijukuDB(stg, { backup: null });
    stgDb.updateMedia(a.id, { title: 'A-edited', media_type: 'comic' }); // changed
    const b = stgDb.findMedia({}).find((m) => m.title === 'B');
    if (b) stgDb.deleteMedia(b.id); // removed（prod のみ残る）
    stgDb.createMedia({ title: 'C-new', media_type: 'comic' }); // added

    const diff = stgDb.diffWithProd(prod);
    expect(diff.summary.media.added).toBe(1);
    expect(diff.summary.media.removed).toBe(1);
    expect(diff.summary.media.changed).toBe(1);
    stgDb.close();
  });
});
