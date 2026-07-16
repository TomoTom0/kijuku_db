/**
 * Kijuku DB - メディア情報管理SDK
 */
import Database from 'better-sqlite3';
import fs from 'node:fs';
import type {
  Media,
  MediaInput,
  MediaFilter,
  QueryOptions,
  Tag,
  TagUsageStats,
  DBOptions,
  MediaAttribute,
  BulkUpdateItem,
  MediaHash,
  MediaHashInput,
  ComputeHashResult,
  MediaTagAssoc,
  BackupDiff,
  DiffOptions,
} from './types.js';
import { computeBackupDiff, type BackupSnapshot } from './diff.js';
import * as migration from './migration.js';
import * as crud from './crud.js';
import * as tag from './tag.js';
import * as search from './search.js';
import * as bulk from './bulk.js';
import * as attribute from './attribute.js';
import * as hashModule from './hash.js';
import { BackupManager, BackupSelector } from './backup.js';
import type { BackupInfo, BackupMetaEntry } from './backup.js';
import * as updateExistModule from './update_exist.js';
import * as thumbnailModule from './thumbnail.js';
import * as fileOps from './file_ops.js';
import * as trash from './trash.js';
import { requireMediaRoot } from './media_path.js';
import type { FileOpOptions, FileOpResult } from './file_ops.js';
import type { TrashEntry, TrashOperation } from './trash.js';
import type {
  ThumbnailOptions,
  CheckThumbnailResult,
  UpdateThumbnailResult,
} from './types.js';

export * from './types.js';
export type { UpdateExistOptions, UpdateExistItemResult, UpdateExistResult } from './update_exist.js';
export * from './errors.js';
export * from './remote.js';
export * from './media_path.js';
export * from './trash.js';
export * from './file_ops.js';
export { BackupManager, BackupSelector, defaultRetentionPolicy } from './backup.js';
export type {
  BackupInfo,
  BackupKind,
  BackupScope,
  AutoRecord,
  RetentionPolicy,
  RetentionTier,
  BackupOptions,
} from './backup.js';
export { loadConfig, globalConfigPath, defaultKijukuConfig, defaultBackupConfig, parseTarget, resolveTarget, systemEnv } from './config.js';
export type { KijukuConfig, BackupConfig, RetentionTierConfig, Target, TargetResolution, EnvGetter } from './config.js';
export { ALLOWED_DISTINCT_FIELDS } from './search.js';
export { hexToBytes, bytesToHex } from './hash.js';
export { startServer } from './server/index.js';
export { AuthManager, generatePassword } from './server/auth.js';

/**
 * Kijuku DBのメインクラス
 */
export class KijukuDB {
  private db: Database.Database;
  private readonly dbPath: string;
  private readonly mediaRoot?: string;
  /** readonly（prod 読込）接続か。WAL 設定・migrate をスキップ（設計 §5.1・§5.3）。 */
  private readonly isReadOnly: boolean;
  private backupManager?: BackupManager;

  constructor(dbPath: string, options?: DBOptions) {
    this.dbPath = dbPath;
    this.isReadOnly = options?.readonly ?? false;
    this.db = new Database(dbPath, {
      timeout: options?.timeout ?? 5000,
      readonly: this.isReadOnly,
      verbose: options?.verbose ? console.log : undefined,
    });

    // SQLite設定
    this.db.pragma('foreign_keys = ON');
    // WAL は RW 接続のみ。readonly で journal_mode=WAL は書込を伴い失敗する（設計 §5.3）。
    if (!this.isReadOnly) {
      this.db.pragma('journal_mode = WAL');
    }

    // バックアップマネージャーの初期化（合意に基づきデフォルトで有効）
    if (options?.backup !== null) {
      this.backupManager = new BackupManager(this.db, dbPath, options?.backup ?? {});
    }

    // media root（ファイル操作APIのサンドボックス境界）
    this.mediaRoot = options?.mediaRoot;
  }

  /**
   * 設定された media root を返す（未設定は undefined）。
   *
   * ファイル操作API（cp/mv/sync/upload/download 等）のサンドボックス境界。
   * 未設定の場合、ファイル操作APIはエラーで拒否される。
   */
  getMediaRoot(): string | undefined {
    return this.mediaRoot;
  }

  /** src を dst へ複製する（dry-run ファースト、上書きは trash 経由）。 */
  mediaCp(
    src: string,
    dst: string,
    opts: FileOpOptions = fileOps.defaultFileOpOptions
  ): FileOpResult {
    const root = requireMediaRoot(this.mediaRoot);
    return fileOps.mediaCp(root, src, dst, opts);
  }

  /** src を dst へ移動する（dry-run ファースト、上書きは trash 経由）。 */
  mediaMv(
    src: string,
    dst: string,
    opts: FileOpOptions = fileOps.defaultFileOpOptions
  ): FileOpResult {
    const root = requireMediaRoot(this.mediaRoot);
    return fileOps.mediaMv(root, src, dst, opts);
  }

  /** src（ディレクトリ）の内容を dst へ同期する（safe モード、dry-run ファースト、余分・上書きは trash 経由）。 */
  mediaSync(
    src: string,
    dst: string,
    opts: FileOpOptions = fileOps.defaultFileOpOptions
  ): FileOpResult {
    const root = requireMediaRoot(this.mediaRoot);
    return fileOps.mediaSync(root, src, dst, opts);
  }

  /** targetRel を trash へ移動する（論理削除・物理削除はしない）。 */
  moveToTrash(targetRel: string, operation: TrashOperation, reason?: string): string {
    const root = requireMediaRoot(this.mediaRoot);
    return trash.moveToTrash(root, targetRel, operation, reason);
  }

  /** trash 内のエントリ一覧を返す。 */
  listTrash(): TrashEntry[] {
    const root = requireMediaRoot(this.mediaRoot);
    return trash.listTrash(root);
  }

  /** trash から id のエントリを復元する（衝突時はエラー）。 */
  restoreFromTrash(id: string): string {
    const root = requireMediaRoot(this.mediaRoot);
    return trash.restoreFromTrash(root, id);
  }

  /** trash 内のエントリを物理削除する（dry-run ファースト）。 */
  purgeTrash(ids?: string[], dryRun = false): string[] {
    const root = requireMediaRoot(this.mediaRoot);
    return trash.purgeTrash(root, ids, dryRun);
  }

  /**
   * マイグレーションを実行
   *
   * readonly（prod 読込）接続では拒否する（設計 §5.1「prod 読込経路は migrate skip」）。
   * CLI の target 解決（resolveTarget）でも prod は shouldMigrate=false となるが、
   * SDK 直接呼出に対する構造的保護としてここでも明示的に拒否する。
   */
  migrate(): void {
    if (this.isReadOnly) {
      throw new Error(
        'migrate is not allowed on a readonly (prod) connection (設計 §5.1)',
      );
    }
    // migrate 前に snapshot（設計 §7.3「migrate 前 snapshot 必須」・Rust と同等）
    this.backupManager?.createPreMigrateSnapshot();
    migration.migrate(this.db);
  }

  /**
   * prod(RO) → stg(RW) のフル複製（sync・設計 §4.2/§4.5）。
   *
   * `BackupManager.copyDbOnline`（Online Backup API）で src を読み取り専用コピーし dst を
   * 新規生成する。既存 dst と WAL/SHM 副産物（`-wal`/`-shm`）は事前に削除し、stg が WAL モードで
   * 使用されていた場合の残留を排除して完全な複製を保証する。Rust の `KijukuDB::replicate_db` と同等。
   *
   * **排他前提**: dst（stg）に接続中のプロセスがないこと（呼出側の責任・設計 §15-11 P1）。
   * `src === dst` は誤設定としてエラー。
   */
  static async replicateDb(src: string, dst: string): Promise<void> {
    if (src === dst) {
      throw new Error(`sync source and destination are the same path: ${src}`);
    }
    for (const suffix of ['', '-wal', '-shm']) {
      const candidate = `${dst}${suffix}`;
      if (fs.existsSync(candidate)) {
        fs.unlinkSync(candidate);
      }
    }
    await BackupManager.copyDbOnline(src, dst);
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

  getDistinctValues(fields: string[], filter: MediaFilter): (string | null)[][] {
    return search.getDistinctValues(this.db, fields, filter);
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
   * 複数メディアのタグを一括取得（N+1回避。タグなしメディアはエントリに含まれない）
   */
  getMediaTagsBulk(mediaIds: number[]): Record<number, Tag[]> {
    return tag.getMediaTagsBulk(this.db, mediaIds);
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
   * タグの使用数統計を取得
   */
  getTagUsageStats(): TagUsageStats[] {
    return tag.getTagUsageStats(this.db);
  }

  /**
   * 未使用のタグを取得
   */
  findUnusedTags(): Tag[] {
    return tag.findUnusedTags(this.db);
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

  // ========== メディアハッシュ操作 ==========

  addMediaHash(input: MediaHashInput): MediaHash {
    return hashModule.addMediaHash(this.db, input);
  }

  addMediaHashes(inputs: MediaHashInput[]): MediaHash[] {
    return hashModule.addMediaHashes(this.db, inputs);
  }

  getMediaHashes(itemUuid: string): MediaHash[] {
    return hashModule.getMediaHashes(this.db, itemUuid);
  }

  getMediaHash(itemUuid: string, filename: string, timeRange: string): MediaHash | null {
    return hashModule.getMediaHash(this.db, itemUuid, filename, timeRange);
  }

  findByContentHash(hashBytes: Uint8Array): MediaHash[] {
    return hashModule.findByContentHash(this.db, hashBytes);
  }

  deleteMediaHash(itemUuid: string, filename: string, timeRange: string): void {
    hashModule.deleteMediaHash(this.db, itemUuid, filename, timeRange);
  }

  deleteMediaHashes(itemUuid: string): void {
    hashModule.deleteMediaHashes(this.db, itemUuid);
  }

  findDuplicateHashes(): Array<{ content_hash: Uint8Array; count: number }> {
    return hashModule.findDuplicateHashes(this.db);
  }

  computeMediaHash(
    itemUuid: string,
    mediaPath: string,
    mediaType: string,
    durationSec?: number
  ): ComputeHashResult {
    return hashModule.computeMediaHash(this.db, itemUuid, mediaPath, mediaType, durationSec);
  }

  computeMediaHashes(filter: MediaFilter, options?: QueryOptions, force?: boolean): ComputeHashResult[] {
    return hashModule.computeMediaHashes(this.db, filter, options, force);
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

  /** 手動バックアップを実行（ラベルなし） */
  async backup(): Promise<string | undefined> {
    return this.backupManager?.backup(undefined);
  }

  /**
   * ラベル付き手動バックアップを実行
   * @param label バックアップのラベル（例: "before_import"）
   */
  async backupWithLabel(label: string): Promise<string | undefined> {
    return this.backupManager?.backup(label);
  }

  /**
   * バックアップを現在のDBに復元する
   *
   * 指定したバックアップの内容を現在のDB接続に上書きします。
   *
   * @param selector 復元するバックアップの選択条件（省略時は最新）
   * @returns 復元に使用したバックアップファイルのパス
   */
  restore(selector: BackupSelector = BackupSelector.latest()): string {
    if (!this.backupManager) {
      throw new Error('Backup manager not configured');
    }
    const restoredPath = this.backupManager.restore(selector);
    // restore後にBackupManagerが接続を再オープンするため、KijukuDBの参照も更新
    this.db = this.backupManager.getDb();
    return restoredPath;
  }

  /** バックアップ一覧を取得 */
  listBackups(): BackupInfo[] {
    return this.backupManager?.listBackups() ?? [];
  }

  /** バックアップマネージャーを取得 */
  getBackupManager(): BackupManager | undefined {
    return this.backupManager;
  }

  /**
   * フィルタで絞り込んだメディアのflag_existをファイル存在状態に基づいて更新する
   */
  updateExist(
    filter: MediaFilter,
    options?: QueryOptions,
    updateOptions: updateExistModule.UpdateExistOptions = { dry_run: false }
  ): updateExistModule.UpdateExistResult {
    return updateExistModule.updateExist(this.db, filter, options, updateOptions);
  }

  /**
   * フィルタで絞り込んだメディアのサムネイル状態をチェックする
   */
  checkThumbnail(
    filter: MediaFilter = {},
    options?: QueryOptions,
  ): CheckThumbnailResult {
    return thumbnailModule.checkThumbnail(this.db, filter, options);
  }

  /**
   * フィルタで絞り込んだメディアのサムネイルを生成・更新する
   */
  updateThumbnail(
    filter: MediaFilter = {},
    options?: QueryOptions,
    thumbnailOptions: ThumbnailOptions = {},
  ): UpdateThumbnailResult {
    const result = thumbnailModule.updateThumbnail(this.db, filter, options, thumbnailOptions);
    if (result.generated > 0) {
      this.backupManager?.recordOperation().catch((err) => {
        console.error('Backup operation failed:', err);
      });
    }
    return result;
  }

  /**
   * データベース接続を閉じる
   */
  close(): void {
    this.db.close();
  }

  // ========== バックアップからの取得メソッド ==========

  /**
   * バックアップDBに対してコールバックを実行するヘルパーメソッド
   *
   * バックアップファイルを読み取り専用で開き、コールバックを実行します。
   * コールバック完了後、DBを自動的にクローズします。
   */
  private withBackupDb<T>(
    selector: BackupSelector,
    callback: (db: KijukuDB) => T
  ): T {
    if (!this.backupManager) {
      throw new Error('Backup manager not configured');
    }
    const backupInfo = this.backupManager.selectBackup(selector);
    if (!backupInfo) {
      throw new Error('No backup found matching selector');
    }

    // 差分バックアップの場合は基底フルから一時フルDBを再構成してから開く。
    // 一時ファイルはコールバック終了後（成功・失敗問わず）に削除する。
    let dbPath: string;
    let tempPath: string | null = null;
    if (backupInfo.kind.type === 'diff') {
      tempPath = this.backupManager.resolveDiffBackupToTempFull(backupInfo);
      dbPath = tempPath;
    } else {
      dbPath = backupInfo.path;
    }

    const backupDb = new KijukuDB(dbPath, { readonly: true });
    try {
      return callback(backupDb);
    } finally {
      backupDb.close();
      if (tempPath) {
        // WAL モードの DB を開くと -wal/-shm 副産物が作られるため全て削除する
        for (const p of [tempPath, `${tempPath}-wal`, `${tempPath}-shm`]) {
          try {
            fs.rmSync(p);
          } catch {
            /* ignore */
          }
        }
      }
    }
  }

  /**
   * バックアップからIDでメディアを取得
   */
  getMediaFromBackup(
    id: number,
    selector: BackupSelector = BackupSelector.latest()
  ): Media | null {
    return this.withBackupDb(selector, (backupDb) => backupDb.getMedia(id));
  }

  /**
   * バックアップからメディアを検索
   */
  findMediaFromBackup(
    filter: MediaFilter,
    options?: QueryOptions,
    selector: BackupSelector = BackupSelector.latest()
  ): Media[] {
    return this.withBackupDb(selector, (backupDb) =>
      backupDb.findMedia(filter, options)
    );
  }

  /**
   * バックアップからタグ名でタグを取得
   */
  getTagByNameFromBackup(
    name: string,
    selector: BackupSelector = BackupSelector.latest()
  ): Tag | null {
    return this.withBackupDb(selector, (backupDb) => backupDb.getTagByName(name));
  }

  /**
   * バックアップから全てのタグを取得
   */
  getAllTagsFromBackup(selector: BackupSelector = BackupSelector.latest()): Tag[] {
    return this.withBackupDb(selector, (backupDb) => backupDb.getAllTags());
  }

  /**
   * バックアップからメディアに関連付けられたタグを取得
   */
  getMediaTagsFromBackup(
    mediaId: number,
    selector: BackupSelector = BackupSelector.latest()
  ): Tag[] {
    return this.withBackupDb(selector, (backupDb) => backupDb.getMediaTags(mediaId));
  }

  /**
   * バックアップからメディアの属性を取得
   */
  getMediaAttributeFromBackup(
    mediaId: number,
    key: string,
    selector: BackupSelector = BackupSelector.latest()
  ): MediaAttribute | null {
    return this.withBackupDb(selector, (backupDb) =>
      backupDb.getMediaAttribute(mediaId, key)
    );
  }

  /**
   * バックアップからメディアの全ての属性を取得
   */
  getMediaAttributesFromBackup(
    mediaId: number,
    selector: BackupSelector = BackupSelector.latest()
  ): MediaAttribute[] {
    return this.withBackupDb(selector, (backupDb) =>
      backupDb.getMediaAttributes(mediaId)
    );
  }

  /**
   * バックアップから特定作品の全ハッシュを取得
   */
  getMediaHashesFromBackup(
    itemUuid: string,
    selector: BackupSelector = BackupSelector.latest()
  ): MediaHash[] {
    return this.withBackupDb(selector, (backupDb) =>
      backupDb.getMediaHashes(itemUuid)
    );
  }

  /**
   * バックアップから特定位置のハッシュを取得
   */
  getMediaHashFromBackup(
    itemUuid: string,
    filename: string,
    timeRange: string,
    selector: BackupSelector = BackupSelector.latest()
  ): MediaHash | null {
    return this.withBackupDb(selector, (backupDb) =>
      backupDb.getMediaHash(itemUuid, filename, timeRange)
    );
  }

  /**
   * バックアップからSHA256による完全一致検索
   */
  findByContentHashFromBackup(
    hashBytes: Uint8Array,
    selector: BackupSelector = BackupSelector.latest()
  ): MediaHash[] {
    return this.withBackupDb(selector, (backupDb) =>
      backupDb.findByContentHash(hashBytes)
    );
  }

  /**
   * バックアップから重複ハッシュを検出
   * 戻り型は findDuplicateHashes() に準じます（{ content_hash, count } の配列）。
   */
  findDuplicateHashesFromBackup(selector: BackupSelector = BackupSelector.latest()) {
    return this.withBackupDb(selector, (backupDb) =>
      backupDb.findDuplicateHashes()
    );
  }

  /**
   * 現在DBのスナップショットを取得（差分比較用・内部）
   */
  private collectSnapshot(): BackupSnapshot {
    return {
      media: search.findMedia(this.db, {}, undefined),
      tags: tag.getAllTags(this.db),
      mediaTags: tag.getAllMediaTags(this.db),
      attributes: attribute.getAllMediaAttributes(this.db),
      hashes: hashModule.getAllMediaHashes(this.db),
    };
  }

  /**
   * バックアップと現在DBの差分を取得（復元判断用）
   *
   * - added:   バックアップに在り現在に無い（復元で復活する）
   * - removed: 現在に在りバックアップに無い（復元で失われる）
   * - changed: 両方に在り内容が異なる（復元で上書きされる）
   */
  diffWithBackup(
    selector: BackupSelector = BackupSelector.latest(),
    options: DiffOptions = {},
  ): BackupDiff {
    const backupSnap = this.withBackupDb(selector, (bdb) => bdb.collectSnapshot());
    const currentSnap = this.collectSnapshot();
    return computeBackupDiff(currentSnap, backupSnap, options);
  }

  /** バックアップにラベルを付与（事後）。undefined でクリア */
  setBackupLabel(id: string, label: string | undefined): void {
    if (!this.backupManager) throw new Error('Backup manager not configured');
    this.backupManager.setBackupLabel(id, label);
  }

  /** バックアップにメモを付与（事後）。undefined でクリア */
  setBackupNote(id: string, note: string | undefined): void {
    if (!this.backupManager) throw new Error('Backup manager not configured');
    this.backupManager.setBackupNote(id, note);
  }

  /** バックアップの事後メタを取得 */
  getBackupMeta(id: string): BackupMetaEntry | null {
    return this.backupManager?.getBackupMeta(id) ?? null;
  }
}
