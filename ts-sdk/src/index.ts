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
  ObserveOptions,
  ObserveResult,
  PromoteOutcome,
  GateConfig,
  GoldenResult,
  AuditTarget,
  AuditResult,
  AuditRecord,
  AuditLogFilter,
} from './types.js';
import {
  computeBackupDiff,
  summarizeDiff,
  evaluateGate,
  defaultGateConfig,
  allGateChecksPassed,
  type BackupSnapshot,
} from './diff.js';
import * as migration from './migration.js';
import * as crud from './crud.js';
import * as tag from './tag.js';
import * as search from './search.js';
import * as bulk from './bulk.js';
import * as attribute from './attribute.js';
import * as hashModule from './hash.js';
import { BackupManager, BackupSelector } from './backup.js';
import type { BackupInfo, BackupMetaEntry, BackupOptions } from './backup.js';
import * as updateExistModule from './update_exist.js';
import * as thumbnailModule from './thumbnail.js';
import * as fileOps from './file_ops.js';
import * as trash from './trash.js';
import {
  acquireStgLock,
  acquireProdLock,
  ProdBusyError,
  PromoteGateFailedError,
  computeProdRevision,
  prodRevisionEqual,
  readStgMeta,
  writeStgMeta,
  type ProdRevision,
  type StgLock,
  type StgMeta,
  type SyncedFrom,
} from './stg-session.js';
import { requireMediaRoot } from './media_path.js';
import type { FileOpOptions, FileOpResult } from './file_ops.js';
import type { TrashEntry, TrashOperation } from './trash.js';
import type {
  ThumbnailOptions,
  CheckThumbnailResult,
  UpdateThumbnailResult,
} from './types.js';
import { RemoteKijukuDB } from './remote.js';
import { parseDbPath, type Target } from './config.js';

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
export { loadConfig, globalConfigPath, defaultKijukuConfig, defaultBackupConfig, parseTarget, resolveTarget, parseDbPath, systemEnv } from './config.js';
export type { KijukuConfig, BackupConfig, RetentionTierConfig, Target, TargetResolution, EnvGetter } from './config.js';
export { ALLOWED_DISTINCT_FIELDS } from './search.js';
export { hexToBytes, bytesToHex } from './hash.js';
export {
  acquireStgLock,
  acquireProdLock,
  computeProdRevision,
  readStgMeta,
  writeStgMeta,
  metaPath,
  lockPath,
  StgBusyError,
  ProdBusyError,
  PromoteGateFailedError,
  type ProdRevision,
  type StgLock,
  type StgMeta,
  type SyncedFrom,
} from './stg-session.js';
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
  /** stg 編集セッションが保持する排他ロック（設計 §15-11）。未取得は undefined。close() で解放。 */
  private stgLock?: StgLock;

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

  /**
   * readonly（prod RO 読込専用・設計 §5.1）セッションで書込操作を拒否する。
   * SDK 直接呼出に対する構造的保護（方式A・Rust の is_write_operation ガードと同等）。
   * SQLite の SQLITE_READONLY に先立ち、ユーザフレンドリなエラーを投げる。
   */
  private assertWritable(opName: string): void {
    if (this.isReadOnly) {
      throw new Error(
        `読込専用セッション（readonly）では書込操作 '${opName}' は実行できません。書込は stg セッション（--target stg）で実行してください（設計 §5.1/§6.2）`,
      );
    }
  }

  /**
   * (b) 制限操作（設計 §9.2）の実行を拒否する。TS Local は prod 直接 gate を持たないため (b) は常に拒否:
   * - stg セッション: (b) は環境が制限（拒否）・prod 直接経路（Remote CLI の --target prod）へ誘導。
   * - prod RO 読込セッション: assertWritable と同様に書込拒否（設計 §5.1）。
   * prod 直接の dry-run+trash+pre-stash gate 実行は Rust CLI 経由（Remote）が主経路。
   */
  private assertNotStgRestricted(opName: string): never {
    if (this.isReadOnly) {
      throw new Error(
        `読込専用セッション（readonly）では書込操作 '${opName}' は実行できません（設計 §5.1）。`,
      );
    }
    throw new Error(
      `(b) 制限操作 '${opName}' は stg では実行できません。prod 直接（Remote CLI の --target prod）で dry-run+trash+pre-stash gate 付きで実行してください（設計 §9.2）`,
    );
  }

  /** src を dst へ複製する（dry-run ファースト、上書きは trash 経由）。 */
  mediaCp(
    src: string,
    dst: string,
    opts: FileOpOptions = fileOps.defaultFileOpOptions
  ): FileOpResult {
    this.assertWritable('mediaCp');
    const root = requireMediaRoot(this.mediaRoot);
    return fileOps.mediaCp(root, src, dst, opts);
  }

  /** src を dst へ移動する（(b) 制限操作・dry-run ファースト・上書きは trash 経由・設計 §9.2）。 */
  mediaMv(
    src: string,
    dst: string,
    opts: FileOpOptions = fileOps.defaultFileOpOptions
  ): FileOpResult {
    this.assertNotStgRestricted('mediaMv');
    const root = requireMediaRoot(this.mediaRoot);
    return fileOps.mediaMv(root, src, dst, opts);
  }

  /** src（ディレクトリ）の内容を dst へ同期する（safe モード、dry-run ファースト、余分・上書きは trash 経由）。 */
  mediaSync(
    src: string,
    dst: string,
    opts: FileOpOptions = fileOps.defaultFileOpOptions
  ): FileOpResult {
    this.assertWritable('mediaSync');
    const root = requireMediaRoot(this.mediaRoot);
    return fileOps.mediaSync(root, src, dst, opts);
  }

  /** targetRel を trash へ移動する（論理削除・物理削除はしない）。 */
  moveToTrash(targetRel: string, operation: TrashOperation, reason?: string): string {
    this.assertWritable('moveToTrash');
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
    this.assertWritable('restoreFromTrash');
    const root = requireMediaRoot(this.mediaRoot);
    return trash.restoreFromTrash(root, id);
  }

  /** trash 内のエントリを物理削除する（(b) 制限操作・dry-run ファースト・設計 §9.2）。 */
  purgeTrash(ids?: string[], dryRun = false): string[] {
    this.assertNotStgRestricted('purgeTrash');
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
    this.assertWritable('migrate');
    // migrate 前に snapshot（設計 §7.3「migrate 前 snapshot 必須」・Rust と同等）
    this.backupManager?.createPreMigrateSnapshot();
    migration.migrate(this.db);
  }

  /**
   * prod 側 `backup/meta/audit.log` へ監査レコードを追記（設計 §10・best-effort・失敗は無視）。
   *  監査ログは prod DB 本体でなく prod 外サイドカー（promote 上書きの影響なし・§15-2）。
   *  `enabled:false` BackupManager で meta/ のみ生成して書く（pre-stash appender と同）。
   */
  private static appendAuditToProd(
    prodDbPath: string,
    operation: string,
    target: AuditTarget,
    result: AuditResult,
    error: string | null,
    summary: Record<string, unknown>,
  ): void {
    let prodDb: Database.Database | undefined;
    try {
      prodDb = new Database(prodDbPath, { readonly: true, fileMustExist: true });
      const bm = new BackupManager(prodDb, prodDbPath, { enabled: false });
      bm.appendAuditRecord({
        timestamp: new Date().toISOString(),
        operation,
        target,
        actor: process.env.USER ?? null,
        prodDbPath,
        result,
        error,
        summary,
      });
    } catch {
      // best-effort（監査書込失敗は操作結果に影響させない）
    } finally {
      prodDb?.close();
    }
  }

  /**
   * prod(RO) → stg(RW) のフル複製（sync・設計 §4.2/§4.5）。
   *
   * `BackupManager.copyDbOnline`（Online Backup API）で src を読み取り専用コピーし dst を
   * 新規生成する。既存 dst と WAL/SHM 副産物（`-wal`/`-shm`）は事前に削除し、stg が WAL モードで
   * 使用されていた場合の残留を排除して完全な複製を保証する。Rust の `KijukuDB::replicate_db` と同等。
   *
   * **排他**: 先頭で `<dst>.lock` の排他ロックを取得し（設計 §15-11）、sync 全体を保護する。
   * 別セッションが stg を編集中なら `StgBusyError`。コピー後に sync 元 prod revision 指紋を
   * `<dst>.meta.json` に記録し、observe が drift を検出できるようにする。
   * `src === dst` は誤設定としてエラー。
   */
  static async replicateDb(
    src: string,
    dst: string,
    operation: 'sync' | 'discard' = 'sync',
  ): Promise<void> {
    if (src === dst) {
      throw new Error(`sync source and destination are the same path: ${src}`);
    }
    const lock = acquireStgLock(dst);
    try {
      // sync 元 prod revision を copy **前**に src から算出する（Rust replicate_db と同順序）。
      // copy 後に算出すると copy 中の prod 更定で「記録 revision」と「実際に copy された snapshot」が
      // 乖離し、observe が drift を見逃して stale な promote を許す TOCTOU になる（設計 §15-11）。
      const revision = (() => {
        const srcDb = new KijukuDB(src, { readonly: true });
        try {
          return computeProdRevision(srcDb.db);
        } finally {
          srcDb.close();
        }
      })();

      for (const suffix of ['', '-wal', '-shm']) {
        const candidate = `${dst}${suffix}`;
        if (fs.existsSync(candidate)) {
          fs.unlinkSync(candidate);
        }
      }
      await BackupManager.copyDbOnline(src, dst);
      writeStgMeta(dst, {
        syncedFrom: { prodPath: src, revision, syncedAt: new Date().toISOString() },
      });
      // 監査: prod(src) 側 backup/meta/audit.log（設計 §10・best-effort）。
      KijukuDB.appendAuditToProd(src, operation, 'stg', 'success', null, {
        stgPath: dst,
        revision,
      });
    } finally {
      lock.release();
    }
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
    this.assertWritable('createMedia');
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
    this.assertWritable('updateMedia');
    crud.updateMedia(this.db, id, data);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * メディアを削除
   */
  deleteMedia(id: number): void {
    this.assertWritable('deleteMedia');
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
    this.assertWritable('bulkCreateMedia');
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
    this.assertWritable('bulkDeleteMedia');
    bulk.bulkDeleteMedia(this.db, ids);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * 複数のメディアを一括更新
   */
  bulkUpdateMedia(updates: BulkUpdateItem[]): void {
    this.assertWritable('bulkUpdateMedia');
    bulk.bulkUpdateMedia(this.db, updates);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * タグを作成
   */
  createTag(name: string): Tag {
    this.assertWritable('createTag');
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
    this.assertWritable('addTagToMedia');
    tag.addTagToMedia(this.db, mediaId, tagId);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * メディアからタグを削除
   */
  removeTagFromMedia(mediaId: number, tagId: number): void {
    this.assertWritable('removeTagFromMedia');
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
    this.assertWritable('setMediaAttribute');
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
    this.assertWritable('deleteMediaAttribute');
    attribute.deleteMediaAttribute(this.db, mediaId, key);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  /**
   * メディアの全ての属性を削除
   */
  deleteAllMediaAttributes(mediaId: number): void {
    this.assertWritable('deleteAllMediaAttributes');
    attribute.deleteAllMediaAttributes(this.db, mediaId);
    this.backupManager?.recordOperation().catch((err) => {
      console.error('Backup operation failed:', err);
    });
  }

  // ========== メディアハッシュ操作 ==========

  addMediaHash(input: MediaHashInput): MediaHash {
    this.assertWritable('addMediaHash');
    return hashModule.addMediaHash(this.db, input);
  }

  addMediaHashes(inputs: MediaHashInput[]): MediaHash[] {
    this.assertWritable('addMediaHashes');
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
    this.assertWritable('deleteMediaHash');
    hashModule.deleteMediaHash(this.db, itemUuid, filename, timeRange);
  }

  deleteMediaHashes(itemUuid: string): void {
    this.assertWritable('deleteMediaHashes');
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
    this.assertWritable('computeMediaHash');
    return hashModule.computeMediaHash(this.db, itemUuid, mediaPath, mediaType, durationSec);
  }

  computeMediaHashes(filter: MediaFilter, options?: QueryOptions, force?: boolean): ComputeHashResult[] {
    this.assertWritable('computeMediaHashes');
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
    this.assertWritable('backup');
    return this.backupManager?.backup(undefined);
  }

  /**
   * ラベル付き手動バックアップを実行
   * @param label バックアップのラベル（例: "before_import"）
   */
  async backupWithLabel(label: string): Promise<string | undefined> {
    this.assertWritable('backupWithLabel');
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
  /**
   * バックアップから復元（(b) 制限操作・設計 §9.2）。TS Local は prod 直接 gate を持たないため常に拒否。
   * prod 直接実行（dry-run+trash+pre-stash gate）は Remote（Rust CLI の --target prod）経由が主経路。
   */
  restore(selector: BackupSelector = BackupSelector.latest()): string {
    this.assertNotStgRestricted('restore');
  }

  /** バックアップ一覧を取得 */
  listBackups(): BackupInfo[] {
    return this.backupManager?.listBackups() ?? [];
  }

  /** pre-stash（即時復旧用ロールバックファイル）一覧を取得（設計 §8）。
   *  promote/(b)操作が返す preStashPath を呼出側が失った場合の発見経路。
   *  戻り値の path はそのまま BackupSelector.byPath() で restore に渡せる。 */
  listPreStashes(): BackupInfo[] {
    return this.backupManager?.listPreStashes() ?? [];
  }

  /** 監査ログを取得（設計 §10・TASK-46）。prod保護操作（sync/discard/observe/promote/(b)操作）の事後追跡用。
   *  新しい順（timestamp降順）で返す。 */
  listAuditLogs(filter: AuditLogFilter = {}): AuditRecord[] {
    return this.backupManager?.readAuditLogs(filter) ?? [];
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
    this.assertWritable('updateExist');
    return updateExistModule.updateExist(this.db, filter, options, updateOptions);
  }

  /**
   * フィルタで絞り込んだメディアのサムネイル状態をチェックする
   */
  checkThumbnail(
    filter: MediaFilter = {},
    options?: QueryOptions,
  ): CheckThumbnailResult {
    this.assertWritable('checkThumbnail');
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
    this.assertWritable('updateThumbnail');
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
    this.stgLock?.release();
    this.stgLock = undefined;
  }

  /**
   * stg 編集セッションの排他ロックを取得・保持（設計 §15-11）。
   *
   * `<dbPath>.lock` の排他ロックを取得し、`close()` まで保持する。
   * 別セッションが保持中なら `StgBusyError`。readonly（prod RO 読込）セッションでは呼ばないこと。
   */
  acquireStgLock(): void {
    if (this.stgLock) return; // 既に保持済み（冪等）
    this.stgLock = acquireStgLock(this.dbPath);
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

  /**
   * prod(RO) と現在DB(stg) の差分を取得（promote 判断用・設計 §4.4・TASK-53）。
   *
   * prod を readonly 別接続で開き（設計 §15-8）、snapshot を比較する。
   * `computeBackupDiff(current=prod, backup=stg)` で呼ぶことで stg 編集視点に反転させる
   * （diffWithBackup は `computeBackupDiff(current=self, backup=backupDB)` で方向が逆）。
   * prod 読込経路では migrate をスキップ（readonly open のため）。
   *
   * - added:   stg のみ（promote で prod に追加）
   * - removed: prod のみ（promote で prod から削除）
   * - changed: 両方で異なる（promote で prod が上書き）
   */
  diffWithProd(prodDbPath: string, options: DiffOptions = {}): BackupDiff {
    const prodSnap = new KijukuDB(prodDbPath, { readonly: true }).collectSnapshot();
    const stgSnap = this.collectSnapshot();
    // セマンティクス反転: current=prod, backup=stg（diffWithBackup と逆）
    const diff = computeBackupDiff(prodSnap, stgSnap, options);
    // 監査: prod 側 backup/meta/audit.log（設計 §10・best-effort）。diff 件数サマリを記録。
    KijukuDB.appendAuditToProd(prodDbPath, 'diffProdStg', 'stg', 'success', null, {
      diffTotals: summarizeDiff(diff).totals,
    });
    return diff;
  }

  /**
   * stg（self）と prod を比較し、機械的 promote gate を評価（設計 §3.4/§4.4・TASK-54）。
   *
   * 内部で差分（`diffWithProd` と同等）を計算し `summarizeDiff` で要約した上で、以下の gate を評価:
   * - `PRAGMA integrity_check` == "ok"
   * - 外部キー整合性（`foreign_key_check` 空 + FK 有効）
   * - prod/stg の `schema_version` 一致
   * - 件数・差分上限（`summary.totals` vs `GateConfig`）
   * - 運用者定義 golden assertion
   *
   * `passed` が全 gate 合格を表す（promote 可否の客観判定・人間 gate でない・設計 §3/§6.4）。
   */
  observe(prodDbPath: string, options: ObserveOptions = {}): ObserveResult {
    const gateConfig: GateConfig = options.gateConfig ?? defaultGateConfig();
    const diffOptions: DiffOptions = options.diffOptions ?? {};

    // --- prod 側（RO 別接続）: snapshot + schema_version + revision 指紋 ---
    const prodDb = new KijukuDB(prodDbPath, { readonly: true });
    const prodSnap = prodDb.collectSnapshot();
    const prodSchemaVersion = prodDb.getSchemaVersion();
    const prodRevision = computeProdRevision(prodDb.db);

    // --- stg 側（self）: snapshot + gate 用 DB 検査 ---
    const stgSnap = this.collectSnapshot();
    const integrity = migration.integrityCheck(this.db);
    const fkViolations = migration.foreignKeyCheck(this.db);
    const fkEnabled = migration.isForeignKeysEnabled(this.db);
    const stgSchemaVersion = migration.getSchemaVersion(this.db);
    const goldenResults: GoldenResult[] = gateConfig.goldenAssertions.map((ga) => {
      try {
        const raw = this.db.prepare(ga.sql).pluck().get();
        const count = typeof raw === 'number' ? raw : Number(raw);
        if (!Number.isFinite(count)) {
          return { ok: false, error: `non-numeric result: ${String(raw)}` };
        }
        return { ok: true, count };
      } catch (e) {
        return { ok: false, error: e instanceof Error ? e.message : String(e) };
      }
    });

    // --- 差分 → 要約 → gate 評価 ---
    const diff = computeBackupDiff(prodSnap, stgSnap, diffOptions);
    const summary = summarizeDiff(diff);
    const checks = evaluateGate(
      summary,
      integrity,
      fkViolations,
      fkEnabled,
      prodSchemaVersion,
      stgSchemaVersion,
      goldenResults,
      gateConfig,
    );

    // prod_sync_revision gate: sync 元 revision（`<stg>.meta.json`）と現 prod revision を比較し、
    // prod が sync 後に更新された（stg が stale）なら不合格（設計 §15-11）。
    // meta なし（旧 stg・後方互換）はスキップ。
    const meta = readStgMeta(this.dbPath);
    if (meta) {
      const drift = !prodRevisionEqual(meta.syncedFrom.revision, prodRevision);
      checks.push({
        name: 'prod_sync_revision',
        passed: !drift,
        detail: drift
          ? `prod drifted since sync (recorded media_count=${meta.syncedFrom.revision.mediaCount} / current ${prodRevision.mediaCount}) -> re-sync required`
          : 'prod unchanged since sync',
      });
    }

    return {
      passed: allGateChecksPassed(checks),
      prodSchemaVersion,
      stgSchemaVersion,
      summary,
      checks,
    };
  }

  /**
   * stg（self）→ prod への反映（promote・設計 §4.5/§6.4・TASK-58/62）。
   *
   * `replicateDb`（sync: prod→stg）の反転。gate 合格で prod を上書きし pre-stash を作る。
   * gate 不合格時は prod を触る前に `PromoteGateFailedError`（pre-stash も無駄にしない）。
   * prod RW は本メソッド内部でのみ一時取得（構造的保護・§5.2・人間 gate なし）。成功時の
   * `preStashPath` は §8 即時復旧の戻し先。
   *
   * `backupOpts` で pre-stash 先（`backupDir`）をカスタマイズ可能（省略時は `enabled:false`
   * 相当・デフォルト `tmp/` のみ・§7.2）。pre-stash 自体は `backupOpts`・`enabled` にかかわらず常時実行。
   */
  async promote(
    prodDbPath: string,
    options: ObserveOptions = {},
    backupOpts?: BackupOptions,
  ): Promise<PromoteOutcome> {
    if (this.dbPath === prodDbPath) {
      throw new Error(`promote: stg and prod are the same path: ${prodDbPath}`);
    }

    // 1. gate 評価 → 不合格なら prod を触る前に拒否（pre-stash も無駄にしない）。
    const observe = this.observe(prodDbPath, options);
    if (!observe.passed) {
      const failedChecks = observe.checks
        .filter((c) => !c.passed)
        .map((c) => `${c.name}: ${c.detail}`);
      throw new PromoteGateFailedError(failedChecks);
    }

    // 2. prod 排他ロック（§5.3）。別セッション保持中は ProdBusyError。
    const lock = acquireProdLock(prodDbPath);
    try {
      // 3. pre-stash 強制（enabled と独立・常時実行・§7.2）。prod を一時 RW 接続で BackupManager 経由。
      //    backupOpts 未指定時は enabled:false（auto/manual/meta dir を作らず tmp/ のみ）。
      let preStashPath: string | undefined;
      const prodDb = new Database(prodDbPath);
      try {
        const bm = new BackupManager(prodDb, prodDbPath, backupOpts ?? { enabled: false });
        preStashPath = bm.createPrePromoteSnapshot();
      } finally {
        prodDb.close();
      }

      // 3b. 排他ロック下で gate を再評価（authoritative）。手順1の observe → ロック取得の間に別 promoter
      //     が prod を更新すると手順1の gate 結果は stale となり上書き競合を生むため、prod を触る前に
      //     ロック保持状態で再観察する。
      const observe = this.observe(prodDbPath, options);
      if (!observe.passed) {
        const failedChecks = observe.checks
          .filter((c) => !c.passed)
          .map((c) => `${c.name}: ${c.detail}`);
        throw new PromoteGateFailedError(failedChecks);
      }

      // 4. 既存 prod + WAL/SHM 削除（空 dst 要求・pre-stash 済みで安全・replicateDb と同パターン）。
      for (const suffix of ['', '-wal', '-shm']) {
        const candidate = `${prodDbPath}${suffix}`;
        if (fs.existsSync(candidate)) {
          fs.unlinkSync(candidate);
        }
      }

      // 5. stg → prod コピー（Online Backup API・src=stg を RO で開く・ファイル全体・§15-1）。
      await BackupManager.copyDbOnline(this.dbPath, prodDbPath);

      return { observe, preStashPath };
    } finally {
      lock.release();
    }
  }

  /** バックアップにラベルを付与（事後）。undefined でクリア */
  setBackupLabel(id: string, label: string | undefined): void {
    this.assertWritable('setBackupLabel');
    if (!this.backupManager) throw new Error('Backup manager not configured');
    this.backupManager.setBackupLabel(id, label);
  }

  /** バックアップにメモを付与（事後）。undefined でクリア */
  setBackupNote(id: string, note: string | undefined): void {
    this.assertWritable('setBackupNote');
    if (!this.backupManager) throw new Error('Backup manager not configured');
    this.backupManager.setBackupNote(id, note);
  }

  /** バックアップの事後メタを取得 */
  getBackupMeta(id: string): BackupMetaEntry | null {
    return this.backupManager?.getBackupMeta(id) ?? null;
  }
}

/**
 * DBインスタンスを作成（ローカルまたはリモート）。dbPath が `host:path` 形式なら
 * RemoteKijukuDB、それ以外は KijukuDB（SDK ファクトリ・dbPath 解析に parseDbPath を使用）。
 *
 * @param dbPath - データベースパス（host:path または ローカルパス）
 * @param verbose - SQL ログ出力
 * @param readonly - prod 読込経路なら true（readonly open・設計 §5.1）
 * @param target - リモート CLI へ伝達する操作対象（remote.ts が --target を付与）
 * @returns KijukuDB または RemoteKijukuDB
 */
export function createDatabase(
  dbPath: string,
  verbose = false,
  readonly = false,
  target?: Target,
): KijukuDB | RemoteKijukuDB {
  const parsed = parseDbPath(dbPath);

  if (parsed.isRemote) {
    // リモートDB
    const cfg: { sshHost: string; dbPath?: string; target?: Target } = {
      sshHost: parsed.sshHost!,
      dbPath: parsed.remotePath,
    };
    if (target !== undefined) cfg.target = target;
    return new RemoteKijukuDB(cfg);
  } else {
    // ローカルDB
    const opts: { verbose: boolean; readonly?: boolean } = { verbose };
    if (readonly) opts.readonly = true;
    return new KijukuDB(parsed.localPath!, opts);
  }
}
