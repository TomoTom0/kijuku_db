/**
 * CRUD操作の実装
 */
import { randomUUID } from 'node:crypto';
import type Database from 'better-sqlite3';
import type { Media, MediaInput } from './types.js';
import {
  NotFoundError,
  handleDatabaseError,
  validateRequired,
  validateMediaType,
} from './errors.js';

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
 * volume_textから volume_number を計算
 * volume_textが整数に変換可能な場合のみ、その値を返す
 */
function calculateVolumeNumber(volumeText: string | undefined): number | null {
  if (!volumeText) return null;

  const trimmed = volumeText.trim();
  const num = parseInt(trimmed, 10);

  // 整数に変換可能で、変換後の文字列が元の文字列と一致する場合のみ
  if (!isNaN(num) && num.toString() === trimmed) {
    return num;
  }

  return null;
}

/**
 * メディアを作成
 */
export function createMedia(db: Database.Database, data: MediaInput): Media {
  try {
    // 必須フィールドのバリデーション
    validateRequired(data.title, 'title');
    validateRequired(data.media_type, 'media_type');

    // media_typeのバリデーション
    validateMediaType(data.media_type);

  // pathが指定されている場合、flag_existを自動的にtrueに設定
  const flagExist = data.path ? 1 : (data.flag_exist ? 1 : 0);

  // volume_textから volume_number を自動計算
  const volumeNumber = calculateVolumeNumber(data.volume_text);

  // UUID: 手動指定があればそれを使用、なければv4を自動生成
  const uuid = data.uuid ?? randomUUID();

  const stmt = db.prepare(`
    INSERT INTO media (
      uuid, title, title_id, path, media_type, thumbnail_path,
      artist, artist_id, description, file_size, duration_sec,
      page_count, series, volume_number, volume_text, volume_title,
      magazine, magazine_id, language, source, external_id,
      artist_en, title_en, chapters, extension, flag_exist,
      title_pron, artist_pron, series_pron
    ) VALUES (
      @uuid, @title, @title_id, @path, @media_type, @thumbnail_path,
      @artist, @artist_id, @description, @file_size, @duration_sec,
      @page_count, @series, @volume_number, @volume_text, @volume_title,
      @magazine, @magazine_id, @language, @source, @external_id,
      @artist_en, @title_en, @chapters, @extension, @flag_exist,
      @title_pron, @artist_pron, @series_pron
    )
  `);

  const result = stmt.run({
    uuid,
    title: data.title,
    title_id: data.title_id ?? null,
    path: data.path ?? null,
    media_type: data.media_type,
    thumbnail_path: data.thumbnail_path ?? null,
    artist: data.artist ?? null,
    artist_id: data.artist_id ?? null,
    description: data.description ?? null,
    file_size: data.file_size ?? null,
    duration_sec: data.duration_sec ?? null,
    page_count: data.page_count ?? null,
    series: data.series ?? null,
    volume_number: volumeNumber,
    volume_text: data.volume_text ?? null,
    volume_title: data.volume_title ?? null,
    magazine: data.magazine ?? null,
    magazine_id: data.magazine_id ?? null,
    language: data.language ?? null,
    source: data.source ?? null,
    external_id: data.external_id ?? null,
    artist_en: data.artist_en ?? null,
    title_en: data.title_en ?? null,
    chapters: data.chapters ?? null,
    extension: data.extension ?? null,
    flag_exist: flagExist,
    title_pron: data.title_pron ?? null,
    artist_pron: data.artist_pron ?? null,
    series_pron: data.series_pron ?? null,
  });

    const insertedId = Number(result.lastInsertRowid);
    const media = getMedia(db, insertedId);

    if (!media) {
      throw new NotFoundError('作成されたメディアの取得に失敗しました', { id: insertedId });
    }

    return media;
  } catch (error: any) {
    handleDatabaseError(error, 'メディアの作成に失敗しました');
  }
}

/**
 * IDでメディアを取得
 */
export function getMedia(db: Database.Database, id: number): Media | null {
  const stmt = db.prepare('SELECT * FROM media WHERE id = ?');
  const row = stmt.get(id);

  if (!row) {
    return null;
  }

  return rowToMedia(row);
}

/**
 * メディアを更新
 */
export function updateMedia(
  db: Database.Database,
  id: number,
  data: Partial<MediaInput>
): void {
  try {
    // メディアが存在するか確認
    const existing = getMedia(db, id);
    if (!existing) {
      throw new NotFoundError(`メディアが見つかりません`, { id });
    }

    // 更新するフィールドを動的に構築
    const fields: string[] = [];
    const values: Record<string, any> = { id };

  if (data.uuid !== undefined) {
    fields.push('uuid = @uuid');
    values.uuid = data.uuid;
  }
  if (data.title !== undefined) {
    fields.push('title = @title');
    values.title = data.title;
  }
  if (data.title_id !== undefined) {
    fields.push('title_id = @title_id');
    values.title_id = data.title_id;
  }
  if (data.path !== undefined) {
    fields.push('path = @path');
    values.path = data.path;
    // pathが設定された場合、flag_existも更新
    if (data.path) {
      fields.push('flag_exist = 1');
    }
  }
    if (data.media_type !== undefined) {
      // media_typeのバリデーション
      validateMediaType(data.media_type);
      fields.push('media_type = @media_type');
      values.media_type = data.media_type;
    }
  if (data.thumbnail_path !== undefined) {
    fields.push('thumbnail_path = @thumbnail_path');
    values.thumbnail_path = data.thumbnail_path;
  }
  if (data.artist !== undefined) {
    fields.push('artist = @artist');
    values.artist = data.artist;
  }
  if (data.artist_id !== undefined) {
    fields.push('artist_id = @artist_id');
    values.artist_id = data.artist_id;
  }
  if (data.description !== undefined) {
    fields.push('description = @description');
    values.description = data.description;
  }
  if (data.file_size !== undefined) {
    fields.push('file_size = @file_size');
    values.file_size = data.file_size;
  }
  if (data.duration_sec !== undefined) {
    fields.push('duration_sec = @duration_sec');
    values.duration_sec = data.duration_sec;
  }
  if (data.page_count !== undefined) {
    fields.push('page_count = @page_count');
    values.page_count = data.page_count;
  }
  if (data.series !== undefined) {
    fields.push('series = @series');
    values.series = data.series;
  }
  if (data.volume_text !== undefined) {
    fields.push('volume_text = @volume_text');
    values.volume_text = data.volume_text;

    // volume_textが更新される場合、volume_numberも自動更新
    const volumeNumber = calculateVolumeNumber(data.volume_text);
    fields.push('volume_number = @volume_number');
    values.volume_number = volumeNumber;
  }
  if (data.volume_title !== undefined) {
    fields.push('volume_title = @volume_title');
    values.volume_title = data.volume_title;
  }
  if (data.magazine !== undefined) {
    fields.push('magazine = @magazine');
    values.magazine = data.magazine;
  }
  if (data.magazine_id !== undefined) {
    fields.push('magazine_id = @magazine_id');
    values.magazine_id = data.magazine_id;
  }
  if (data.language !== undefined) {
    fields.push('language = @language');
    values.language = data.language;
  }
  if (data.source !== undefined) {
    fields.push('source = @source');
    values.source = data.source;
  }
  if (data.external_id !== undefined) {
    fields.push('external_id = @external_id');
    values.external_id = data.external_id;
  }
  if (data.artist_en !== undefined) {
    fields.push('artist_en = @artist_en');
    values.artist_en = data.artist_en;
  }
  if (data.title_en !== undefined) {
    fields.push('title_en = @title_en');
    values.title_en = data.title_en;
  }
  if (data.chapters !== undefined) {
    fields.push('chapters = @chapters');
    values.chapters = data.chapters;
  }
  if (data.extension !== undefined) {
    fields.push('extension = @extension');
    values.extension = data.extension;
  }
  if (data.flag_exist !== undefined) {
    fields.push('flag_exist = @flag_exist');
    values.flag_exist = data.flag_exist ? 1 : 0;
  }
  if (data.title_pron !== undefined) {
    fields.push('title_pron = @title_pron');
    values.title_pron = data.title_pron;
  }
  if (data.artist_pron !== undefined) {
    fields.push('artist_pron = @artist_pron');
    values.artist_pron = data.artist_pron;
  }
  if (data.series_pron !== undefined) {
    fields.push('series_pron = @series_pron');
    values.series_pron = data.series_pron;
  }

    if (fields.length === 0) {
      return; // 更新するフィールドがない場合は何もしない
    }

    const sql = `UPDATE media SET ${fields.join(', ')} WHERE id = @id`;
    const stmt = db.prepare(sql);
    stmt.run(values);
  } catch (error: any) {
    handleDatabaseError(error, `メディアの更新に失敗しました (id: ${id})`);
  }
}

/**
 * メディアを削除
 */
export function deleteMedia(db: Database.Database, id: number): void {
  try {
    // メディアが存在するか確認
    const existing = getMedia(db, id);
    if (!existing) {
      throw new NotFoundError(`メディアが見つかりません`, { id });
    }

    const stmt = db.prepare('DELETE FROM media WHERE id = ?');
    stmt.run(id);
  } catch (error: any) {
    handleDatabaseError(error, `メディアの削除に失敗しました (id: ${id})`);
  }
}
