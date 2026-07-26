/**
 * バックアップ機能
 *
 * ディレクトリ構成:
 *   backup/
 *     auto/   - 自動バックアップ（フル .db / 差分 .diff）
 *     manual/ - 手動バックアップ（常にフル .db）
 *     tmp/    - restore前自動退避 (.db)
 *     meta/
 *       auto-records.csv
 *
 * ファイル名規則:
 *   {stem}.{timestamp}.db            フル
 *   {stem}.{timestamp}.diff          差分
 *   {stem}.{timestamp}-{label}.db    ラベル付き手動
 *   {stem}.{timestamp}-pre_restore.db tmp退避
 *
 * timestamp = YYYYMMDDHHMMSS-mmm (18文字固定)
 */
import Database from 'better-sqlite3';
import * as fs from 'node:fs';
import * as path from 'node:path';

// 時間定数（秒）
const HOUR_SECS = 3600;
const DAY_SECS = 86400;
const WEEK_SECS = 604800;
const MONTH_SECS = 2592000;
const QUARTER_SECS = 7776000;
const YEAR_SECS = 31536000;

// ============================================================
// 型定義
// ============================================================

/** バックアップのスコープ（保存先ディレクトリ） */
export type BackupScope = 'auto' | 'manual' | 'tmp';

/** バックアップの種別 */
export type BackupKind =
  | { type: 'full' }
  | { type: 'diff'; baseId: string };

/** ラベルの由来 */
export type LabelSource = 'filename' | 'sidecar';

/** バックアップ情報 */
export interface BackupInfo {
  /** バックアップID（タイムスタンプ文字列、18文字） */
  id: string;
  /** ファイル名 */
  name: string;
  /** ファイルパス */
  path: string;
  /** 作成日時 */
  createdAt: Date;
  /** スコープ（auto / manual / tmp） */
  scope: BackupScope;
  /** 種別（フル / 差分） */
  kind: BackupKind;
  /** ラベル（サイドカー優先、なければファイル名由来） */
  label?: string;
  /** ラベルの由来 */
  labelSource?: LabelSource;
  /** メモ（サイドカー backup-meta.json 由来） */
  note?: string;
}

/** バックアップの事後メタ（サイドカー backup-meta.json の1エントリ） */
export interface BackupMetaEntry {
  id: string;
  label?: string;
  note?: string;
  updatedAt: string;
}

/** auto-records.csv の1レコード */
export interface AutoRecord {
  id: string;
  createdAt: string;
  tier: string;
  kind: 'full' | 'diff';
  baseId: string;
  sizeBytes: number;
  status: 'kept' | 'pruned';
  prunedAt: string;
}

/** 保持ポリシーの1段階 */
export interface RetentionTier {
  maxAgeSecs: number;
  keepIntervalSecs: number;
}

/** 自動バックアップの粗密保持ポリシー */
export interface RetentionPolicy {
  tiers: RetentionTier[];
}

/** デフォルトの保持ポリシーを生成 */
export function defaultRetentionPolicy(): RetentionPolicy {
  return {
    tiers: [
      { maxAgeSecs: HOUR_SECS,    keepIntervalSecs: 0 },
      { maxAgeSecs: DAY_SECS,     keepIntervalSecs: HOUR_SECS },
      { maxAgeSecs: WEEK_SECS,    keepIntervalSecs: DAY_SECS },
      { maxAgeSecs: MONTH_SECS,   keepIntervalSecs: WEEK_SECS },
      { maxAgeSecs: QUARTER_SECS, keepIntervalSecs: MONTH_SECS },
      { maxAgeSecs: YEAR_SECS,    keepIntervalSecs: QUARTER_SECS },
      { maxAgeSecs: Infinity,     keepIntervalSecs: YEAR_SECS },
    ],
  };
}

/** バックアップ選択条件 */
export class BackupSelector {
  private constructor(
    private readonly _kind: BackupSelectorKind,
    public readonly scopeFilter: BackupScope | undefined = undefined,
  ) {}

  static latest(): BackupSelector {
    return new BackupSelector({ type: 'latest' });
  }
  static nth(n: number): BackupSelector {
    return new BackupSelector({ type: 'nth', n });
  }
  static before(date: Date): BackupSelector {
    return new BackupSelector({ type: 'before', date });
  }
  static after(date: Date): BackupSelector {
    return new BackupSelector({ type: 'after', date });
  }
  static closestTo(date: Date): BackupSelector {
    return new BackupSelector({ type: 'closestTo', date });
  }
  /** ID（タイムスタンプ文字列）でバックアップを直接指定する */
  static byId(id: string): BackupSelector {
    return new BackupSelector({ type: 'byId', id });
  }
  /** 既知パスでバックアップファイルを直接指定する（設計 §8 即時復旧・pre-stash 戻し）。
   *  listBackups から除外される pre-stash(`...-pre_promote.db` 等)を restore/diff する経路。
   *  backupDir 配下の .db ファイルのみ許可（selectBackup で検証）。 */
  static byPath(path: string): BackupSelector {
    return new BackupSelector({ type: 'byPath', path });
  }

  /** スコープを限定する (TASK-150) */
  scope(scope: BackupScope): BackupSelector {
    return new BackupSelector(this._kind, scope);
  }

  get kind(): BackupSelectorKind {
    return this._kind;
  }
}

type BackupSelectorKind =
  | { type: 'latest' }
  | { type: 'nth'; n: number }
  | { type: 'before'; date: Date }
  | { type: 'after'; date: Date }
  | { type: 'closestTo'; date: Date }
  | { type: 'byId'; id: string }
  | { type: 'byPath'; path: string };

/** バックアップ設定オプション */
export interface BackupOptions {
  /** バックアップ保存先ディレクトリ */
  backupDir?: string;
  /** 自動バックアップのトリガー間隔（ミリ秒） */
  intervalMs?: number;
  /** バックアップ機能全体の有効/無効 */
  enabled?: boolean;
  /** 自動バックアップの有効/無効 */
  autoEnabled?: boolean;
  /** tmp/ の保持期間（秒） */
  tmpRetentionSecs?: number;
  /** 最大バックアップ数（retentionPolicy 未設定時のみ有効） */
  maxBackups?: number;
  /** 最大保持日数（retentionPolicy 未設定時のみ有効） */
  maxAgeDays?: number;
  /** 保持ポリシー */
  retentionPolicy?: RetentionPolicy;
  /** DBロック時に1回の試行で待機する最大時間（ミリ秒）・Rust parity */
  busyTimeoutMs?: number;
  /** DBロック時のリトライ間隔（ミリ秒）のリスト。長さがリトライ回数を決定する・Rust parity */
  retryIntervalsMs?: number[];
  /** バックアップ進捗コールバック */
  onProgress?: (info: { totalPages: number; remainingPages: number }) => void;
}

// ============================================================
// BackupManager
// ============================================================

export class BackupManager {
  private db: Database.Database;
  private lastBackupTime: number | null = null;
  private lastOperationTime: number | null = null;
  private readonly backupDir: string;
  private readonly dbStem: string;
  private readonly intervalMs: number;
  private readonly enabled: boolean;
  private readonly autoEnabled: boolean;
  private readonly tmpRetentionSecs: number;
  private readonly maxBackups?: number;
  private readonly maxAgeDays?: number;
  private readonly retentionPolicy?: RetentionPolicy;
  private readonly onProgress?: (info: { totalPages: number; remainingPages: number }) => void;

  constructor(db: Database.Database, private readonly dbPath: string, options: BackupOptions) {
    this.db = db;
    this.dbStem = path.basename(dbPath, path.extname(dbPath)) || 'database';
    this.backupDir = options.backupDir ?? path.join(path.dirname(dbPath), 'backup');
    this.intervalMs = options.intervalMs ?? 3_600_000;
    this.enabled = options.enabled ?? true;
    this.autoEnabled = options.autoEnabled ?? true;
    this.tmpRetentionSecs = options.tmpRetentionSecs ?? 604_800;
    this.maxBackups = options.maxBackups;
    this.maxAgeDays = options.maxAgeDays;
    this.retentionPolicy = options.retentionPolicy;
    this.onProgress = options.onProgress;

    if (this.enabled) {
      fs.mkdirSync(path.join(this.backupDir, 'auto'), { recursive: true });
      fs.mkdirSync(path.join(this.backupDir, 'manual'), { recursive: true });
      fs.mkdirSync(path.join(this.backupDir, 'meta'), { recursive: true });
    }
  }

  /** 操作を記録し、必要に応じて自動バックアップを実行 */
  async recordOperation(): Promise<void> {
    if (!this.enabled || !this.autoEnabled) return;

    const now = Date.now();
    this.lastOperationTime = now;

    if (this.lastBackupTime === null || now - this.lastBackupTime >= this.intervalMs) {
      await this.backupAuto();
    }
  }

  /** 自動バックアップを実行（内部用） */
  async backupAuto(): Promise<string> {
    const timestamp = currentTimestampStr();
    const autoDir = path.join(this.backupDir, 'auto');
    fs.mkdirSync(autoDir, { recursive: true });

    // 今日のフルバックアップを探す（差分の基底）
    const todayFull = this.findTodayFullBackup();

    let backupPath: string;
    let isDiff = false;
    let baseId: string | undefined;

    if (todayFull !== null) {
      // 差分バックアップを試みる
      try {
        const diffPath = await this.createDiffBackupFile(timestamp, todayFull);
        backupPath = diffPath;
        isDiff = true;
        baseId = todayFull.id;
      } catch {
        // 差分作成失敗時はフルにフォールバック
        const filename = `${this.dbStem}.${timestamp}.db`;
        backupPath = path.join(autoDir, filename);
        await this.copyDbTo(backupPath);
      }
    } else {
      // フルバックアップ（今日の最初 = daily）
      const filename = `${this.dbStem}.${timestamp}.db`;
      backupPath = path.join(autoDir, filename);
      await this.copyDbTo(backupPath);
    }

    this.lastBackupTime = Date.now();

    // auto-records.csv に追記
    const sizeBytes = fs.existsSync(backupPath)
      ? fs.statSync(backupPath).size
      : 0;
    const tier = isDiff
      ? (() => {
          const elapsedMs = todayFull
            ? Date.now() - todayFull.createdAt.getTime()
            : 0;
          return elapsedMs < HOUR_SECS * 1000 ? 'recent' : 'hourly';
        })()
      : 'daily';

    try {
      this.appendAutoRecord({
        id: timestamp,
        createdAt: new Date().toISOString(),
        tier,
        kind: isDiff ? 'diff' : 'full',
        baseId: baseId ?? '',
        sizeBytes,
        status: 'kept',
        prunedAt: '',
      });
    } catch {
      // 記録失敗はバックアップ失敗にしない
    }

    this.cleanupOldBackups();
    return backupPath;
  }

  /** 手動バックアップを実行（常にフル） */
  async backup(label?: string): Promise<string> {
    if (!this.enabled) {
      throw new Error('Backup is disabled');
    }

    const timestamp = currentTimestampStr();
    const manualDir = path.join(this.backupDir, 'manual');
    fs.mkdirSync(manualDir, { recursive: true });

    const filename = label
      ? `${this.dbStem}.${timestamp}-${label.replace(/[^a-zA-Z0-9_-]/g, '_')}.db`
      : `${this.dbStem}.${timestamp}.db`;

    const backupPath = path.join(manualDir, filename);
    await this.copyDbTo(backupPath);

    this.lastBackupTime = Date.now();
    this.cleanupOldBackups();
    return backupPath;
  }

  /**
   * migrate 実行前に現在の DB を tmp/ へ snapshot する（設計 §7.3「migrate 前 snapshot 必須」）。
   *
   * enabled に関わらず常時退避する（設計 §7.3・migrate 前 snapshot 必須）。失敗時は例外を伝播し
   * 呼び出し側の migrate を中止させる（Rust の `create_pre_migrate_snapshot` と同等）。
   * WAL チェックポイント後にファイルコピーする。
   */
  createPreMigrateSnapshot(): string | undefined {
    // `:memory:` 等、ファイル実体のない DB は snapshot 対象外（インメモリ DB のディスク
    // snapshot は無意味）。本番（stg/prod のファイル DB）では常時 snapshot する（設計 §7.3）。
    if (!this.isFileBackedDb()) return undefined;
    const timestamp = currentTimestampStr();
    const tmpDir = path.join(this.backupDir, 'tmp');
    fs.mkdirSync(tmpDir, { recursive: true });
    const preMigratePath = path.join(
      tmpDir,
      `${this.dbStem}.${timestamp}-pre_migrate.db`,
    );
    this.db.pragma('wal_checkpoint(TRUNCATE)');
    fs.copyFileSync(this.dbPath, preMigratePath);
    return preMigratePath;
  }

  /**
   * promote 実行前に prod を退避する（設計 §7.2/§4.5）。
   * enabled に関わらず常時退避。リスト除外フィルタでユーザー向け一覧から非表示。
   * ファイル実体のない DB（`:memory:` 等）では undefined を返す（createPreMigrateSnapshot と同じ前提）。
   */
  createPrePromoteSnapshot(): string | undefined {
    if (!this.isFileBackedDb()) return undefined;
    const timestamp = currentTimestampStr();
    const tmpDir = path.join(this.backupDir, 'tmp');
    fs.mkdirSync(tmpDir, { recursive: true });
    const prePromotePath = path.join(
      tmpDir,
      `${this.dbStem}.${timestamp}-pre_promote.db`,
    );
    this.db.pragma('wal_checkpoint(TRUNCATE)');
    fs.copyFileSync(this.dbPath, prePromotePath);
    return prePromotePath;
  }

  /**
   * バックアップを復元する
   *
   * 復元前に現在の状態を tmp/ へ自動退避する（TASK-149）
   * WAL チェックポイント後にファイルコピーを行い、接続を再初期化します。
   */
  restore(selector: BackupSelector): string {
    const backupInfo = this.selectBackup(selector);
    if (!backupInfo) {
      throw new Error('No backup found matching selector');
    }

    // 1. 現在のDBを tmp/ へ退避（enabled に関わらず常時退避・設計 §7.2）。
    //    ファイル実体のない DB（`:memory:` 等）では skip（snapshot 対象外）。
    if (this.isFileBackedDb()) {
      const timestamp = currentTimestampStr();
      const tmpDir = path.join(this.backupDir, 'tmp');
      fs.mkdirSync(tmpDir, { recursive: true });
      const preRestorePath = path.join(
        tmpDir,
        `${this.dbStem}.${timestamp}-pre_restore.db`,
      );
      this.db.pragma('wal_checkpoint(TRUNCATE)');
      fs.copyFileSync(this.dbPath, preRestorePath);
    }

    // 2. バックアップを復元
    let sourcePath: string;
    let isTempSource = false;
    if (backupInfo.kind.type === 'diff') {
      // 差分を基底フルに適用して一時フルを作成
      sourcePath = this.resolveDiffBackupToTempFull(backupInfo);
      isTempSource = true;
    } else {
      sourcePath = backupInfo.path;
    }

    // WAL checkpoint & close
    this.db.pragma('wal_checkpoint(TRUNCATE)');
    this.db.close();

    // ファイルコピー
    fs.copyFileSync(sourcePath, this.dbPath);
    const walPath = `${this.dbPath}-wal`;
    const shmPath = `${this.dbPath}-shm`;
    if (fs.existsSync(walPath)) fs.rmSync(walPath);
    if (fs.existsSync(shmPath)) fs.rmSync(shmPath);

    // 一時ファイル削除（WAL の -wal/-shm 副産物も含む）
    if (isTempSource) {
      for (const p of [sourcePath, `${sourcePath}-wal`, `${sourcePath}-shm`]) {
        try {
          fs.rmSync(p);
        } catch { /* ignore */ }
      }
    }

    // 接続を再オープン
    const newDb = new Database(this.dbPath);
    newDb.pragma('foreign_keys = ON');
    this.db = newDb;

    return backupInfo.path;
  }

  /** 現在のDB接続を取得（restore後の参照更新用） */
  getDb(): Database.Database {
    return this.db;
  }

  /** バックアップ一覧を取得（新しい順、全スコープ横断） */
  listBackups(): BackupInfo[] {
    return this.listBackupsFiltered(undefined);
  }

  /** スコープを指定してバックアップ一覧を取得（TASK-150） */
  listBackupsInScope(scope: BackupScope): BackupInfo[] {
    return this.listBackupsFiltered(scope);
  }

  private listBackupsFiltered(scopeFilter: BackupScope | undefined): BackupInfo[] {
    const meta = this.readBackupMetaStore();
    const scopes: BackupScope[] = scopeFilter
      ? [scopeFilter]
      : ['auto', 'manual', 'tmp'];

    const backups: BackupInfo[] = [];

    for (const scope of scopes) {
      const subdir = path.join(this.backupDir, scope);
      if (!fs.existsSync(subdir)) continue;

      for (const name of fs.readdirSync(subdir)) {
        const parsed = parseBackupFilename(name, this.dbStem);
        if (!parsed) continue;

        // pre-stash（pre_migrate/pre_restore/pre_promote）は運用用ロールバックファイルで
        // ユーザー向け一覧から除外する（設計 §7.2/§7.3・§8・Rust backup.rs と同等）。
        // §8 復旧は listPreStashes() / byPath 経由で発見・戻しする。
        if (isPreStashName(name)) {
          continue;
        }

        // auto 以外は .db のみ
        if (scope !== 'auto' && parsed.extension !== 'db') continue;

        const filePath = path.join(subdir, name);
        const stat = fs.statSync(filePath);

        let kind: BackupKind;
        if (parsed.extension === 'diff') {
          const baseId = readDiffBaseId(filePath);
          kind = { type: 'diff', baseId };
        } else {
          kind = { type: 'full' };
        }

        const metaEntry = meta[parsed.id];
        const hasSidecarLabel = !!metaEntry && metaEntry.label !== undefined;
        backups.push({
          id: parsed.id,
          name,
          path: filePath,
          createdAt: stat.mtime,
          scope,
          kind,
          label: hasSidecarLabel ? metaEntry!.label : parsed.label,
          labelSource: hasSidecarLabel ? 'sidecar' : 'filename',
          note: metaEntry?.note,
        });
      }
    }

    return backups.sort((a, b) => compareBackupIdDesc(a.id, b.id));
  }

  /** 最後のバックアップ時刻を取得 */
  getLastBackupTime(): Date | null {
    return this.lastBackupTime !== null ? new Date(this.lastBackupTime) : null;
  }

  /** 最後の操作時刻を取得 */
  getLastOperationTime(): Date | null {
    return this.lastOperationTime !== null ? new Date(this.lastOperationTime) : null;
  }

  /** 次回バックアップまでの残り時間（ミリ秒）を取得 */
  getTimeUntilNextBackup(): number {
    if (this.lastBackupTime === null) return 0;
    const elapsed = Date.now() - this.lastBackupTime;
    return Math.max(0, this.intervalMs - elapsed);
  }

  /** 条件に一致するバックアップを選択 */
  selectBackup(selector: BackupSelector): BackupInfo | null {
    // ByPath は listBackups が pre-stash を除外するため専用経路（§8 即時復旧・既知パス）。
    // scopeFilter は無視（明示パス優先）。
    if (selector.kind.type === 'byPath') {
      return this.selectBackupByPath(selector.kind.path);
    }
    const backups = selector.scopeFilter
      ? this.listBackupsInScope(selector.scopeFilter)
      : this.listBackups();

    if (backups.length === 0) return null;

    const kind = selector.kind;
    switch (kind.type) {
      case 'latest':
        return backups[0] ?? null;
      case 'nth':
        return backups[kind.n] ?? null;
      case 'before':
        return backups.find((b) => b.createdAt < kind.date) ?? null;
      case 'after':
        return backups.filter((b) => b.createdAt > kind.date).at(-1) ?? null;
      case 'closestTo': {
        const targetTime = kind.date.getTime();
        return backups.reduce((closest, current) => {
          const closestDiff = Math.abs(closest.createdAt.getTime() - targetTime);
          const currentDiff = Math.abs(current.createdAt.getTime() - targetTime);
          return currentDiff < closestDiff ? current : closest;
        });
      }
      // ID（タイムスタンプ文字列）で検索。.db/.diff 両方を list 結果から探す
      // （findBackupByIdInScope は基底フル .db のみを返すため差分選択に使えない）。
      case 'byId':
        return backups.find((b) => b.id === kind.id) ?? null;
    }
  }

  /** 既知のバックアップファイルパスから BackupInfo を合成（設計 §8 即時復旧・既知パス経由・Rust backup.rs と同等）。
   *  listBackups が pre-stash を除外するため、ByPath selector と listPreStashes は指定パスから直接
   *  BackupInfo を合成する。backupDir 配下の .db ファイルのみ許可（path traversal・外部ファイル参照を拒否）。 */
  private buildBackupInfoFromPath(filePath: string): BackupInfo {
    // 実在確認＋絶対パス化。不在は throw。traversal(`..`) も解決後に backupDir 配下チェックで弾かれる。
    let resolved: string;
    try {
      resolved = fs.realpathSync(filePath);
    } catch {
      throw new Error(`backup path not found: ${filePath}`);
    }

    // 拡張子は .db のみ（pre-stash は常にフル .db）。
    if (!resolved.endsWith('.db')) {
      throw new Error(`ByPath selector requires a .db file: ${filePath}`);
    }

    // backupDir 配下のみ許可（path traversal・外部ファイル参照の拒否）。
    let backupDirResolved: string;
    try {
      backupDirResolved = fs.realpathSync(this.backupDir);
    } catch {
      throw new Error(`backup_dir not accessible: ${this.backupDir}`);
    }
    const underBackupDir =
      resolved === backupDirResolved || resolved.startsWith(backupDirResolved + path.sep);
    if (!underBackupDir) {
      throw new Error(
        `ByPath selector path must be under backup_dir ${backupDirResolved}: ${filePath}`,
      );
    }

    const name = path.basename(resolved);
    const parsed = parseBackupFilename(name, this.dbStem);
    const id = parsed ? parsed.id : name;
    const stat = fs.statSync(resolved);

    // scope は親ディレクトリ名から推導（pre-stash は tmp）。判定不能時は tmp。
    const parentName = path.basename(path.dirname(resolved));
    const scope: BackupScope =
      parentName === 'auto' ? 'auto' : parentName === 'manual' ? 'manual' : 'tmp';

    return {
      id,
      name,
      path: resolved,
      createdAt: stat.mtime,
      scope,
      kind: { type: 'full' },
      // pre-stash の "pre_promote" 等はユーザー向けラベルではないため undefined で上書き。
      label: undefined,
      labelSource: 'filename',
      note: undefined,
    };
  }

  /** ByPath selector のための BackupInfo 合成（設計 §8 即時復旧）。buildBackupInfoFromPath の thin wrapper。 */
  private selectBackupByPath(filePath: string): BackupInfo {
    return this.buildBackupInfoFromPath(filePath);
  }

  /** pre-stash（即時復旧用ロールバックファイル）一覧を取得（設計 §8・Rust backup.rs と同等）。
   *  `tmp/` 配下の `*-pre_{migrate,restore,promote}.db` を返す。promote/(b)操作が返す
   *  preStashPath を呼出側が失った場合の発見経路。戻り値の path はそのまま byPath で
   *  restore に渡せる（backupDir 配下のため）。listBackups と同じく id 降順。
   *  復旧用途のため、読めないファイル（走査中の削除等）は飛ばす。 */
  listPreStashes(): BackupInfo[] {
    const tmpDir = path.join(this.backupDir, 'tmp');
    if (!fs.existsSync(tmpDir)) return [];

    const stashes: BackupInfo[] = [];
    for (const name of fs.readdirSync(tmpDir)) {
      if (!isPreStashName(name)) continue;
      const filePath = path.join(tmpDir, name);
      try {
        stashes.push(this.buildBackupInfoFromPath(filePath));
      } catch {
        // 復旧用途: 1ファイルの読み込み失敗（走査中削除等）で一覧全体を落とさない。
      }
    }

    return stashes.sort((a, b) => compareBackupIdDesc(a.id, b.id));
  }

  /** 条件に一致するバックアップのパスを取得 */
  getBackupPath(selector: BackupSelector): string | null {
    return this.selectBackup(selector)?.path ?? null;
  }

  /** 差分backup復元・参照用の一時フルDBパス（pid付きでプロセス間衝突回避） */
  tempFullPathForDiff(baseId: string): string {
    return path.join(
      this.backupDir,
      'tmp',
      `${this.dbStem}.restore_temp_${baseId}_${process.pid}.db`,
    );
  }

  /**
   * 差分backupを基底フルから再構成し、一時フルDBのパスを返す。
   * 呼び出し元が使用後に一時ファイルを削除する責任を持つ。
   */
  resolveDiffBackupToTempFull(diffBackup: BackupInfo): string {
    if (diffBackup.kind.type !== 'diff') {
      throw new Error('Not a diff backup');
    }
    const baseId = diffBackup.kind.baseId;
    const base = this.findBackupByIdInScope(baseId, 'auto');
    if (!base) {
      throw new Error(`Base backup ${baseId} not found`);
    }
    const tempPath = this.tempFullPathForDiff(baseId);
    fs.mkdirSync(path.dirname(tempPath), { recursive: true });
    applyDiffToFile(base.path, diffBackup.path, tempPath);
    return tempPath;
  }

  /** 古いバックアップを削除（tmp の期限切れ削除を含む） */
  cleanupOldBackups(): void {
    if (!this.enabled) return;
    this.cleanupAutoBackups();
    this.cleanupTmpBackups();
    this.cleanupBackupMeta();
  }

  /** バックアップにラベルを付与（事後）。undefined でクリア */
  setBackupLabel(id: string, label: string | undefined): void {
    this.updateBackupMeta(id, (e) => {
      e.label = label;
    });
  }

  /** バックアップにメモを付与。undefined でクリア（最大4096文字） */
  setBackupNote(id: string, note: string | undefined): void {
    if (note !== undefined && [...note].length > 4096) {
      throw new Error('noteが長すぎます（最大4096文字）');
    }
    this.updateBackupMeta(id, (e) => {
      e.note = note;
    });
  }

  /** バックアップの事後メタを取得 */
  getBackupMeta(id: string): BackupMetaEntry | null {
    return this.readBackupMetaStore()[id] ?? null;
  }

  // ---- private: サイドカー backup-meta.json ----

  private metaPath(): string {
    return path.join(this.backupDir, 'meta', 'backup-meta.json');
  }

  private readBackupMetaStore(): Record<string, BackupMetaEntry> {
    const p = this.metaPath();
    // ファイル不在時のみ空ストアを返す。読込/パース失敗時はエラーをスローする
    // （失敗時に{}を返すと updateBackupMeta が空ストアに書き込み、既存の全メタデータを消失させるため）。
    if (!fs.existsSync(p)) return {};
    const content = fs.readFileSync(p, 'utf-8');
    if (!content.trim()) return {};
    try {
      const parsed = JSON.parse(content) as { entries?: Record<string, BackupMetaEntry> };
      return parsed.entries ?? {};
    } catch (e) {
      throw new Error(`Failed to parse backup meta store: ${e}`);
    }
  }

  private writeBackupMetaStore(entries: Record<string, BackupMetaEntry>): void {
    const p = this.metaPath();
    fs.mkdirSync(path.dirname(p), { recursive: true });
    const tmp = p + '.tmp';
    fs.writeFileSync(tmp, JSON.stringify({ entries }, null, 2));
    fs.renameSync(tmp, p);
  }

  private updateBackupMeta(id: string, f: (e: BackupMetaEntry) => void): void {
    const exists = this.listBackups().some((b) => b.id === id);
    if (!exists) {
      throw new Error(`Backup ${id} not found`);
    }
    const entries = this.readBackupMetaStore();
    const entry: BackupMetaEntry = entries[id] ?? {
      id,
      updatedAt: new Date().toISOString(),
    };
    f(entry);
    entry.updatedAt = new Date().toISOString();
    entries[id] = entry;
    this.writeBackupMetaStore(entries);
  }

  /** 実在しないバックアップのメタエントリを掃除 */
  private cleanupBackupMeta(): void {
    const entries = this.readBackupMetaStore();
    if (Object.keys(entries).length === 0) return;
    const existing = new Set(this.listBackups().map((b) => b.id));
    const surviving: Record<string, BackupMetaEntry> = {};
    let changed = false;
    for (const [id, e] of Object.entries(entries)) {
      if (existing.has(id)) {
        surviving[id] = e;
      } else {
        changed = true;
      }
    }
    if (changed) this.writeBackupMetaStore(surviving);
  }

  // ---- private helpers ----

  /**
   * ファイル実体のある DB（stg/prod 等の本番運用）か？
   * `:memory:` や未作成ファイルは snapshot/退避対象外（インメモリ DB のディスク snapshot は無意味）。
   */
  private isFileBackedDb(): boolean {
    return this.dbPath !== ':memory:' && fs.existsSync(this.dbPath);
  }

  private async copyDbTo(dstPath: string): Promise<void> {
    await this.db.backup(dstPath, {
      progress: (info) => {
        if (this.onProgress) this.onProgress(info);
        return 200;
      },
    });
  }

  /**
   * 任意 src→dst の Online Backup コピー（設計 §4.5・sync/promote 基盤）。
   * BackupManager インスタンスに依存しない static メソッド。src は読み取り専用で開く。
   * Rust の `BackupManager::copy_db_online` と同等。
   */
  static async copyDbOnline(src: string, dst: string): Promise<void> {
    const srcDb = new Database(src, { readonly: true, timeout: 5000 });
    try {
      await srcDb.backup(dst);
    } finally {
      srcDb.close();
    }
  }

  /** 今日のフルバックアップを探す（差分の基底候補） */
  private findTodayFullBackup(): BackupInfo | null {
    const autoDir = path.join(this.backupDir, 'auto');
    if (!fs.existsSync(autoDir)) return null;

    const today = todayDateStr();
    const candidates: BackupInfo[] = [];

    for (const name of fs.readdirSync(autoDir)) {
      const parsed = parseBackupFilename(name, this.dbStem);
      if (!parsed || parsed.extension !== 'db') continue;

      // タイムスタンプの日付部分が today と一致するか
      if (parsed.id.startsWith(today)) {
        const filePath = path.join(autoDir, name);
        const stat = fs.statSync(filePath);
        candidates.push({
          id: parsed.id,
          name,
          path: filePath,
          createdAt: stat.mtime,
          scope: 'auto',
          kind: { type: 'full' },
          label: undefined,
        });
      }
    }

    if (candidates.length === 0) return null;
    return candidates.sort((a, b) => compareBackupIdDesc(a.id, b.id))[0];
  }

  /** IDでバックアップを検索（指定スコープ内、基底フル .db のみ） */
  findBackupByIdInScope(id: string, scope: BackupScope): BackupInfo | null {
    const subdir = path.join(this.backupDir, scope);
    if (!fs.existsSync(subdir)) return null;

    for (const name of fs.readdirSync(subdir)) {
      const parsed = parseBackupFilename(name, this.dbStem);
      // 基底フル(.db)のみ。.diff と同一タイムスタンプの場合に .diff（=SQLite DB
      // ではない）が誤って選ばれるのを防ぐ（readdirSync 順序依存の非決定バグ）。
      if (parsed?.id === id && parsed?.extension === 'db') {
        const filePath = path.join(subdir, name);
        const stat = fs.statSync(filePath);
        return {
          id: parsed.id,
          name,
          path: filePath,
          createdAt: stat.mtime,
          scope,
          kind: { type: 'full' },
          label: parsed.label,
        };
      }
    }
    return null;
  }

  /** 差分バックアップファイルを作成（TASK-147） */
  private async createDiffBackupFile(timestamp: string, base: BackupInfo): Promise<string> {
    const autoDir = path.join(this.backupDir, 'auto');

    // 現在のDBの一時フルコピーを作成
    const tempPath = path.join(autoDir, `${this.dbStem}.${timestamp}.tmp_full.db`);
    await this.copyDbTo(tempPath);

    const diffPath = path.join(autoDir, `${this.dbStem}.${timestamp}.diff`);
    try {
      createDiffFile(base.id, base.path, tempPath, diffPath);
    } finally {
      try { fs.rmSync(tempPath); } catch { /* ignore */ }
    }

    return diffPath;
  }

  /** 自動バックアップのクリーンアップ */
  private cleanupAutoBackups(): void {
    const now = Date.now();
    const toDelete: string[] = [];

    if (this.retentionPolicy) {
      // retention_policy による間引き（auto のみ）
      const autoBackups = this.listBackupsInScope('auto');
      const candidates = pruneAutoBackupsByPolicy(autoBackups, this.retentionPolicy, now);

      // 基底フル削除制約チェック
      const records = this.readAutoRecords();

      for (const filePath of candidates) {
        const name = path.basename(filePath);
        const parsed = parseBackupFilename(name, this.dbStem);
        if (!parsed) {
          toDelete.push(filePath);
          continue;
        }
        if (parsed.extension !== 'db') {
          // .diff は直接削除
          toDelete.push(filePath);
          continue;
        }

        const hasKeptDiff = records.some(
          (r) => r.baseId === parsed.id && r.status === 'kept',
        );
        if (!hasKeptDiff) {
          toDelete.push(filePath);
        }
      }
    } else {
      // max_age_days / max_backups による削除（auto のみ対象・manual は対象外＝手動削除のみ §7.4）
      const allBackups = this.listBackups().filter((b) => b.scope === 'auto');

      if (this.maxAgeDays !== undefined) {
        const maxAgeMs = this.maxAgeDays * DAY_SECS * 1000;
        for (const b of allBackups) {
          const age = now - b.createdAt.getTime();
          if (age > maxAgeMs && !toDelete.includes(b.path)) {
            toDelete.push(b.path);
          }
        }
      }

      if (this.maxBackups !== undefined && allBackups.length > this.maxBackups) {
        for (const b of allBackups.slice(this.maxBackups)) {
          if (!toDelete.includes(b.path)) {
            toDelete.push(b.path);
          }
        }
      }
    }

    for (const filePath of toDelete) {
      try { fs.rmSync(filePath); } catch { /* ignore */ }
      const name = path.basename(filePath);
      const parsed = parseBackupFilename(name, this.dbStem);
      if (parsed) {
        try { this.updateAutoRecordPruned(parsed.id); } catch { /* ignore */ }
      }
    }
  }

  /** tmp/ の期限切れファイルを削除（TASK-149） */
  private cleanupTmpBackups(): void {
    const tmpDir = path.join(this.backupDir, 'tmp');
    if (!fs.existsSync(tmpDir)) return;

    const now = Date.now();
    const retentionMs = this.tmpRetentionSecs * 1000;

    for (const name of fs.readdirSync(tmpDir)) {
      const filePath = path.join(tmpDir, name);
      try {
        const stat = fs.statSync(filePath);
        if (now - stat.mtimeMs > retentionMs) {
          fs.rmSync(filePath);
        }
      } catch { /* ignore */ }
    }
  }

  // ---- auto-records.csv (TASK-148) ----

  private get recordsPath(): string {
    return path.join(this.backupDir, 'meta', 'auto-records.csv');
  }

  private readAutoRecords(): AutoRecord[] {
    if (!fs.existsSync(this.recordsPath)) return [];
    const content = fs.readFileSync(this.recordsPath, 'utf-8');
    const lines = content.split('\n').filter(Boolean);
    return lines
      .slice(1) // skip header
      .map(parseCsvRecord)
      .filter((r): r is AutoRecord => r !== null);
  }

  private appendAutoRecord(record: AutoRecord): void {
    const needsHeader =
      !fs.existsSync(this.recordsPath) ||
      fs.statSync(this.recordsPath).size === 0;

    const row = [
      record.id,
      record.createdAt,
      record.tier,
      record.kind,
      record.baseId,
      record.sizeBytes,
      record.status,
      record.prunedAt,
    ].join(',');

    const content =
      (needsHeader ? 'id,created_at,tier,type,base_id,size_bytes,status,pruned_at\n' : '') +
      row +
      '\n';

    fs.appendFileSync(this.recordsPath, content);
  }

  private updateAutoRecordPruned(id: string): void {
    if (!fs.existsSync(this.recordsPath)) return;
    const content = fs.readFileSync(this.recordsPath, 'utf-8');
    const prunedAt = new Date().toISOString();
    const updated = content
      .split('\n')
      .map((line) => {
        if (!line.startsWith(`${id},`)) return line;
        const parts = line.split(',');
        if (parts.length >= 8) {
          parts[6] = 'pruned';
          parts[7] = prunedAt;
          return parts.join(',');
        }
        return line;
      })
      .join('\n');
    fs.writeFileSync(this.recordsPath, updated);
  }
}

// ============================================================
// 差分バックアップ実装 (TASK-147)
// ============================================================

/** SQLite DB ファイルのページサイズを取得 */
function getSqlitePageSize(dbPath: string): number {
  const fd = fs.openSync(dbPath, 'r');
  const buf = Buffer.alloc(18);
  fs.readSync(fd, buf, 0, 18, 0);
  fs.closeSync(fd);
  const raw = buf.readUInt16BE(16);
  return raw === 1 ? 65536 : raw;
}

/**
 * バックアップの作成順降順比較（新しいほど前）。
 *
 * ソートキーには ファイルの mtime ではなく、ファイル名タイムスタンプ
 * (id = "YYYYMMDDHHMMSS-mmm", fixed-width) を使う。mtime は粒度が粗く
 * 同ミリ秒に作られたフル/差分が同値になることで順序が非決定になり、
 * latest 選択が誤って古いフルを選ぶ不具合の原因となるため。
 * id は currentTimestampStr + lastBackupTimestampMs で単調一意が保証されている。
 */
function compareBackupIdDesc(aId: string, bId: string): number {
  if (aId > bId) return -1;
  if (aId < bId) return 1;
  return 0;
}

/**
 * 差分ファイルを作成する
 *
 * ファイル形式:
 *   base_id       18 bytes
 *   page_size      4 bytes big-endian
 *   total_pages    4 bytes big-endian
 *   changed_pages  4 bytes big-endian
 *   For each changed page:
 *     page_number  4 bytes big-endian (1-indexed)
 *     page_data    (page_size bytes)
 */
function createDiffFile(
  baseId: string,
  basePath: string,
  currentPath: string,
  diffPath: string,
): void {
  const pageSize = getSqlitePageSize(basePath);
  const currentPageCount = Math.floor(fs.statSync(currentPath).size / pageSize);
  const basePageCount = Math.floor(fs.statSync(basePath).size / pageSize);

  const outFd = fs.openSync(diffPath, 'w');
  try {
    // ヘッダのプレースホルダ（30バイト）を書き出し、後で上書き
    fs.writeSync(outFd, Buffer.alloc(30), 0, 30, 0);

    const baseFd = fs.openSync(basePath, 'r');
    const currentFd = fs.openSync(currentPath, 'r');
    try {
      const baseBuf = Buffer.alloc(pageSize);
      const currentBuf = Buffer.alloc(pageSize);
      const numBuf = Buffer.alloc(4);
      let changedCount = 0;
      let outOffset = 30;

      for (let pageNum = 1; pageNum <= currentPageCount; pageNum++) {
        fs.readSync(currentFd, currentBuf, 0, pageSize, (pageNum - 1) * pageSize);
        if (pageNum <= basePageCount) {
          fs.readSync(baseFd, baseBuf, 0, pageSize, (pageNum - 1) * pageSize);
          if (!currentBuf.equals(baseBuf)) {
            numBuf.writeUInt32BE(pageNum, 0);
            fs.writeSync(outFd, numBuf, 0, 4, outOffset); outOffset += 4;
            fs.writeSync(outFd, currentBuf, 0, pageSize, outOffset); outOffset += pageSize;
            changedCount++;
          }
        } else {
          // 基底に存在しないページは常に差分として記録
          numBuf.writeUInt32BE(pageNum, 0);
          fs.writeSync(outFd, numBuf, 0, 4, outOffset); outOffset += 4;
          fs.writeSync(outFd, currentBuf, 0, pageSize, outOffset); outOffset += pageSize;
          changedCount++;
        }
      }

      // 先頭に戻って実際のヘッダを書き込む
      const header = Buffer.alloc(30);
      Buffer.from(baseId.slice(0, 18).padEnd(18, '\0')).copy(header, 0);
      header.writeUInt32BE(pageSize, 18);
      header.writeUInt32BE(currentPageCount, 22);
      header.writeUInt32BE(changedCount, 26);
      fs.writeSync(outFd, header, 0, 30, 0);
    } finally {
      fs.closeSync(baseFd);
      fs.closeSync(currentFd);
    }
  } finally {
    fs.closeSync(outFd);
  }
}

/** 差分ファイルのヘッダから base_id を読む */
function readDiffBaseId(diffPath: string): string {
  try {
    const fd = fs.openSync(diffPath, 'r');
    const buf = Buffer.alloc(18);
    fs.readSync(fd, buf, 0, 18, 0);
    fs.closeSync(fd);
    return buf.toString('utf-8').replace(/\0/g, '');
  } catch {
    return '';
  }
}

/** 差分を基底フルに適用して outputPath に書き出す */
function applyDiffToFile(basePath: string, diffPath: string, outputPath: string): void {
  const diffFd = fs.openSync(diffPath, 'r');
  try {
    const header = Buffer.alloc(30);
    if (fs.readSync(diffFd, header, 0, 30, 0) < 30) {
      throw new Error('Invalid diff file: too small');
    }

    const pageSize = header.readUInt32BE(18);
    const totalPages = header.readUInt32BE(22);
    const changedCount = header.readUInt32BE(26);

    const basePageCount = Math.floor(fs.statSync(basePath).size / pageSize);

    const baseFd = fs.openSync(basePath, 'r');
    const outFd = fs.openSync(outputPath, 'w');
    try {
      const baseBuf = Buffer.alloc(pageSize);
      const patchBuf = Buffer.alloc(pageSize);
      const zeroBuf = Buffer.alloc(pageSize);
      const numBuf = Buffer.alloc(4);

      // 差分エントリはpage_num昇順で記録されているため、ストリーミングマージが可能
      let patchesRemaining = changedCount;
      let nextPatchNum: number | null = null;
      let diffReadOffset = 30;

      if (patchesRemaining > 0) {
        fs.readSync(diffFd, numBuf, 0, 4, diffReadOffset); diffReadOffset += 4;
        fs.readSync(diffFd, patchBuf, 0, pageSize, diffReadOffset); diffReadOffset += pageSize;
        patchesRemaining--;
        nextPatchNum = numBuf.readUInt32BE(0);
      }

      for (let pageNum = 1; pageNum <= totalPages; pageNum++) {
        if (nextPatchNum === pageNum) {
          fs.writeSync(outFd, patchBuf, 0, pageSize, (pageNum - 1) * pageSize);
          if (patchesRemaining > 0) {
            fs.readSync(diffFd, numBuf, 0, 4, diffReadOffset); diffReadOffset += 4;
            fs.readSync(diffFd, patchBuf, 0, pageSize, diffReadOffset); diffReadOffset += pageSize;
            patchesRemaining--;
            nextPatchNum = numBuf.readUInt32BE(0);
          } else {
            nextPatchNum = null;
          }
        } else if (pageNum <= basePageCount) {
          fs.readSync(baseFd, baseBuf, 0, pageSize, (pageNum - 1) * pageSize);
          fs.writeSync(outFd, baseBuf, 0, pageSize, (pageNum - 1) * pageSize);
        } else {
          fs.writeSync(outFd, zeroBuf, 0, pageSize, (pageNum - 1) * pageSize);
        }
      }
    } finally {
      fs.closeSync(baseFd);
      fs.closeSync(outFd);
    }
  } finally {
    fs.closeSync(diffFd);
  }
}

// ============================================================
// 保持ポリシーによる間引き
// ============================================================

function pruneAutoBackupsByPolicy(
  backups: BackupInfo[],
  policy: RetentionPolicy,
  nowMs: number,
): string[] {
  const toDelete: string[] = [];
  const autoBackups = backups.filter((b) => b.scope === 'auto');

  let minAgeSecs = 0;

  for (const tier of policy.tiers) {
    if (tier.keepIntervalSecs === 0) {
      minAgeSecs = tier.maxAgeSecs;
      continue;
    }

    const tierBackups = autoBackups.filter((b) => {
      const ageSecs = (nowMs - b.createdAt.getTime()) / 1000;
      return ageSecs >= minAgeSecs && ageSecs < tier.maxAgeSecs;
    });

    if (tierBackups.length === 0) {
      minAgeSecs = tier.maxAgeSecs;
      continue;
    }

    const buckets = new Map<number, BackupInfo[]>();
    for (const backup of tierBackups) {
      const ageSecs = (nowMs - backup.createdAt.getTime()) / 1000;
      const bucket = Math.floor(ageSecs / tier.keepIntervalSecs);
      const list = buckets.get(bucket) ?? [];
      list.push(backup);
      buckets.set(bucket, list);
    }

    for (const bucketBackups of buckets.values()) {
      const sorted = [...bucketBackups].sort(
        (a, b) => b.createdAt.getTime() - a.createdAt.getTime(),
      );
      for (const backup of sorted.slice(1)) {
        toDelete.push(backup.path);
      }
    }

    minAgeSecs = tier.maxAgeSecs;
  }

  return toDelete;
}

// ============================================================
// ファイル名パース
// ============================================================

interface ParsedFilename {
  id: string;
  extension: string;
  label?: string;
}

/**
 * ファイル名から ID・拡張子・ラベルを解析
 *
 * timestamp は YYYYMMDDHHMMSS-mmm の 18 文字固定。
 */
/** pre-stash（pre_migrate/pre_restore/pre_promote）ロールバックファイルか（設計 §7.2/§7.3・§8）。
 *  listBackups はこれをユーザー向け一覧から除外し、listPreStashes はこれを抽出する（Rust と同等）。 */
function isPreStashName(name: string): boolean {
  return (
    name.endsWith('-pre_migrate.db') ||
    name.endsWith('-pre_restore.db') ||
    name.endsWith('-pre_promote.db')
  );
}

function parseBackupFilename(name: string, stem: string): ParsedFilename | null {
  const prefix = `${stem}.`;
  if (!name.startsWith(prefix)) return null;

  const rest = name.slice(prefix.length);
  if (rest.length < 21) return null; // 18 + ".db" = 21 最小

  // タイムスタンプ検証
  const ts = rest.slice(0, 18);
  if (!/^\d{14}-\d{3}$/.test(ts)) return null;

  const afterTs = rest.slice(18);

  // 一時ファイルはスキップ
  if (afterTs.includes('.tmp_full.')) return null;

  if (afterTs === '.db') return { id: ts, extension: 'db' };
  if (afterTs === '.diff') return { id: ts, extension: 'diff' };

  const labelMatch = afterTs.match(/^-(.+)\.db$/);
  if (labelMatch) {
    return { id: ts, extension: 'db', label: labelMatch[1] };
  }

  return null;
}

// ============================================================
// auto-records.csv パース
// ============================================================

function parseCsvRecord(line: string): AutoRecord | null {
  const parts = line.split(',');
  if (parts.length < 8) return null;
  return {
    id: parts[0],
    createdAt: parts[1],
    tier: parts[2],
    kind: parts[3] as 'full' | 'diff',
    baseId: parts[4],
    sizeBytes: parseInt(parts[5], 10) || 0,
    status: parts[6] === 'kept' ? 'kept' : 'pruned',
    prunedAt: parts[7],
  };
}

// ============================================================
// ユーティリティ
// ============================================================

let lastBackupTimestampMs = 0;

/**
 * バックアップファイル名用タイムスタンプ（YYYYMMDDHHMMSS-mmm, 18文字固定）
 *
 * 連続呼び出しで同じミリ秒になると同名ファイル衝突（上書き）し、
 * 差分バックアップの baseId / 復元が乱れるため、前回の呼び出しより必ず
 * +1ms 以上進めた一意のタイムスタンプを返す。
 */
function currentTimestampStr(): string {
  let ms = Date.now();
  if (ms <= lastBackupTimestampMs) {
    ms = lastBackupTimestampMs + 1;
  }
  lastBackupTimestampMs = ms;
  const now = new Date(ms);
  const pad = (n: number, len = 2): string => n.toString().padStart(len, '0');
  return (
    `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}` +
    `${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}` +
    `-${pad(now.getMilliseconds(), 3)}`
  );
}

/** YYYYMMDD 形式の今日の日付文字列 */
function todayDateStr(): string {
  const now = new Date();
  const pad = (n: number): string => n.toString().padStart(2, '0');
  return `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`;
}
