/**
 * メディア追加属性のテスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB } from '../../src/index.js';

describe('MediaAttribute', () => {
  let db: KijukuDB;
  let mediaId: number;

  beforeEach(() => {
    db = new KijukuDB(':memory:');
    db.migrate();
    const media = db.createMedia({ title: 'テスト', media_type: 'comic' });
    mediaId = media.id;
  });

  afterEach(() => {
    db.close();
  });

  describe('setMediaAttribute', () => {
    test('属性を設定できる', () => {
      db.setMediaAttribute(mediaId, 'source_url', 'https://example.com');
      const attr = db.getMediaAttribute(mediaId, 'source_url');
      expect(attr).not.toBeNull();
      expect(attr!.value).toBe('https://example.com');
    });

    test('value_type を指定して設定できる', () => {
      db.setMediaAttribute(mediaId, 'page_count', '100', 'integer');
      const attr = db.getMediaAttribute(mediaId, 'page_count');
      expect(attr!.value_type).toBe('integer');
    });

    test('同じキーで再設定すると上書きされる', () => {
      db.setMediaAttribute(mediaId, 'note', '初期値');
      db.setMediaAttribute(mediaId, 'note', '更新値');
      const attr = db.getMediaAttribute(mediaId, 'note');
      expect(attr!.value).toBe('更新値');
    });

    test('value に null を設定できる', () => {
      db.setMediaAttribute(mediaId, 'note', null);
      const attr = db.getMediaAttribute(mediaId, 'note');
      expect(attr!.value).toBeNull();
    });
  });

  describe('getMediaAttribute', () => {
    test('存在しないキーは falsy を返す', () => {
      const attr = db.getMediaAttribute(mediaId, 'nonexistent');
      expect(attr).toBeFalsy();
    });

    test('media_id と key の両方が一致するものだけ返す', () => {
      const other = db.createMedia({ title: '別メディア', media_type: 'video' });
      db.setMediaAttribute(mediaId, 'note', 'A用');
      db.setMediaAttribute(other.id, 'note', 'B用');

      expect(db.getMediaAttribute(mediaId, 'note')!.value).toBe('A用');
      expect(db.getMediaAttribute(other.id, 'note')!.value).toBe('B用');
    });
  });

  describe('getMediaAttributes', () => {
    test('全属性をキー昇順で返す', () => {
      db.setMediaAttribute(mediaId, 'zzz', '最後');
      db.setMediaAttribute(mediaId, 'aaa', '最初');
      db.setMediaAttribute(mediaId, 'mmm', '中間');

      const attrs = db.getMediaAttributes(mediaId);
      expect(attrs).toHaveLength(3);
      expect(attrs.map(a => a.key)).toEqual(['aaa', 'mmm', 'zzz']);
    });

    test('属性がない場合は空配列を返す', () => {
      expect(db.getMediaAttributes(mediaId)).toHaveLength(0);
    });
  });

  describe('deleteMediaAttribute', () => {
    test('指定したキーの属性を削除できる', () => {
      db.setMediaAttribute(mediaId, 'key1', 'v1');
      db.setMediaAttribute(mediaId, 'key2', 'v2');
      db.deleteMediaAttribute(mediaId, 'key1');

      expect(db.getMediaAttribute(mediaId, 'key1')).toBeFalsy();
      expect(db.getMediaAttribute(mediaId, 'key2')).not.toBeNull();
    });
  });

  describe('deleteAllMediaAttributes', () => {
    test('メディアの全属性を削除できる', () => {
      db.setMediaAttribute(mediaId, 'key1', 'v1');
      db.setMediaAttribute(mediaId, 'key2', 'v2');
      db.deleteAllMediaAttributes(mediaId);

      expect(db.getMediaAttributes(mediaId)).toHaveLength(0);
    });

    test('他のメディアの属性は削除されない', () => {
      const other = db.createMedia({ title: '別', media_type: 'video' });
      db.setMediaAttribute(mediaId, 'key', 'val');
      db.setMediaAttribute(other.id, 'key', 'val');

      db.deleteAllMediaAttributes(mediaId);

      expect(db.getMediaAttributes(mediaId)).toHaveLength(0);
      expect(db.getMediaAttributes(other.id)).toHaveLength(1);
    });
  });
});
