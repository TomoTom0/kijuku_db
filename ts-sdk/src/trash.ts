/**
 * trash（論理削除）方式: ファイル操作で削除・上書きされるファイルを
 * `media_root/.trash/` 配下へ退避し、物理削除を防ぐ。
 *
 * 物理削除を伴うのは {@link purgeTrash} の apply（かつ dry-run ファースト）のみ。
 * 通常の削除・上書きはすべて trash への移動となり、{@link restoreFromTrash} で復元できる。
 */
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { NotFoundError, ValidationError } from './errors.js';
import { isProtected, resolveExistingWithinRoot, trashDir } from './media_path.js';

export type TrashId = string;
export type TrashOperation = 'delete' | 'overwrite' | 'sync_extra';

/** trash エントリのメタ情報（`.trash/<id>/.meta.json` に保存） */
export interface TrashMeta {
  /** 元のパス（media root からの相対） */
  originalPath: string;
  /** trash へ移動した日時（RFC3339） */
  trashedAt: string;
  /** どの操作で trash 行きになったか（delete / overwrite / sync_extra） */
  operation: TrashOperation;
  /** 操作の呼び出し元（media_mv / media_cp 等、任意） */
  reason?: string;
}

/** 一覧取得で返される trash エントリ */
export interface TrashEntry {
  id: TrashId;
  meta: TrashMeta;
}

let trashCounter = 0;

/** 一意な trash ID を生成する（ms タイムスタンプ + モジュール内カウンタ）。 */
function generateTrashId(): string {
  const ms = Date.now();
  const c = trashCounter++;
  return `${ms.toString().padStart(20, '0')}-${c}`;
}

function nowIso(): string {
  return new Date().toISOString();
}

function canonicalizeRoot(root: string): string {
  try {
    return realpathSync(root);
  } catch {
    return root;
  }
}

/**
 * trash エントリ ID が生成された単一エントリ名（`.trash/<id>` の direct child）として安全か検査する。
 *
 * `id` は単一の名前（`..`・`.`・パス区切り・絶対パスを含まない）でなければならず、
 * かつ `path.join(trash, id)` の親が `trash` 自身と一致することを確認する。
 * `purgeTrash` の呼出し側提供 ID 検証に使用し、trash 外への脱出削除を防ぐ。
 */
function ensureSafeTrashId(id: string, trash: string): void {
  const isSingleName =
    id.length > 0 &&
    !id.includes('/') &&
    !id.includes('\\') &&
    id !== '.' &&
    id !== '..';
  const directlyBeneath = path.posix.dirname(path.posix.join(trash, id)) === trash;
  if (!isSingleName || !directlyBeneath) {
    throw new ValidationError(
      `invalid trash id (must be a single entry name beneath .trash): ${id}`
    );
  }
}

/** JSON を一時ファイル経由でアトミックに書き込む（backup.ts のパターン踏襲）。 */
function atomicWriteJson(filePath: string, value: TrashMeta): void {
  const tmp = `${filePath}.tmp`;
  writeFileSync(tmp, JSON.stringify(value, null, 2));
  renameSync(tmp, filePath);
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null;
}

/** `.meta.json` を型安全にパースする（as キャスト不使用）。 */
function parseTrashMeta(data: string): TrashMeta {
  const v: unknown = JSON.parse(data);
  if (!isObject(v)) {
    throw new ValidationError('invalid trash meta format');
  }
  if (typeof v.originalPath !== 'string' || typeof v.trashedAt !== 'string') {
    throw new ValidationError('invalid trash meta format');
  }
  const operation = v.operation;
  if (
    operation !== 'delete' &&
    operation !== 'overwrite' &&
    operation !== 'sync_extra'
  ) {
    throw new ValidationError(`invalid trash meta operation: ${String(operation)}`);
  }
  return {
    originalPath: v.originalPath,
    trashedAt: v.trashedAt,
    operation,
    reason: typeof v.reason === 'string' ? v.reason : undefined,
  };
}

/**
 * `targetRel` を trash へ移動し、trash ID を返す。
 *
 * target は media root 配下の既存パスで、かつ trash ディレクトリ自身でなければならない。
 * 存在しないパスはエラー。.trash/ 配下のパスは（再帰退避を避けるため）拒否する。
 */
export function moveToTrash(
  root: string,
  targetRel: string,
  operation: TrashOperation,
  reason?: string
): TrashId {
  const rootC = canonicalizeRoot(root);
  const trash = trashDir(rootC);
  const target = resolveExistingWithinRoot(root, targetRel);

  // trash ディレクトリ配下は再帰退避を避けて拒否
  if (isProtected(target, [trash])) {
    throw new ValidationError(
      `cannot trash a path inside the trash directory: ${target}`
    );
  }
  if (!existsSync(target)) {
    throw new NotFoundError(`trash target not found: ${target}`);
  }

  const rel = path.posix.relative(rootC, target);
  if (!rel || rel.startsWith('..')) {
    throw new ValidationError(
      `target is not under media root: ${target} (root: ${rootC})`
    );
  }

  const id = generateTrashId();
  const entryDir = path.posix.join(trash, id);
  mkdirSync(entryDir, { recursive: true });

  const dest = path.posix.join(entryDir, rel);
  mkdirSync(path.posix.dirname(dest), { recursive: true });
  renameSync(target, dest);

  atomicWriteJson(path.posix.join(entryDir, '.meta.json'), {
    originalPath: rel,
    trashedAt: nowIso(),
    operation,
    reason,
  });

  return id;
}

/** trash 内の全エントリを一覧する（trash が無ければ空）。 */
export function listTrash(root: string): TrashEntry[] {
  const rootC = canonicalizeRoot(root);
  const trash = trashDir(rootC);
  if (!existsSync(trash)) return [];

  const entries: TrashEntry[] = [];
  for (const name of readdirSync(trash)) {
    const metaPath = path.posix.join(trash, name, '.meta.json');
    if (!existsSync(metaPath)) continue;
    const meta = parseTrashMeta(readFileSync(metaPath, 'utf-8'));
    entries.push({ id: name, meta });
  }
  return entries;
}

/**
 * trash エントリ `id` を元の位置へ復元し、復元先のパスを返す。
 *
 * 元の位置に既にファイルが存在する場合は**上書きせずエラー**にする（安全停止）。
 */
export function restoreFromTrash(root: string, id: string): string {
  const rootC = canonicalizeRoot(root);
  const trash = trashDir(rootC);
  const entryDir = path.posix.join(trash, id);
  const metaPath = path.posix.join(entryDir, '.meta.json');

  if (!existsSync(metaPath)) {
    throw new NotFoundError(`trash entry not found: ${id}`);
  }
  const meta = parseTrashMeta(readFileSync(metaPath, 'utf-8'));

  const dest = path.posix.join(rootC, meta.originalPath);
  if (existsSync(dest)) {
    throw new ValidationError(
      `restore destination already exists (not overwritten): ${dest}`
    );
  }

  const src = path.posix.join(entryDir, meta.originalPath);
  mkdirSync(path.posix.dirname(dest), { recursive: true });
  renameSync(src, dest);

  rmSync(entryDir, { recursive: true, force: true });
  return dest;
}

/**
 * trash を物理削除する。dryRun の場合は対象 ID の一覧を返すだけで削除しない。
 *
 * `ids` を指定すればその ID のみ、未指定なら全エントリを対象とする。
 * これが trash からファイルを完全に消す唯一の経路である。
 */
export function purgeTrash(root: string, ids?: string[], dryRun = false): string[] {
  const rootC = canonicalizeRoot(root);
  const trash = trashDir(rootC);

  const toPurge: string[] = ids
    ? (() => {
        // 呼出し側提供 ID を検証: 生成エントリ名（単一コンポーネント）のみ許可。
        // `..`・絶対パス・区切り込みの ID で trash 外へ脱出して rmSync する攻撃を防ぐ。
        for (const id of ids) {
          ensureSafeTrashId(id, trash);
        }
        return [...ids];
      })()
    : existsSync(trash)
      ? readdirSync(trash)
      : [];

  if (dryRun) return toPurge;

  for (const id of toPurge) {
    const p = path.posix.join(trash, id);
    if (existsSync(p)) {
      rmSync(p, { recursive: true, force: true });
    }
  }
  return toPurge;
}
