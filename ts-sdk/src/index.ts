/**
 * Kijuku DB - メディア情報管理SDK
 */
import Database from 'better-sqlite3';
import type {
  Media,
  MediaInput,
  MediaFilter,
  QueryOptions,
  Tag,
  DBOptions,
} from './types.js';
import * as migration from './migration.js';
import * as crud from './crud.js';
import * as tag from './tag.js';
import * as search from './search.js';
import * as bulk from './bulk.js';

export * from './types.js';

/**
 * Kijuku DBのメインクラス
 */
export class KijukuDB {
  private db: Database.Database;

  constructor(dbPath: string, options?: DBOptions) {
    this.db = new Database(dbPath, {
      timeout: options?.timeout ?? 5000,
      readonly: options?.readonly ?? false,
      verbose: options?.verbose ? console.log : undefined,
    });

    // SQLite設定
    this.db.pragma('foreign_keys = ON');
    this.db.pragma('journal_mode = WAL');
  }

  /**
   * マイグレーションを実行
   */
  migrate(): void {
    migration.migrate(this.db);
  }

  /**
   * 現在のスキーマバージョンを取得
   */
  getSchemaVersion(): number {
    return migration.getSchemaVersion(this.db);
  }

  /**
   * テーブル一覧を取得（テスト用）
   */
  getTables(): string[] {
    return migration.getTables(this.db);
  }

  /**
   * 外部キー制約が有効化されているか確認（テスト用）
   */
  isForeignKeysEnabled(): boolean {
    return migration.isForeignKeysEnabled(this.db);
  }

  /**
   * メディアを作成
   */
  createMedia(data: MediaInput): Media {
    return crud.createMedia(this.db, data);
  }

  /**
   * IDでメディアを取得
   */
  getMedia(id: number): Media | null {
    return crud.getMedia(this.db, id);
  }

  /**
   * メディアを更新
   */
  updateMedia(id: number, data: Partial<MediaInput>): void {
    crud.updateMedia(this.db, id, data);
  }

  /**
   * メディアを削除
   */
  deleteMedia(id: number): void {
    crud.deleteMedia(this.db, id);
  }

  /**
   * メディアを検索
   */
  findMedia(filter: MediaFilter, options?: QueryOptions): Media[] {
    return search.findMedia(this.db, filter, options);
  }

  /**
   * 複数のメディアを一括作成
   */
  bulkCreateMedia(dataList: MediaInput[]): Media[] {
    return bulk.bulkCreateMedia(this.db, dataList);
  }

  /**
   * タグを作成
   */
  createTag(name: string): Tag {
    return tag.createTag(this.db, name);
  }

  /**
   * メディアにタグを追加
   */
  addTagToMedia(mediaId: number, tagId: number): void {
    tag.addTagToMedia(this.db, mediaId, tagId);
  }

  /**
   * メディアからタグを削除
   */
  removeTagFromMedia(mediaId: number, tagId: number): void {
    tag.removeTagFromMedia(this.db, mediaId, tagId);
  }

  /**
   * メディアに関連付けられたタグを取得
   */
  getMediaTags(mediaId: number): Tag[] {
    return tag.getMediaTags(this.db, mediaId);
  }

  /**
   * タグ名でタグを取得
   */
  getTagByName(name: string): Tag | null {
    return tag.getTagByName(this.db, name);
  }

  /**
   * 全てのタグを取得
   */
  getAllTags(): Tag[] {
    return tag.getAllTags(this.db);
  }

  /**
   * トランザクションを実行
   */
  transaction<T>(fn: () => T): T {
    return this.db.transaction(fn)();
  }

  /**
   * データベース接続を閉じる
   */
  close(): void {
    this.db.close();
  }
}
