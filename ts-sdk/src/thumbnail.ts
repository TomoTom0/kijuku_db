/**
 * サムネイル操作の実装
 */
import type Database from 'better-sqlite3';
import * as fs from 'fs';
import * as path from 'path';
import { spawnSync } from 'child_process';
import { findMedia } from './search.js';
import { updateMedia } from './crud.js';
import type {
  MediaFilter,
  QueryOptions,
  ThumbnailOptions,
  CheckThumbnailResult,
  CheckThumbnailItemResult,
  CheckThumbnailStatus,
  UpdateThumbnailResult,
  UpdateThumbnailItemResult,
  UpdateThumbnailStatus,
} from './types.js';
import type { Media } from './types.js';

/**
 * pathからサムネイルの期待パスを計算する
 *
 * pathに含まれる最後の"content"コンポーネントを見つけ、
 * その親ディレクトリの cover/{uuid}.jpg を返す。
 * contentが含まれない場合はundefinedを返す。
 */
export function resolveThumbnailPath(pathStr: string, uuid: string): string | undefined {
  const parts = pathStr.split(path.sep === '\\' ? /[\\/]/ : '/');
  const lastContentIdx = parts.lastIndexOf('content');
  if (lastContentIdx === -1) return undefined;

  const parentParts = parts.slice(0, lastContentIdx);
  const hasNormal = parentParts.some(p => p !== '' && p !== '.' && p !== '..');
  if (!hasNormal) return undefined;

  return path.join(parentParts.join('/'), 'cover', `${uuid}.jpg`);
}

function checkMediaThumbnail(media: Media): CheckThumbnailItemResult {
  if (!media.path) {
    return {
      id: media.id,
      uuid: media.uuid,
      title: media.title,
      expected_path: undefined,
      current_path: media.thumbnail_path,
      status: { type: 'skipped', reason: 'pathが未設定' },
    };
  }

  const expectedPath = resolveThumbnailPath(media.path, media.uuid);
  if (!expectedPath) {
    return {
      id: media.id,
      uuid: media.uuid,
      title: media.title,
      expected_path: undefined,
      current_path: media.thumbnail_path,
      status: { type: 'skipped', reason: 'pathにcontentが含まれていない' },
    };
  }

  let status: CheckThumbnailStatus;
  if (media.thumbnail_path === expectedPath && fs.existsSync(expectedPath)) {
    status = { type: 'ok' };
  } else if (!media.thumbnail_path || media.thumbnail_path !== expectedPath) {
    status = { type: 'missing' };
  } else {
    status = { type: 'fileNotFound' };
  }

  return {
    id: media.id,
    uuid: media.uuid,
    title: media.title,
    expected_path: expectedPath,
    current_path: media.thumbnail_path,
    status,
  };
}

export function checkThumbnail(
  db: Database.Database,
  filter: MediaFilter,
  options?: QueryOptions,
): CheckThumbnailResult {
  const mediaList = findMedia(db, filter, options);
  let ok = 0, missing = 0, file_not_found = 0, skipped = 0;
  const details: CheckThumbnailItemResult[] = [];

  for (const media of mediaList) {
    const item = checkMediaThumbnail(media);
    switch (item.status.type) {
      case 'ok': ok++; break;
      case 'missing': missing++; break;
      case 'fileNotFound': file_not_found++; break;
      case 'skipped': skipped++; break;
    }
    details.push(item);
  }

  return { total: mediaList.length, ok, missing, file_not_found, skipped, details };
}

/**
 * video/musicの実際のファイルパスを解決する
 *
 * pathにextが含まれていればそのまま、含まれていなければ
 * {path}.{ext} または {uuid}.* 形式で代替を探す。
 */
function resolveMediaFilePath(pathStr: string, ext: string, uuid: string): string | undefined {
  if (fs.existsSync(pathStr)) return pathStr;
  const withExt = `${pathStr}.${ext}`;
  if (fs.existsSync(withExt)) return withExt;
  const parent = path.dirname(pathStr);
  try {
    for (const entry of fs.readdirSync(parent)) {
      const dotPos = entry.lastIndexOf('.');
      if (dotPos > 0) {
        const stem = entry.substring(0, dotPos);
        const fileExt = entry.substring(dotPos + 1);
        if (stem === uuid && fileExt.length > 0) {
          return path.join(parent, entry);
        }
      }
    }
  } catch { /* ignore */ }
  return undefined;
}

function updateMediaThumbnail(
  db: Database.Database,
  media: Media,
  options: ThumbnailOptions,
): UpdateThumbnailItemResult {
  const build = (status: UpdateThumbnailStatus, thumbnailPath?: string): UpdateThumbnailItemResult => ({
    id: media.id,
    uuid: media.uuid,
    title: media.title,
    thumbnail_path: thumbnailPath,
    status,
  });

  if (!media.path) {
    return build({ type: 'skipped', reason: 'pathが未設定' });
  }

  const expectedPath = resolveThumbnailPath(media.path, media.uuid);
  if (!expectedPath) {
    return build({ type: 'skipped', reason: 'pathにcontentが含まれていない' });
  }

  // media_typeに応じたソースファイルのチェック
  switch (media.media_type) {
    case 'comic': {
      const ext = media.extension ?? 'jpg';
      const firstPage = path.join(media.path, `001.${ext}`);
      if (!fs.existsSync(firstPage)) {
        return build({ type: 'skipped', reason: `001.${ext} が存在しない` });
      }
      break;
    }
    case 'video': {
      const ext = media.extension ?? 'mp4';
      if (!resolveMediaFilePath(media.path, ext, media.uuid)) {
        return build({ type: 'skipped', reason: '動画ファイルが存在しない' });
      }
      break;
    }
    case 'music':
      return build({ type: 'skipped', reason: 'musicはサムネイル対象外' });
  }

  if (!options.force) {
    if (media.thumbnail_path === expectedPath && fs.existsSync(expectedPath)) {
      return build({ type: 'alreadyExists' }, expectedPath);
    }
  }

  if (options.dry_run) {
    return build({ type: 'generated' }, expectedPath);
  }

  const coverDir = path.dirname(expectedPath);
  try {
    fs.mkdirSync(coverDir, { recursive: true });
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return build({ type: 'error', message: `cover/ディレクトリの作成に失敗: ${message}` });
  }

  // media_typeに応じたサムネイル生成
  let cmdResult: ReturnType<typeof spawnSync>;
  switch (media.media_type) {
    case 'comic': {
      const ext = media.extension ?? 'jpg';
      const firstPage = path.join(media.path, `001.${ext}`);
      cmdResult = spawnSync('convert', [firstPage, '-resize', 'x180', '-quality', '85', expectedPath]);
      break;
    }
    case 'video': {
      const ext = media.extension ?? 'mp4';
      const resolved = resolveMediaFilePath(media.path, ext, media.uuid)!;
      cmdResult = spawnSync('ffmpeg', [
        '-ss', '00:00:01', '-i', resolved,
        '-vframes', '1', '-q:v', '2', '-y', expectedPath,
      ]);
      break;
    }
    case 'music':
      return build({ type: 'skipped', reason: 'musicはサムネイル対象外' });
  }

  const cmdName = media.media_type === 'video' ? 'ffmpeg' : 'convert';
  if (cmdResult!.error) {
    return build({ type: 'error', message: `${cmdName}の実行に失敗: ${cmdResult!.error.message}` });
  }
  if (cmdResult!.status !== 0) {
    const stderr = cmdResult!.stderr?.toString().trim() ?? '';
    return build({ type: 'error', message: `${cmdName}失敗: ${stderr}` });
  }

  try {
    updateMedia(db, media.id, { thumbnail_path: expectedPath });
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return build({ type: 'error', message: `DB更新に失敗: ${message}` }, expectedPath);
  }

  return build({ type: 'generated' }, expectedPath);
}

export function updateThumbnail(
  db: Database.Database,
  filter: MediaFilter,
  options?: QueryOptions,
  thumbnailOptions: ThumbnailOptions = {},
): UpdateThumbnailResult {
  const mediaList = findMedia(db, filter, options);
  let generated = 0, already_exists = 0, skipped = 0, errors = 0;
  const details: UpdateThumbnailItemResult[] = [];

  for (const media of mediaList) {
    const item = updateMediaThumbnail(db, media, thumbnailOptions);
    switch (item.status.type) {
      case 'generated': generated++; break;
      case 'alreadyExists': already_exists++; break;
      case 'skipped': skipped++; break;
      case 'error': errors++; break;
    }
    details.push(item);
  }

  return { total: mediaList.length, generated, already_exists, skipped, errors, details };
}
