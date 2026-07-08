import type Database from 'better-sqlite3';
import type { MediaHash, MediaHashInput, ComputeHashResult, MediaFilter, QueryOptions, Media } from './types.js';
import { findMedia } from './search.js';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';

function rowToMediaHash(row: Record<string, unknown>): MediaHash {
  const rawHash = row.content_hash as Uint8Array;
  const rawEmbedding = row.embedding as Uint8Array | null;
  return {
    item_uuid: row.item_uuid as string,
    filename: row.filename as string,
    time_range: row.time_range as string,
    content_hash: new Uint8Array(rawHash.buffer, rawHash.byteOffset, rawHash.byteLength),
    alternative_of: (row.alternative_of as string) ?? undefined,
    embedding: rawEmbedding ? new Uint8Array(rawEmbedding.buffer, rawEmbedding.byteOffset, rawEmbedding.byteLength) : undefined,
    created_at: row.created_at as string,
    updated_at: row.updated_at as string,
  };
}

/** HEX文字列からUint8Arrayに変換 */
export function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) {
    throw new Error('HEX string must have even length');
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    bytes[i / 2] = parseInt(hex.substring(i, i + 2), 16);
  }
  return bytes;
}

/** Uint8ArrayからHEX文字列に変換 */
export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');
}

/** メディアハッシュを登録（単件） */
export function addMediaHash(db: Database.Database, input: MediaHashInput): MediaHash {
  const stmt = db.prepare(`
    INSERT INTO media_hashes (item_uuid, filename, time_range, content_hash, alternative_of)
    VALUES (@item_uuid, @filename, @time_range, @content_hash, @alternative_of)
    ON CONFLICT(item_uuid, filename, time_range) DO UPDATE SET
      content_hash = @content_hash,
      alternative_of = @alternative_of
  `);
  stmt.run({
    item_uuid: input.item_uuid,
    filename: input.filename,
    time_range: input.time_range,
    content_hash: Buffer.from(input.content_hash),
    alternative_of: input.alternative_of ?? null,
  });
  return getMediaHash(db, input.item_uuid, input.filename, input.time_range)!;
}

/** メディアハッシュを一括登録 */
export function addMediaHashes(db: Database.Database, inputs: MediaHashInput[]): MediaHash[] {
  const stmt = db.prepare(`
    INSERT INTO media_hashes (item_uuid, filename, time_range, content_hash, alternative_of)
    VALUES (@item_uuid, @filename, @time_range, @content_hash, @alternative_of)
    ON CONFLICT(item_uuid, filename, time_range) DO UPDATE SET
      content_hash = @content_hash,
      alternative_of = @alternative_of
  `);
  const insertMany = db.transaction((items: MediaHashInput[]) => {
    for (const item of items) {
      stmt.run({
        item_uuid: item.item_uuid,
        filename: item.filename,
        time_range: item.time_range,
        content_hash: Buffer.from(item.content_hash),
        alternative_of: item.alternative_of ?? null,
      });
    }
  });
  insertMany(inputs);

  return inputs
    .map(i => getMediaHash(db, i.item_uuid, i.filename, i.time_range))
    .filter((h): h is MediaHash => h !== null);
}

/** 特定作品の全ハッシュを取得 */
export function getMediaHashes(db: Database.Database, itemUuid: string): MediaHash[] {
  const stmt = db.prepare(`
    SELECT item_uuid, filename, time_range, content_hash, alternative_of, embedding, created_at, updated_at
    FROM media_hashes
    WHERE item_uuid = ?
    ORDER BY filename, time_range
  `);
  const rows = stmt.all(itemUuid) as Record<string, unknown>[];
  return rows.map(rowToMediaHash);
}

/** 全てのメディアハッシュを取得（差分比較用） */
export function getAllMediaHashes(db: Database.Database): MediaHash[] {
  const stmt = db.prepare(`
    SELECT item_uuid, filename, time_range, content_hash, alternative_of, embedding, created_at, updated_at
    FROM media_hashes
    ORDER BY item_uuid, filename, time_range
  `);
  const rows = stmt.all() as Record<string, unknown>[];
  return rows.map(rowToMediaHash);
}

/** 特定位置のハッシュを取得 */
export function getMediaHash(
  db: Database.Database,
  itemUuid: string,
  filename: string,
  timeRange: string
): MediaHash | null {
  const stmt = db.prepare(`
    SELECT item_uuid, filename, time_range, content_hash, alternative_of, embedding, created_at, updated_at
    FROM media_hashes
    WHERE item_uuid = ? AND filename = ? AND time_range = ?
  `);
  const row = stmt.get(itemUuid, filename, timeRange) as Record<string, unknown> | undefined;
  return row ? rowToMediaHash(row) : null;
}

/** SHA256による完全一致検索 */
export function findByContentHash(db: Database.Database, hash: Uint8Array): MediaHash[] {
  const stmt = db.prepare(`
    SELECT item_uuid, filename, time_range, content_hash, alternative_of, embedding, created_at, updated_at
    FROM media_hashes
    WHERE content_hash = ?
  `);
  const rows = stmt.all(Buffer.from(hash)) as Record<string, unknown>[];
  return rows.map(rowToMediaHash);
}

/** 特定位置のハッシュを削除（代替行の連鎖削除を含む） */
export function deleteMediaHash(
  db: Database.Database,
  itemUuid: string,
  filename: string,
  timeRange: string
): void {
  // 削除対象が原本の場合、代替行も削除（空文字ではalternative_ofを検索しない）
  if (filename) {
    db.prepare('DELETE FROM media_hashes WHERE item_uuid = ? AND alternative_of = ?')
      .run(itemUuid, filename);
  }
  if (timeRange) {
    db.prepare('DELETE FROM media_hashes WHERE item_uuid = ? AND alternative_of = ?')
      .run(itemUuid, timeRange);
  }
  db.prepare('DELETE FROM media_hashes WHERE item_uuid = ? AND filename = ? AND time_range = ?')
    .run(itemUuid, filename, timeRange);
}

/** 特定作品のハッシュを全削除 */
export function deleteMediaHashes(db: Database.Database, itemUuid: string): void {
  db.prepare('DELETE FROM media_hashes WHERE item_uuid = ?').run(itemUuid);
}

/** 重複ハッシュの検出 */
export function findDuplicateHashes(db: Database.Database): Array<{ content_hash: Uint8Array; count: number }> {
  const stmt = db.prepare(`
    SELECT content_hash, COUNT(*) as cnt
    FROM media_hashes
    GROUP BY content_hash
    HAVING cnt > 1
  `);
  const rows = stmt.all() as Array<{ content_hash: Uint8Array; cnt: number }>;
  return rows.map(r => {
    const h = r.content_hash;
    return { content_hash: new Uint8Array(h.buffer, h.byteOffset, h.byteLength), count: r.cnt };
  });
}

// ========== compute functions ==========

function computeSha256File(filePath: string): Uint8Array {
  const hash = crypto.createHash('sha256');
  const fd = fs.openSync(filePath, 'r');
  const buffer = Buffer.alloc(8192);
  try {
    let bytesRead = 0;
    while ((bytesRead = fs.readSync(fd, buffer, 0, buffer.length, null)) > 0) {
      hash.update(buffer.subarray(0, bytesRead));
    }
  } finally {
    fs.closeSync(fd);
  }
  return new Uint8Array(hash.digest());
}

function computeSha256Bytes(data: Uint8Array): Uint8Array {
  const hash = crypto.createHash('sha256');
  hash.update(data);
  return new Uint8Array(hash.digest());
}

const IMAGE_EXTENSIONS = new Set(['.jpg', '.jpeg', '.png', '.gif', '.webp']);

function isImageFile(name: string): boolean {
  const ext = path.extname(name).toLowerCase();
  return IMAGE_EXTENSIONS.has(ext);
}

/** 特定のメディアのハッシュを計算・登録 */
export function computeMediaHash(
  db: Database.Database,
  itemUuid: string,
  mediaPath: string,
  mediaType: string,
  durationSec?: number
): ComputeHashResult {
  if (!fs.existsSync(mediaPath)) {
    return { item_uuid: itemUuid, hashes: [], skipped: true, skip_reason: `Path does not exist: ${mediaPath}` };
  }

  switch (mediaType) {
    case 'music': return computeMusicHash(db, itemUuid, mediaPath, durationSec);
    case 'video': return computeVideoHash(db, itemUuid, mediaPath);
    case 'comic': return computeComicHash(db, itemUuid, mediaPath);
    default:
      return { item_uuid: itemUuid, hashes: [], skipped: true, skip_reason: `Unknown media type: ${mediaType}` };
  }
}

function computeMusicHash(
  db: Database.Database,
  itemUuid: string,
  mediaPath: string,
  durationSec?: number
): ComputeHashResult {
  let stat: fs.Stats;
  try {
    stat = fs.statSync(mediaPath);
  } catch (e) {
    return { item_uuid: itemUuid, hashes: [], skipped: true, skip_reason: `Failed to read file metadata: ${e instanceof Error ? e.message : String(e)}` };
  }

  if (stat.size === 0) {
    return { item_uuid: itemUuid, hashes: [], skipped: true, skip_reason: 'File size is 0' };
  }

  let wholeHash: Uint8Array;
  let prefixHash: Uint8Array;
  try {
    wholeHash = computeSha256File(mediaPath);

    // 先頭30秒分のバイト数を推定
    let prefixBytes = 1024 * 1024; // デフォルト1MB
    if (durationSec && durationSec > 0) {
      prefixBytes = Math.max(1, Math.floor(stat.size * 30 / durationSec));
    }
    const hash = crypto.createHash('sha256');
    const fd = fs.openSync(mediaPath, 'r');
    const buffer = Buffer.alloc(8192);
    try {
      let remaining = prefixBytes;
      while (remaining > 0) {
        const toRead = Math.min(buffer.length, remaining);
        const bytesRead = fs.readSync(fd, buffer, 0, toRead, null);
        if (bytesRead === 0) break;
        hash.update(buffer.subarray(0, bytesRead));
        remaining -= bytesRead;
      }
    } finally {
      fs.closeSync(fd);
    }
    prefixHash = new Uint8Array(hash.digest());
  } catch (e) {
    return { item_uuid: itemUuid, hashes: [], skipped: true, skip_reason: `Failed to compute hash: ${e instanceof Error ? e.message : String(e)}` };
  }

  const inputs: MediaHashInput[] = [
    { item_uuid: itemUuid, filename: '', time_range: '', content_hash: wholeHash },
    { item_uuid: itemUuid, filename: '', time_range: '0.0-30.0', content_hash: prefixHash },
  ];

  const hashes = addMediaHashes(db, inputs);
  return { item_uuid: itemUuid, hashes, skipped: false };
}

function computeVideoHash(
  db: Database.Database,
  itemUuid: string,
  mediaPath: string
): ComputeHashResult {
  let stat: fs.Stats;
  try {
    stat = fs.statSync(mediaPath);
  } catch (e) {
    return { item_uuid: itemUuid, hashes: [], skipped: true, skip_reason: `Failed to read file metadata: ${e instanceof Error ? e.message : String(e)}` };
  }

  if (stat.size === 0) {
    return { item_uuid: itemUuid, hashes: [], skipped: true, skip_reason: 'File size is 0' };
  }

  let wholeHash: Uint8Array;
  try {
    wholeHash = computeSha256File(mediaPath);
  } catch (e) {
    return { item_uuid: itemUuid, hashes: [], skipped: true, skip_reason: `Failed to compute hash: ${e instanceof Error ? e.message : String(e)}` };
  }

  const hashes = addMediaHashes(db, [
    { item_uuid: itemUuid, filename: '', time_range: '', content_hash: wholeHash },
  ]);
  return { item_uuid: itemUuid, hashes, skipped: false };
}

function computeComicHash(
  db: Database.Database,
  itemUuid: string,
  mediaPath: string
): ComputeHashResult {
  let files: string[];
  try {
    if (!fs.statSync(mediaPath).isDirectory()) {
      return { item_uuid: itemUuid, hashes: [], skipped: true, skip_reason: `Comic path is not a directory: ${mediaPath}` };
    }
    files = fs.readdirSync(mediaPath)
      .filter(f => {
        try {
          return fs.statSync(path.join(mediaPath, f)).isFile() && isImageFile(f);
        } catch {
          return false;
        }
      })
      .sort();
  } catch (e) {
    return { item_uuid: itemUuid, hashes: [], skipped: true, skip_reason: `Failed to read directory: ${e instanceof Error ? e.message : String(e)}` };
  }

  if (files.length === 0) {
    return { item_uuid: itemUuid, hashes: [], skipped: true, skip_reason: 'No image files found in directory' };
  }

  const inputs: MediaHashInput[] = [];
  const pageHashes: Uint8Array[] = [];

  for (const name of files) {
    const filePath = path.join(mediaPath, name);
    try {
      const hash = computeSha256File(filePath);
      pageHashes.push(hash);
      inputs.push({ item_uuid: itemUuid, filename: name, time_range: '', content_hash: hash });
    } catch {
      // skip unreadable files
    }
  }

  // 全体hash
  const combined = new Uint8Array(pageHashes.reduce((acc, h) => acc + h.length, 0));
  let offset = 0;
  for (const h of pageHashes) {
    combined.set(h, offset);
    offset += h.length;
  }
  const wholeHash = computeSha256Bytes(combined);
  inputs.push({ item_uuid: itemUuid, filename: '', time_range: '', content_hash: wholeHash });

  const hashes = addMediaHashes(db, inputs);
  return { item_uuid: itemUuid, hashes, skipped: false };
}

/** フィルタ条件でメディアを絞り込み、ハッシュを計算・登録 */
export function computeMediaHashes(
  db: Database.Database,
  filter: MediaFilter,
  options?: QueryOptions,
  force = false
): ComputeHashResult[] {
  const mediaList = findMedia(db, filter, options);
  const results: ComputeHashResult[] = [];

  for (const media of mediaList) {
    if (!media.flag_exist) continue;
    const mediaPath = media.path;
    if (!mediaPath) continue;

    if (!force) {
      const existing = getMediaHashes(db, media.uuid);
      if (existing.length > 0) continue;
    }

    const result = computeMediaHash(db, media.uuid, mediaPath, media.media_type, media.duration_sec);
    results.push(result);
  }

  return results;
}
