/**
 * flag_existチェック・更新機能
 */
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import type Database from 'better-sqlite3';
import type { Media, MediaFilter, QueryOptions } from './types.js';
import { findMedia } from './search.js';
import { updateMedia } from './crud.js';

const UPDATED_IDS_INLINE_LIMIT = 1000;

function tempFilePath(suffix: string): string {
  const ts = Date.now();
  const pid = process.pid;
  const rand = Math.floor(Math.random() * 1e9);
  return path.join(os.tmpdir(), `kijuku-update-exist-${pid}-${ts}-${rand}${suffix}`);
}

export interface UpdateExistOptions {
  dry_run: boolean;
}

export interface UpdateExistItemResult {
  id: number;
  uuid: string;
  title: string;
  flag_exist_before: boolean;
  flag_exist_after: boolean;
  extension_used: string | null;
  found_extension: string | null;
  page_count_warning: string | null;
  page_count_set: number | null;
}

export interface UpdateExistResult {
  total: number;
  updated: number;
  /** 変更があったメディアのID一覧（1000件以下の場合のみインライン） */
  updated_ids: number[] | null;
  /** updated_idsが1000件超の場合のファイルパス */
  updated_ids_file: string | null;
  /** 全件の詳細結果を含むJSONファイルのパス */
  detail_file: string;
}

function defaultExtension(mediaType: string): string {
  switch (mediaType) {
    case 'comic': return 'jpg';
    case 'video': return 'mp4';
    case 'music': return 'm4a';
    default: return 'jpg';
  }
}

function countPagesInDir(dir: string, ext: string): number {
  try {
    const entries = fs.readdirSync(dir);
    let count = 0;
    const suffix = `.${ext}`;
    for (const fname of entries) {
      if (fname.endsWith(suffix)) {
        const stem = fname.slice(0, fname.length - suffix.length);
        if (/^\d+$/.test(stem)) {
          count++;
        }
      }
    }
    return count;
  } catch {
    return 0;
  }
}

function findDominantExtensionInDir(dir: string): string | null {
  try {
    const entries = fs.readdirSync(dir);
    const extCounts: Map<string, number> = new Map();
    for (const fname of entries) {
      const dotPos = fname.lastIndexOf('.');
      if (dotPos < 0) continue;
      const stem = fname.slice(0, dotPos);
      const ext = fname.slice(dotPos + 1);
      if (/^\d+$/.test(stem) && ext.length > 0) {
        extCounts.set(ext, (extCounts.get(ext) ?? 0) + 1);
      }
    }
    if (extCounts.size === 0) return null;
    let maxExt: string | null = null;
    let maxCount = 0;
    for (const [ext, count] of extCounts) {
      if (count > maxCount) {
        maxCount = count;
        maxExt = ext;
      }
    }
    return maxExt;
  } catch {
    return null;
  }
}

function findAlternativeExtensionForFile(filePath: string, uuid: string): string | null {
  try {
    const parent = path.dirname(filePath);
    const entries = fs.readdirSync(parent);
    for (const fname of entries) {
      const dotPos = fname.lastIndexOf('.');
      if (dotPos < 0) continue;
      const stem = fname.slice(0, dotPos);
      const ext = fname.slice(dotPos + 1);
      if (stem === uuid && ext.length > 0) {
        return ext;
      }
    }
    return null;
  } catch {
    return null;
  }
}

function processMedia(
  db: Database.Database,
  media: Media,
  options: UpdateExistOptions
): UpdateExistItemResult {
  const ext = media.extension ?? defaultExtension(media.media_type);
  const flagExistBefore = media.flag_exist;
  let flagExistAfter = false;
  let foundExtension: string | null = null;
  let pageCountWarning: string | null = null;
  let pageCountSet: number | null = null;
  let extensionUsed: string | null = ext;

  if (media.path == null) {
    flagExistAfter = false;
  } else {
    const mediaPath = media.path;
    if (media.media_type === 'comic') {
      const firstPage = path.join(mediaPath, `001.${ext}`);
      if (fs.existsSync(firstPage)) {
        flagExistAfter = true;
        const actualCount = countPagesInDir(mediaPath, ext);
        if (media.page_count == null) {
          if (actualCount > 0) {
            pageCountSet = actualCount;
          }
        } else if (media.page_count !== actualCount) {
          pageCountWarning = `page_count不一致: DB=${media.page_count}, 実際=${actualCount}`;
        }
      } else {
        const altExt = findDominantExtensionInDir(mediaPath);
        if (altExt != null) {
          foundExtension = altExt;
          extensionUsed = altExt;
        }
        flagExistAfter = false;
      }
    } else {
      if (fs.existsSync(mediaPath)) {
        flagExistAfter = true;
      } else {
        const altExt = findAlternativeExtensionForFile(mediaPath, media.uuid);
        if (altExt != null) {
          foundExtension = altExt;
          extensionUsed = altExt;
        }
        flagExistAfter = false;
      }
    }
  }

  if (!options.dry_run) {
    const updateData: Record<string, unknown> = { flag_exist: flagExistAfter };
    if (foundExtension != null) {
      updateData['extension'] = foundExtension;
      updateData['flag_exist'] = true;
      flagExistAfter = true; // 結果オブジェクトのために更新
    }
    if (pageCountSet != null) {
      updateData['page_count'] = pageCountSet;
    }
    updateMedia(db, media.id, updateData as any);
  }

  return {
    id: media.id,
    uuid: media.uuid,
    title: media.title,
    flag_exist_before: flagExistBefore,
    flag_exist_after: flagExistAfter,
    extension_used: extensionUsed,
    found_extension: foundExtension,
    page_count_warning: pageCountWarning,
    page_count_set: pageCountSet,
  };
}

export function updateExist(
  db: Database.Database,
  filter: MediaFilter,
  queryOptions: QueryOptions | undefined,
  options: UpdateExistOptions
): UpdateExistResult {
  const mediaList = findMedia(db, filter, queryOptions);
  const total = mediaList.length;
  let updated = 0;
  const items: UpdateExistItemResult[] = [];
  const updatedIds: number[] = [];

  for (const media of mediaList) {
    const item = processMedia(db, media, options);
    if (
      item.flag_exist_before !== item.flag_exist_after ||
      item.found_extension != null ||
      item.page_count_set != null
    ) {
      updated++;
      updatedIds.push(item.id);
    }
    items.push(item);
  }

  for (const item of items) {
    if (item.page_count_warning != null) {
      console.error(`WARNING [${item.id}] ${item.title}: ${item.page_count_warning}`);
    }
    if (item.found_extension != null) {
      if (options.dry_run) {
        console.error(`INFO [${item.id}] ${item.title}: extension=${item.found_extension} で発見（dry_run: 更新なし）`);
      } else {
        console.error(`INFO [${item.id}] ${item.title}: extension を ${item.found_extension} に更新`);
      }
    }
  }

  // 詳細結果を一時ファイルに書き出す
  const detailFile = tempFilePath('-detail.json');
  fs.writeFileSync(detailFile, JSON.stringify(items));

  // updated_idsが1000件超の場合はファイルに書き出す
  let updatedIdsInline: number[] | null;
  let updatedIdsFile: string | null;
  if (updatedIds.length > UPDATED_IDS_INLINE_LIMIT) {
    const idsFile = tempFilePath('-ids.json');
    fs.writeFileSync(idsFile, JSON.stringify(updatedIds));
    updatedIdsInline = null;
    updatedIdsFile = idsFile;
  } else {
    updatedIdsInline = updatedIds;
    updatedIdsFile = null;
  }

  return {
    total,
    updated,
    updated_ids: updatedIdsInline,
    updated_ids_file: updatedIdsFile,
    detail_file: detailFile,
  };
}
