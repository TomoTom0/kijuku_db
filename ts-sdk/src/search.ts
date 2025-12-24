/**
 * 検索・フィルタ機能
 */
import type Database from 'better-sqlite3';
import type { Media, MediaFilter, QueryOptions } from './types.js';

/**
 * SQLiteの行データをMediaオブジェクトに変換
 */
function rowToMedia(row: any): Media {
  return {
    ...row,
    flag_exist: Boolean(row.flag_exist),
    created_at: new Date(row.created_at),
    updated_at: new Date(row.updated_at),
  };
}

/**
 * メディアを検索
 */
export function findMedia(
  db: Database.Database,
  filter: MediaFilter,
  options?: QueryOptions
): Media[] {
  const whereClauses: string[] = [];
  const params: Record<string, any> = {};

  // フィルタ条件を構築
  if (filter.title !== undefined) {
    whereClauses.push('m.title = @title');
    params.title = filter.title;
  }
  if (filter.title_id !== undefined) {
    whereClauses.push('m.title_id = @title_id');
    params.title_id = filter.title_id;
  }
  if (filter.artist !== undefined) {
    whereClauses.push('m.artist = @artist');
    params.artist = filter.artist;
  }
  if (filter.artist_id !== undefined) {
    whereClauses.push('m.artist_id = @artist_id');
    params.artist_id = filter.artist_id;
  }
  if (filter.media_type !== undefined) {
    whereClauses.push('m.media_type = @media_type');
    params.media_type = filter.media_type;
  }
  if (filter.series !== undefined) {
    whereClauses.push('m.series = @series');
    params.series = filter.series;
  }
  if (filter.source !== undefined) {
    whereClauses.push('m.source = @source');
    params.source = filter.source;
  }

  // タグフィルタの処理
  let fromClause = 'FROM media m';
  if (filter.tag_ids && filter.tag_ids.length > 0) {
    fromClause = `
      FROM media m
      INNER JOIN media_tags mt ON m.id = mt.media_id
    `;
    const tagPlaceholders = filter.tag_ids.map((_, i) => `@tag_id_${i}`).join(', ');
    whereClauses.push(`mt.tag_id IN (${tagPlaceholders})`);
    filter.tag_ids.forEach((tagId, i) => {
      params[`tag_id_${i}`] = tagId;
    });
  }

  // WHERE句の構築
  const whereClause =
    whereClauses.length > 0 ? `WHERE ${whereClauses.join(' AND ')}` : '';

  // GROUP BY句（タグフィルタ使用時に重複を排除）
  const groupByClause = filter.tag_ids && filter.tag_ids.length > 0 ? 'GROUP BY m.id' : '';

  // ORDER BY句の構築
  let orderByClause = '';
  if (options?.orderBy) {
    const order = options.order ?? 'ASC';
    orderByClause = `ORDER BY m.${options.orderBy} ${order}`;
  }

  // LIMIT/OFFSET句の構築
  let limitClause = '';
  if (options?.limit !== undefined || options?.offset !== undefined) {
    // OFFSETを使う場合、LIMITも必要
    if (options?.limit !== undefined) {
      limitClause = `LIMIT @limit`;
      params.limit = options.limit;
    } else {
      // OFFSETのみの場合、LIMITに大きな値を設定
      limitClause = `LIMIT -1`;
    }

    if (options?.offset !== undefined) {
      limitClause += ` OFFSET @offset`;
      params.offset = options.offset;
    }
  }

  // SQLクエリの組み立て
  const sql = `
    SELECT m.*
    ${fromClause}
    ${whereClause}
    ${groupByClause}
    ${orderByClause}
    ${limitClause}
  `.trim();

  const stmt = db.prepare(sql);
  const rows = stmt.all(params);

  return rows.map(rowToMedia);
}
