/**
 * タグ管理のテスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB } from '../../src/index.js';

describe('Tag', () => {
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

  describe('createTag', () => {
    test('タグを作成できる', () => {
      const tag = db.createTag('アクション');
      expect(tag.id).toBeGreaterThan(0);
      expect(tag.name).toBe('アクション');
    });

    test('前後の空白はトリムされる', () => {
      const tag = db.createTag('  ファンタジー  ');
      expect(tag.name).toBe('ファンタジー');
    });

    test('重複名のタグはエラー', () => {
      db.createTag('SF');
      expect(() => db.createTag('SF')).toThrow();
    });

    test('空文字はエラー', () => {
      expect(() => db.createTag('')).toThrow();
      expect(() => db.createTag('   ')).toThrow();
    });
  });

  describe('getTagByName', () => {
    test('名前でタグを取得できる', () => {
      db.createTag('ホラー');
      const tag = db.getTagByName('ホラー');
      expect(tag).not.toBeNull();
      expect(tag!.name).toBe('ホラー');
    });

    test('存在しない名前は null を返す', () => {
      expect(db.getTagByName('存在しない')).toBeNull();
    });
  });

  describe('getAllTags', () => {
    test('全タグを名前昇順で返す', () => {
      db.createTag('zzz');
      db.createTag('aaa');
      db.createTag('mmm');
      const tags = db.getAllTags();
      expect(tags.map(t => t.name)).toEqual(['aaa', 'mmm', 'zzz']);
    });

    test('タグがない場合は空配列', () => {
      expect(db.getAllTags()).toHaveLength(0);
    });
  });

  describe('addTagToMedia / getMediaTags', () => {
    test('メディアにタグを追加して取得できる', () => {
      const tag = db.createTag('アクション');
      db.addTagToMedia(mediaId, tag.id);
      const tags = db.getMediaTags(mediaId);
      expect(tags).toHaveLength(1);
      expect(tags[0].name).toBe('アクション');
    });

    test('同じタグを二重に追加しても冪等', () => {
      const tag = db.createTag('アクション');
      db.addTagToMedia(mediaId, tag.id);
      db.addTagToMedia(mediaId, tag.id);
      expect(db.getMediaTags(mediaId)).toHaveLength(1);
    });

    test('存在しないタグIDを追加するとエラー', () => {
      expect(() => db.addTagToMedia(mediaId, 99999)).toThrow();
    });
  });

  describe('removeTagFromMedia', () => {
    test('メディアからタグを削除できる', () => {
      const tag = db.createTag('SF');
      db.addTagToMedia(mediaId, tag.id);
      db.removeTagFromMedia(mediaId, tag.id);
      expect(db.getMediaTags(mediaId)).toHaveLength(0);
    });

    test('紐付いていないタグを削除するとエラー', () => {
      const tag = db.createTag('SF');
      expect(() => db.removeTagFromMedia(mediaId, tag.id)).toThrow();
    });
  });

  describe('getTagUsageStats', () => {
    test('使用数の多い順に返す', () => {
      const t1 = db.createTag('人気タグ');
      const t2 = db.createTag('不人気タグ');
      const m2 = db.createMedia({ title: '2', media_type: 'video' });

      db.addTagToMedia(mediaId, t1.id);
      db.addTagToMedia(m2.id, t1.id);
      db.addTagToMedia(mediaId, t2.id);

      const stats = db.getTagUsageStats();
      expect(stats[0].tag_name).toBe('人気タグ');
      expect(stats[0].count).toBe(2);
      expect(stats[1].count).toBe(1);
    });
  });

  describe('findUnusedTags', () => {
    test('未使用のタグのみ返す', () => {
      const used = db.createTag('使用中');
      const unused = db.createTag('未使用');
      db.addTagToMedia(mediaId, used.id);

      const tags = db.findUnusedTags();
      expect(tags.map(t => t.name)).toContain('未使用');
      expect(tags.map(t => t.name)).not.toContain('使用中');
    });
  });
});
