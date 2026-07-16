/**
 * media root 配下のファイルパスを安全に解決するヘルパ。
 *
 * すべてのファイル操作（cp/mv/sync/upload/download 等）は、このモジュール経由で
 * パスを media root 配下に拘束する。これにより、`..`・絶対パス・シンボリックリンク
 * 経由での media root 外への脱出（パストラバーサル）を防ぐ。
 */
import { existsSync, realpathSync } from 'node:fs';
import path from 'node:path';
import { ValidationError } from './errors.js';

/**
 * パスを lexical に正規化する（`.` と `..` を解決）。
 *
 * シンボリックリンクは解決しない（ファイルシステムアクセス不要）。そのため
 * 存在しないパス（新規作成先など）にも使用できる。ルート（`/`）より上には登らない。
 */
export function normalizeLexical(input: string): string {
  const isAbsolute = input.startsWith('/');
  const segments = input.split('/');
  const result: string[] = [];
  for (const seg of segments) {
    if (seg === '' || seg === '.') continue;
    if (seg === '..') {
      result.pop();
    } else {
      result.push(seg);
    }
  }
  return (isAbsolute ? '/' : '') + result.join('/');
}

/** `target` が `root` 配下（または root 自身）か判定する。どちらも正規化済みのこと。 */
function isWithin(target: string, root: string): boolean {
  if (target === root) return true;
  const rootWithSlash = root.endsWith('/') ? root : root + '/';
  return target.startsWith(rootWithSlash);
}

/**
 * `rel` を `root` 配下に解決し、脱出を検査する。
 *
 * `rel` が相対パスなら `root` に結合する。絶対パスの場合はそのまま検査に回す
 * （結果が `root` 配下でなければ拒否）。lexical 正規化のみ行うため、新規パス
 * （存在しない作成先）にも使用できる。シンボリックリンク経由の脱出を防ぐには
 * {@link resolveExistingWithinRoot} を使うこと。
 */
export function resolveWithinRoot(root: string, rel: string): string {
  const joined = rel.startsWith('/') ? rel : path.posix.join(root, rel);
  const normalized = normalizeLexical(joined);
  const rootNormalized = normalizeLexical(root);
  if (!isWithin(normalized, rootNormalized)) {
    throw new ValidationError(
      `path escapes media root: ${normalized} (root: ${rootNormalized})`
    );
  }
  return normalized;
}

/**
 * resolveWithinRoot に加え、既存パスのシンボリックリンクを実体まで解決して再検査する。
 *
 * 操作対象が存在する場合（cp/mv の src など）に使う。存在しないパス（新規作成先など）は
 * lexical 解決結果をそのまま返す。
 */
export function resolveExistingWithinRoot(root: string, rel: string): string {
  const lexical = resolveWithinRoot(root, rel);
  if (existsSync(lexical)) {
    const canonical = realpathSync(lexical);
    let rootCanonical: string;
    try {
      rootCanonical = realpathSync(root);
    } catch {
      rootCanonical = normalizeLexical(root);
    }
    if (!isWithin(canonical, rootCanonical)) {
      throw new ValidationError(
        `path escapes media root via symlink: ${canonical} (root: ${rootCanonical})`
      );
    }
    return canonical;
  }
  return lexical;
}

/**
 * `target` が保護パス（操作禁止領域）に含まれるか判定する。
 *
 * `protectedPaths` には正規化済みのパス（`.trash/`、DB ファイル、backupDir など）を渡す。
 * `target` がいずれかの保護パスと同じ、またはその配下にある場合に true。
 */
export function isProtected(target: string, protectedPaths: string[]): boolean {
  const targetNorm = normalizeLexical(target);
  return protectedPaths.some(
    (p) => isWithin(targetNorm, p) || isWithin(p, targetNorm)
  );
}

/**
 * media root 配下の trash（論理削除先）ディレクトリパスを返す。
 */
export function trashDir(root: string): string {
  return path.posix.join(root, '.trash');
}

/**
 * 設定から media root を取り出し、未設定ならエラーにする。
 */
export function requireMediaRoot(optRoot: string | undefined | null): string {
  if (!optRoot) {
    throw new ValidationError(
      'media root is not configured; set DBOptions.mediaRoot to use file operations'
    );
  }
  return normalizeLexical(optRoot);
}
