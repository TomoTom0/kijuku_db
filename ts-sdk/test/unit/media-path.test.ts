/**
 * media_path モジュールの単体テスト
 */
import { describe, it, expect } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import {
  normalizeLexical,
  resolveWithinRoot,
  resolveDestinationWithinRoot,
  isProtected,
  trashDir,
  requireMediaRoot,
} from '../../src/media_path.js';

describe('normalizeLexical', () => {
  it('resolves . and ..', () => {
    expect(normalizeLexical('/a/./b/../c')).toBe('/a/c');
    expect(normalizeLexical('/a/b/../../c')).toBe('/c');
    // ルートより上には登らない
    expect(normalizeLexical('/../etc')).toBe('/etc');
  });
});

describe('resolveWithinRoot', () => {
  const root = '/media';

  it('accepts descendant', () => {
    expect(resolveWithinRoot(root, 'a/b.jpg')).toBe('/media/a/b.jpg');
  });

  it('rejects traversal', () => {
    expect(() => resolveWithinRoot(root, '../escape')).toThrow();
    expect(() => resolveWithinRoot(root, 'a/../../../etc/passwd')).toThrow();
  });

  it('rejects absolute outside', () => {
    expect(() => resolveWithinRoot(root, '/etc/passwd')).toThrow();
  });

  it('rejects sibling prefix', () => {
    // /media-evil は /media の配下ではない（ディレクトリ境界で判定）
    expect(() => resolveWithinRoot(root, '../media-evil/x')).toThrow();
  });
});

describe('isProtected', () => {
  it('detects trash', () => {
    const root = '/media';
    const trash = trashDir(root); // /media/.trash
    expect(isProtected(trash, [trash])).toBe(true);
    expect(isProtected(`${trash}/x`, [trash])).toBe(true);
    expect(isProtected('/media/a.jpg', [trash])).toBe(false);
  });
});

describe('requireMediaRoot', () => {
  it('errors when unset', () => {
    expect(() => requireMediaRoot(undefined)).toThrow();
    expect(() => requireMediaRoot('')).toThrow();
  });

  it('returns normalized root when set', () => {
    expect(requireMediaRoot('/media')).toBe('/media');
  });
});

describe('resolveDestinationWithinRoot', () => {
  it('accepts new descendant and rejects traversal (lexical)', () => {
    const root = '/media';
    expect(resolveDestinationWithinRoot(root, 'new/sub/x.jpg')).toBe('/media/new/sub/x.jpg');
    expect(() => resolveDestinationWithinRoot(root, '../escape')).toThrow();
  });

  it('rejects destination through escaping symlink', () => {
    const root = mkdtempSync(path.join(tmpdir(), 'kijuku-mp-'));
    const outside = mkdtempSync(path.join(tmpdir(), 'kijuku-out-'));
    try {
      // root/link -> outside（root 外）
      symlinkSync(outside, path.join(root, 'link'));
      // dst = link/new.db は lexical には root 配下だが symlink 先は root 外 → 拒否
      expect(() => resolveDestinationWithinRoot(root, 'link/new.db')).toThrow();
      // 多段: root/a/b/b-link -> outside
      mkdirSync(path.join(root, 'a/b'), { recursive: true });
      symlinkSync(outside, path.join(root, 'a/b/b-link'));
      expect(() => resolveDestinationWithinRoot(root, 'a/b/b-link/x')).toThrow();
      // 正常な root 内の新規パスは受理
      expect(resolveDestinationWithinRoot(root, 'plain/new.db')).toBe(
        `${root}/plain/new.db`
      );
    } finally {
      // rmSync は symlink を辿らずリンク自体を削除するので順序は問わない
      rmSync(root, { recursive: true, force: true });
      rmSync(outside, { recursive: true, force: true });
    }
  });
});
