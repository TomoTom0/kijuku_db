/**
 * きじゅくDB カスタムエラー定義
 */

/**
 * エラーコード一覧
 */
export enum ErrorCode {
  // データベースエラー (DB-xxx)
  DB_CONNECTION_FAILED = 'DB_CONNECTION_FAILED',
  DB_QUERY_FAILED = 'DB_QUERY_FAILED',
  DB_TRANSACTION_FAILED = 'DB_TRANSACTION_FAILED',

  // データ検証エラー (VALIDATION-xxx)
  VALIDATION_REQUIRED_FIELD = 'VALIDATION_REQUIRED_FIELD',
  VALIDATION_INVALID_TYPE = 'VALIDATION_INVALID_TYPE',
  VALIDATION_INVALID_VALUE = 'VALIDATION_INVALID_VALUE',

  // データ存在エラー (NOT_FOUND-xxx)
  NOT_FOUND_MEDIA = 'NOT_FOUND_MEDIA',
  NOT_FOUND_TAG = 'NOT_FOUND_TAG',

  // データ重複エラー (CONFLICT-xxx)
  CONFLICT_DUPLICATE_KEY = 'CONFLICT_DUPLICATE_KEY',
  CONFLICT_ALREADY_EXISTS = 'CONFLICT_ALREADY_EXISTS',

  // その他のエラー (UNKNOWN)
  UNKNOWN_ERROR = 'UNKNOWN_ERROR',
}

/**
 * エラーの基底クラス
 */
export class KijukuDBError extends Error {
  public readonly code: ErrorCode;
  public readonly details?: Record<string, any>;

  constructor(
    message: string,
    code: ErrorCode = ErrorCode.UNKNOWN_ERROR,
    details?: Record<string, any>
  ) {
    super(message);
    this.name = 'KijukuDBError';
    this.code = code;
    this.details = details;

    Object.setPrototypeOf(this, KijukuDBError.prototype);
  }

  toJSON() {
    return {
      name: this.name,
      message: this.message,
      code: this.code,
      details: this.details,
    };
  }
}

/**
 * データベースエラー
 */
export class DatabaseError extends KijukuDBError {
  constructor(message: string, details?: Record<string, any>) {
    super(message, ErrorCode.DB_QUERY_FAILED, details);
    this.name = 'DatabaseError';
    Object.setPrototypeOf(this, DatabaseError.prototype);
  }
}

/**
 * バリデーションエラー
 */
export class ValidationError extends KijukuDBError {
  constructor(message: string, details?: Record<string, any>) {
    super(message, ErrorCode.VALIDATION_INVALID_VALUE, details);
    this.name = 'ValidationError';
    Object.setPrototypeOf(this, ValidationError.prototype);
  }
}

/**
 * データが見つからないエラー
 */
export class NotFoundError extends KijukuDBError {
  constructor(message: string, details?: Record<string, any>) {
    super(message, ErrorCode.NOT_FOUND_MEDIA, details);
    this.name = 'NotFoundError';
    Object.setPrototypeOf(this, NotFoundError.prototype);
  }
}

/**
 * データ重複エラー
 */
export class ConflictError extends KijukuDBError {
  constructor(message: string, details?: Record<string, any>) {
    super(message, ErrorCode.CONFLICT_DUPLICATE_KEY, details);
    this.name = 'ConflictError';
    Object.setPrototypeOf(this, ConflictError.prototype);
  }
}

/**
 * SQLiteエラーをきじゅくDBエラーに変換
 */
export function handleDatabaseError(error: any, context?: string): never {
  if (error instanceof KijukuDBError) {
    throw error;
  }

  const message = context
    ? `${context}: ${error.message || 'Unknown database error'}`
    : error.message || 'Unknown database error';

  const details: Record<string, any> = {
    originalError: error.message,
  };

  if (error.code) {
    details.sqliteCode = error.code;
  }

  // SQLiteエラーコードに基づいて適切なエラーにマッピング
  if (error.code === 'SQLITE_CONSTRAINT' || error.message?.includes('UNIQUE constraint')) {
    throw new ConflictError(message, details);
  }

  throw new DatabaseError(message, details);
}

/**
 * 必須フィールドのバリデーション
 */
export function validateRequired<T>(
  value: T | undefined | null,
  fieldName: string
): asserts value is T {
  if (value === undefined || value === null) {
    throw new ValidationError(`必須フィールド「${fieldName}」が指定されていません`, {
      field: fieldName,
      code: ErrorCode.VALIDATION_REQUIRED_FIELD,
    });
  }
}

/**
 * 値の型チェック
 */
export function validateType(
  value: any,
  fieldName: string,
  expectedType: string
): void {
  const actualType = typeof value;
  if (actualType !== expectedType) {
    throw new ValidationError(
      `フィールド「${fieldName}」の型が不正です。期待: ${expectedType}, 実際: ${actualType}`,
      {
        field: fieldName,
        expectedType,
        actualType,
        code: ErrorCode.VALIDATION_INVALID_TYPE,
      }
    );
  }
}

/**
 * メディアタイプのバリデーション
 */
export function validateMediaType(mediaType: string): void {
  const validTypes = ['comic', 'video', 'music'];
  if (!validTypes.includes(mediaType)) {
    throw new ValidationError(
      `無効なメディアタイプです: ${mediaType}。有効な値: ${validTypes.join(', ')}`,
      {
        field: 'media_type',
        value: mediaType,
        validValues: validTypes,
        code: ErrorCode.VALIDATION_INVALID_VALUE,
      }
    );
  }
}
