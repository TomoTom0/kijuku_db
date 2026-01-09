/**
 * バルク操作の実装
 */
import type Database from 'better-sqlite3';
import type { BulkUpdateItem, Media, MediaInput } from './types.js';
import { createMedia, deleteMedia, updateMedia } from './crud.js';

/**
 * 複数のメディアを一括作成
 */
export function bulkCreateMedia(
  db: Database.Database,
  dataList: MediaInput[]
): Media[] {
  if (dataList.length === 0) {
    return [];
  }

  // トランザクション内で一括処理
  return db.transaction(() => {
    const results: Media[] = [];

    for (const data of dataList) {
      const media = createMedia(db, data);
      results.push(media);
    }

    return results;
  })();
}

/**
 * 複数のメディアを一括削除
 */
export function bulkDeleteMedia(db: Database.Database, ids: number[]): void {
  if (ids.length === 0) {
    return;
  }

  // トランザクション内で一括処理
  db.transaction(() => {
    for (const id of ids) {
      deleteMedia(db, id);
    }
  })();
}

/**
 * 複数のメディアを一括更新
 */
export function bulkUpdateMedia(
  db: Database.Database,
  updates: BulkUpdateItem[]
): void {
  if (updates.length === 0) {
    return;
  }

  // トランザクション内で一括処理
  db.transaction(() => {
    for (const item of updates) {
      updateMedia(db, item.id, item.data);
    }
  })();
}
