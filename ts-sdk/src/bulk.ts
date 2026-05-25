/**
 * バルク操作の実装
 */
import type Database from 'better-sqlite3';
import type { BulkUpdateItem, Media, MediaInput } from './types.js';
import { createMedia, deleteMedia, updateMedia } from './crud.js';

const DEFAULT_MAX_BATCH_SIZE = 500;

/**
 * 複数のメディアを一括作成
 *
 * 大量データでのDBロック長期化を防ぐため、maxBatchSize件ごとに
 * トランザクションを分割して処理する。
 */
export function bulkCreateMedia(
  db: Database.Database,
  dataList: MediaInput[],
  maxBatchSize = DEFAULT_MAX_BATCH_SIZE
): Media[] {
  if (dataList.length === 0) {
    return [];
  }

  const results: Media[] = [];
  for (let i = 0; i < dataList.length; i += maxBatchSize) {
    const batch = dataList.slice(i, i + maxBatchSize);
    const batchResults = db.transaction(() => batch.map(data => createMedia(db, data)))();
    results.push(...batchResults);
  }
  return results;
}

/**
 * 複数のメディアを一括削除
 *
 * 大量データでのDBロック長期化を防ぐため、maxBatchSize件ごとに
 * トランザクションを分割して処理する。
 */
export function bulkDeleteMedia(
  db: Database.Database,
  ids: number[],
  maxBatchSize = DEFAULT_MAX_BATCH_SIZE
): void {
  if (ids.length === 0) {
    return;
  }

  for (let i = 0; i < ids.length; i += maxBatchSize) {
    const batch = ids.slice(i, i + maxBatchSize);
    db.transaction(() => {
      for (const id of batch) {
        deleteMedia(db, id);
      }
    })();
  }
}

/**
 * 複数のメディアを一括更新
 *
 * 大量データでのDBロック長期化を防ぐため、maxBatchSize件ごとに
 * トランザクションを分割して処理する。
 */
export function bulkUpdateMedia(
  db: Database.Database,
  updates: BulkUpdateItem[],
  maxBatchSize = DEFAULT_MAX_BATCH_SIZE
): void {
  if (updates.length === 0) {
    return;
  }

  for (let i = 0; i < updates.length; i += maxBatchSize) {
    const batch = updates.slice(i, i + maxBatchSize);
    db.transaction(() => {
      for (const item of batch) {
        updateMedia(db, item.id, item.data);
      }
    })();
  }
}
