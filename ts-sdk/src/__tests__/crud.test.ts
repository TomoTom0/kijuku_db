/**
 * 基本CRUD操作のテスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB, MediaInput } from '../index.js';
import fs from 'fs';
import path from 'path';

describe('CRUD Operations', () => {
  let db: KijukuDB;

  beforeEach(() => {
    // インメモリDBを使用
    db = new KijukuDB(':memory:');
    db.migrate();
  });

  afterEach(() => {
    db.close();
  });

  describe('createMedia', () => {
    test('新しいメディアを作成できる', () => {
      const input: MediaInput = {
        title: 'テストコミック1',
        media_type: 'comic',
        artist: 'テスト作者',
      };

      const media = db.createMedia(input);

      expect(media.id).toBeGreaterThan(0);
      expect(media.title).toBe('テストコミック1');
      expect(media.media_type).toBe('comic');
      expect(media.artist).toBe('テスト作者');
      expect(media.flag_exist).toBe(false);
      expect(media.created_at).toBeInstanceOf(Date);
      expect(media.updated_at).toBeInstanceOf(Date);
    });

    test('pathが指定されている場合、flag_existが自動的にtrueになる', () => {
      const input: MediaInput = {
        title: 'テストコミック2',
        media_type: 'comic',
        path: '/path/to/comic.cbz',
      };

      const media = db.createMedia(input);

      expect(media.flag_exist).toBe(true);
      expect(media.path).toBe('/path/to/comic.cbz');
    });

    test('必須フィールドが不足している場合、エラーが発生する', () => {
      const input = {
        media_type: 'comic',
      } as MediaInput;

      expect(() => db.createMedia(input)).toThrow();
    });

    test('無効なmedia_typeの場合、エラーが発生する', () => {
      const input = {
        title: 'テストコミック3',
        media_type: 'invalid',
      } as MediaInput;

      expect(() => db.createMedia(input)).toThrow();
    });

    test('オプションフィールドを含めて作成できる', () => {
      const input: MediaInput = {
        title: 'テストビデオ1',
        media_type: 'video',
        artist: 'テスト作者',
        description: 'テスト説明',
        file_size: 1024000,
        duration_sec: 3600,
        series: 'テストシリーズ',
        language: 'ja',
        source: 'test-source',
      };

      const media = db.createMedia(input);

      expect(media.description).toBe('テスト説明');
      expect(media.file_size).toBe(1024000);
      expect(media.duration_sec).toBe(3600);
      expect(media.series).toBe('テストシリーズ');
      expect(media.language).toBe('ja');
      expect(media.source).toBe('test-source');
    });
  });

  describe('getMedia', () => {
    test('IDでメディアを取得できる', () => {
      const input: MediaInput = {
        title: 'テストコミック1',
        media_type: 'comic',
      };

      const created = db.createMedia(input);
      const retrieved = db.getMedia(created.id);

      expect(retrieved).not.toBeNull();
      expect(retrieved?.id).toBe(created.id);
      expect(retrieved?.title).toBe('テストコミック1');
      expect(retrieved?.media_type).toBe('comic');
    });

    test('存在しないIDの場合、nullを返す', () => {
      const media = db.getMedia(999);
      expect(media).toBeNull();
    });

    test('取得したメディアのcreated_at/updated_atがDate型である', () => {
      const input: MediaInput = {
        title: 'テストコミック2',
        media_type: 'comic',
      };

      const created = db.createMedia(input);
      const retrieved = db.getMedia(created.id);

      expect(retrieved?.created_at).toBeInstanceOf(Date);
      expect(retrieved?.updated_at).toBeInstanceOf(Date);
    });
  });

  describe('updateMedia', () => {
    test('メディアの情報を更新できる', () => {
      const input: MediaInput = {
        title: 'テストコミック1',
        media_type: 'comic',
      };

      const created = db.createMedia(input);

      db.updateMedia(created.id, {
        title: '更新されたタイトル',
        artist: '新しい作者',
      });

      const updated = db.getMedia(created.id);

      expect(updated?.title).toBe('更新されたタイトル');
      expect(updated?.artist).toBe('新しい作者');
      expect(updated?.media_type).toBe('comic');
    });

    test('一部のフィールドのみを更新できる', () => {
      const input: MediaInput = {
        title: 'テストコミック2',
        media_type: 'comic',
        artist: 'テスト作者',
        description: '元の説明',
      };

      const created = db.createMedia(input);

      db.updateMedia(created.id, {
        description: '更新された説明',
      });

      const updated = db.getMedia(created.id);

      expect(updated?.title).toBe('テストコミック2');
      expect(updated?.artist).toBe('テスト作者');
      expect(updated?.description).toBe('更新された説明');
    });

    test('存在しないIDの場合、エラーが発生する', () => {
      expect(() =>
        db.updateMedia(999, { title: '更新されたタイトル' })
      ).toThrow();
    });

    test('updated_atが自動的に更新される', () => {
      const input: MediaInput = {
        title: 'テストコミック3',
        media_type: 'comic',
      };

      const created = db.createMedia(input);
      const originalUpdatedAt = created.updated_at;

      // 少し待機してから更新
      setTimeout(() => {
        db.updateMedia(created.id, { title: '更新されたタイトル' });

        const updated = db.getMedia(created.id);

        expect(updated?.updated_at.getTime()).toBeGreaterThan(
          originalUpdatedAt.getTime()
        );
      }, 100);
    });
  });

  describe('deleteMedia', () => {
    test('メディアを削除できる', () => {
      const input: MediaInput = {
        title: 'テストコミック1',
        media_type: 'comic',
      };

      const created = db.createMedia(input);

      db.deleteMedia(created.id);

      const deleted = db.getMedia(created.id);
      expect(deleted).toBeNull();
    });

    test('存在しないIDの場合、エラーが発生する', () => {
      expect(() => db.deleteMedia(999)).toThrow();
    });

    test('削除後に同じIDで作成しても別のメディアとして扱われる', () => {
      const input1: MediaInput = {
        title: 'テストコミック1',
        media_type: 'comic',
      };

      const created1 = db.createMedia(input1);
      const id1 = created1.id;

      db.deleteMedia(id1);

      const input2: MediaInput = {
        title: 'テストコミック2',
        media_type: 'comic',
      };

      const created2 = db.createMedia(input2);

      expect(created2.id).toBeGreaterThan(id1);
      expect(created2.title).toBe('テストコミック2');
    });
  });
});
