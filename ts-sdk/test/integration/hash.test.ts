/**
 * ハッシュ機能のテスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB } from '../../src/index.js';
import { hexToBytes, bytesToHex } from '../../src/hash.js';
import fs from 'fs';
import path from 'path';
import os from 'os';

describe('Hash Operations', () => {
  let db: KijukuDB;

  beforeEach(() => {
    db = new KijukuDB(':memory:');
    db.migrate();
  });

  afterEach(() => {
    db.close();
  });

  function createTestMedia(overrides: Record<string, unknown> = {}) {
    return db.createMedia({
      title: 'test',
      media_type: 'music',
      ...overrides,
    });
  }

  describe('addMediaHash / getMediaHash', () => {
    test('ハッシュを登録して取得できる', () => {
      const media = createTestMedia();
      const hash = db.addMediaHash({
        item_uuid: media.uuid,
        filename: '',
        time_range: '',
        content_hash: new Uint8Array(32).fill(0xab),
      });

      expect(hash.item_uuid).toBe(media.uuid);
      expect(hash.filename).toBe('');
      expect(hash.content_hash).toEqual(new Uint8Array(32).fill(0xab));

      const got = db.getMediaHash(media.uuid, '', '');
      expect(got).not.toBeNull();
      expect(got!.content_hash).toEqual(new Uint8Array(32).fill(0xab));
    });

    test('存在しないハッシュはnullを返す', () => {
      const got = db.getMediaHash('nonexistent', '', '');
      expect(got).toBeNull();
    });
  });

  describe('addMediaHashes', () => {
    test('一括登録できる', () => {
      const media = createTestMedia();
      const results = db.addMediaHashes([
        {
          item_uuid: media.uuid,
          filename: '',
          time_range: '',
          content_hash: new Uint8Array(32).fill(1),
        },
        {
          item_uuid: media.uuid,
          filename: '001.jpg',
          time_range: '',
          content_hash: new Uint8Array(32).fill(2),
        },
      ]);
      expect(results).toHaveLength(2);

      const all = db.getMediaHashes(media.uuid);
      expect(all).toHaveLength(2);
    });
  });

  describe('findByContentHash', () => {
    test('SHA256で検索できる', () => {
      const media = createTestMedia();
      db.addMediaHash({
        item_uuid: media.uuid,
        filename: '',
        time_range: '',
        content_hash: new Uint8Array(32).fill(0xff),
      });

      const found = db.findByContentHash(new Uint8Array(32).fill(0xff));
      expect(found).toHaveLength(1);
      expect(found[0].item_uuid).toBe(media.uuid);

      const notFound = db.findByContentHash(new Uint8Array(32).fill(0x00));
      expect(notFound).toHaveLength(0);
    });
  });

  describe('deleteMediaHash', () => {
    test('ハッシュを削除できる', () => {
      const media = createTestMedia();
      db.addMediaHash({
        item_uuid: media.uuid,
        filename: '001.jpg',
        time_range: '',
        content_hash: new Uint8Array(32).fill(1),
      });

      db.deleteMediaHash(media.uuid, '001.jpg', '');
      expect(db.getMediaHash(media.uuid, '001.jpg', '')).toBeNull();
    });

    test('代替行の連鎖削除', () => {
      const media = createTestMedia();
      // 原本
      db.addMediaHash({
        item_uuid: media.uuid,
        filename: '001.jpg',
        time_range: '',
        content_hash: new Uint8Array(32).fill(1),
      });
      // 代替
      db.addMediaHash({
        item_uuid: media.uuid,
        filename: '001.png',
        time_range: '',
        content_hash: new Uint8Array(32).fill(2),
        alternative_of: '001.jpg',
      });

      expect(db.getMediaHashes(media.uuid)).toHaveLength(2);

      db.deleteMediaHash(media.uuid, '001.jpg', '');
      expect(db.getMediaHashes(media.uuid)).toHaveLength(0);
    });
  });

  describe('deleteMediaHashes', () => {
    test('作品のハッシュを全削除できる', () => {
      const media = createTestMedia();
      db.addMediaHashes([
        { item_uuid: media.uuid, filename: '', time_range: '', content_hash: new Uint8Array(32).fill(1) },
        { item_uuid: media.uuid, filename: '001.jpg', time_range: '', content_hash: new Uint8Array(32).fill(2) },
      ]);

      db.deleteMediaHashes(media.uuid);
      expect(db.getMediaHashes(media.uuid)).toHaveLength(0);
    });
  });

  describe('findDuplicateHashes', () => {
    test('重複ハッシュを検出できる', () => {
      const m1 = createTestMedia({ title: 'test1' });
      const m2 = db.createMedia({ title: 'test2', media_type: 'music' });

      db.addMediaHash({
        item_uuid: m1.uuid, filename: '', time_range: '',
        content_hash: new Uint8Array(32).fill(0xff),
      });
      db.addMediaHash({
        item_uuid: m2.uuid, filename: '', time_range: '',
        content_hash: new Uint8Array(32).fill(0xff),
      });

      const dupes = db.findDuplicateHashes();
      expect(dupes).toHaveLength(1);
      expect(dupes[0].count).toBe(2);
    });
  });

  describe('upsert', () => {
    test('同じPKでupsertできる', () => {
      const media = createTestMedia();
      db.addMediaHash({
        item_uuid: media.uuid, filename: '', time_range: '',
        content_hash: new Uint8Array(32).fill(1),
      });
      db.addMediaHash({
        item_uuid: media.uuid, filename: '', time_range: '',
        content_hash: new Uint8Array(32).fill(2),
      });

      const all = db.getMediaHashes(media.uuid);
      expect(all).toHaveLength(1);
      expect(all[0].content_hash).toEqual(new Uint8Array(32).fill(2));
    });
  });

  describe('CASCADE delete', () => {
    test('media削除でハッシュも削除される', () => {
      const media = createTestMedia();
      db.addMediaHash({
        item_uuid: media.uuid, filename: '', time_range: '',
        content_hash: new Uint8Array(32).fill(1),
      });
      expect(db.getMediaHashes(media.uuid)).toHaveLength(1);

      db.deleteMedia(media.id);
      expect(db.getMediaHashes(media.uuid)).toHaveLength(0);
    });
  });

  describe('hexToBytes / bytesToHex', () => {
    test('HEX <-> bytes変換が正しい', () => {
      const bytes = new Uint8Array([0xab, 0xcd, 0xef, 0x01]);
      const hex = bytesToHex(bytes);
      expect(hex).toBe('abcdef01');

      const back = hexToBytes(hex);
      expect(back).toEqual(bytes);
    });

    test('奇数長HEXはエラー', () => {
      expect(() => hexToBytes('abc')).toThrow();
    });
  });

  describe('computeMediaHash', () => {
    let tmpDir: string;

    beforeEach(() => {
      tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'kijuku-hash-test-'));
    });

    afterEach(() => {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    });

    test('Music: 全体+先頭30秒の2件が登録される', () => {
      const filePath = path.join(tmpDir, 'test.mp3');
      fs.writeFileSync(filePath, Buffer.alloc(1024));

      const media = createTestMedia({ path: filePath });
      const result = db.computeMediaHash(media.uuid, filePath, 'music', 180);

      expect(result.skipped).toBe(false);
      expect(result.hashes).toHaveLength(2);
      expect(result.hashes[0].time_range).toBe('');
      expect(result.hashes[1].time_range).toBe('0.0-30.0');
    });

    test('Video: 全体のみ1件が登録される', () => {
      const filePath = path.join(tmpDir, 'test.mp4');
      fs.writeFileSync(filePath, Buffer.alloc(2048));

      const media = createTestMedia({ media_type: 'video', path: filePath });
      const result = db.computeMediaHash(media.uuid, filePath, 'video');

      expect(result.skipped).toBe(false);
      expect(result.hashes).toHaveLength(1);
    });

    test('Comic: 各ページ+全体が登録される', () => {
      fs.writeFileSync(path.join(tmpDir, '001.jpg'), Buffer.alloc(100));
      fs.writeFileSync(path.join(tmpDir, '002.jpg'), Buffer.alloc(100));

      const media = createTestMedia({ media_type: 'comic', path: tmpDir });
      const result = db.computeMediaHash(media.uuid, tmpDir, 'comic');

      expect(result.skipped).toBe(false);
      // 2ページ + 全体 = 3
      expect(result.hashes).toHaveLength(3);
    });

    test('存在しないパスはスキップ', () => {
      const result = db.computeMediaHash('nonexistent-uuid', '/nonexistent', 'music');
      expect(result.skipped).toBe(true);
    });

    test('サイズ0のファイルはスキップ', () => {
      const filePath = path.join(tmpDir, 'empty.mp3');
      fs.writeFileSync(filePath, Buffer.alloc(0));

      const result = db.computeMediaHash('some-uuid', filePath, 'music');
      expect(result.skipped).toBe(true);
    });
  });
});
