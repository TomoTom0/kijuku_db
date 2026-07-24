/**
 * readonly セッションの書込拒否ガードのテスト（本番DB保護 P1・TASK-52 T4）
 *
 * 設計 docs/design/db-protection.md §5.1（prod readonly）/§6.2。
 * Rust cli.rs の is_write_operation ガードと同等: SDK 直接呼出でも readonly セッションでの
 * 書込操作を事前に拒否する（SQLite SQLITE_READONLY より前のユーザフレンドリなエラー）。
 *
 * SDK 主利用経路（ローカル KijukuDB 直接呼出）は CLI を経由しないため、CLI 側ガードだけでなく
 * この SDK 側ガードが本番DB保護の必須構成要素となる（方式A・TASK-52）。
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { KijukuDB } from '../../src/index.js';

let dirs: string[] = [];
let db: KijukuDB;

afterEach(() => {
  db?.close?.();
  for (const d of dirs) rmSync(d, { recursive: true, force: true });
  dirs = [];
});

/** RW で migrate + 軽データ投入した DB を readonly で開く。 */
function openReadOnly(): KijukuDB {
  const dir = mkdtempSync(path.join(tmpdir(), 'kijuku-ro-guard-'));
  dirs.push(dir);
  const dbPath = path.join(dir, 'test.db');
  const rw = new KijukuDB(dbPath, { backup: null });
  rw.migrate();
  rw.createMedia({ title: '作品', media_type: 'comic' });
  rw.close();
  return new KijukuDB(dbPath, { readonly: true, backup: null });
}

describe('readonly セッションの書込拒否ガード（設計 §5.1/§6.2・TASK-52 T4）', () => {
  beforeEach(() => {
    db = openReadOnly();
  });

  it('同期書込操作は拒否される（migrate/createMedia/updateMedia/deleteMedia/createTag/setBackupLabel）', () => {
    expect(() => db.migrate()).toThrow(/readonly/);
    expect(() => db.createMedia({ title: 'x', media_type: 'comic' })).toThrow(/readonly/);
    expect(() => db.updateMedia(1, { title: 'y' })).toThrow(/readonly/);
    expect(() => db.deleteMedia(1)).toThrow(/readonly/);
    expect(() => db.createTag('t')).toThrow(/readonly/);
    expect(() => db.setBackupLabel('dummy', 'lbl')).toThrow(/readonly/);
  });

  it('非同期書込操作は拒否される（backup/backupWithLabel）', async () => {
    await expect(db.backup()).rejects.toThrow(/readonly/);
    await expect(db.backupWithLabel('lbl')).rejects.toThrow(/readonly/);
  });

  it('ファイル操作系書込は拒否される（mediaRoot 未設定でも assertWritable が先に効く）', () => {
    expect(() => db.mediaCp('a', 'b')).toThrow(/readonly/);
    expect(() => db.moveToTrash('a', 'delete')).toThrow(/readonly/);
  });

  it('読込操作は成功する（書込拒否ガードの対象外）', () => {
    expect(() => db.getSchemaVersion()).not.toThrow();
    expect(() => db.getAllTags()).not.toThrow();
    expect(() => db.getMedia(1)).not.toThrow();
  });
});

describe('(b) 制限操作は stg(Local) で拒否される（設計 §9.2・TASK-59 P2-C4）', () => {
  beforeEach(() => {
    // writable（stg）Local
    const dir = mkdtempSync(path.join(tmpdir(), 'kijuku-b-guard-'));
    dirs.push(dir);
    const dbPath = path.join(dir, 'test.db');
    db = new KijukuDB(dbPath, { backup: null });
    db.migrate();
  });

  it('mediaMv/purgeTrash/restore は (b) 制限操作として stg で拒否される', () => {
    // assertNotStgRestricted が mediaRoot 解決より先に効く
    expect(() => db.mediaMv('a', 'b')).toThrow(/制限操作.*mediaMv/);
    expect(() => db.purgeTrash()).toThrow(/制限操作.*purgeTrash/);
    expect(() => db.restore()).toThrow(/制限操作.*restore/);
  });

  it('(a) 許可操作は stg で実行可能（mediaCp/moveToTrash は制限対象外）', () => {
    // mediaCp/moveToTrash は (a): assertWritable は通る（mediaRoot 未設定なら別エラーだが (b) 拒否ではない）
    expect(() => db.mediaCp('a', 'b')).not.toThrow(/制限操作/);
    expect(() => db.moveToTrash('a', 'delete')).not.toThrow(/制限操作/);
  });
});
