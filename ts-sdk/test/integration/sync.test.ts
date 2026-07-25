/**
 * sync（prod→stg フル複製）のテスト（設計 §4.2/§4.5・TASK-50 C4）。
 *
 * `KijukuDB.replicateDb` が prod(RO)→stg(RW) の完全な複製を行うこと、
 * 既存 stg の上書きが prod で完全になること（WAL/SHM 副産物の削除込み）、
 * `src === dst` を誤設定として弾くことを検証する。
 * Rust の `rust-sdk/tests/sync_test.rs` と同等の検証を TS 側で保証する（SDK parity）。
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB } from '../../src/index.js';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

describe('KijukuDB.replicateDb (sync: prod -> stg)', () => {
  let dir: string;

  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kijuku-sync-'));
  });

  afterEach(() => {
    fs.rmSync(dir, { recursive: true, force: true });
  });

  test('prod(RO)->stg(RW) のフル複製がデータ・スキーマ・バージョンをコピーする', async () => {
    const prod = path.join(dir, 'kijuku.db');
    const stg = path.join(dir, 'kijuku.stg.db');

    // backup は本テストの対象外なので無効化（ノイズ回避）
    const prodDb = new KijukuDB(prod, { backup: null });
    prodDb.migrate();
    prodDb.createMedia({ title: 'prod メディア1', media_type: 'comic' });
    prodDb.createMedia({ title: 'prod メディア2', media_type: 'comic' });
    const prodVersion = prodDb.getSchemaVersion();
    prodDb.close();

    expect(fs.existsSync(stg)).toBe(false);

    await KijukuDB.replicateDb(prod, stg);

    // stg を開いて prod と一致することを検証
    expect(fs.existsSync(stg)).toBe(true);
    const stgDb = new KijukuDB(stg, { backup: null });
    expect(stgDb.getSchemaVersion()).toBe(prodVersion);
    const all = stgDb.findMedia({});
    expect(all).toHaveLength(2);
    // 順序に依存しないようソートして比較
    expect(all.map((m) => m.title).sort()).toEqual(['prod メディア1', 'prod メディア2']);
    stgDb.close();
  });

  test('既存 stg（stg 側の追加分を含む）の再 sync が prod で完全に上書きする', async () => {
    const prod = path.join(dir, 'kijuku.db');
    const stg = path.join(dir, 'kijuku.stg.db');

    const prodDb = new KijukuDB(prod, { backup: null });
    prodDb.migrate();
    prodDb.createMedia({ title: 'prod メディア1', media_type: 'comic' });
    prodDb.close();

    // 1回目の sync
    await KijukuDB.replicateDb(prod, stg);
    expect(fs.existsSync(stg)).toBe(true);

    // stg 側で独自にレコードを追加（LLM 編集のシミュレート）
    const stgDb = new KijukuDB(stg, { backup: null });
    stgDb.createMedia({
      title: 'stg 側の追加分（再 sync で消えるべき）',
      media_type: 'comic',
    });
    stgDb.close();

    // 再 sync（stg の WAL/SHM 副産物が残り得る状態からの上書き）
    await KijukuDB.replicateDb(prod, stg);

    // 再 sync 後: stg は prod と完全一致（stg 固有の追加分は削除され、prod データのみ残る）
    const stgDb2 = new KijukuDB(stg, { backup: null });
    const all = stgDb2.findMedia({});
    expect(all).toHaveLength(1);
    expect(all[0].title).toBe('prod メディア1');
    stgDb2.close();
  });

  test('src === dst は誤設定としてエラー', async () => {
    const same = path.join(dir, 'same.db');
    await expect(KijukuDB.replicateDb(same, same)).rejects.toThrow(/same path/);
  });

  test('discard（stg 破棄・再 sync・設計 §4.6）: stg を prod で上書き復元し prod は無傷', async () => {
    // discard は gate 不合格等で stg を捨てて prod から再構築する経路。処理は sync と同一
    // （`replicateDb(prod, stg)`）。ここでは discard の意味論（stg 編集の破棄 + prod 無傷）を検証。
    const prod = path.join(dir, 'kijuku.db');
    const stg = path.join(dir, 'kijuku.stg.db');

    const prodDb = new KijukuDB(prod, { backup: null });
    prodDb.migrate();
    prodDb.createMedia({ title: 'prod メディア1', media_type: 'comic' });
    prodDb.createMedia({ title: 'prod メディア2', media_type: 'comic' });
    prodDb.close();

    // 1. sync（書込セッション開始・§6.1）
    await KijukuDB.replicateDb(prod, stg);

    // 2. stg に LLM 編集（promote せず破棄するシナリオのシミュレート）
    const stgDb = new KijukuDB(stg, { backup: null });
    stgDb.createMedia({ title: 'stg 側の破棄される編集', media_type: 'comic' });
    stgDb.close();

    // prod の状態をキャプチャ（discard 前後で「prod は一切触られない」§4.6 を検証するため）
    const readProdTitles = (): string[] => {
      const db = new KijukuDB(prod, { backup: null });
      const titles = db
        .findMedia({})
        .map((m) => m.title)
        .sort();
      db.close();
      return titles;
    };
    const prodTitlesBefore = readProdTitles();

    // 3. discard（stg を破棄して prod から再 sync・§4.6）。処理は `replicateDb(prod, stg)` と同一。
    await KijukuDB.replicateDb(prod, stg);

    // 4. stg は prod で上書き復元（stg 固有の編集は破棄され prod データのみ残る）
    const stgDb2 = new KijukuDB(stg, { backup: null });
    const stgTitles = stgDb2
      .findMedia({})
      .map((m) => m.title)
      .sort();
    expect(stgTitles).toEqual(['prod メディア1', 'prod メディア2']);
    stgDb2.close();

    // 5. prod は discard 前後で無傷（§4.6・prod は一切触られない）
    const prodTitlesAfter = readProdTitles();
    expect(prodTitlesAfter).toEqual(prodTitlesBefore);
    expect(prodTitlesAfter).toEqual(['prod メディア1', 'prod メディア2']);
  });
});
