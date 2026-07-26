/**
 * trash モジュールの単体テスト
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  rmSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import {
  moveToTrash,
  listTrash,
  restoreFromTrash,
  purgeTrash,
} from '../../src/trash.js';

function setupRoot(): string {
  const root = mkdtempSync(path.join(tmpdir(), 'kijuku-trash-'));
  mkdirSync(path.join(root, 'a/b'), { recursive: true });
  writeFileSync(path.join(root, 'a/b/c.jpg'), 'content');
  writeFileSync(path.join(root, 'top.txt'), 'top');
  return root;
}

describe('trash', () => {
  let root: string;

  beforeEach(() => {
    root = setupRoot();
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  it('move/list/restore roundtrip', () => {
    const id = moveToTrash(root, 'a/b/c.jpg', 'delete', 'test');
    // 元の場所からは消える
    expect(existsSync(path.join(root, 'a/b/c.jpg'))).toBe(false);

    const entries = listTrash(root);
    expect(entries).toHaveLength(1);
    expect(entries[0].meta.originalPath).toBe('a/b/c.jpg');

    const restored = restoreFromTrash(root, id);
    expect(restored).toBe(path.join(root, 'a/b/c.jpg'));
    expect(existsSync(path.join(root, 'a/b/c.jpg'))).toBe(true);
    // 復元後は trash エントリが掃除される
    expect(listTrash(root)).toHaveLength(0);
  });

  it('purge dry-run then apply', () => {
    const id = moveToTrash(root, 'top.txt', 'delete');
    const trashPath = path.join(root, '.trash', id);

    // dry-run では実体は残る
    expect(purgeTrash(root, undefined, true)).toEqual([id]);
    expect(existsSync(trashPath)).toBe(true);

    // apply で物理削除
    expect(purgeTrash(root, undefined, false)).toEqual([id]);
    expect(existsSync(trashPath)).toBe(false);
  });

  it('purge rejects traversal ids', () => {
    // 呼出し側提供 ID にトラバーサル成分があれば trash 外へ脱出できない（dry-run/apply 両方）。
    const realId = moveToTrash(root, 'top.txt', 'delete');
    const realEntryPath = path.join(root, '.trash', realId);

    const malicious = [
      '..',
      '../escape',
      '../../important',
      '/etc/passwd',
      'a/b',
      '.',
      '',
      `${realId}/sub`,
    ];
    for (const id of malicious) {
      expect(() => purgeTrash(root, [id], true), `dry-run reject: ${id}`).toThrow();
      expect(() => purgeTrash(root, [id], false), `apply reject: ${id}`).toThrow();
    }
    // 攻撃 ID では削除が起きず、正常エントリは残る
    expect(existsSync(realEntryPath)).toBe(true);
    // 正常 ID は削除できる
    expect(purgeTrash(root, [realId], false)).toEqual([realId]);
    expect(existsSync(realEntryPath)).toBe(false);
  });

  it('rejects trash itself', () => {
    moveToTrash(root, 'top.txt', 'delete');
    expect(() => moveToTrash(root, '.trash', 'delete')).toThrow();
  });

  it('rejects outside root', () => {
    expect(() => moveToTrash(root, '../escape.txt', 'delete')).toThrow();
    expect(() => moveToTrash(root, '/etc/passwd', 'delete')).toThrow();
  });

  it('errors when not found', () => {
    expect(() => moveToTrash(root, 'nope.txt', 'delete')).toThrow();
  });

  it('restore errors on collision', () => {
    const id = moveToTrash(root, 'top.txt', 'delete');
    // 元位置に別ファイルを復活させる
    writeFileSync(path.join(root, 'top.txt'), 'new');
    expect(() => restoreFromTrash(root, id)).toThrow();
    // trash エントリは残る
    expect(listTrash(root)).toHaveLength(1);
  });

  it('restore errors on unknown id', () => {
    expect(() => restoreFromTrash(root, 'nope')).toThrow();
  });
});
