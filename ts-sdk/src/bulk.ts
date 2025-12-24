/**
 * バルク操作の実装
 */
import type Database from 'better-sqlite3';
import type { Media, MediaInput } from './types.js';
import { createMedia } from './crud.js';

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
