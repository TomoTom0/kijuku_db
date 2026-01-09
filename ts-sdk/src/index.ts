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
  MediaAttribute,
  BulkUpdateItem,
} from './types.js';
import * as migration from './migration.js';
import * as crud from './crud.js';
import * as tag from './tag.js';
import * as search from './search.js';
import * as bulk from './bulk.js';
import * as attribute from './attribute.js';
import { BackupManager, BackupSelector } from './backup.js';

export * from './types.js';
export * from './errors.js';
export * from './remote.js';
export { BackupManager, BackupSelector } from './backup.js';
export type { BackupInfo, BackupSelector as BackupSelectorType } from './backup.js';
export { startServer } from './server/index.js';
export { AuthManager, generatePassword } from './server/auth.js';

/**
 * Kijuku DBのメインクラス
 */
export class KijukuDB {
  private db: Database.Database;
  private backupManager?: BackupManager;

  constructor(dbPath: string, options?: DBOptions) {
    this.db = new Database(dbPath, {
      timeout: options?.timeout ?? 5000,
      readonly: options?.readonly ?? false,
      verbose: options?.verbose ? console.log : undefined,
    });

    // SQLite設定
    this.db.pragma('foreign_keys = ON');
    this.db.pragma('journal_mode = WAL');

    // バックアップマネージャーの初期化
    if (options?.backup) {
      this.backupManager = new BackupManager(this.db, dbPath, options.backup);
    }
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
    const result = crud.createMedia(this.db, data);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
    return result;
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
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * メディアを削除
   */
  deleteMedia(id: number): void {
    crud.deleteMedia(this.db, id);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
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
    const result = bulk.bulkCreateMedia(this.db, dataList);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
    return result;
  }

  /**
   * 複数のメディアを一括削除
   */
  bulkDeleteMedia(ids: number[]): void {
    bulk.bulkDeleteMedia(this.db, ids);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * 複数のメディアを一括更新
   */
  bulkUpdateMedia(updates: BulkUpdateItem[]): void {
    bulk.bulkUpdateMedia(this.db, updates);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * タグを作成
   */
  createTag(name: string): Tag {
    const result = tag.createTag(this.db, name);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
    return result;
  }

  /**
   * メディアにタグを追加
   */
  addTagToMedia(mediaId: number, tagId: number): void {
    tag.addTagToMedia(this.db, mediaId, tagId);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * メディアからタグを削除
   */
  removeTagFromMedia(mediaId: number, tagId: number): void {
    tag.removeTagFromMedia(this.db, mediaId, tagId);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
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
   * メディアに属性を設定
   */
  setMediaAttribute(
    mediaId: number,
    key: string,
    value: string | null,
    valueType?: string
  ): void {
    attribute.setMediaAttribute(this.db, mediaId, key, value, valueType);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * メディアの属性を取得
   */
  getMediaAttribute(mediaId: number, key: string): MediaAttribute | null {
    return attribute.getMediaAttribute(this.db, mediaId, key);
  }

  /**
   * メディアの全ての属性を取得
   */
  getMediaAttributes(mediaId: number): MediaAttribute[] {
    return attribute.getMediaAttributes(this.db, mediaId);
  }

  /**
   * メディアの属性を削除
   */
  deleteMediaAttribute(mediaId: number, key: string): void {
    attribute.deleteMediaAttribute(this.db, mediaId, key);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * メディアの全ての属性を削除
   */
  deleteAllMediaAttributes(mediaId: number): void {
    attribute.deleteAllMediaAttributes(this.db, mediaId);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * トランザクションを実行
   */
  transaction<T>(fn: () => T): T {
    const result = this.db.transaction(fn)();
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
    return result;
  }

  /**
   * 手動でバックアップを実行
   */
  async backup(): Promise<string | undefined> {
    return this.backupManager?.backup();
  }

  /**
   * バックアップ一覧を取得
   */
  listBackups(): Array<{ name: string; path: string; createdAt: Date }> {
    return this.backupManager?.listBackups() ?? [];
  }

  /**
   * バックアップマネージャーを取得
   */
  getBackupManager(): BackupManager | undefined {
    return this.backupManager;
  }

  /**
   * データベース接続を閉じる
   */
  close(): void {
    this.db.close();
  }

  // ========== バックアップからの取得メソッド ==========

  /**
   * バックアップDBに対して処理を実行するヘルパーメソッド
   */
  private withBackupDb<T>(
    selector: BackupSelector,
    fn: (db: KijukuDB) => T
  ): T {
    if (!this.backupManager) {
      throw new Error('Backup manager not configured');
    }

    const backupPath = this.backupManager.getBackupPath(selector);
    if (!backupPath) {
      throw new Error('No backup found matching selector');
    }

    const backupDb = new KijukuDB(backupPath, { readonly: true });
    try {
      return fn(backupDb);
    } finally {
      backupDb.close();
    }
  }

  /**
   * バックアップからIDでメディアを取得
   */
  getMediaFromBackup(
    id: number,
    selector: BackupSelector = BackupSelector.latest()
  ): Media | null {
    return this.withBackupDb(selector, (db) => db.getMedia(id));
  }

  /**
   * バックアップからメディアを検索
   */
  findMediaFromBackup(
    filter: MediaFilter,
    options?: QueryOptions,
    selector: BackupSelector = BackupSelector.latest()
  ): Media[] {
    return this.withBackupDb(selector, (db) => db.findMedia(filter, options));
  }

  /**
   * バックアップからタグ名でタグを取得
   */
  getTagByNameFromBackup(
    name: string,
    selector: BackupSelector = BackupSelector.latest()
  ): Tag | null {
    return this.withBackupDb(selector, (db) => db.getTagByName(name));
  }

  /**
   * バックアップから全てのタグを取得
   */
  getAllTagsFromBackup(selector: BackupSelector = BackupSelector.latest()): Tag[] {
    return this.withBackupDb(selector, (db) => db.getAllTags());
  }

  /**
   * バックアップからメディアに関連付けられたタグを取得
   */
  getMediaTagsFromBackup(
    mediaId: number,
    selector: BackupSelector = BackupSelector.latest()
  ): Tag[] {
    return this.withBackupDb(selector, (db) => db.getMediaTags(mediaId));
  }

  /**
   * バックアップからメディアの属性を取得
   */
  getMediaAttributeFromBackup(
    mediaId: number,
    key: string,
    selector: BackupSelector = BackupSelector.latest()
  ): MediaAttribute | null {
    return this.withBackupDb(selector, (db) =>
      db.getMediaAttribute(mediaId, key)
    );
  }

  /**
   * バックアップからメディアの全ての属性を取得
   */
  getMediaAttributesFromBackup(
    mediaId: number,
    selector: BackupSelector = BackupSelector.latest()
  ): MediaAttribute[] {
    return this.withBackupDb(selector, (db) =>
      db.getMediaAttributes(mediaId)
    );
  }
}
