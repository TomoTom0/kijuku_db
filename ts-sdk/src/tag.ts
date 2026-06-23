/**
 * タグ管理機能
 */
import type Database from 'better-sqlite3';
import type { Tag, TagUsageStats } from './types.js';

/**
 * タグを作成
 */
export function createTag(db: Database.Database, name: string): Tag {
  if (!name || name.trim() === '') {
    throw new Error('Tag name is required');
  }

  try {
    const stmt = db.prepare('INSERT INTO tags (name) VALUES (?)');
    const result = stmt.run(name.trim());

    const insertedId = Number(result.lastInsertRowid);

    return {
      id: insertedId,
      name: name.trim(),
    };
  } catch (error: any) {
    // UNIQUE制約違反の場合
    if (error.code === 'SQLITE_CONSTRAINT_UNIQUE') {
      throw new Error(`Tag "${name}" already exists`);
    }
    throw error;
  }
}

/**
 * メディアにタグを追加
 */
export function addTagToMedia(
  db: Database.Database,
  mediaId: number,
  tagId: number
): void {
  try {
    const stmt = db.prepare(
      'INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?, ?)'
    );
    stmt.run(mediaId, tagId);
  } catch (error: any) {
    // 外部キー制約違反の場合
    if (error.code === 'SQLITE_CONSTRAINT_FOREIGNKEY') {
      throw new Error(`Media ${mediaId} or Tag ${tagId} not found`);
    }
    throw error;
  }
}

/**
 * メディアからタグを削除
 */
export function removeTagFromMedia(
  db: Database.Database,
  mediaId: number,
  tagId: number
): void {
  const stmt = db.prepare(
    'DELETE FROM media_tags WHERE media_id = ? AND tag_id = ?'
  );
  const result = stmt.run(mediaId, tagId);

  if (result.changes === 0) {
    throw new Error(
      `Tag ${tagId} is not associated with Media ${mediaId}`
    );
  }
}

/**
 * メディアに関連付けられたタグを取得
 */
export function getMediaTags(db: Database.Database, mediaId: number): Tag[] {
  const stmt = db.prepare(`
    SELECT t.id, t.name
    FROM tags t
    INNER JOIN media_tags mt ON t.id = mt.tag_id
    WHERE mt.media_id = ?
    ORDER BY t.name
  `);

  return stmt.all(mediaId) as Tag[];
}

/**
 * 複数メディアのタグを一括取得（N+1回避）
 *
 * media_tags と tags の JOIN 1発で複数メディアのタグを取得し、
 * media_id -> Tag[] のマップに集約する。タグを持たないメディアは
 * 結果のエントリに含まれない（呼び出し側で補完すること）。
 * 999件超はチャンク分割して順次取得・マージする。
 */
export function getMediaTagsBulk(
  db: Database.Database,
  mediaIds: number[]
): Record<number, Tag[]> {
  const result: Record<number, Tag[]> = {};
  if (mediaIds.length === 0) {
    return result;
  }

  // 重複IDを排除（IN句は集合扱いで結果の重複は生じないが、プレースホルダーの
  // 無駄な増加と999件チャンク制限への早期到達を防ぐ）
  const uniqueMediaIds = Array.from(new Set(mediaIds));
  const CHUNK_SIZE = 999;
  for (let i = 0; i < uniqueMediaIds.length; i += CHUNK_SIZE) {
    const chunk = uniqueMediaIds.slice(i, i + CHUNK_SIZE);
    const placeholders = chunk.map(() => '?').join(', ');
    const stmt = db.prepare(`
      SELECT mt.media_id AS media_id, t.id AS id, t.name AS name
      FROM media_tags mt
      INNER JOIN tags t ON t.id = mt.tag_id
      WHERE mt.media_id IN (${placeholders})
      ORDER BY t.name
    `);
    const rows = stmt.all(...chunk) as Array<{
      media_id: number;
      id: number;
      name: string;
    }>;
    for (const row of rows) {
      if (!result[row.media_id]) {
        result[row.media_id] = [];
      }
      result[row.media_id].push({ id: row.id, name: row.name });
    }
  }

  return result;
}

/**
 * タグ名でタグを取得
 */
export function getTagByName(
  db: Database.Database,
  name: string
): Tag | null {
  const stmt = db.prepare('SELECT id, name FROM tags WHERE name = ?');
  const row = stmt.get(name.trim());

  if (!row) {
    return null;
  }

  return row as Tag;
}

/**
 * 全てのタグを取得
 */
export function getAllTags(db: Database.Database): Tag[] {
  const stmt = db.prepare('SELECT id, name FROM tags ORDER BY name');
  return stmt.all() as Tag[];
}

/**
 * タグの使用数統計を取得
 */
export function getTagUsageStats(db: Database.Database): TagUsageStats[] {
  const stmt = db.prepare(`
    SELECT t.id as tag_id, t.name as tag_name, COUNT(mt.media_id) as count
    FROM tags t
    LEFT JOIN media_tags mt ON t.id = mt.tag_id
    GROUP BY t.id, t.name
    ORDER BY count DESC, t.name ASC
  `);

  return stmt.all() as TagUsageStats[];
}

/**
 * 未使用のタグを取得
 */
export function findUnusedTags(db: Database.Database): Tag[] {
  const stmt = db.prepare(`
    SELECT t.id, t.name
    FROM tags t
    LEFT JOIN media_tags mt ON t.id = mt.tag_id
    WHERE mt.media_id IS NULL
    ORDER BY t.name
  `);

  return stmt.all() as Tag[];
}
