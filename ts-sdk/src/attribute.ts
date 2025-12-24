/**
 * メディア追加属性の操作
 */
import type Database from 'better-sqlite3';
import type { MediaAttribute } from './types.js';

/**
 * メディアに属性を設定
 */
export function setMediaAttribute(
  db: Database.Database,
  mediaId: number,
  key: string,
  value: string | null,
  valueType?: string
): void {
  const stmt = db.prepare(`
    INSERT INTO media_attributes (media_id, key, value, value_type)
    VALUES (@media_id, @key, @value, @value_type)
    ON CONFLICT(media_id, key) DO UPDATE SET
      value = @value,
      value_type = @value_type
  `);

  stmt.run({
    media_id: mediaId,
    key,
    value,
    value_type: valueType ?? null,
  });
}

/**
 * メディアの属性を取得
 */
export function getMediaAttribute(
  db: Database.Database,
  mediaId: number,
  key: string
): MediaAttribute | null {
  const stmt = db.prepare(`
    SELECT media_id, key, value, value_type
    FROM media_attributes
    WHERE media_id = @media_id AND key = @key
  `);

  return stmt.get({ media_id: mediaId, key }) as MediaAttribute | null;
}

/**
 * メディアの全ての属性を取得
 */
export function getMediaAttributes(
  db: Database.Database,
  mediaId: number
): MediaAttribute[] {
  const stmt = db.prepare(`
    SELECT media_id, key, value, value_type
    FROM media_attributes
    WHERE media_id = @media_id
    ORDER BY key
  `);

  return stmt.all({ media_id: mediaId }) as MediaAttribute[];
}

/**
 * メディアの属性を削除
 */
export function deleteMediaAttribute(
  db: Database.Database,
  mediaId: number,
  key: string
): void {
  const stmt = db.prepare(`
    DELETE FROM media_attributes
    WHERE media_id = @media_id AND key = @key
  `);

  stmt.run({ media_id: mediaId, key });
}

/**
 * メディアの全ての属性を削除
 */
export function deleteAllMediaAttributes(
  db: Database.Database,
  mediaId: number
): void {
  const stmt = db.prepare(`
    DELETE FROM media_attributes
    WHERE media_id = @media_id
  `);

  stmt.run({ media_id: mediaId });
}
