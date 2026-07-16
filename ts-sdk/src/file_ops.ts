/**
 * media root 配下のファイル操作（cp/mv/sync）。
 *
 * すべて dry-run ファースト。上書き・削除で消えるファイルは trash（論理削除）経由。
 * `media_root` 配下への拘束と保護パスの拒否は `./media_path.js` が担う。
 */
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  statSync,
} from 'node:fs';
import path from 'node:path';
import { ValidationError } from './errors.js';
import {
  isProtected,
  resolveExistingWithinRoot,
  resolveWithinRoot,
  trashDir,
} from './media_path.js';
import { moveToTrash } from './trash.js';

/** ファイル操作のオプション */
export interface FileOpOptions {
  /** 実際に変更を適用するか。`false`（既定）なら dry-run で計画のみ返す */
  apply: boolean;
  /**
   * DB の Media.path を追従させるか。既定 false（ゆるい方針）。
   * ※ 当面はフラグを受け取るのみで DB 更新は未サポート（後続タスクで拡張）。
   */
  updateDb: boolean;
}

export const defaultFileOpOptions: FileOpOptions = { apply: false, updateDb: false };

/** 個々の操作ステップ（dry-run の計画表示にも使う） */
export type FileOpStep =
  | { kind: 'copy'; from: string; to: string }
  | { kind: 'move'; from: string; to: string }
  | { kind: 'trash'; path: string; reason: string };

/** 操作の実行結果（dry-run 含む） */
export interface FileOpResult {
  steps: FileOpStep[];
  /** 実際にファイルシステムへ変更を加えたか */
  applied: boolean;
}

function canonicalizeRoot(root: string): string {
  try {
    return realpathSync(root);
  } catch {
    return root;
  }
}

/** root からの相対文字列を返す */
function relString(target: string, root: string): string {
  if (!target.startsWith(root)) {
    throw new ValidationError(`target is not under media root: ${target} (root: ${root})`);
  }
  return path.posix.relative(root, target);
}

/** 1件コピー（ファイル/ディレクトリ両対応） */
function copyRecursive(src: string, dst: string): void {
  if (statSync(src).isDirectory()) {
    mkdirSync(dst, { recursive: true });
    for (const name of readdirSync(src)) {
      copyRecursive(path.posix.join(src, name), path.posix.join(dst, name));
    }
  } else {
    mkdirSync(path.posix.dirname(dst), { recursive: true });
    copyFileSync(src, dst);
  }
}

/** src と dst を解決し、保護パスでないことを検証する */
function resolvePair(
  root: string,
  srcRel: string,
  dstRel: string
): { src: string; dst: string } {
  const src = resolveExistingWithinRoot(root, srcRel);
  const dst = resolveWithinRoot(root, dstRel);
  const trash = trashDir(canonicalizeRoot(root));
  if (isProtected(src, [trash]) || isProtected(dst, [trash])) {
    throw new ValidationError(
      `source or destination is a protected path (src: ${src}, dst: ${dst})`
    );
  }
  return { src, dst };
}

/** src 配下の相対ファイルパス一覧を収集する */
function collectFiles(base: string): string[] {
  const out: string[] = [];
  const walk = (cur: string): void => {
    for (const name of readdirSync(cur)) {
      const p = path.posix.join(cur, name);
      if (statSync(p).isDirectory()) {
        walk(p);
      } else {
        out.push(path.posix.relative(base, p));
      }
    }
  };
  walk(base);
  return out;
}

function buffersEqual(a: Buffer, b: Buffer): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

/** `srcRel` を `dstRel` へ複製する（dry-run ファースト、上書きは trash 経由）。 */
export function mediaCp(
  root: string,
  srcRel: string,
  dstRel: string,
  opts: FileOpOptions = defaultFileOpOptions
): FileOpResult {
  const rootC = canonicalizeRoot(root);
  const { src, dst } = resolvePair(root, srcRel, dstRel);

  const steps: FileOpStep[] = [];
  if (existsSync(dst)) {
    steps.push({ kind: 'trash', path: relString(dst, rootC), reason: 'media_cp overwrite' });
  }
  steps.push({ kind: 'copy', from: relString(src, rootC), to: relString(dst, rootC) });

  if (!opts.apply) {
    return { steps, applied: false };
  }

  if (existsSync(dst)) {
    moveToTrash(root, dstRel, 'overwrite', 'media_cp');
  }
  copyRecursive(src, dst);
  return { steps, applied: true };
}

/** `srcRel` を `dstRel` へ移動する（dry-run ファースト、上書きは trash 経由）。 */
export function mediaMv(
  root: string,
  srcRel: string,
  dstRel: string,
  opts: FileOpOptions = defaultFileOpOptions
): FileOpResult {
  const rootC = canonicalizeRoot(root);
  const { src, dst } = resolvePair(root, srcRel, dstRel);

  const steps: FileOpStep[] = [];
  if (existsSync(dst)) {
    steps.push({ kind: 'trash', path: relString(dst, rootC), reason: 'media_mv overwrite' });
  }
  steps.push({ kind: 'move', from: relString(src, rootC), to: relString(dst, rootC) });

  if (!opts.apply) {
    return { steps, applied: false };
  }

  if (existsSync(dst)) {
    moveToTrash(root, dstRel, 'overwrite', 'media_mv');
  }
  mkdirSync(path.posix.dirname(dst), { recursive: true });
  renameSync(src, dst);
  return { steps, applied: true };
}

/**
 * `srcRel`（ディレクトリ）の内容を `dstRel` へ同期する（safe モード）。
 *
 * dst にあって src に無いファイルは trash へ回す（生 --delete 相当だが trash 経由）。
 * 内容が異なるファイルは旧内容を trash へ退避してからコピーする。
 */
export function mediaSync(
  root: string,
  srcRel: string,
  dstRel: string,
  opts: FileOpOptions = defaultFileOpOptions
): FileOpResult {
  const rootC = canonicalizeRoot(root);
  const { src, dst } = resolvePair(root, srcRel, dstRel);

  if (!statSync(src).isDirectory()) {
    throw new ValidationError(`media_sync source must be a directory: ${src}`);
  }

  const srcFiles = collectFiles(src);
  const dstStat = existsSync(dst) ? statSync(dst) : undefined;
  const dstFiles = dstStat?.isDirectory() ? collectFiles(dst) : [];
  const srcSet = new Set(srcFiles);

  const steps: FileOpStep[] = [];
  // 1. src に無い dst のファイルは trash
  for (const rel of dstFiles) {
    if (!srcSet.has(rel)) {
      steps.push({
        kind: 'trash',
        path: relString(path.posix.join(dst, rel), rootC),
        reason: 'media_sync extra',
      });
    }
  }
  // 2. src の全ファイルを dst へコピー（差分: 内容が異なる場合のみ）
  for (const rel of srcFiles) {
    const to = path.posix.join(dst, rel);
    const fromBytes = readFileSync(path.posix.join(src, rel));
    let differs = true;
    if (existsSync(to)) {
      try {
        differs = !buffersEqual(fromBytes, readFileSync(to));
      } catch {
        differs = true;
      }
    }
    if (differs) {
      if (existsSync(to)) {
        steps.push({ kind: 'trash', path: relString(to, rootC), reason: 'media_sync update' });
      }
      steps.push({
        kind: 'copy',
        from: relString(path.posix.join(src, rel), rootC),
        to: relString(to, rootC),
      });
    }
  }

  if (!opts.apply) {
    return { steps, applied: false };
  }

  // 実行
  mkdirSync(dst, { recursive: true });
  for (const rel of dstFiles) {
    if (!srcSet.has(rel)) {
      moveToTrash(root, `${dstRel.replace(/\/$/, '')}/${rel}`, 'sync_extra', 'media_sync');
    }
  }
  for (const rel of srcFiles) {
    const from = path.posix.join(src, rel);
    const to = path.posix.join(dst, rel);
    let differs = true;
    if (existsSync(to)) {
      try {
        differs = !buffersEqual(readFileSync(from), readFileSync(to));
      } catch {
        differs = true;
      }
    }
    if (!differs) continue;
    if (existsSync(to)) {
      moveToTrash(root, `${dstRel.replace(/\/$/, '')}/${rel}`, 'overwrite', 'media_sync');
    }
    copyRecursive(from, to);
  }

  return { steps, applied: true };
}
