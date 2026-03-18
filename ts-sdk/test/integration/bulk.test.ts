/**
 * バルク操作のテスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB, MediaInput } from '../../src/index.js';

describe('Bulk Operations', () => {
  let db: KijukuDB;

  beforeEach(() => {
    db = new KijukuDB(':memory:');
    db.migrate();
  });

  afterEach(() => {
    db.close();
  });

  describe('bulkCreateMedia', () => {
    test('複数のメディアを一括で作成できる', () => {
      const inputs: MediaInput[] = [
        { title: 'コミック1', media_type: 'comic' },
        { title: 'コミック2', media_type: 'comic' },
        { title: 'ビデオ1', media_type: 'video' },
      ];

      const results = db.bulkCreateMedia(inputs);

      expect(results).toHaveLength(3);
      results.forEach((media, index) => {
        expect(media.id).toBeGreaterThan(0);
        expect(media.title).toBe(inputs[index].title);
        expect(media.media_type).toBe(inputs[index].media_type);
      });
    });

    test('空配列の場合、空配列を返す', () => {
      const results = db.bulkCreateMedia([]);
      expect(results).toHaveLength(0);
    });

    test('大量のメディアを効率的に作成できる', () => {
      const inputs: MediaInput[] = Array.from({ length: 100 }, (_, i) => ({
        title: `メディア${i + 1}`,
        media_type: 'comic' as const,
        artist: `作者${(i % 10) + 1}`,
      }));

      const start = Date.now();
      const results = db.bulkCreateMedia(inputs);
      const duration = Date.now() - start;

      expect(results).toHaveLength(100);
      expect(duration).toBeLessThan(1000); // 1秒以内に完了すること
    });

    test('バルク作成中にエラーが発生した場合、全てロールバックされる', () => {
      const inputs: MediaInput[] = [
        { title: 'コミック1', media_type: 'comic' },
        { title: '', media_type: 'invalid' } as MediaInput, // 無効なデータ
        { title: 'コミック3', media_type: 'comic' },
      ];

      expect(() => db.bulkCreateMedia(inputs)).toThrow();

      // 全てロールバックされているはず
      const allMedia = db.findMedia({});
      expect(allMedia).toHaveLength(0);
    });

    test('オプションフィールドを含むメディアを一括作成できる', () => {
      const inputs: MediaInput[] = [
        {
          title: 'コミック1',
          media_type: 'comic',
          artist: '作者A',
          series: 'シリーズX',
          source: 'source1',
        },
        {
          title: 'ビデオ1',
          media_type: 'video',
          artist: '作者B',
          duration_sec: 3600,
          file_size: 1024000,
        },
      ];

      const results = db.bulkCreateMedia(inputs);

      expect(results).toHaveLength(2);
      expect(results[0].artist).toBe('作者A');
      expect(results[0].series).toBe('シリーズX');
      expect(results[1].duration_sec).toBe(3600);
      expect(results[1].file_size).toBe(1024000);
    });

    test('pathが指定されている場合、flag_existが自動的にtrueになる', () => {
      const inputs: MediaInput[] = [
        {
          title: 'コミック1',
          media_type: 'comic',
          path: '/path/to/comic1.cbz',
        },
        {
          title: 'コミック2',
          media_type: 'comic',
        },
      ];

      const results = db.bulkCreateMedia(inputs);

      expect(results).toHaveLength(2);
      expect(results[0].flag_exist).toBe(true);
      expect(results[0].path).toBe('/path/to/comic1.cbz');
      expect(results[1].flag_exist).toBe(false);
      expect(results[1].path).toBeNull();
    });

    test('バルク作成後にIDが正しく連番になっている', () => {
      const inputs: MediaInput[] = [
        { title: 'コミック1', media_type: 'comic' },
        { title: 'コミック2', media_type: 'comic' },
        { title: 'コミック3', media_type: 'comic' },
      ];

      const results = db.bulkCreateMedia(inputs);

      expect(results).toHaveLength(3);
      expect(results[1].id).toBe(results[0].id + 1);
      expect(results[2].id).toBe(results[1].id + 1);
    });

    test('バルク作成後に個別に取得できる', () => {
      const inputs: MediaInput[] = [
        { title: 'コミック1', media_type: 'comic' },
        { title: 'コミック2', media_type: 'comic' },
      ];

      const results = db.bulkCreateMedia(inputs);

      const retrieved1 = db.getMedia(results[0].id);
      const retrieved2 = db.getMedia(results[1].id);

      expect(retrieved1).not.toBeNull();
      expect(retrieved2).not.toBeNull();
      expect(retrieved1?.title).toBe('コミック1');
      expect(retrieved2?.title).toBe('コミック2');
    });

    test('通常のcreateMediaとの混在が正しく処理される', () => {
      const media1 = db.createMedia({
        title: 'コミック1',
        media_type: 'comic',
      });

      const bulkResults = db.bulkCreateMedia([
        { title: 'コミック2', media_type: 'comic' },
        { title: 'コミック3', media_type: 'comic' },
      ]);

      const media4 = db.createMedia({
        title: 'コミック4',
        media_type: 'comic',
      });

      const allMedia = db.findMedia({}, { sortKeys: [{ field: 'id', order: 'ASC' }] });

      expect(allMedia).toHaveLength(4);
      expect(allMedia[0].title).toBe('コミック1');
      expect(allMedia[1].title).toBe('コミック2');
      expect(allMedia[2].title).toBe('コミック3');
      expect(allMedia[3].title).toBe('コミック4');
    });
  });
});
