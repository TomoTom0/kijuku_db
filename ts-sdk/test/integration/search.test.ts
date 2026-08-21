/**
 * 検索・フィルタ機能のテスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB, MediaInput, MediaFilter, QueryOptions } from '../../src/index.js';

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

    test('id_inで複数IDを一括取得できる', () => {
      const all = db.findMedia({});
      const targetIds = [all[0].id, all[2].id];
      const results = db.findMedia({ id_in: targetIds });
      expect(results).toHaveLength(2);
      const resultIds = results.map((m) => m.id);
      expect(resultIds).toContain(targetIds[0]);
      expect(resultIds).toContain(targetIds[1]);
    });

    test('id_inが空配列の場合、全件取得になる', () => {
      const results = db.findMedia({ id_in: [] });
      expect(results).toHaveLength(4);
    });

    test('id_inに重複IDが含まれていても重複なく取得できる', () => {
      const all = db.findMedia({});
      const targetIds = [all[0].id, all[2].id];
      // 重複して指定しても結果は同一
      const results = db.findMedia({
        id_in: [targetIds[0], targetIds[0], targetIds[1], targetIds[1]],
      });
      expect(results).toHaveLength(2);
      const resultIds = results.map((m) => m.id);
      expect(resultIds).toContain(targetIds[0]);
      expect(resultIds).toContain(targetIds[1]);
    });

    test('id_inと他のフィルタを組み合わせられる', () => {
      const all = db.findMedia({});
      const allIds = all.map((m) => m.id);
      // 全IDを渡しつつmedia_type=comicでフィルタ
      const results = db.findMedia({ id_in: allIds, media_type: 'comic' });
      expect(results).toHaveLength(2);
      results.forEach((m) => {
        expect(m.media_type).toBe('comic');
      });
    });

    test('exclude_idsで指定IDを除外できる', () => {
      const all = db.findMedia({});
      expect(all).toHaveLength(4);
      const allIds = all.map((m) => m.id);
      // 最初の2件を除外
      const results = db.findMedia({ exclude_ids: [allIds[0], allIds[1]] });
      expect(results).toHaveLength(2);
      const resultIds = results.map((m) => m.id);
      expect(resultIds).not.toContain(allIds[0]);
      expect(resultIds).not.toContain(allIds[1]);
    });

    test('exclude_idsが空配列の場合、全件取得になる', () => {
      const results = db.findMedia({ exclude_ids: [] });
      expect(results).toHaveLength(4);
    });

    test('exclude_idsに重複IDが含まれていても正しく除外される', () => {
      const all = db.findMedia({});
      expect(all).toHaveLength(4);
      const allIds = all.map((m) => m.id);
      // 重複した exclude_ids（同じIDを複数回指定）でも結果は同一
      const results = db.findMedia({
        exclude_ids: [allIds[0], allIds[0], allIds[1], allIds[1]],
      });
      expect(results).toHaveLength(2);
      const resultIds = results.map((m) => m.id);
      expect(resultIds).not.toContain(allIds[0]);
      expect(resultIds).not.toContain(allIds[1]);
    });

    test('id_inとexclude_idsを組み合わせられる', () => {
      const all = db.findMedia({});
      const allIds = all.map((m) => m.id);
      // id_in=全ID, exclude_ids=最初の2件 → 残り2件（IN と NOT IN の併用）
      const results = db.findMedia({
        id_in: allIds,
        exclude_ids: [allIds[0], allIds[1]],
      });
      expect(results).toHaveLength(2);
      const resultIds = results.map((m) => m.id);
      expect(resultIds).not.toContain(allIds[0]);
      expect(resultIds).not.toContain(allIds[1]);
    });

    test('uuidで完全一致フィルタできる', () => {
      const all = db.findMedia({});
      const targetUuid = all[0].uuid;
      const results = db.findMedia({ uuid: targetUuid });
      expect(results).toHaveLength(1);
      expect(results[0].uuid).toBe(targetUuid);
      expect(results[0].id).toBe(all[0].id);
    });

    test('uuidが一致しない場合、空配列を返す', () => {
      const results = db.findMedia({ uuid: '00000000-0000-0000-0000-000000000000' });
      expect(results).toHaveLength(0);
    });

    test('uuid_inで複数UUIDを一括取得できる', () => {
      const all = db.findMedia({});
      const results = db.findMedia({ uuid_in: [all[0].uuid, all[2].uuid] });
      expect(results).toHaveLength(2);
      const resultUuids = results.map((m) => m.uuid);
      expect(resultUuids).toContain(all[0].uuid);
      expect(resultUuids).toContain(all[2].uuid);
    });

    test('uuid_inに重複UUIDが含まれていても重複なく取得できる', () => {
      const all = db.findMedia({});
      const results = db.findMedia({
        uuid_in: [all[0].uuid, all[0].uuid, all[1].uuid],
      });
      expect(results).toHaveLength(2);
    });

    test('uuid_inが空配列の場合、全件取得になる', () => {
      const results = db.findMedia({ uuid_in: [] });
      expect(results).toHaveLength(4);
    });

    test('uuid_inと他のフィルタを組み合わせられる', () => {
      const all = db.findMedia({});
      // uuid_in=全UUID + media_type=comic → コミック2件のみ
      const results = db.findMedia({
        uuid_in: all.map((m) => m.uuid),
        media_type: 'comic',
      });
      expect(results).toHaveLength(2);
      results.forEach((media) => {
        expect(media.media_type).toBe('comic');
      });
    });
  });

  describe('findMedia - ソート', () => {
    test('created_atで昇順ソートできる', () => {
      const results = db.findMedia(
        {},
        { sortKeys: [{ field: 'created_at', order: 'ASC' }] }
      );
      expect(results).toHaveLength(4);
      expect(results[0].title).toBe('コミック1');
      expect(results[3].title).toBe('ミュージック1');
    });

    test('titleで降順ソートできる', () => {
      const results = db.findMedia({}, { sortKeys: [{ field: 'title', order: 'DESC' }] });
      expect(results).toHaveLength(4);
      expect(results[0].title).toBe('ミュージック1');
      expect(results[3].title).toBe('コミック1');
    });

    test('artistでソートできる', () => {
      const results = db.findMedia({}, { sortKeys: [{ field: 'artist', order: 'ASC' }] });
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
        { offset: 2, sortKeys: [{ field: 'created_at', order: 'ASC' }] }
      );
      expect(results).toHaveLength(2);
      expect(results[0].title).toBe('ビデオ1');
    });

    test('limitとoffsetを組み合わせてページネーションできる', () => {
      const page1 = db.findMedia(
        {},
        { limit: 2, offset: 0, sortKeys: [{ field: 'created_at', order: 'ASC' }] }
      );
      const page2 = db.findMedia(
        {},
        { limit: 2, offset: 2, sortKeys: [{ field: 'created_at', order: 'ASC' }] }
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

  describe('findMedia - 部分一致フィルタ', () => {
    test('volume_titleで部分一致フィルタできる', () => {
      db.createMedia({
        title: '巻タイトルA',
        media_type: 'comic',
        volume_title: '序章',
      });
      db.createMedia({
        title: '巻タイトルB',
        media_type: 'comic',
        volume_title: '最終章',
      });

      const results = db.findMedia({ volume_title: '序' });
      expect(results).toHaveLength(1);
      expect(results[0].title).toBe('巻タイトルA');
    });

    test('title_enで部分一致フィルタできる', () => {
      db.createMedia({
        title: '作品A',
        media_type: 'comic',
        title_en: 'Adventure Story',
      });
      db.createMedia({
        title: '作品B',
        media_type: 'comic',
        title_en: 'Mystery Novel',
      });

      const results = db.findMedia({ title_en: 'venture' });
      expect(results).toHaveLength(1);
      expect(results[0].title).toBe('作品A');
    });

    test('artist_enで部分一致フィルタできる', () => {
      db.createMedia({
        title: '作品A',
        media_type: 'comic',
        artist_en: 'John Smith',
      });
      db.createMedia({
        title: '作品B',
        media_type: 'comic',
        artist_en: 'Jane Doe',
      });

      const results = db.findMedia({ artist_en: 'Smith' });
      expect(results).toHaveLength(1);
      expect(results[0].title).toBe('作品A');
    });
  });

  describe('findMedia - 複合条件', () => {
    test('フィルタ、ソート、ページネーションを組み合わせられる', () => {
      const results = db.findMedia(
        { media_type: 'comic' },
        { sortKeys: [{ field: 'title', order: 'DESC' }], limit: 1, offset: 0 }
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
        { sortKeys: [{ field: 'title', order: 'ASC' }] }
      );

      expect(results).toHaveLength(2);
      expect(results[0].title).toBe('コミック1');
      expect(results[1].title).toBe('コミック2');
    });
  });

  describe('findMedia - OR条件フィルタ', () => {
    test('or_filtersで複数の条件をOR結合できる', () => {
      // artist="作者A" OR artist="作者B"
      const results = db.findMedia({
        or_filters: [
          { artist: '作者A' },
          { artist: '作者B' },
        ],
      });

      expect(results).toHaveLength(3);
      const artists = results.map((m) => m.artist);
      expect(artists).toContain('作者A');
      expect(artists).toContain('作者B');
      expect(artists).not.toContain('作者C');
    });

    test('or_filters内で複数条件をAND結合できる', () => {
      // (artist="作者A" AND series="シリーズX") OR (artist="作者C")
      const results = db.findMedia({
        or_filters: [
          { artist: '作者A', series: 'シリーズX' },
          { artist: '作者C' },
        ],
      });

      expect(results).toHaveLength(2);
      const titles = results.map((m) => m.title);
      expect(titles).toContain('コミック1'); // 作者A + シリーズX
      expect(titles).toContain('ミュージック1'); // 作者C
    });

    test('メイン条件とor_filtersを組み合わせられる', () => {
      // (media_type="comic") OR (artist="作者A")
      const results = db.findMedia({
        media_type: 'comic',
        or_filters: [
          { artist: '作者A' },
        ],
      });

      expect(results).toHaveLength(3);
      const titles = results.map((m) => m.title);
      expect(titles).toContain('コミック1');
      expect(titles).toContain('コミック2');
      expect(titles).toContain('ビデオ1'); // 作者Aだがcomicではない
    });

    test('or_filtersが空の場合は無視される', () => {
      const results = db.findMedia({
        artist: '作者A',
        or_filters: [],
      });

      expect(results).toHaveLength(2);
    });

    test('or_filters内の空のフィルタは無視される', () => {
      const results = db.findMedia({
        or_filters: [
          { artist: '作者A' },
          {}, // 空のフィルタ
        ],
      });

      // 空のフィルタは条件なしとなり、全件マッチするわけではない
      // 実装では空のフィルタは条件として追加されない
      expect(results).toHaveLength(2); // 作者Aのみ
    });
  });

  describe('getDistinctValues', () => {
    test('単一フィールドの重複なし値を取得できる', () => {
      const rows = db.getDistinctValues(['artist'], {});
      const artists = rows.map(r => r[0]);
      expect(artists).toContain('作者A');
      expect(artists).toContain('作者B');
      expect(artists).toContain('作者C');
      // 重複がないこと（作者Aは2件あるが1件のみ返る）
      const filtered = artists.filter(a => a === '作者A');
      expect(filtered).toHaveLength(1);
    });

    test('フィルタで絞り込んだ値を取得できる', () => {
      const rows = db.getDistinctValues(['artist'], { media_type: 'comic' });
      const artists = rows.map(r => r[0]);
      expect(artists).toContain('作者A');
      expect(artists).toContain('作者B');
      expect(artists).not.toContain('作者C'); // musicのみ
    });

    test('複数フィールドの組み合わせを取得できる', () => {
      const rows = db.getDistinctValues(['media_type', 'artist'], {});
      expect(rows.length).toBeGreaterThan(0);
      // 各行に2要素あること
      rows.forEach(row => expect(row).toHaveLength(2));
      // comic + 作者A の組み合わせが含まれること
      const comicAuthorA = rows.find(r => r[0] === 'comic' && r[1] === '作者A');
      expect(comicAuthorA).toBeDefined();
    });

    test('昇順ソートされた結果が返る', () => {
      const rows = db.getDistinctValues(['artist'], {});
      const artists = rows.map(r => r[0]).filter(Boolean) as string[];
      const sorted = [...artists].sort();
      expect(artists).toEqual(sorted);
    });

    test('空のfieldsを渡すとエラーになる', () => {
      expect(() => db.getDistinctValues([], {})).toThrow();
    });

    test('ホワイトリスト外のフィールドを渡すとエラーになる', () => {
      expect(() => db.getDistinctValues(['id'], {})).toThrow();
      expect(() => db.getDistinctValues(['created_at'], {})).toThrow();
    });
  });
});
