/**
 * きじゅくDB パフォーマンスベンチマーク
 *
 * 大量データでの各操作のパフォーマンスを測定します。
 */

import Database from 'better-sqlite3';
import { resolve, dirname } from 'path';
import { unlinkSync } from 'fs';
import { fileURLToPath } from 'url';
import { migrate } from '../src/migration';
import { createMedia, getMedia, updateMedia, deleteMedia } from '../src/crud';
import { findMedia } from '../src/search';
import { bulkCreateMedia } from '../src/bulk';
import type { MediaInput, MediaType } from '../src/types';

// ESモジュールで__dirnameを取得
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// テスト用データベースパス
const BENCH_DB_PATH = resolve(__dirname, '../data/benchmark.db');

// ベンチマーク設定
const SAMPLE_SIZES = [100, 1000, 10000];

// テストデータ生成
function generateMediaData(count: number, mediaType: MediaType = 'comic'): MediaInput[] {
  const data: MediaInput[] = [];
  for (let i = 0; i < count; i++) {
    data.push({
      title: `テストメディア ${i + 1}`,
      title_id: `test-${i + 1}`,
      media_type: mediaType,
      artist: `作者 ${(i % 100) + 1}`,
      artist_id: `artist-${(i % 100) + 1}`,
      series: i % 10 === 0 ? `シリーズ ${Math.floor(i / 10) + 1}` : undefined,
      description: `これはベンチマーク用のテストデータです。番号: ${i + 1}`,
      page_count: Math.floor(Math.random() * 200) + 50,
      file_size: Math.floor(Math.random() * 100000000) + 1000000,
      flag_exist: true,
    });
  }
  return data;
}

// 時間計測ヘルパー
function measureTime(fn: () => void, label: string): number {
  const start = performance.now();
  fn();
  const end = performance.now();
  const duration = end - start;
  console.log(`  ${label}: ${duration.toFixed(2)}ms`);
  return duration;
}

async function measureTimeAsync(fn: () => Promise<void>, label: string): Promise<number> {
  const start = performance.now();
  await fn();
  const end = performance.now();
  const duration = end - start;
  console.log(`  ${label}: ${duration.toFixed(2)}ms`);
  return duration;
}

// データベース初期化
function setupDatabase(): Database.Database {
  try {
    unlinkSync(BENCH_DB_PATH);
  } catch {}

  const db = new Database(BENCH_DB_PATH);
  migrate(db);
  return db;
}

// 1. 大量データ挿入のベンチマーク
function benchmarkBulkInsert(db: Database.Database) {
  console.log('\n=== 大量データ挿入ベンチマーク ===');

  for (const size of SAMPLE_SIZES) {
    console.log(`\nデータ件数: ${size}`);
    const data = generateMediaData(size);

    // 通常の挿入
    const normalDb = setupDatabase();
    const normalTime = measureTime(() => {
      const insert = normalDb.prepare(`
        INSERT INTO media (title, title_id, media_type, artist, artist_id, series, description, page_count, file_size, flag_exist)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      `);
      for (const item of data) {
        insert.run(
          item.title, item.title_id, item.media_type, item.artist, item.artist_id,
          item.series, item.description, item.page_count, item.file_size, item.flag_exist ? 1 : 0
        );
      }
    }, '通常の挿入');
    normalDb.close();

    // トランザクション付き挿入
    const txDb = setupDatabase();
    const txTime = measureTime(() => {
      const insert = txDb.prepare(`
        INSERT INTO media (title, title_id, media_type, artist, artist_id, series, description, page_count, file_size, flag_exist)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      `);
      const insertMany = txDb.transaction((items: MediaInput[]) => {
        for (const item of items) {
          insert.run(
            item.title, item.title_id, item.media_type, item.artist, item.artist_id,
            item.series, item.description, item.page_count, item.file_size, item.flag_exist ? 1 : 0
          );
        }
      });
      insertMany(data);
    }, 'トランザクション付き挿入');
    txDb.close();

    // bulkCreateMedia関数
    const bulkDb = setupDatabase();
    const bulkTime = measureTime(() => {
      bulkCreateMedia(bulkDb, data);
    }, 'bulkCreateMedia関数');
    bulkDb.close();

    console.log(`  速度改善: 通常 1.00x / TX ${(normalTime / txTime).toFixed(2)}x / BulkAPI ${(normalTime / bulkTime).toFixed(2)}x`);
  }
}

// 2. 検索クエリのベンチマーク
function benchmarkSearch(db: Database.Database) {
  console.log('\n=== 検索クエリベンチマーク ===');

  for (const size of SAMPLE_SIZES) {
    console.log(`\nデータ件数: ${size}`);
    const searchDb = setupDatabase();
    bulkCreateMedia(searchDb, generateMediaData(size));

    // ID検索
    measureTime(() => {
      getMedia(searchDb, 1);
    }, 'ID検索 (1件)');

    // タイトル完全一致検索
    measureTime(() => {
      findMedia(searchDb, { title: 'テストメディア 1' });
    }, 'タイトル完全一致検索');

    // 作者検索
    measureTime(() => {
      findMedia(searchDb, { artist: '作者 1' });
    }, '作者検索');

    // 複合条件検索
    measureTime(() => {
      findMedia(searchDb, {
        media_type: 'comic',
        artist: '作者 1'
      });
    }, '複合条件検索');

    // ページネーション
    measureTime(() => {
      findMedia(searchDb, {}, { limit: 100, offset: 0 });
    }, 'ページネーション (100件取得)');

    searchDb.close();
  }
}

// 3. 更新操作のベンチマーク
function benchmarkUpdate(db: Database.Database) {
  console.log('\n=== 更新操作ベンチマーク ===');

  for (const size of [100, 1000]) {
    console.log(`\nデータ件数: ${size}`);
    const updateDb = setupDatabase();
    bulkCreateMedia(updateDb, generateMediaData(size));

    // 単一更新
    measureTime(() => {
      for (let i = 1; i <= 100; i++) {
        updateMedia(updateDb, i, {
          description: `更新されたデータ ${i}`
        });
      }
    }, '単一更新 (100件)');

    // トランザクション付き更新
    measureTime(() => {
      const update = updateDb.prepare(`
        UPDATE media SET description = ? WHERE id = ?
      `);
      const updateMany = updateDb.transaction((count: number) => {
        for (let i = 1; i <= count; i++) {
          update.run(`一括更新 ${i}`, i);
        }
      });
      updateMany(100);
    }, 'トランザクション付き更新 (100件)');

    updateDb.close();
  }
}

// 4. 削除操作のベンチマーク
function benchmarkDelete(db: Database.Database) {
  console.log('\n=== 削除操作ベンチマーク ===');

  for (const size of [100, 1000]) {
    console.log(`\nデータ件数: ${size}`);
    const deleteDb = setupDatabase();
    bulkCreateMedia(deleteDb, generateMediaData(size));

    // 単一削除
    measureTime(() => {
      for (let i = 1; i <= 100; i++) {
        deleteMedia(deleteDb, i);
      }
    }, '単一削除 (100件)');

    deleteDb.close();
  }
}

// メイン実行
function main() {
  console.log('きじゅくDB パフォーマンスベンチマーク');
  console.log('=====================================');

  const db = setupDatabase();

  try {
    benchmarkBulkInsert(db);
    benchmarkSearch(db);
    benchmarkUpdate(db);
    benchmarkDelete(db);

    console.log('\n=== ベンチマーク完了 ===\n');
  } finally {
    db.close();
  }
}

main();
