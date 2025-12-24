import { describe, it, expect, beforeEach } from 'vitest';
import Database from 'better-sqlite3';
import { migrate } from '../../src/migration.js';
import { createMedia, getMedia, updateMedia, deleteMedia } from '../../src/crud.js';
import {
  KijukuDBError,
  ValidationError,
  NotFoundError,
  ConflictError,
  ErrorCode,
} from '../../src/errors.js';
import type { MediaInput } from '../../src/types.js';

describe('エラーハンドリング', () => {
  let db: Database.Database;

  beforeEach(() => {
    db = new Database(':memory:');
    migrate(db);
  });

  describe('ValidationError', () => {
    it('タイトルが空の場合、ValidationErrorをスロー', () => {
      const invalidData: any = {
        title: undefined,
        media_type: 'comic',
      };

      expect(() => createMedia(db, invalidData)).toThrow(ValidationError);
      expect(() => createMedia(db, invalidData)).toThrow(/必須フィールド/);
    });

    it('media_typeが空の場合、ValidationErrorをスロー', () => {
      const invalidData: any = {
        title: 'テスト',
        media_type: undefined,
      };

      expect(() => createMedia(db, invalidData)).toThrow(ValidationError);
    });

    it('無効なmedia_typeの場合、ValidationErrorをスロー', () => {
      const invalidData: any = {
        title: 'テスト',
        media_type: 'invalid',
      };

      expect(() => createMedia(db, invalidData)).toThrow(ValidationError);
      expect(() => createMedia(db, invalidData)).toThrow(/無効なメディアタイプ/);
    });
  });

  describe('NotFoundError', () => {
    it('存在しないメディアの更新時、NotFoundErrorをスロー', () => {
      expect(() => updateMedia(db, 9999, { title: '更新' })).toThrow(NotFoundError);
      expect(() => updateMedia(db, 9999, { title: '更新' })).toThrow(/メディアが見つかりません/);
    });

    it('存在しないメディアの削除時、NotFoundErrorをスロー', () => {
      expect(() => deleteMedia(db, 9999)).toThrow(NotFoundError);
    });
  });

  describe('ConflictError', () => {
    it('重複したpathの場合、ConflictErrorをスロー', () => {
      const data1: MediaInput = {
        title: 'テスト1',
        path: '/path/to/media.jpg',
        media_type: 'comic',
      };

      const data2: MediaInput = {
        title: 'テスト2',
        path: '/path/to/media.jpg', // 同じpath
        media_type: 'comic',
      };

      createMedia(db, data1);
      expect(() => createMedia(db, data2)).toThrow(ConflictError);
    });
  });

  describe('エラーオブジェクト', () => {
    it('KijukuDBErrorは適切なエラーコードを持つ', () => {
      try {
        const invalidData: any = {
          title: 'テスト',
          media_type: 'invalid',
        };
        createMedia(db, invalidData);
      } catch (error) {
        expect(error).toBeInstanceOf(ValidationError);
        expect((error as KijukuDBError).code).toBeDefined();
      }
    });

    it('エラーはJSON形式に変換可能', () => {
      const error = new ValidationError('テストエラー', { field: 'test' });
      const json = error.toJSON();

      expect(json).toEqual({
        name: 'ValidationError',
        message: 'テストエラー',
        code: ErrorCode.VALIDATION_INVALID_VALUE,
        details: { field: 'test' },
      });
    });
  });
});
