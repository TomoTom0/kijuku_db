/**
 * 検索・フィルタ機能のテスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB, MediaInput, MediaFilter, QueryOptions } from '../index.js';

describe('Search and Filter', () => {
  let db: KijukuDB;

  beforeEach(() => {
    db = new KijukuDB(':memory:');
    db.migrate();

    // テストデータを作成
    db.createMedia({
      title: 'コミック1',
      media_type: 'comic',
      artist: '作者A',
      series: 'シリーズX',
      source: 'source1',
    });
    db.createMedia({
      title: 'コミック2',
      media_type: 'comic',
      artist: '作者B',
      series: 'シリーズX',
      source: 'source1',
    });
    db.createMedia({
      title: 'ビデオ1',
      media_type: 'video',
      artist: '作者A',
      series: 'シリーズY',
      source: 'source2',
    });
    db.createMedia({
      title: 'ミュージック1',
      media_type: 'music',
      artist: '作者C',
      source: 'source2',
    });
  });

  afterEach(() => {
    db.close();
  });

  describe('findMedia - 基本フィルタ', () => {
    test('フィルタなしで全てのメディアを取得できる', () => {
      const results = db.findMedia({});
      expect(results).toHaveLength(4);
    });

    test('titleでフィルタできる', () => {
      const results = db.findMedia({ title: 'コミック1' });
      expect(results).toHaveLength(1);
      expect(results[0].title).toBe('コミック1');
    });

    test('media_typeでフィルタできる', () => {
      const results = db.findMedia({ media_type: 'comic' });
      expect(results).toHaveLength(2);
      results.forEach((media) => {
        expect(media.media_type).toBe('comic');
      });
    });

    test('artistでフィルタできる', () => {
      const results = db.findMedia({ artist: '作者A' });
      expect(results).toHaveLength(2);
      results.forEach((media) => {
        expect(media.artist).toBe('作者A');
      });
    });

    test('seriesでフィルタできる', () => {
      const results = db.findMedia({ series: 'シリーズX' });
      expect(results).toHaveLength(2);
      results.forEach((media) => {
        expect(media.series).toBe('シリーズX');
      });
    });

    test('sourceでフィルタできる', () => {
      const results = db.findMedia({ source: 'source1' });
      expect(results).toHaveLength(2);
      results.forEach((media) => {
        expect(media.source).toBe('source1');
      });
    });

    test('複数のフィルタを組み合わせられる', () => {
      const results = db.findMedia({
        media_type: 'comic',
        series: 'シリーズX',
      });
      expect(results).toHaveLength(2);
      results.forEach((media) => {
        expect(media.media_type).toBe('comic');
        expect(media.series).toBe('シリーズX');
      });
    });

    test('条件に合うメディアがない場合、空配列を返す', () => {
      const results = db.findMedia({ title: '存在しないタイトル' });
      expect(results).toHaveLength(0);
    });
  });

  describe('findMedia - ソート', () => {
    test('created_atで昇順ソートできる', () => {
      const results = db.findMedia(
        {},
        { orderBy: 'created_at', order: 'ASC' }
      );
      expect(results).toHaveLength(4);
      expect(results[0].title).toBe('コミック1');
      expect(results[3].title).toBe('ミュージック1');
    });

    test('titleで降順ソートできる', () => {
      const results = db.findMedia({}, { orderBy: 'title', order: 'DESC' });
      expect(results).toHaveLength(4);
      expect(results[0].title).toBe('ミュージック1');
      expect(results[3].title).toBe('コミック1');
    });

    test('artistでソートできる', () => {
      const results = db.findMedia({}, { orderBy: 'artist', order: 'ASC' });
      expect(results).toHaveLength(4);
      expect(results[0].artist).toBe('作者A');
      expect(results[2].artist).toBe('作者B');
      expect(results[3].artist).toBe('作者C');
    });
  });

  describe('findMedia - ページネーション', () => {
    test('limitで取得件数を制限できる', () => {
      const results = db.findMedia({}, { limit: 2 });
      expect(results).toHaveLength(2);
    });

    test('offsetで取得開始位置を指定できる', () => {
      const results = db.findMedia(
        {},
        { offset: 2, orderBy: 'created_at', order: 'ASC' }
      );
      expect(results).toHaveLength(2);
      expect(results[0].title).toBe('ビデオ1');
    });

    test('limitとoffsetを組み合わせてページネーションできる', () => {
      const page1 = db.findMedia(
        {},
        { limit: 2, offset: 0, orderBy: 'created_at', order: 'ASC' }
      );
      const page2 = db.findMedia(
        {},
        { limit: 2, offset: 2, orderBy: 'created_at', order: 'ASC' }
      );

      expect(page1).toHaveLength(2);
      expect(page2).toHaveLength(2);
      expect(page1[0].title).toBe('コミック1');
      expect(page2[0].title).toBe('ビデオ1');
    });
  });

  describe('findMedia - タグ検索', () => {
    test('タグIDでフィルタできる', () => {
      // タグを作成
      const tag1 = db.createTag('アクション');
      const tag2 = db.createTag('コメディ');

      // メディア1にタグ1を追加
      const media1 = db.findMedia({ title: 'コミック1' })[0];
      db.addTagToMedia(media1.id, tag1.id);

      // メディア2にタグ1とタグ2を追加
      const media2 = db.findMedia({ title: 'コミック2' })[0];
      db.addTagToMedia(media2.id, tag1.id);
      db.addTagToMedia(media2.id, tag2.id);

      // タグ1で検索
      const resultsTag1 = db.findMedia({ tag_ids: [tag1.id] });
      expect(resultsTag1).toHaveLength(2);

      // タグ2で検索
      const resultsTag2 = db.findMedia({ tag_ids: [tag2.id] });
      expect(resultsTag2).toHaveLength(1);
      expect(resultsTag2[0].title).toBe('コミック2');
    });

    test('複数のタグIDでフィルタできる（OR条件）', () => {
      const tag1 = db.createTag('アクション');
      const tag2 = db.createTag('コメディ');

      const media1 = db.findMedia({ title: 'コミック1' })[0];
      db.addTagToMedia(media1.id, tag1.id);

      const media2 = db.findMedia({ title: 'コミック2' })[0];
      db.addTagToMedia(media2.id, tag2.id);

      // タグ1またはタグ2を持つメディアを検索
      const results = db.findMedia({ tag_ids: [tag1.id, tag2.id] });
      expect(results).toHaveLength(2);
    });
  });

  describe('findMedia - 複合条件', () => {
    test('フィルタ、ソート、ページネーションを組み合わせられる', () => {
      const results = db.findMedia(
        { media_type: 'comic' },
        { orderBy: 'title', order: 'DESC', limit: 1, offset: 0 }
      );

      expect(results).toHaveLength(1);
      expect(results[0].title).toBe('コミック2');
    });

    test('複数フィルタ、タグ、ソートを組み合わせられる', () => {
      const tag = db.createTag('人気');

      const media1 = db.findMedia({ title: 'コミック1' })[0];
      db.addTagToMedia(media1.id, tag.id);

      const media2 = db.findMedia({ title: 'コミック2' })[0];
      db.addTagToMedia(media2.id, tag.id);

      const results = db.findMedia(
        {
          media_type: 'comic',
          series: 'シリーズX',
          tag_ids: [tag.id],
        },
        { orderBy: 'title', order: 'ASC' }
      );

      expect(results).toHaveLength(2);
      expect(results[0].title).toBe('コミック1');
      expect(results[1].title).toBe('コミック2');
    });
  });
});
