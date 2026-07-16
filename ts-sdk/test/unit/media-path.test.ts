/**
 * media_path モジュールの単体テスト
 */
import { describe, it, expect } from 'vitest';
import {
  normalizeLexical,
  resolveWithinRoot,
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
