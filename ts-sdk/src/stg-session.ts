/**
 * stg（ステージングDB）の排他ロックと sync 元 revision 記録（設計 §15-11・TASK-55）。
 *
 * - `acquireStgLock`: `<stg>.lock` の排他ロック（PID ベース・新規依存なし）。書込系
 *   （sync / stg 編集セッション）が取得し、複数セッション/LLM の stg 同時編集を防止。
 * - `StgMeta` / `ProdRevision`: sync 時に prod の指紋を `<stg>.meta.json` に原子書き込みし、
 *   observe が prod の drift（sync 後更新）を検出して stale な promote をブロック。
 */
import fs from 'node:fs';
import path from 'node:path';
import type Database from 'better-sqlite3';

/** prod の revision 指紋（schema_version + 各テーブル件数/max-id・設計 §15-11）。 */
export interface ProdRevision {
  schemaVersion: number;
  mediaCount: number;
  mediaMaxId: number;
  tagsCount: number;
  tagsMaxId: number;
  mediaTagsCount: number;
  mediaAttributesCount: number;
  mediaHashesCount: number;
}

/** sync 元 prod 情報（revision 記録・設計 §15-11）。 */
export interface SyncedFrom {
  prodPath: string;
  revision: ProdRevision;
  syncedAt: string;
}

/** stg メタデータ（`<stg>.meta.json`）。 */
export interface StgMeta {
  syncedFrom: SyncedFrom;
}

/** stg が別セッションで使用中（排他ロック取得失敗・設計 §15-11）。 */
export class StgBusyError extends Error {
  readonly stgPath: string;
  readonly holderPid?: number;
  constructor(stgPath: string, holderPid?: number) {
    super(`Stg is busy (locked by another session): ${stgPath}`);
    this.name = 'StgBusyError';
    this.stgPath = stgPath;
    this.holderPid = holderPid;
  }
}

/** prod が別セッション/promote で使用中（prod 排他ロック取得失敗・設計 §5.3・TASK-58）。
 *  Rust `KijukuError::ProdBusy` と parity（StgBusy と分離）。 */
export class ProdBusyError extends Error {
  readonly prodPath: string;
  readonly holderPid?: number;
  constructor(prodPath: string, holderPid?: number) {
    super(`Prod is busy (locked by another promote/admin session): ${prodPath}`);
    this.name = 'ProdBusyError';
    this.prodPath = prodPath;
    this.holderPid = holderPid;
  }
}

/** promote gate 不合格（prod に触れる前に拒否・設計 §4.5/§6.4・TASK-58）。
 *  Rust `KijukuError::PromoteGateFailed` と parity。`failedChecks` は `${name}: ${detail}` 形式。 */
export class PromoteGateFailedError extends Error {
  readonly failedChecks: string[];
  constructor(failedChecks: string[]) {
    super(`Promote gate failed (prod not touched): ${failedChecks.join('; ')}`);
    this.name = 'PromoteGateFailedError';
    this.failedChecks = failedChecks;
  }
}

/** `<stgDbPath>.meta.json` のパス。 */
export function metaPath(stgDbPath: string): string {
  return `${stgDbPath}.meta.json`;
}

/** 2つの revision 指紋が一致するか（全フィールド比較・Rust の PartialEq と同等）。 */
export function prodRevisionEqual(a: ProdRevision, b: ProdRevision): boolean {
  return (
    a.schemaVersion === b.schemaVersion &&
    a.mediaCount === b.mediaCount &&
    a.mediaMaxId === b.mediaMaxId &&
    a.tagsCount === b.tagsCount &&
    a.tagsMaxId === b.tagsMaxId &&
    a.mediaTagsCount === b.mediaTagsCount &&
    a.mediaAttributesCount === b.mediaAttributesCount &&
    a.mediaHashesCount === b.mediaHashesCount
  );
}

/** `<stgDbPath>.lock` のパス。 */
export function lockPath(stgDbPath: string): string {
  return `${stgDbPath}.lock`;
}

/** 接続から prod revision 指紋を算出（5テーブルの集計 + schema_version）。 */
export function computeProdRevision(db: Database.Database): ProdRevision {
  const countMax = (table: string, idCol: string): [number, number] => {
    const row = db
      .prepare(`SELECT COUNT(*) AS c, COALESCE(MAX(${idCol}), 0) AS m FROM ${table}`)
      .get() as { c: number; m: number };
    return [Number(row.c), Number(row.m)];
  };
  const countOnly = (table: string): number => {
    const row = db.prepare(`SELECT COUNT(*) AS c FROM ${table}`).get() as { c: number };
    return Number(row.c);
  };

  const [mediaCount, mediaMaxId] = countMax('media', 'id');
  const [tagsCount, tagsMaxId] = countMax('tags', 'id');
  const mediaTagsCount = countOnly('media_tags');
  const mediaAttributesCount = countOnly('media_attributes');
  const mediaHashesCount = countOnly('media_hashes');
  const svRow = db
    .prepare('SELECT version FROM schema_version ORDER BY version DESC LIMIT 1')
    .get() as { version: number } | undefined;
  const schemaVersion = svRow ? Number(svRow.version) : 0;

  return {
    schemaVersion,
    mediaCount,
    mediaMaxId,
    tagsCount,
    tagsMaxId,
    mediaTagsCount,
    mediaAttributesCount,
    mediaHashesCount,
  };
}

/** stg メタを読込（ファイル不在時は undefined・後方互換）。 */
export function readStgMeta(stgDbPath: string): StgMeta | undefined {
  try {
    const content = fs.readFileSync(metaPath(stgDbPath), 'utf8');
    return JSON.parse(content) as StgMeta;
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code === 'ENOENT') return undefined;
    throw e;
  }
}

/** stg メタを原子書き込み（tmp → rename・backup-meta パターンと同一）。 */
export function writeStgMeta(stgDbPath: string, meta: StgMeta): void {
  const p = metaPath(stgDbPath);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  const tmp = `${stgDbPath}.meta.json.tmp`;
  fs.writeFileSync(tmp, JSON.stringify(meta, null, 2));
  fs.renameSync(tmp, p);
}

/** ロックファイルの内容（holder PID・取得時刻）。 */
interface LockFileContent {
  pid: number;
  acquiredAt: string;
}

/** PID が生存しているか（process.kill(0) で存在確認）。 */
function isPidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** 排他ロックのハンドル。`release()` で解放。 */
export interface StgLock {
  readonly lockPath: string;
  release(): void;
}

/**
 * stg の排他ロックを取得（既に取得済みなら `StgBusyError`）。
 *
 * `<stg>.lock` を `O_EXCL`(wx) で作成し holder PID を書込む。既存時は holder PID を読み、
 * そのプロセスが既に終了していれば stale として破棄・再取得する。
 */
export function acquireStgLock(stgDbPath: string): StgLock {
  const lp = lockPath(stgDbPath);
  fs.mkdirSync(path.dirname(lp), { recursive: true });

  const createOrSteal = (): { fd: number } | { holder: number | undefined } => {
    try {
      const fd = fs.openSync(lp, 'wx');
      return { fd };
    } catch (e) {
      const code = (e as NodeJS.ErrnoException).code;
      if (code !== 'EEXIST') throw e;
    }
    // 既存ロック -> holder PID を読込
    let holderPid: number | undefined;
    try {
      const content = fs.readFileSync(lp, 'utf8').trim();
      const parsed = JSON.parse(content) as Partial<LockFileContent>;
      holderPid = typeof parsed.pid === 'number' ? parsed.pid : undefined;
    } catch {
      holderPid = undefined;
    }
    // stale（holder プロセス不在）なら破棄して再取得。PID 不明・生存中は StgBusy。
    if (holderPid !== undefined && !isPidAlive(holderPid)) {
      try {
        fs.unlinkSync(lp);
      } catch {
        /* ignore */
      }
      return createOrSteal();
    }
    return { holder: holderPid };
  };

  const result = createOrSteal();
  if (!('fd' in result)) {
    throw new StgBusyError(stgDbPath, result.holder);
  }
  const fd = result.fd;
  const content: LockFileContent = { pid: process.pid, acquiredAt: new Date().toISOString() };
  fs.writeFileSync(fd, JSON.stringify(content));

  let released = false;
  return {
    lockPath: lp,
    release(): void {
      if (released) return;
      released = true;
      try {
        fs.closeSync(fd);
      } catch {
        /* ignore */
      }
      try {
        fs.unlinkSync(lp);
      } catch {
        /* ignore */
      }
    },
  };
}

/**
 * prod の排他ロックを取得（promote/(b) 管理操作用・設計 §5.3・TASK-58）。
 *
 * 機構は `acquireStgLock` と同一（`${prod}.lock` の `O_EXCL` + holder PID stale 検出）だが、
 * 別セッション保持中は `StgBusyError` でなく `ProdBusyError` を throw する（Rust `ProdRwScope` と parity）。
 */
export function acquireProdLock(prodDbPath: string): StgLock {
  try {
    return acquireStgLock(prodDbPath);
  } catch (e) {
    if (e instanceof StgBusyError) {
      throw new ProdBusyError(prodDbPath, e.holderPid);
    }
    throw e;
  }
}
