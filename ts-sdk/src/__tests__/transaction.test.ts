/**
 * トランザクション管理のテスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB, MediaInput } from '../index.js';

describe('Transaction', () => {
  let db: KijukuDB;

  beforeEach(() => {
    db = new KijukuDB(':memory:');
    db.migrate();
  });

  afterEach(() => {
    db.close();
  });

  test('トランザクション内で複数の操作を実行できる', () => {
    const result = db.transaction(() => {
      const media1 = db.createMedia({
        title: 'コミック1',
        media_type: 'comic',
      });
      const media2 = db.createMedia({
        title: 'コミック2',
        media_type: 'comic',
      });

      return { media1, media2 };
    });

    expect(result.media1.id).toBeGreaterThan(0);
    expect(result.media2.id).toBeGreaterThan(0);

    // トランザクション外で確認
    const retrieved1 = db.getMedia(result.media1.id);
    const retrieved2 = db.getMedia(result.media2.id);

    expect(retrieved1).not.toBeNull();
    expect(retrieved2).not.toBeNull();
    expect(retrieved1?.title).toBe('コミック1');
    expect(retrieved2?.title).toBe('コミック2');
  });

  test('トランザクション内でエラーが発生した場合、自動的にロールバックされる', () => {
    expect(() => {
      db.transaction(() => {
        db.createMedia({
          title: 'コミック1',
          media_type: 'comic',
        });

        // 無効なデータで例外を発生させる
        db.createMedia({
          title: '',
          media_type: 'invalid',
        } as MediaInput);
      });
    }).toThrow();

    // ロールバックされているため、最初のメディアも作成されていないはず
    const allMedia = db.findMedia({});
    expect(allMedia).toHaveLength(0);
  });

  test('トランザクション内での更新と削除が正しく処理される', () => {
    // トランザクション外で作成
    const media = db.createMedia({
      title: 'コミック1',
      media_type: 'comic',
    });

    db.transaction(() => {
      db.updateMedia(media.id, {
        title: '更新されたタイトル',
      });

      const updated = db.getMedia(media.id);
      expect(updated?.title).toBe('更新されたタイトル');
    });

    // トランザクション外で確認
    const retrieved = db.getMedia(media.id);
    expect(retrieved?.title).toBe('更新されたタイトル');
  });

  test('トランザクション内で削除が正しく処理される', () => {
    const media1 = db.createMedia({
      title: 'コミック1',
      media_type: 'comic',
    });
    const media2 = db.createMedia({
      title: 'コミック2',
      media_type: 'comic',
    });

    db.transaction(() => {
      db.deleteMedia(media1.id);
    });

    // トランザクション外で確認
    const deleted = db.getMedia(media1.id);
    const existing = db.getMedia(media2.id);

    expect(deleted).toBeNull();
    expect(existing).not.toBeNull();
  });

  test('ネストしたトランザクションでエラーが発生した場合、全体がロールバックされる', () => {
    expect(() => {
      db.transaction(() => {
        db.createMedia({
          title: 'コミック1',
          media_type: 'comic',
        });

        db.transaction(() => {
          db.createMedia({
            title: 'コミック2',
            media_type: 'comic',
          });

          // エラーを発生させる
          throw new Error('Test error');
        });
      });
    }).toThrow();

    // 全てロールバックされているはず
    const allMedia = db.findMedia({});
    expect(allMedia).toHaveLength(0);
  });

  test('トランザクション内でタグ操作が正しく処理される', () => {
    const media = db.createMedia({
      title: 'コミック1',
      media_type: 'comic',
    });

    db.transaction(() => {
      const tag1 = db.createTag('アクション');
      const tag2 = db.createTag('コメディ');

      db.addTagToMedia(media.id, tag1.id);
      db.addTagToMedia(media.id, tag2.id);
    });

    // トランザクション外で確認
    const tags = db.getMediaTags(media.id);
    expect(tags).toHaveLength(2);
    expect(tags.map((t) => t.name).sort()).toEqual(['アクション', 'コメディ']);
  });

  test('トランザクション内でのタグ操作でエラーが発生した場合、ロールバックされる', () => {
    const media = db.createMedia({
      title: 'コミック1',
      media_type: 'comic',
    });

    expect(() => {
      db.transaction(() => {
        const tag = db.createTag('アクション');
        db.addTagToMedia(media.id, tag.id);

        // 存在しないタグIDでエラーを発生させる
        db.addTagToMedia(media.id, 999);
      });
    }).toThrow();

    // ロールバックされているため、タグも作成されていないはず
    const tags = db.getAllTags();
    expect(tags).toHaveLength(0);

    const mediaTags = db.getMediaTags(media.id);
    expect(mediaTags).toHaveLength(0);
  });

  test('トランザクションから値を返すことができる', () => {
    const count = db.transaction(() => {
      db.createMedia({ title: 'コミック1', media_type: 'comic' });
      db.createMedia({ title: 'コミック2', media_type: 'comic' });
      db.createMedia({ title: 'ビデオ1', media_type: 'video' });

      return db.findMedia({}).length;
    });

    expect(count).toBe(3);
  });
});
