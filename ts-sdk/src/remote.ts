/**
 * リモートDB操作用のSSH接続ラッパー
 */
import { Client, ConnectConfig } from 'ssh2';
import SSHConfig from 'ssh-config';
import { readFileSync } from 'fs';
import { homedir } from 'os';
import { resolve } from 'path';
import type {
  Media,
  MediaInput,
  MediaFilter,
  QueryOptions,
  Tag,
  TagUsageStats,
  MediaAttribute,
  TableColumnInfo,
  BulkUpdateItem,
  MediaHash,
  MediaHashInput,
  ComputeHashResult,
  BackupDiff,
  DiffDetail,
  ObserveOptions,
  ObserveResult,
  PromoteOutcome,
} from './types.js';
import type { UpdateExistOptions, UpdateExistResult } from './update_exist.js';
import type { BackupInfo, BackupScope, BackupKind, BackupMetaEntry, BackupOptions } from './backup.js';
import type { ThumbnailOptions, CheckThumbnailResult, UpdateThumbnailResult } from './types.js';
import type { FileOpOptions, FileOpResult } from './file_ops.js';
import { defaultFileOpOptions } from './file_ops.js';
import type { TrashEntry, TrashOperation } from './trash.js';
import { resolveWithinRoot, isProtected, trashDir } from './media_path.js';
import type { Target } from './config.js';
import { SDK_VERSION } from './version.js';

/**
 * バックアップセレクター
 */
export type RemoteBackupSelector =
  | { type: 'latest' }
  | { type: 'nth'; n: number }
  | { type: 'byId'; id: string }
  | { type: 'byPath'; path: string };

/** リモート CLI が返す BackupInfo の生 JSON 形状（listBackups/listPreStashes で共有）。 */
type RemoteBackupInfoRaw = {
  id: string;
  name: string;
  path: string;
  createdAt: number;
  scope: string;
  kind: { type: string; baseId?: string };
  label?: string;
  labelSource?: string;
  note?: string;
};

/**
 * リモート接続設定
 */
export interface RemoteConfig {
  sshHost: string;           // .ssh/configのHost名（必須）
  dbPath?: string;           // リモートの prod DB パス（デフォルト: ~/.local/share/kijuku/kijuku.db）
  /** リモートの stg DB パス（未指定時は dbPath から `<stem>.stg.db` を導出・設計 §4.1） */
  stgDbPath?: string;
  workDir?: string;          // 作業ディレクトリ（省略可、将来の拡張用）
  binaryPath?: string;       // バイナリパス（デフォルト: ~/.local/bin/kijuku-cli）
  port?: number;             // SSHポート（省略時はSSH設定から読み取り）
  mediaRoot?: string;        // リモートホスト上の media root（ファイル操作APIのサンドボックス境界）
  /** 操作対象 DB（未指定 = デフォルト stg・設計 §13）。prod は readonly + migrate skip。 */
  target?: Target;
}

const DEFAULT_REMOTE_PROD_DB = '~/.local/share/kijuku/kijuku.db';
const DEFAULT_REMOTE_STG_DB = '~/.local/share/kijuku/kijuku.stg.db';

/** デフォルト RPC タイムアウト（短操作・Rust `DEFAULT_RPC_TIMEOUT_MS` parity）。 */
const DEFAULT_RPC_TIMEOUT_MS = 30_000;

/**
 * 適応的タイムアウトを適用する長時間操作の operation 名（Rust `LONG_RUNNING_OPS` parity）。
 * これらは `executeRemoteCommand` が DB サイズから `calcBackupTimeoutMs` でタイムアウトを算出する。
 */
const LONG_RUNNING_OPS: ReadonlySet<string> = new Set([
  'backup',
  'restore',
  'sync',
  'discard',
  'diffBackup',
  'diffProdStg',
  'observe',
  'promote',
]);

/**
 * リモートの prod DB パスから stg DB パスを導出する（設計 §4.1）。
 * `<stem>.db` → `<stem>.stg.db`（拡張子の前に `.stg` を挿入）。`~` 含むパスもそのまま処理。
 */
export function deriveStgDbPath(prodPath: string): string {
  const lastSlash = prodPath.lastIndexOf('/');
  const lastDot = prodPath.lastIndexOf('.');
  if (lastDot > lastSlash) {
    return `${prodPath.slice(0, lastDot)}.stg${prodPath.slice(lastDot)}`;
  }
  return `${prodPath}.stg.db`;
}

/**
 * `RemoteConfig` から target に応じたリモート DB パスを解決する（設計 §4.1・§13）。
 * - prod: `dbPath`（未設定時は prod デフォルト）
 * - stg: `stgDbPath`、未設定なら `dbPath` から導出、それも無ければ stg デフォルト
 */
export function resolveRemoteDbPath(config: RemoteConfig): string {
  const target: Target = config.target ?? 'stg';
  if (target === 'prod') {
    return config.dbPath ?? DEFAULT_REMOTE_PROD_DB;
  }
  return (
    config.stgDbPath ??
    (config.dbPath !== undefined ? deriveStgDbPath(config.dbPath) : undefined) ??
    DEFAULT_REMOTE_STG_DB
  );
}

/**
 * `MAJOR.MINOR.PATCH` を `[major, minor, patch]` にパース（TASK-69・Rust `parse_semver` parity）。
 * 3要素未満は 0 補間。4要素以上・非整数・負数は `null`。
 * kijuku-cli のバージョンは厳密 `MAJOR.MINOR.PATCH` 形式前提（pre-release 非対応・パース失敗は呼び元でデプロイ扱い）。
 */
export function parseSemver(s: string): [number, number, number] | null {
  const parts = s.split('.');
  if (parts.length > 3) return null;
  const nums = parts.map((p) => Number(p));
  if (nums.some((n) => !Number.isInteger(n) || n < 0)) return null;
  return [nums[0] ?? 0, nums[1] ?? 0, nums[2] ?? 0];
}

/**
 * ローカルよりリモートが古い場合にデプロイが必要か（TASK-69・Rust `needs_deploy` parity）。
 * - `local > remote`（厳密大なり）→ `true`（アップデート）
 * - equal / ローカルが古い（ダウングレード保護）→ `false`
 * - リモート未取得（`null`）またはパース失敗 → `true`（フェイルセーフ・自動回復）
 */
export function needsDeploy(local: string, remote: string | null): boolean {
  const l = parseSemver(local);
  if (!l) return true;
  const r = remote !== null ? parseSemver(remote) : null;
  if (!r) return true;
  return l[0] !== r[0] ? l[0] > r[0] : l[1] !== r[1] ? l[1] > r[1] : l[2] > r[2];
}

/**
 * コマンドリクエスト
 */
interface CommandRequest {
  operation: string;
  params: any;
}

/**
 * コマンドレスポンス
 */
interface CommandResponse {
  success: boolean;
  data?: any;
  error?: string;
}

/**
 * SSH セッション健全性に関わるエラー（TASK-71・Rust `KijukuError::Ssh` parity）。
 * `withSession` はこのエラーを検出すると Session slot を無効化し、次回 RPC で再接続する。
 * アプリケーションエラー（exit≠0・JSON パース失敗・ファイル作成失敗等）は素の `Error` のままで slot を保持する。
 */
class SshSessionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'SshSessionError';
    Object.setPrototypeOf(this, SshSessionError.prototype);
  }
}

/**
 * 適応的タイムアウト算出に用いる DB パスと `calcBackupTimeoutMs` 結果への乗数（Rust `long_op_target` parity）。
 * 戻り値は長操作の場合のみ非 null。
 * - restore: 現在 DB の退避 + 復元の 2 段階で 2 倍
 * - sync/discard: prod パス（target に依存しない）
 * - 上記以外の長操作: `getRemoteDbPath` と同一・乗数 1
 */
interface LongOpTarget {
  dbPath: string;
  multiplier: number;
}

/**
 * リモートKijuku DB操作クラス
 *
 * SSH `Client` は単一 slot でキャッシュ・再利用される（TASK-71・Rust parity）。
 * 初回 RPC で TCP+handshake+認証を済ませた接続を確立し、以降の連続 RPC はそれを使い回す。
 * セッション系エラー（`SshSessionError`）時は slot を無効化し、次回 RPC で再接続する。
 *
 * **直列化のトレードオフ**: `withSession` は RPC 全期間 Promise chain を占有する。
 * 同一インスタンスで RPC を並行発行しても直列化される（真の並行には複数 Session が必要・本タスクの範囲外）。
 */
export class RemoteKijukuDB {
  private sshClient: Client | null = null;
  private config: RemoteConfig;
  /** `ensureRemoteBinary`（毎 RPC 先頭の getServerVersion ラウンドトリップ）完了フラグ。slot 生存中は true でスキップしレイテンシを削減。 */
  private binaryEnsured = false;
  /** 新規 SSH 接続を確立した回数（診断用・連続 RPC で 1 のままなら再利用を示す）。 */
  private sessionConnectCount = 0;
  /** RPC を直列化する Promise chain（同一インスタンスの並行 RPC を順次化・二重 connect を防ぐ）。 */
  private sessionChain: Promise<unknown> = Promise.resolve();

  constructor(config: RemoteConfig) {
    this.config = config;
  }

  /**
   * 新規 SSH 接続を確立した回数（診断用・TASK-71）。連続 RPC で 1 のままなら Session 再利用を示す。
   */
  get connectCount(): number {
    return this.sessionConnectCount;
  }

  /**
   * SSH設定を読み込む
   */
  private loadSSHConfig(): ConnectConfig {
    const configPath = resolve(homedir(), '.ssh', 'config');
    const configContent = readFileSync(configPath, 'utf-8');
    const config = SSHConfig.parse(configContent);

    const hostConfig = config.compute(this.config.sshHost);

    // Portの型を適切に処理（数値または文字列の可能性）
    // ssh-configライブラリはプロパティ名の大文字・小文字が揺れる可能性があるため両方チェック
    let port = 22; // デフォルト
    if (this.config.port !== undefined) {
      port = this.config.port;
    } else if ((hostConfig as any).port !== undefined || hostConfig.Port !== undefined) {
      const portValue = (hostConfig as any).port ?? hostConfig.Port;
      port = Number(portValue);
    }

    const connectConfig: ConnectConfig = {
      host: String(hostConfig.HostName || ''),
      port,
      username: String(hostConfig.User || ''),
    };

    // 認証情報
    if (hostConfig.IdentityFile) {
      let identityFile = Array.isArray(hostConfig.IdentityFile)
        ? hostConfig.IdentityFile[0]
        : hostConfig.IdentityFile;
      // ~/ を展開
      if (typeof identityFile === 'string' && identityFile.startsWith('~/')) {
        identityFile = resolve(homedir(), identityFile.substring(2));
      }
      connectConfig.privateKey = readFileSync(String(identityFile));
    }

    return connectConfig;
  }

  /**
   * リモートバイナリパスを取得
   */
  private getRemoteBinaryPath(): string {
    return this.config.binaryPath || '~/.local/bin/kijuku-cli';
  }

  /**
   * リモートDBパスを取得（target に応じて prod/stg を解決・設計 §4.1・§13）
   */
  private getRemoteDbPath(): string {
    return resolveRemoteDbPath(this.config);
  }

  /**
   * 操作対象 target を取得（未指定 = stg・設計 §13）
   */
  private getTarget(): Target {
    return this.config.target ?? 'stg';
  }

  /**
   * シェルコマンド内でパスを安全に使用できるようにクォートする
   * ~はSSHリモートの$HOMEに変換し、シングルクォートで残りを保護する
   */
  private escapeShellPath(filePath: string): string {
    if (filePath.startsWith('~/')) {
      const rest = filePath.slice(2).replace(/'/g, "'\\''");
      return `"$HOME"/'${rest}'`;
    }
    if (filePath === '~') {
      return '"$HOME"';
    }
    return `'${filePath.replace(/'/g, "'\\''")}'`;
  }

  /**
   * SSH接続を確立（キャッシュ済み Client 再構築用・都度 TCP+handshake+認証）。
   * 戻り値の `Client` は `withSession` 経由でプールにキャッシュされる（TASK-71）。
   */
  private connect(connectTimeoutMs = 30_000): Promise<void> {
    if (this.sshClient) {
      return Promise.resolve(); // 既に接続済み
    }

    return new Promise((resolve, reject) => {
      const client = new Client();
      const sshConfig = this.loadSSHConfig();

      client.on('ready', () => {
        this.sshClient = client;
        resolve();
      });

      client.on('error', (err) => {
        reject(new SshSessionError(`SSH接続エラー: ${err.message}`));
      });

      client.connect({ ...sshConfig, readyTimeout: connectTimeoutMs });
    });
  }

  /**
   * プールされた SSH Client 上で `fn` を実行（TASK-71・Rust `with_session` parity）。
   * 初回は `connect()` で確立してキャッシュ、以降は再利用。`fn` が `SshSessionError` を投げた場合は
   * slot を無効化し、次回 RPC で再接続する（フェイルセーフ）。Promise chain で RPC 全体を直列化する。
   */
  private withSession<R>(fn: (client: Client) => Promise<R>): Promise<R> {
    const run = this.sessionChain.then(async (): Promise<R> => {
      if (!this.sshClient) {
        await this.connect(); // 失敗時は SshSessionError が伝播（slot は null のまま）
        this.sessionConnectCount++;
      }
      const client = this.sshClient;
      if (!client) {
        // 接続直後であるため通常到達しない（型ナロウリング用の防御）
        throw new SshSessionError('SSH session unavailable after connect');
      }
      try {
        return await fn(client);
      } catch (err) {
        if (err instanceof SshSessionError) {
          await this.invalidateSession();
        }
        throw err;
      }
    });
    // this RPC の成否に関わらず chain を維持し、後続 RPC が滞らないようにする
    this.sessionChain = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  }

  /**
   * プールされた SSH Client を切断し、slot を無効化（TASK-71）。
   * `binaryEnsured` もリセットし、次回接続時に `ensureRemoteBinary` を再実行させる。
   */
  private async invalidateSession(): Promise<void> {
    const client = this.sshClient;
    this.sshClient = null;
    this.binaryEnsured = false;
    if (client) {
      try {
        client.end();
      } catch {
        // 切断エラーは無視（slot は既に無効化済み）
      }
    }
  }

  /**
   * プールされた SSH Client を切断し、slot を無効化（TASK-71・Rust `disconnect` parity）。
   * SDK 長期利用者が明示的に呼ぶ用途で、以降の RPC は再接続される。実行中の RPC がある場合はその完了を待つ。
   */
  async disconnect(): Promise<void> {
    const run = this.sessionChain.then(() => this.invalidateSession());
    this.sessionChain = run.then(
      () => undefined,
      () => undefined,
    );
    await run;
  }

  /**
   * 長操作の適応的タイムアウト算出に用いる DB パスと乗数（Rust `long_op_target` parity）。
   * 長操作でない場合は `null`（呼出元は `DEFAULT_RPC_TIMEOUT_MS` を使用）。
   */
  private longOpTarget(operation: string): LongOpTarget | null {
    switch (operation) {
      case 'restore':
        return { dbPath: this.getRemoteDbPath(), multiplier: 2 };
      case 'sync':
      case 'discard':
        return { dbPath: this.config.dbPath ?? DEFAULT_REMOTE_PROD_DB, multiplier: 1 };
      default:
        return LONG_RUNNING_OPS.has(operation)
          ? { dbPath: this.getRemoteDbPath(), multiplier: 1 }
          : null;
    }
  }

  /**
   * リモートでコマンドを実行
   */
  private async execCommand(command: string, timeoutMs = 30_000): Promise<string> {
    const client = this.sshClient;
    if (!client) {
      throw new Error('SSH接続が確立されていません');
    }

    return new Promise((resolve, reject) => {
      let timer: ReturnType<typeof setTimeout> | null = null;

      client.exec(command, (err, stream) => {
        if (err) {
          // channel/exec 確立失敗 = セッション健全性破綻 → slot 無効化（Rust `Ssh` parity）
          reject(new SshSessionError(`コマンド実行エラー: ${err.message}`));
          return;
        }

        timer = setTimeout(() => {
          stream.destroy();
          // 読み取りタイムアウト = デッド channel の可能性 → slot 無効化（Rust `Ssh` parity）
          reject(new SshSessionError(`コマンドがタイムアウトしました (${timeoutMs}ms)`));
        }, timeoutMs);

        let stdout = '';
        let stderr = '';

        stream.on('close', (code: number) => {
          if (timer) clearTimeout(timer);
          if (code !== 0) {
            reject(new Error(`コマンドが失敗しました (exit code: ${code}): ${stderr}`));
          } else {
            resolve(stdout);
          }
        });

        stream.on('data', (data: Buffer) => {
          stdout += data.toString();
        });

        stream.stderr.on('data', (data: Buffer) => {
          stderr += data.toString();
        });
      });
    });
  }

  /**
   * リモートファイルのサイズを取得（バイト）
   */
  private async getRemoteFileSize(filePath: string): Promise<number> {
    const output = await this.execCommand(`stat -c %s ${this.escapeShellPath(filePath)} 2>/dev/null || echo 0`);
    return parseInt(output.trim(), 10) || 0;
  }

  /**
   * DBサイズに基づいてバックアップのタイムアウトを計算（ms）
   *
   * rusqliteバックアップ設定: 750,000ページ/バッチ, 10秒スリープ
   * HDD想定速度: 50MB/s
   */
  private calcBackupTimeoutMs(dbSizeBytes: number): number {
    const PAGE_SIZE = 4096;
    const HDD_BYTES_PER_MS = (50 * 1024 * 1024) / 1000;
    const BATCH_PAGES = 750_000;
    const SLEEP_PER_BATCH_MS = 10_000;
    const MARGIN = 2;

    const copyTimeMs = dbSizeBytes / HDD_BYTES_PER_MS;
    const numBatches = Math.ceil(dbSizeBytes / (BATCH_PAGES * PAGE_SIZE));
    const sleepTimeMs = numBatches * SLEEP_PER_BATCH_MS;

    return Math.max((copyTimeMs + sleepTimeMs) * MARGIN, 60_000);
  }

  /**
   * リモート CLI バイナリのバージョンを取得（TASK-69・`getServerVersion` operation）。
   * バイナリ未存在/起動失敗/operation 未対応（古いバイナリ）は `null`（呼び元でデプロイ→自動回復）。
   */
  private async getServerVersion(): Promise<string | null> {
    const remoteBinaryPath = this.getRemoteBinaryPath();
    const remoteDbPath = this.getRemoteDbPath();
    const target = this.getTarget();
    const jsonInput = JSON.stringify({ operation: 'getServerVersion', params: {} });
    const escapedJson = jsonInput.replace(/'/g, "'\\''");
    const mediaRootFlag = this.config.mediaRoot
      ? ` --media-root ${this.escapeShellPath(this.config.mediaRoot)}`
      : '';
    const command = `echo '${escapedJson}' | ${this.escapeShellPath(remoteBinaryPath)} --db ${this.escapeShellPath(remoteDbPath)} --target ${target}${mediaRootFlag}`;
    try {
      const output = await this.execCommand(command);
      const response: CommandResponse = JSON.parse(output.trim());
      if (!response.success) return null;
      const data: unknown = response.data;
      if (typeof data === 'object' && data !== null && 'version' in data) {
        const version = data.version;
        if (typeof version === 'string') return version;
      }
      return null;
    } catch {
      return null;
    }
  }

  /**
   * ローカルからリモートにファイルを転送
   */
  private async uploadFile(localPath: string, remotePath: string, timeoutMs = 120_000): Promise<void> {
    const client = this.sshClient;
    if (!client) {
      throw new Error('SSH接続が確立されていません');
    }

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error(`ファイル転送がタイムアウトしました (${timeoutMs}ms): ${remotePath}`));
      }, timeoutMs);

      client.sftp((err, sftp) => {
        if (err) {
          clearTimeout(timer);
          // SFTP サブシステム確立失敗 = セッション健全性破綻 → slot 無効化（Rust `Ssh` parity）
          reject(new SshSessionError(`SFTP接続エラー: ${err.message}`));
          return;
        }

        const localData = readFileSync(localPath);
        sftp.writeFile(remotePath, localData, (writeErr) => {
          clearTimeout(timer);
          if (writeErr) {
            // リモートファイル作成失敗 = アプリケーションエラー → slot 保持（Rust `Other` parity）
            reject(new Error(`ファイル転送エラー: ${writeErr.message}`));
          } else {
            resolve();
          }
        });
      });
    });
  }

  /**
   * リモートにバイナリを自動デプロイ
   */
  private async deployBinaryToRemote(): Promise<void> {
    const localBinaryPath = resolve(homedir(), '.local', 'bin', 'kijuku-cli');
    // deploy-local.sh parity: 実体 + symlink 構成（TASK-69・Rust parity）。ssh2 SFTP は ~ を展開する。
    const remoteReal = '~/.local/kijuku-db/bin/kijuku-cli';
    const remoteLink = '~/.local/bin/kijuku-cli';

    // (1) 実体Dir + symlinkDir を作成
    await this.execCommand(
      `mkdir -p ${this.escapeShellPath('~/.local/kijuku-db/bin')} ${this.escapeShellPath('~/.local/bin')}`,
    );
    // (2) バイナリを転送（uploadFile は media_root 制約なしの SFTP 直接書き込み）
    await this.uploadFile(localBinaryPath, remoteReal);
    // (3) 実行権限を付与
    await this.execCommand(`chmod +x ${this.escapeShellPath(remoteReal)}`);
    // (4) symlink 作成（冪等・上書き）
    await this.execCommand(
      `ln -sf ${this.escapeShellPath(remoteReal)} ${this.escapeShellPath(remoteLink)}`,
    );
  }

  /**
   * リモートでJSONコマンドを実行（TASK-71・Rust `execute_remote_command_timed` + `execute_on_session` parity）。
   *
   * - SSH `Client` は `withSession` でプールから取得（初回のみ接続・以降再利用・`SshSessionError` で無効化）。
   * - `binaryEnsured` で `ensureRemoteBinary`（getServerVersion ラウンドトリップ）を初回のみ実行し、
   *   連続 RPC のオーバーヘッドを削減。
   * - タイムアウトは `timeoutMs` 指定時はそれを採用、未指定時は長操作なら DB サイズから適応算出、
   *   短操作なら `DEFAULT_RPC_TIMEOUT_MS` (30s)。
   */
  private async executeRemoteCommand(request: CommandRequest, timeoutMs?: number): Promise<CommandResponse> {
    return this.withSession(async () => {
      // バージョンベース自動デプロイ（TASK-69）。binaryEnsured で初回のみ（連続 RPC の getServerVersion
      // オーバーヘッドを削減・Rust `binary_ensured` parity）。
      if (!this.binaryEnsured) {
        const remoteVersion = await this.getServerVersion();
        if (needsDeploy(SDK_VERSION, remoteVersion)) {
          await this.deployBinaryToRemote();
        }
        this.binaryEnsured = true;
      }

      // 適応的タイムアウト解決（Rust `execute_on_session` parity）。
      //   指定あり → 呼出元上書き
      //   未指定かつ長操作 → DB サイズから算出（restore は退避+復元で ×2）
      //   未指定かつ短操作 → デフォルト 30s
      let effectiveTimeout: number;
      if (timeoutMs !== undefined) {
        effectiveTimeout = timeoutMs;
      } else {
        const opTarget = this.longOpTarget(request.operation);
        effectiveTimeout = opTarget
          ? this.calcBackupTimeoutMs(await this.getRemoteFileSize(opTarget.dbPath)) * opTarget.multiplier
          : DEFAULT_RPC_TIMEOUT_MS;
      }

      // コマンドを実行
      const remoteBinaryPath = this.getRemoteBinaryPath();
      const remoteDbPath = this.getRemoteDbPath();
      const target = this.getTarget();
      const jsonInput = JSON.stringify(request);
      const escapedJson = jsonInput.replace(/'/g, "'\\''");
      const mediaRootFlag = this.config.mediaRoot
        ? ` --media-root ${this.escapeShellPath(this.config.mediaRoot)}`
        : '';
      // --target を常に付与（設計 §13「CLI が常に勝つ」）。リモート側 resolveTarget が
      // target から readonly を正しく導出し、prod を RW で開く保護ホールを防ぐ。
      const command = `echo '${escapedJson}' | ${this.escapeShellPath(remoteBinaryPath)} --db ${this.escapeShellPath(remoteDbPath)} --target ${target}${mediaRootFlag}`;

      const output = await this.execCommand(command, effectiveTimeout);
      const response: CommandResponse = JSON.parse(output.trim());
      return response;
    });
  }

  /**
   * レスポンスのエラーチェック
   */
  private checkResponse(response: CommandResponse): any {
    if (!response.success) {
      throw new Error(response.error || '不明なエラー');
    }
    return response.data;
  }

  /**
   * マイグレーションを実行
   */
  async migrate(): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'migrate',
      params: {},
    });
    this.checkResponse(response);
  }

  /**
   * 現在のスキーマバージョンを取得
   */
  async getSchemaVersion(): Promise<number> {
    const response = await this.executeRemoteCommand({
      operation: 'getSchemaVersion',
      params: {},
    });
    const data = this.checkResponse(response);
    return data.version;
  }

  /**
   * テーブル一覧を取得
   */
  async getTables(): Promise<string[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getTables',
      params: {},
    });
    return this.checkResponse(response);
  }

  /**
   * 特定テーブルのカラム情報を取得
   */
  async getTableInfo(tableName: string): Promise<TableColumnInfo[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getTableInfo',
      params: { table_name: tableName },
    });
    return this.checkResponse(response);
  }

  /**
   * メディアを作成
   */
  async createMedia(data: MediaInput): Promise<Media> {
    const response = await this.executeRemoteCommand({
      operation: 'createMedia',
      params: { data },
    });
    return this.checkResponse(response);
  }

  /** src を dst へ複製する（リモート CLI に委譲・dry-run ファースト・上書きは trash 経由）。 */
  async mediaCp(src: string, dst: string, options: FileOpOptions = defaultFileOpOptions): Promise<FileOpResult> {
    const response = await this.executeRemoteCommand({
      operation: 'mediaCp',
      params: { src, dst, options },
    });
    return this.checkResponse(response);
  }

  /** src を dst へ移動する（リモート CLI に委譲・dry-run ファースト・上書きは trash 経由）。 */
  async mediaMv(src: string, dst: string, options: FileOpOptions = defaultFileOpOptions): Promise<FileOpResult> {
    const response = await this.executeRemoteCommand({
      operation: 'mediaMv',
      params: { src, dst, options },
    });
    return this.checkResponse(response);
  }

  /** src（ディレクトリ）の内容を dst へ同期する（リモート CLI に委譲・safe モード）。 */
  async mediaSync(src: string, dst: string, options: FileOpOptions = defaultFileOpOptions): Promise<FileOpResult> {
    const response = await this.executeRemoteCommand({
      operation: 'mediaSync',
      params: { src, dst, options },
    });
    return this.checkResponse(response);
  }

  /** targetRel を trash へ移動する（リモート CLI に委譲・論理削除）。 */
  async moveToTrash(targetRel: string, operation: TrashOperation, reason?: string): Promise<string> {
    const response = await this.executeRemoteCommand({
      operation: 'moveToTrash',
      params: { target_rel: targetRel, operation, reason: reason ?? null },
    });
    return this.checkResponse(response);
  }

  /** trash 内のエントリ一覧を返す（リモート CLI に委譲）。 */
  async listTrash(): Promise<TrashEntry[]> {
    const response = await this.executeRemoteCommand({
      operation: 'listTrash',
      params: {},
    });
    return this.checkResponse(response);
  }

  /** trash から id のエントリを復元する（リモート CLI に委譲）。戻り値はリモートホスト上の絶対パス。 */
  async restoreFromTrash(id: string): Promise<string> {
    const response = await this.executeRemoteCommand({
      operation: 'restoreFromTrash',
      params: { id },
    });
    return this.checkResponse(response);
  }

  /** trash 内のエントリを物理削除する（リモート CLI に委譲・dry-run ファースト）。 */
  async purgeTrash(ids?: string[], dryRun = false): Promise<string[]> {
    const response = await this.executeRemoteCommand({
      operation: 'purgeTrash',
      params: { ids: ids ?? null, dry_run: dryRun },
    });
    return this.checkResponse(response);
  }

  /**
   * ローカルの localPath をリモートの remoteRel（mediaRoot 相対）へアップロード（SFTP・fastPut）。
   */
  async upload(localPath: string, remoteRel: string, timeoutMs = 120_000): Promise<void> {
    const remoteAbs = this.resolveRemoteWithinRoot(remoteRel);
    await this.withSession(async (client) => {
      // 親ディレクトリを再帰作成（mkdir -p 相当）
      const lastSlash = remoteAbs.lastIndexOf('/');
      if (lastSlash > 0) {
        const remoteDir = remoteAbs.substring(0, lastSlash);
        await this.execCommand(`mkdir -p ${this.escapeShellPath(remoteDir)}`);
      }
      await new Promise<void>((resolvePromise, reject) => {
        const timer = setTimeout(() => {
          reject(new Error(`アップロードがタイムアウトしました (${timeoutMs}ms): ${remoteAbs}`));
        }, timeoutMs);
        client.sftp((err, sftp) => {
          if (err) {
            clearTimeout(timer);
            // SFTP サブシステム確立失敗 = セッション健全性破綻 → slot 無効化（Rust `Ssh` parity）
            reject(new SshSessionError(`SFTP接続エラー: ${err.message}`));
            return;
          }
          sftp.fastPut(localPath, remoteAbs, (putErr) => {
            clearTimeout(timer);
            if (putErr) {
              // 書込失敗 = アプリケーションエラー → slot 保持（Rust `Other`/`Io` parity）
              reject(new Error(`アップロードエラー: ${putErr.message}`));
            } else {
              resolvePromise();
            }
          });
        });
      });
    });
  }

  /**
   * リモートの remoteRel（mediaRoot 相対）をローカルの localPath へダウンロード（SFTP・fastGet）。
   */
  async download(remoteRel: string, localPath: string, timeoutMs = 120_000): Promise<void> {
    const remoteAbs = this.resolveRemoteWithinRoot(remoteRel);
    await this.withSession(async (client) => {
      await new Promise<void>((resolvePromise, reject) => {
        const timer = setTimeout(() => {
          reject(new Error(`ダウンロードがタイムアウトしました (${timeoutMs}ms): ${remoteAbs}`));
        }, timeoutMs);
        client.sftp((err, sftp) => {
          if (err) {
            clearTimeout(timer);
            // SFTP サブシステム確立失敗 = セッション健全性破綻 → slot 無効化（Rust `Ssh` parity）
            reject(new SshSessionError(`SFTP接続エラー: ${err.message}`));
            return;
          }
          sftp.fastGet(remoteAbs, localPath, (getErr) => {
            clearTimeout(timer);
            if (getErr) {
              // 読取失敗 = アプリケーションエラー → slot 保持（Rust `Other`/`Io` parity）
              reject(new Error(`ダウンロードエラー: ${getErr.message}`));
            } else {
              resolvePromise();
            }
          });
        });
      });
    });
  }

  /**
   * リモートの remoteRel を mediaRoot 配下に解決し、保護パス（.trash 等）を拒否する。
   * リモート FS を canonicalize できないため lexical 解決のみ。
   */
  private resolveRemoteWithinRoot(remoteRel: string): string {
    if (!this.config.mediaRoot) {
      throw new Error('mediaRoot is not configured; set RemoteConfig.mediaRoot to use upload/download');
    }
    const abs = resolveWithinRoot(this.config.mediaRoot, remoteRel);
    if (isProtected(abs, [trashDir(this.config.mediaRoot)])) {
      throw new Error(`remote path is a protected path (${remoteRel}); refused`);
    }
    return abs;
  }

  /**
   * IDでメディアを取得
   */
  async getMedia(id: number): Promise<Media | null> {
    const response = await this.executeRemoteCommand({
      operation: 'getMedia',
      params: { id },
    });
    return this.checkResponse(response);
  }

  /**
   * メディアを更新
   */
  async updateMedia(id: number, data: Partial<MediaInput>): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'updateMedia',
      params: { id, data },
    });
    this.checkResponse(response);
  }

  /**
   * メディアを削除
   */
  async deleteMedia(id: number): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'deleteMedia',
      params: { id },
    });
    this.checkResponse(response);
  }

  /**
   * メディアを検索
   */
  async findMedia(filter: MediaFilter, options?: QueryOptions): Promise<Media[]> {
    const response = await this.executeRemoteCommand({
      operation: 'findMedia',
      params: { filter, options },
    });
    return this.checkResponse(response);
  }

  async getDistinctValues(fields: string[], filter: MediaFilter): Promise<(string | null)[][]> {
    const response = await this.executeRemoteCommand({
      operation: 'getDistinctValues',
      params: { fields, filter },
    });
    return this.checkResponse(response);
  }

  /**
   * 複数のメディアを一括作成
   */
  async bulkCreateMedia(dataList: MediaInput[]): Promise<Media[]> {
    const response = await this.executeRemoteCommand({
      operation: 'bulkCreateMedia',
      params: { data_list: dataList },
    });
    return this.checkResponse(response);
  }

  /**
   * 複数のメディアを一括削除
   */
  async bulkDeleteMedia(ids: number[]): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'bulkDeleteMedia',
      params: { ids },
    });
    this.checkResponse(response);
  }

  /**
   * 複数のメディアを一括更新
   */
  async bulkUpdateMedia(updates: BulkUpdateItem[]): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'bulkUpdateMedia',
      params: { updates },
    });
    this.checkResponse(response);
  }

  /**
   * タグを作成
   */
  async createTag(name: string): Promise<Tag> {
    const response = await this.executeRemoteCommand({
      operation: 'createTag',
      params: { name },
    });
    return this.checkResponse(response);
  }

  /**
   * タグ名でタグを取得
   */
  async getTagByName(name: string): Promise<Tag | null> {
    const response = await this.executeRemoteCommand({
      operation: 'getTagByName',
      params: { name },
    });
    return this.checkResponse(response);
  }

  /**
   * 全てのタグを取得
   */
  async getAllTags(): Promise<Tag[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getAllTags',
      params: {},
    });
    return this.checkResponse(response);
  }

  /**
   * メディアにタグを追加
   */
  async addTagToMedia(mediaId: number, tagId: number): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'addTagToMedia',
      params: { media_id: mediaId, tag_id: tagId },
    });
    this.checkResponse(response);
  }

  /**
   * メディアからタグを削除
   */
  async removeTagFromMedia(mediaId: number, tagId: number): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'removeTagFromMedia',
      params: { media_id: mediaId, tag_id: tagId },
    });
    this.checkResponse(response);
  }

  /**
   * メディアに関連付けられたタグを取得
   */
  async getMediaTags(mediaId: number): Promise<Tag[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getMediaTags',
      params: { media_id: mediaId },
    });
    return this.checkResponse(response);
  }

  /**
   * 複数メディアのタグを一括取得（N+1回避。タグなしメディアはエントリに含まれない）
   *
   * リモートからは `{ [media_id: string]: Tag[] }` で返る（JSON object のキーは文字列）。
   * number キーの Record に正規化して返す。
   */
  async getMediaTagsBulk(
    mediaIds: number[]
  ): Promise<Record<number, Tag[]>> {
    const response = await this.executeRemoteCommand({
      operation: 'getMediaTagsBulk',
      params: { media_ids: mediaIds },
    });
    const raw = this.checkResponse(response);
    const result: Record<number, Tag[]> = {};
    for (const key of Object.keys(raw)) {
      result[Number(key)] = raw[key];
    }
    return result;
  }

  /**
   * タグの使用数統計を取得
   */
  async getTagUsageStats(): Promise<TagUsageStats[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getTagUsageStats',
      params: {},
    });
    return this.checkResponse(response);
  }

  /**
   * 未使用のタグを取得
   */
  async findUnusedTags(): Promise<Tag[]> {
    const response = await this.executeRemoteCommand({
      operation: 'findUnusedTags',
      params: {},
    });
    return this.checkResponse(response);
  }

  /**
   * メディアに属性を設定
   */
  async setMediaAttribute(
    mediaId: number,
    key: string,
    value: string | null,
    valueType?: string
  ): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'setMediaAttribute',
      params: { media_id: mediaId, key, value, value_type: valueType },
    });
    this.checkResponse(response);
  }

  /**
   * メディアの属性を取得
   */
  async getMediaAttribute(mediaId: number, key: string): Promise<MediaAttribute | null> {
    const response = await this.executeRemoteCommand({
      operation: 'getMediaAttribute',
      params: { media_id: mediaId, key },
    });
    return this.checkResponse(response);
  }

  /**
   * メディアの全ての属性を取得
   */
  async getMediaAttributes(mediaId: number): Promise<MediaAttribute[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getMediaAttributes',
      params: { media_id: mediaId },
    });
    return this.checkResponse(response);
  }

  /**
   * メディアの属性を削除
   */
  async deleteMediaAttribute(mediaId: number, key: string): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'deleteMediaAttribute',
      params: { media_id: mediaId, key },
    });
    this.checkResponse(response);
  }

  /**
   * メディアの全ての属性を削除
   */
  async deleteAllMediaAttributes(mediaId: number): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'deleteAllMediaAttributes',
      params: { media_id: mediaId },
    });
    this.checkResponse(response);
  }

  /**
   * フィルタで絞り込んだメディアのflag_existをファイル存在状態に基づいて更新する
   */
  async updateExist(
    filter: MediaFilter,
    options?: QueryOptions,
    updateOptions: UpdateExistOptions = { dry_run: false }
  ): Promise<UpdateExistResult> {
    const response = await this.executeRemoteCommand({
      operation: 'updateExist',
      params: {
        filter,
        options: options ?? null,
        update_options: updateOptions,
      },
    });
    return this.checkResponse(response);
  }

  /**
   * フィルタで絞り込んだメディアのサムネイル状態をチェックする
   */
  async checkThumbnail(
    filter: MediaFilter = {},
    options?: QueryOptions,
  ): Promise<CheckThumbnailResult> {
    const response = await this.executeRemoteCommand({
      operation: 'checkThumbnail',
      params: {
        filter,
        options: options ?? null,
        thumbnail_options: {},
      },
    });
    return this.checkResponse(response);
  }

  /**
   * フィルタで絞り込んだメディアのサムネイルを生成・更新する
   */
  async updateThumbnail(
    filter: MediaFilter = {},
    options?: QueryOptions,
    thumbnailOptions: ThumbnailOptions = {},
  ): Promise<UpdateThumbnailResult> {
    const response = await this.executeRemoteCommand({
      operation: 'updateThumbnail',
      params: {
        filter,
        options: options ?? null,
        thumbnail_options: thumbnailOptions,
      },
    });
    return this.checkResponse(response);
  }

  /**
   * 手動バックアップを実行
   */
  async backup(label?: string, timeoutMs?: number): Promise<string> {
    const response = await this.executeRemoteCommand(
      {
        operation: 'backup',
        params: { label: label ?? null },
      },
      timeoutMs,
    );
    const data = this.checkResponse(response);
    return data.path;
  }

  /**
   * バックアップ一覧を取得
   */
  async listBackups(): Promise<BackupInfo[]> {
    const response = await this.executeRemoteCommand({
      operation: 'listBackups',
      params: {},
    });
    const data: RemoteBackupInfoRaw[] = this.checkResponse(response);
    return data.map((item) => this.convertBackupInfoFromRemote(item));
  }

  /**
   * pre-stash（即時復旧用ロールバックファイル）一覧を取得（設計 §8）。
   * promote/(b)操作が返す preStashPath を呼出側が失った場合の発見経路。
   * 戻り値の path はそのまま RemoteBackupSelector の byPath で restore に渡せる。
   */
  async listPreStashes(): Promise<BackupInfo[]> {
    const response = await this.executeRemoteCommand({
      operation: 'listPreStashes',
      params: {},
    });
    const data: RemoteBackupInfoRaw[] = this.checkResponse(response);
    return data.map((item) => this.convertBackupInfoFromRemote(item));
  }

  /** リモート CLI の BackupInfo JSON を BackupInfo に変換（listBackups/listPreStashes で共有）。 */
  private convertBackupInfoFromRemote(item: RemoteBackupInfoRaw): BackupInfo {
    return {
      id: item.id,
      name: item.name,
      path: item.path,
      createdAt: new Date(item.createdAt * 1000),
      scope: item.scope as BackupScope,
      kind: item.kind.type === 'diff'
        ? ({ type: 'diff', baseId: item.kind.baseId! } as BackupKind)
        : ({ type: 'full' } as BackupKind),
      label: item.label,
      labelSource: (item.labelSource ?? 'filename') as BackupInfo['labelSource'],
      note: item.note,
    };
  }

  // ========== メディアハッシュ操作 ==========

  private hexToUint8Array(hex: string): Uint8Array {
    if (hex.length % 2 !== 0) {
      throw new Error('Hex string must have an even length');
    }
    const bytes = new Uint8Array(hex.length / 2);
    for (let i = 0; i < hex.length; i += 2) {
      bytes[i / 2] = parseInt(hex.substring(i, i + 2), 16);
    }
    return bytes;
  }

  private convertMediaHashFromRemote(raw: any): MediaHash {
    if (!raw) {
      throw new Error('Invalid remote media hash data');
    }
    return {
      ...raw,
      content_hash: this.hexToUint8Array(raw.content_hash),
      embedding: raw.embedding ? this.hexToUint8Array(raw.embedding) : undefined,
    };
  }

  async addMediaHash(input: MediaHashInput): Promise<MediaHash> {
    const response = await this.executeRemoteCommand({
      operation: 'addMediaHash',
      params: {
        input: {
          ...input,
          content_hash: Array.from(input.content_hash).map(b => b.toString(16).padStart(2, '0')).join(''),
        },
      },
    });
    return this.convertMediaHashFromRemote(this.checkResponse(response));
  }

  async addMediaHashes(inputs: MediaHashInput[]): Promise<MediaHash[]> {
    const converted = inputs.map(input => ({
      ...input,
      content_hash: Array.from(input.content_hash).map(b => b.toString(16).padStart(2, '0')).join(''),
    }));
    const response = await this.executeRemoteCommand({
      operation: 'addMediaHashes',
      params: { inputs: converted },
    });
    const data: any[] = this.checkResponse(response);
    return data.map(r => this.convertMediaHashFromRemote(r));
  }

  async getMediaHashes(itemUuid: string): Promise<MediaHash[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getMediaHashes',
      params: { item_uuid: itemUuid },
    });
    const data: any[] = this.checkResponse(response);
    return data.map(r => this.convertMediaHashFromRemote(r));
  }

  async getMediaHash(itemUuid: string, filename: string, timeRange: string): Promise<MediaHash | null> {
    const response = await this.executeRemoteCommand({
      operation: 'getMediaHash',
      params: { item_uuid: itemUuid, filename, time_range: timeRange },
    });
    const data = this.checkResponse(response);
    return data ? this.convertMediaHashFromRemote(data) : null;
  }

  async findByContentHash(hashBytes: Uint8Array): Promise<MediaHash[]> {
    const response = await this.executeRemoteCommand({
      operation: 'findByContentHash',
      params: { hash_hex: Array.from(hashBytes).map(b => b.toString(16).padStart(2, '0')).join('') },
    });
    const data: any[] = this.checkResponse(response);
    return data.map(r => this.convertMediaHashFromRemote(r));
  }

  async deleteMediaHash(itemUuid: string, filename: string, timeRange: string): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'deleteMediaHash',
      params: { item_uuid: itemUuid, filename, time_range: timeRange },
    });
    this.checkResponse(response);
  }

  async deleteMediaHashes(itemUuid: string): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'deleteMediaHashes',
      params: { item_uuid: itemUuid },
    });
    this.checkResponse(response);
  }

  async findDuplicateHashes(): Promise<Array<{ content_hash: Uint8Array; count: number }>> {
    const response = await this.executeRemoteCommand({
      operation: 'findDuplicateHashes',
      params: {},
    });
    const data: Array<{ content_hash: string; count: number }> = this.checkResponse(response);
    return data.map(item => ({ content_hash: this.hexToUint8Array(item.content_hash), count: item.count }));
  }

  async computeMediaHash(
    itemUuid: string,
    mediaPath: string,
    mediaType: string,
    durationSec?: number
  ): Promise<ComputeHashResult> {
    const response = await this.executeRemoteCommand({
      operation: 'computeMediaHash',
      params: { item_uuid: itemUuid, media_path: mediaPath, media_type: mediaType, duration_sec: durationSec ?? null },
    });
    const data: any = this.checkResponse(response);
    return { ...data, hashes: data.hashes.map((r: any) => this.convertMediaHashFromRemote(r)) };
  }

  async computeMediaHashes(filter: MediaFilter, options?: QueryOptions, force?: boolean): Promise<ComputeHashResult[]> {
    const response = await this.executeRemoteCommand({
      operation: 'computeMediaHashes',
      params: { filter, options: options ?? null, force: force ?? false },
    });
    const data: any[] = this.checkResponse(response);
    return data.map(r => ({ ...r, hashes: r.hashes.map((h: any) => this.convertMediaHashFromRemote(h)) }));
  }

  /**
   * バックアップを復元（(b) 制限操作・設計 §9.2）。`config.target='prod'` の前提で prod 直接経路
   * （ProdRwScope + pre-stash gate）で実行する（target=stg のまま呼ぶと (b) 操作として CLI 側で拒否される）。
   * `dryRun: true` で復元差分を返し prod 不変（§15-13）。
   */
  async restore(
    selector?: RemoteBackupSelector,
    timeoutMs?: number,
  ): Promise<string>;
  async restore(
    selector: RemoteBackupSelector,
    timeoutMs: number | undefined,
    dryRun: true,
  ): Promise<BackupDiff>;
  async restore(
    selector: RemoteBackupSelector = { type: 'latest' },
    timeoutMs?: number,
    dryRun = false,
  ): Promise<string | BackupDiff> {
    // restore は「現在の DB の退避バックアップ」+「バックアップファイルからの復元」の 2 段階。
    // バックアップファイルのサイズは事前に取得できないため、longOpTarget で ×2 のタイムアウトを適用。
    const response = await this.executeRemoteCommand(
      {
        operation: 'restore',
        params: { selector, dryRun },
      },
      timeoutMs,
    );
    const data = this.checkResponse(response);
    if (dryRun) {
      return data.diff;
    }
    return data.path;
  }

  /**
   * prod→stg コピー操作の共通基盤（sync/discard・設計 §4.2/§4.6）。`operation` に 'sync'/'discard'
   * を渡す。リモートホスト上で CLI の当該操作を起動し、NAS 上で prod→stg コピーを完結させる
   * （リモート↔ローカル間のファイル転送は発生しない）。stg 排他（接続中プロセスがないこと）は
   * 呼出側の責任（設計 §15-11 P1）。prod/stg パスは config から解決して `from`/`to` で明示渡し、
   * リモート側の環境変数に依存しない。
   */
  private async runProdToStg(
    operation: 'sync' | 'discard',
    timeoutMs?: number,
  ): Promise<{ prodPath: string; stgPath: string }> {
    const prodPath = this.config.dbPath ?? DEFAULT_REMOTE_PROD_DB;
    const stgPath =
      this.config.stgDbPath ??
      (this.config.dbPath !== undefined ? deriveStgDbPath(this.config.dbPath) : undefined) ??
      DEFAULT_REMOTE_STG_DB;
    // prod→stg の単一コピー（Online Backup 1パス）。適応タイムアウトは longOpTarget が prodPath から算出。
    const response = await this.executeRemoteCommand(
      { operation, params: { from: prodPath, to: stgPath } },
      timeoutMs,
    );
    const data = this.checkResponse(response);
    return { prodPath: data.prodPath, stgPath: data.stgPath };
  }

  /**
   * prod(RO) → stg(RW) のフル複製（sync・設計 §4.2）。書込セッション開始時の初期同期。
   */
  async sync(timeoutMs?: number): Promise<{ prodPath: string; stgPath: string }> {
    return this.runProdToStg('sync', timeoutMs);
  }

  /**
   * stg 破棄・再 sync（discard・設計 §4.6）。書込セッション中断・observe gate 不合格時に stg を
   * 捨てて prod から再構築する。処理は `sync` と同一（prod→stg の Online Backup コピー・既存 stg は
   * 上書き破棄・prod は一切触らない）。操作名のみ監査ログ（§10）で区別するため独立メソッド。
   */
  async discard(timeoutMs?: number): Promise<{ prodPath: string; stgPath: string }> {
    return this.runProdToStg('discard', timeoutMs);
  }
  async diffWithBackup(
    params: { selector?: RemoteBackupSelector; options?: { detail?: DiffDetail } } = {},
    timeoutMs?: number,
  ): Promise<BackupDiff> {
    const response = await this.executeRemoteCommand(
      {
        operation: 'diffBackup',
        params: { selector: params.selector, options: params.options },
      },
      timeoutMs,
    );
    return this.checkResponse(response);
  }
  /**
   * prod(RO) と現在DB(stg) の差分を取得（promote 判断用・設計 §4.4・TASK-53）。
   * リモート CLI は self=stg 起動を想定し、`prodDbPath` で prod を別途指定する
   * （build_remote_command は --db を1つしか渡せないため・設計 §4.2/§4.4）。
   * `prodDbPath` 省略時は CLI 側で prod target のデフォルトパスを解決する。
   */
  async diffWithProd(
    params: { prodDbPath?: string; options?: { detail?: DiffDetail } } = {},
    timeoutMs?: number,
  ): Promise<BackupDiff> {
    const response = await this.executeRemoteCommand(
      {
        operation: 'diffProdStg',
        params: { prodDbPath: params.prodDbPath, options: params.options },
      },
      timeoutMs,
    );
    return this.checkResponse(response);
  }

  /**
   * stg と prod を比較し機械的 promote gate を評価（promote 可否・設計 §3.4/§4.4・TASK-54）。
   * リモート CLI の `observe` operation を呼び、`ObserveResult` を受け取る。
   */
  async observe(
    params: { prodDbPath?: string; options?: ObserveOptions } = {},
    timeoutMs?: number,
  ): Promise<ObserveResult> {
    const response = await this.executeRemoteCommand(
      {
        operation: 'observe',
        params: { prodDbPath: params.prodDbPath, options: params.options },
      },
      timeoutMs,
    );
    return this.checkResponse(response);
  }

  /**
   * stg→prod へ反映（promote・設計 §4.5・TASK-58/62）。
   * リモート CLI の `promote` operation を呼び、`PromoteOutcome` を受け取る。
   * gate 不合格時はリモートから error 文字列が返り `checkResponse` が素の Error を throw する
   * （Local の `PromoteGateFailedError` とは型が異なる・observe と同じ既存制約）。
   *
   * `backupOpts` はそのままリモート CLI の `backupOpts` に受け渡し（pre-stash 先カスタマイズ・§7.2）。
   */
  async promote(
    params: { prodDbPath?: string; options?: ObserveOptions; backupOpts?: BackupOptions } = {},
    timeoutMs?: number,
  ): Promise<PromoteOutcome> {
    const response = await this.executeRemoteCommand(
      {
        operation: 'promote',
        params: { prodDbPath: params.prodDbPath, options: params.options, backupOpts: params.backupOpts },
      },
      timeoutMs,
    );
    return this.checkResponse(response);
  }

  /** バックアップにラベルを付与（事後） */
  async setBackupLabel(id: string, label: string | undefined): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'setBackupLabel',
      params: { id, label },
    });
    this.checkResponse(response);
  }

  /** バックアップにメモを付与（事後） */
  async setBackupNote(id: string, note: string | undefined): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'setBackupNote',
      params: { id, note },
    });
    this.checkResponse(response);
  }

  /** バックアップの事後メタを取得 */
  async getBackupMeta(id: string): Promise<BackupMetaEntry | null> {
    const response = await this.executeRemoteCommand({
      operation: 'getBackupMeta',
      params: { id },
    });
    return this.checkResponse(response);
  }
}
