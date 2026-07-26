/**
 * file_ops モジュールの単体テスト
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
  existsSync,
  symlinkSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { mediaCp, mediaMv, mediaSync } from '../../src/file_ops.js';
import { listTrash } from '../../src/trash.js';

function setup(): string {
  const root = mkdtempSync(path.join(tmpdir(), 'kijuku-fileops-'));
  mkdirSync(path.join(root, 'src/sub'), { recursive: true });
  writeFileSync(path.join(root, 'src/a.txt'), 'A');
  writeFileSync(path.join(root, 'src/sub/b.txt'), 'B');
  writeFileSync(path.join(root, 'lonely.txt'), 'L');
  return root;
}

describe('file_ops', () => {
  let root: string;

  beforeEach(() => {
    root = setup();
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  it('cp dry-run does not write', () => {
    const r = mediaCp(root, 'lonely.txt', 'copied.txt');
    expect(r.applied).toBe(false);
    expect(existsSync(path.join(root, 'copied.txt'))).toBe(false);
  });

  it('cp apply copies file', () => {
    const r = mediaCp(root, 'lonely.txt', 'copied.txt', { apply: true, updateDb: false });
    expect(r.applied).toBe(true);
    expect(existsSync(path.join(root, 'copied.txt'))).toBe(true);
    // src は残る（copy）
    expect(existsSync(path.join(root, 'lonely.txt'))).toBe(true);
  });

  it('cp overwrite trashes existing', () => {
    writeFileSync(path.join(root, 'dst.txt'), 'OLD');
    mediaCp(root, 'lonely.txt', 'dst.txt', { apply: true, updateDb: false });
    expect(readFileSync(path.join(root, 'dst.txt'), 'utf-8')).toBe('L');
    expect(listTrash(root)).toHaveLength(1);
  });

  it('mv apply moves file', () => {
    mediaMv(root, 'lonely.txt', 'moved.txt', { apply: true, updateDb: false });
    expect(existsSync(path.join(root, 'moved.txt'))).toBe(true);
    expect(existsSync(path.join(root, 'lonely.txt'))).toBe(false);
  });

  it('mv overwrite trashes existing', () => {
    writeFileSync(path.join(root, 'dst.txt'), 'OLD');
    mediaMv(root, 'lonely.txt', 'dst.txt', { apply: true, updateDb: false });
    expect(readFileSync(path.join(root, 'dst.txt'), 'utf-8')).toBe('L');
    expect(listTrash(root)).toHaveLength(1);
  });

  it('sync safe mode trashes extras and copies', () => {
    mkdirSync(path.join(root, 'dst'), { recursive: true });
    writeFileSync(path.join(root, 'dst/a.txt'), 'OLD-A');
    writeFileSync(path.join(root, 'dst/extra.txt'), 'EXTRA');

    const r = mediaSync(root, 'src', 'dst', { apply: true, updateDb: false });
    expect(r.applied).toBe(true);

    expect(readFileSync(path.join(root, 'dst/a.txt'), 'utf-8')).toBe('A');
    expect(existsSync(path.join(root, 'dst/extra.txt'))).toBe(false);
    // extra.txt（余分）と a.txt（旧内容）の2件が trash へ
    expect(listTrash(root)).toHaveLength(2);
    expect(existsSync(path.join(root, 'dst/sub/b.txt'))).toBe(true);
  });

  it('rejects protected trash as target', () => {
    expect(() => mediaCp(root, 'lonely.txt', '.trash', { apply: true, updateDb: false })).toThrow();
  });

  it('rejects outside root', () => {
    expect(() => mediaCp(root, '../escape.txt', 'x')).toThrow();
    expect(() => mediaCp(root, 'lonely.txt', '../escape.txt')).toThrow();
  });

  it('rejects dst through escaping symlink', () => {
    // root/link -> outside（root 外）。dst = link/x は lexical には root 配下だが脱出する。
    const outside = mkdtempSync(path.join(tmpdir(), 'kijuku-out-'));
    try {
      symlinkSync(outside, path.join(root, 'link'));
      expect(() => mediaCp(root, 'lonely.txt', 'link/x')).toThrow();
      expect(() => mediaMv(root, 'lonely.txt', 'link/x')).toThrow();
      expect(() => mediaCp(root, 'lonely.txt', 'link/x', { apply: true, updateDb: false })).toThrow();
      // 外部へ書き込まれていない
      expect(existsSync(path.join(outside, 'x'))).toBe(false);
    } finally {
      rmSync(outside, { recursive: true, force: true });
    }
  });
});
