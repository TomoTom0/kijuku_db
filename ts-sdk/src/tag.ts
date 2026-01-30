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
      'INSERT INTO media_tags (media_id, tag_id) VALUES (?, ?)'
    );
    stmt.run(mediaId, tagId);
  } catch (error: any) {
    // UNIQUE制約違反の場合（すでに追加済み）
    if (error.code === 'SQLITE_CONSTRAINT_UNIQUE') {
      // すでに追加されている場合は何もしない
      return;
    }
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
