/**
 * データベースの自動バックアップ管理
 */
import Database from 'better-sqlite3';
import * as fs from 'node:fs';
import * as path from 'node:path';

/**
 * バックアップ選択条件
 */
export type BackupSelector =
  | { type: 'latest' }
  | { type: 'nth'; n: number }
  | { type: 'before'; date: Date }
  | { type: 'after'; date: Date }
  | { type: 'closestTo'; date: Date };

/**
 * バックアップ選択条件のヘルパー関数
 */
export const BackupSelector = {
  latest: (): BackupSelector => ({ type: 'latest' }),
  nth: (n: number): BackupSelector => ({ type: 'nth', n }),
  before: (date: Date): BackupSelector => ({ type: 'before', date }),
  after: (date: Date): BackupSelector => ({ type: 'after', date }),
  closestTo: (date: Date): BackupSelector => ({ type: 'closestTo', date }),
};

/**
 * バックアップ情報
 */
export interface BackupInfo {
  name: string;
  path: string;
  createdAt: Date;
}

/**
 * バックアップ設定オプション
 */
export interface BackupOptions {
  /**
   * バックアップファイルの保存先ディレクトリ
   * 省略時はdbPathの親ディレクトリに"backup"フォルダを作成
   */
  backupDir?: string;

  /**
   * バックアップをトリガーする時間間隔（ミリ秒）
   * デフォルト: 3600000 (1時間)
   */
  intervalMs?: number;

  /**
   * バックアップ機能の有効/無効
   * デフォルト: true
   */
  enabled?: boolean;

  /**
   * バックアップ進捗のコールバック
   */
  onProgress?: (info: { totalPages: number; remainingPages: number }) => void;
}

/**
 * バックアップマネージャークラス
 */
export class BackupManager {
  private lastBackupTime: number | null = null;
  private lastOperationTime: number | null = null;
  private readonly backupDir: string;
  private readonly dbStem: string;
  private readonly intervalMs: number;
  private readonly enabled: boolean;
  private readonly onProgress?: (info: { totalPages: number; remainingPages: number }) => void;

  constructor(private db: Database.Database, dbPath: string, options: BackupOptions) {
    // DBファイル名のステム（拡張子を除いた部分）を取得
    this.dbStem = path.basename(dbPath, path.extname(dbPath)) || 'database';

    // バックアップディレクトリを決定
    // 指定がない場合はdbPathの親ディレクトリに"backup"フォルダを作成
    this.backupDir = options.backupDir ?? path.join(path.dirname(dbPath), 'backup');

    this.intervalMs = options.intervalMs ?? 3600000; // デフォルト1時間
    this.enabled = options.enabled ?? true;
    this.onProgress = options.onProgress;

    // バックアップディレクトリが存在しない場合は作成
    if (this.enabled && !fs.existsSync(this.backupDir)) {
      fs.mkdirSync(this.backupDir, { recursive: true });
    }
  }

  /**
   * 操作を記録し、必要に応じてバックアップを実行
   */
  async recordOperation(): Promise<void> {
    if (!this.enabled) {
      return;
    }

    const now = Date.now();
    this.lastOperationTime = now;

    // 前回のバックアップからの経過時間をチェック
    if (this.lastBackupTime === null || now - this.lastBackupTime >= this.intervalMs) {
      await this.backup();
    }
  }

  /**
   * 手動でバックアップを実行
   */
  async backup(): Promise<string> {
    if (!this.enabled) {
      throw new Error('Backup is disabled');
    }

    // タイムスタンプを生成 (yyyymmddhhmmss-mmm形式、ミリ秒を含む)
    const now = new Date();
    const pad = (n: number, len = 2) => n.toString().padStart(len, '0');
    const timestamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}-${pad(now.getMilliseconds(), 3)}`;

    // ファイル名形式: {db_stem}.backup-{yyyymmddhhmmss-mmm}.db
    const backupFileName = `${this.dbStem}.backup-${timestamp}.db`;
    const backupPath = path.join(this.backupDir, backupFileName);

    await this.db.backup(backupPath, {
      progress: (info) => {
        if (this.onProgress) {
          this.onProgress(info);
        }
        return 200; // ページ数ごとに進捗を報告
      },
    });

    this.lastBackupTime = Date.now();

    return backupPath;
  }

  /**
   * バックアップ一覧を取得
   */
  listBackups(): Array<{ name: string; path: string; createdAt: Date }> {
    if (!fs.existsSync(this.backupDir)) {
      return [];
    }

    // このDBのバックアップファイルのプレフィックス: {db_stem}.backup-
    const prefix = `${this.dbStem}.backup-`;

    return fs
      .readdirSync(this.backupDir)
      .filter((file) => file.startsWith(prefix) && file.endsWith('.db'))
      .map((file) => ({
        name: file,
        path: path.join(this.backupDir, file),
        createdAt: fs.statSync(path.join(this.backupDir, file)).mtime,
      }))
      .sort((a, b) => b.createdAt.getTime() - a.createdAt.getTime());
  }

  /**
   * 最後のバックアップ時刻を取得
   */
  getLastBackupTime(): Date | null {
    return this.lastBackupTime !== null ? new Date(this.lastBackupTime) : null;
  }

  /**
   * 最後の操作時刻を取得
   */
  getLastOperationTime(): Date | null {
    return this.lastOperationTime !== null ? new Date(this.lastOperationTime) : null;
  }

  /**
   * 次回バックアップまでの残り時間（ミリ秒）を取得
   */
  getTimeUntilNextBackup(): number | null {
    if (this.lastBackupTime === null) {
      return 0; // 次の操作で即座にバックアップ
    }
    const elapsed = Date.now() - this.lastBackupTime;
    const remaining = this.intervalMs - elapsed;
    return remaining > 0 ? remaining : 0;
  }

  /**
   * 条件に一致するバックアップを選択
   */
  selectBackup(selector: BackupSelector): BackupInfo | null {
    const backups = this.listBackups();

    if (backups.length === 0) {
      return null;
    }

    switch (selector.type) {
      case 'latest':
        return backups[0] ?? null;

      case 'nth':
        return backups[selector.n] ?? null;

      case 'before':
        // 指定日時より前の最新バックアップ（降順なので最初に見つかったものが最新）
        return backups.find((b) => b.createdAt < selector.date) ?? null;

      case 'after':
        // 指定日時より後の最古バックアップ（降順なので最後に見つかったものが最古）
        return backups.filter((b) => b.createdAt > selector.date).at(-1) ?? null;

      case 'closestTo': {
        // 指定日時に最も近いバックアップ
        const targetTime = selector.date.getTime();
        return backups.reduce((closest, current) => {
          const closestDiff = Math.abs(closest.createdAt.getTime() - targetTime);
          const currentDiff = Math.abs(current.createdAt.getTime() - targetTime);
          return currentDiff < closestDiff ? current : closest;
        });
      }
    }
  }

  /**
   * 条件に一致するバックアップのパスを取得
   */
  getBackupPath(selector: BackupSelector): string | null {
    return this.selectBackup(selector)?.path ?? null;
  }
}
