/**
 * エラークラスとエラーハンドリングの単体テスト
 */
import { describe, it, expect } from 'vitest';
import {
  KijukuDBError,
  DatabaseError,
  ValidationError,
  NotFoundError,
  ConflictError,
  ErrorCode,
  handleDatabaseError,
  validateRequired,
  validateType,
  validateMediaType,
} from '../../src/errors.js';

describe('エラークラス', () => {
  describe('KijukuDBError', () => {
    it('メッセージとコードを持つエラーを作成できる', () => {
      const error = new KijukuDBError('テストエラー', ErrorCode.UNKNOWN_ERROR);

      expect(error).toBeInstanceOf(Error);
      expect(error).toBeInstanceOf(KijukuDBError);
      expect(error.name).toBe('KijukuDBError');
      expect(error.message).toBe('テストエラー');
      expect(error.code).toBe(ErrorCode.UNKNOWN_ERROR);
    });

    it('detailsを含むエラーを作成できる', () => {
      const details = { field: 'test', value: 123 };
      const error = new KijukuDBError('テストエラー', ErrorCode.UNKNOWN_ERROR, details);

      expect(error.details).toEqual(details);
    });

    it('デフォルトコードはUNKNOWN_ERROR', () => {
      const error = new KijukuDBError('テストエラー');

      expect(error.code).toBe(ErrorCode.UNKNOWN_ERROR);
    });

    it('スタックトレースが存在する', () => {
      const error = new KijukuDBError('テストエラー');

      expect(error.stack).toBeDefined();
      expect(error.stack).toContain('KijukuDBError');
    });

    it('toJSON()でオブジェクトに変換できる', () => {
      const error = new KijukuDBError(
        'テストエラー',
        ErrorCode.VALIDATION_INVALID_VALUE,
        { field: 'title' }
      );
      const json = error.toJSON();

      expect(json).toEqual({
        name: 'KijukuDBError',
        message: 'テストエラー',
        code: ErrorCode.VALIDATION_INVALID_VALUE,
        details: { field: 'title' },
      });
    });

    it('toJSON()でdetailsがnullの場合もシリアライズできる', () => {
      const error = new KijukuDBError('テストエラー', ErrorCode.UNKNOWN_ERROR);
      const json = error.toJSON();

      expect(json.details).toBeUndefined();
    });
  });

  describe('DatabaseError', () => {
    it('DatabaseErrorを作成できる', () => {
      const error = new DatabaseError('データベースエラー');

      expect(error).toBeInstanceOf(Error);
      expect(error).toBeInstanceOf(KijukuDBError);
      expect(error).toBeInstanceOf(DatabaseError);
      expect(error.name).toBe('DatabaseError');
      expect(error.message).toBe('データベースエラー');
      expect(error.code).toBe(ErrorCode.DB_QUERY_FAILED);
    });

    it('detailsを含むDatabaseErrorを作成できる', () => {
      const details = { query: 'SELECT * FROM media' };
      const error = new DatabaseError('クエリ失敗', details);

      expect(error.details).toEqual(details);
    });
  });

  describe('ValidationError', () => {
    it('ValidationErrorを作成できる', () => {
      const error = new ValidationError('バリデーションエラー');

      expect(error).toBeInstanceOf(Error);
      expect(error).toBeInstanceOf(KijukuDBError);
      expect(error).toBeInstanceOf(ValidationError);
      expect(error.name).toBe('ValidationError');
      expect(error.message).toBe('バリデーションエラー');
      expect(error.code).toBe(ErrorCode.VALIDATION_INVALID_VALUE);
    });

    it('detailsを含むValidationErrorを作成できる', () => {
      const details = { field: 'title', expectedType: 'string' };
      const error = new ValidationError('型エラー', details);

      expect(error.details).toEqual(details);
    });
  });

  describe('NotFoundError', () => {
    it('NotFoundErrorを作成できる', () => {
      const error = new NotFoundError('リソースが見つかりません');

      expect(error).toBeInstanceOf(Error);
      expect(error).toBeInstanceOf(KijukuDBError);
      expect(error).toBeInstanceOf(NotFoundError);
      expect(error.name).toBe('NotFoundError');
      expect(error.message).toBe('リソースが見つかりません');
      expect(error.code).toBe(ErrorCode.NOT_FOUND_MEDIA);
    });

    it('detailsを含むNotFoundErrorを作成できる', () => {
      const details = { id: 123 };
      const error = new NotFoundError('メディアが見つかりません', details);

      expect(error.details).toEqual(details);
    });
  });

  describe('ConflictError', () => {
    it('ConflictErrorを作成できる', () => {
      const error = new ConflictError('重複エラー');

      expect(error).toBeInstanceOf(Error);
      expect(error).toBeInstanceOf(KijukuDBError);
      expect(error).toBeInstanceOf(ConflictError);
      expect(error.name).toBe('ConflictError');
      expect(error.message).toBe('重複エラー');
      expect(error.code).toBe(ErrorCode.CONFLICT_DUPLICATE_KEY);
    });

    it('detailsを含むConflictErrorを作成できる', () => {
      const details = { key: 'path', value: '/test.jpg' };
      const error = new ConflictError('重複キー', details);

      expect(error.details).toEqual(details);
    });
  });
});

describe('エラーハンドラ関数', () => {
  describe('handleDatabaseError', () => {
    it('KijukuDBErrorはそのまま再スローされる', () => {
      const originalError = new ValidationError('元のエラー');

      expect(() => handleDatabaseError(originalError)).toThrow(ValidationError);
      expect(() => handleDatabaseError(originalError)).toThrow('元のエラー');
    });

    it('SQLite UNIQUE制約エラーはConflictErrorに変換される', () => {
      const sqliteError = {
        code: 'SQLITE_CONSTRAINT',
        message: 'UNIQUE constraint failed',
      };

      expect(() => handleDatabaseError(sqliteError)).toThrow(ConflictError);
    });

    it('UNIQUE制約エラー（メッセージベース）はConflictErrorに変換される', () => {
      const sqliteError = {
        message: 'UNIQUE constraint failed: media.path',
      };

      expect(() => handleDatabaseError(sqliteError)).toThrow(ConflictError);
    });

    it('その他のSQLiteエラーはDatabaseErrorに変換される', () => {
      const sqliteError = {
        code: 'SQLITE_ERROR',
        message: 'SQL error',
      };

      expect(() => handleDatabaseError(sqliteError)).toThrow(DatabaseError);
    });

    it('contextを含むエラーメッセージを作成できる', () => {
      const sqliteError = {
        message: 'table not found',
      };

      expect(() => handleDatabaseError(sqliteError, 'createMedia')).toThrow(
        'createMedia: table not found'
      );
    });

    it('エラーにSQLiteコードが含まれる場合、detailsに追加される', () => {
      const sqliteError = {
        code: 'SQLITE_ERROR',
        message: 'SQL error',
      };

      try {
        handleDatabaseError(sqliteError);
      } catch (error) {
        expect(error).toBeInstanceOf(DatabaseError);
        expect((error as DatabaseError).details?.sqliteCode).toBe('SQLITE_ERROR');
      }
    });
  });

  describe('validateRequired', () => {
    it('値が存在する場合、エラーをスローしない', () => {
      expect(() => validateRequired('test', 'title')).not.toThrow();
      expect(() => validateRequired(0, 'count')).not.toThrow();
      expect(() => validateRequired(false, 'flag')).not.toThrow();
    });

    it('undefinedの場合、ValidationErrorをスローする', () => {
      expect(() => validateRequired(undefined, 'title')).toThrow(ValidationError);
      expect(() => validateRequired(undefined, 'title')).toThrow(/必須フィールド「title」/);
    });

    it('nullの場合、ValidationErrorをスローする', () => {
      expect(() => validateRequired(null, 'media_type')).toThrow(ValidationError);
      expect(() => validateRequired(null, 'media_type')).toThrow(
        /必須フィールド「media_type」/
      );
    });

    it('エラーにフィールド名とエラーコードが含まれる', () => {
      try {
        validateRequired(undefined, 'test_field');
      } catch (error) {
        expect(error).toBeInstanceOf(ValidationError);
        expect((error as ValidationError).details?.field).toBe('test_field');
        expect((error as ValidationError).details?.code).toBe(
          ErrorCode.VALIDATION_REQUIRED_FIELD
        );
      }
    });
  });

  describe('validateType', () => {
    it('正しい型の場合、エラーをスローしない', () => {
      expect(() => validateType('test', 'title', 'string')).not.toThrow();
      expect(() => validateType(123, 'count', 'number')).not.toThrow();
      expect(() => validateType(true, 'flag', 'boolean')).not.toThrow();
    });

    it('型が不正な場合、ValidationErrorをスローする', () => {
      expect(() => validateType(123, 'title', 'string')).toThrow(ValidationError);
      expect(() => validateType(123, 'title', 'string')).toThrow(/型が不正/);
    });

    it('エラーに期待型と実際型が含まれる', () => {
      try {
        validateType(123, 'title', 'string');
      } catch (error) {
        expect(error).toBeInstanceOf(ValidationError);
        const details = (error as ValidationError).details;
        expect(details?.field).toBe('title');
        expect(details?.expectedType).toBe('string');
        expect(details?.actualType).toBe('number');
        expect(details?.code).toBe(ErrorCode.VALIDATION_INVALID_TYPE);
      }
    });
  });

  describe('validateMediaType', () => {
    it('有効なメディアタイプの場合、エラーをスローしない', () => {
      expect(() => validateMediaType('comic')).not.toThrow();
      expect(() => validateMediaType('video')).not.toThrow();
      expect(() => validateMediaType('music')).not.toThrow();
    });

    it('無効なメディアタイプの場合、ValidationErrorをスローする', () => {
      expect(() => validateMediaType('invalid')).toThrow(ValidationError);
      expect(() => validateMediaType('invalid')).toThrow(/無効なメディアタイプ/);
    });

    it('エラーに無効な値と有効な値のリストが含まれる', () => {
      try {
        validateMediaType('book');
      } catch (error) {
        expect(error).toBeInstanceOf(ValidationError);
        const details = (error as ValidationError).details;
        expect(details?.field).toBe('media_type');
        expect(details?.value).toBe('book');
        expect(details?.validValues).toEqual(['comic', 'video', 'music']);
        expect(details?.code).toBe(ErrorCode.VALIDATION_INVALID_VALUE);
      }
    });
  });
});
