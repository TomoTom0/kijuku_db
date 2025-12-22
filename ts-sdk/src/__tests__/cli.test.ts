/**
 * CLIツールの統合テスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB } from '../index.js';
import fs from 'fs';
import path from 'path';

describe('CLI Integration', () => {
  let testDbPath: string;

  beforeEach(() => {
    testDbPath = path.join('/tmp', `test-cli-${Date.now()}.db`);
  });

  afterEach(() => {
    if (fs.existsSync(testDbPath)) {
      fs.unlinkSync(testDbPath);
    }
    const walPath = `${testDbPath}-wal`;
    const shmPath = `${testDbPath}-shm`;
    if (fs.existsSync(walPath)) fs.unlinkSync(walPath);
    if (fs.existsSync(shmPath)) fs.unlinkSync(shmPath);
  });

  test('migrateコマンドの機能: データベースを初期化できる', () => {
    const db = new KijukuDB(testDbPath);

    // マイグレーション実行
    db.migrate();

    // スキーマバージョンを確認
    const version = db.getSchemaVersion();
    expect(version).toBe(1);

    // テーブルが作成されていることを確認
    const tables = db.getTables();
    expect(tables).toContain('media');
    expect(tables).toContain('tags');
    expect(tables).toContain('media_tags');
    expect(tables).toContain('schema_version');

    db.close();
  });

  test('importコマンドの機能: JSONファイルからインポートできる', () => {
    const db = new KijukuDB(testDbPath);
    db.migrate();

    // テストデータ
    const testData = [
      {
        title: 'インポートテスト1',
        media_type: 'comic',
        artist: '作者A',
      },
      {
        title: 'インポートテスト2',
        media_type: 'video',
        artist: '作者B',
      },
    ];

    // bulkCreateMediaでインポート（CLIのimportコマンドと同じ処理）
    const results = db.bulkCreateMedia(testData);

    expect(results).toHaveLength(2);
    expect(results[0].title).toBe('インポートテスト1');
    expect(results[1].title).toBe('インポートテスト2');

    db.close();
  });

  test('searchコマンドの機能: メディアを検索できる', () => {
    const db = new KijukuDB(testDbPath);
    db.migrate();

    // テストデータを作成
    db.createMedia({ title: 'コミック1', media_type: 'comic', artist: '作者A' });
    db.createMedia({ title: 'コミック2', media_type: 'comic', artist: '作者B' });
    db.createMedia({ title: 'ビデオ1', media_type: 'video', artist: '作者A' });

    // 検索（CLIのsearchコマンドと同じ処理）
    const allResults = db.findMedia({});
    expect(allResults).toHaveLength(3);

    const comicResults = db.findMedia({ media_type: 'comic' });
    expect(comicResults).toHaveLength(2);

    const artistResults = db.findMedia({ artist: '作者A' });
    expect(artistResults).toHaveLength(2);

    db.close();
  });

  test('CLIワークフロー: migrate -> import -> searchの一連の流れ', () => {
    // 1. migrate
    const db = new KijukuDB(testDbPath);
    db.migrate();
    expect(db.getSchemaVersion()).toBe(1);

    // 2. import
    const importData = [
      { title: 'ワークフローテスト1', media_type: 'comic', artist: 'テスト作者' },
      { title: 'ワークフローテスト2', media_type: 'video', artist: 'テスト作者' },
      { title: 'ワークフローテスト3', media_type: 'music', artist: '別の作者' },
    ];
    const imported = db.bulkCreateMedia(importData);
    expect(imported).toHaveLength(3);

    // 3. search
    const allMedia = db.findMedia({});
    expect(allMedia).toHaveLength(3);

    const byArtist = db.findMedia({ artist: 'テスト作者' });
    expect(byArtist).toHaveLength(2);

    const byType = db.findMedia({ media_type: 'comic' });
    expect(byType).toHaveLength(1);
    expect(byType[0].title).toBe('ワークフローテスト1');

    db.close();
  });

  test('CLIのエラーハンドリング: 無効なデータでインポートに失敗する', () => {
    const db = new KijukuDB(testDbPath);
    db.migrate();

    const invalidData = [
      { title: '有効なデータ', media_type: 'comic' },
      { title: '', media_type: 'invalid' }, // 無効なデータ
    ];

    // バルクインポートはトランザクション内で実行されるため、
    // エラーが発生すると全てロールバックされる
    expect(() => db.bulkCreateMedia(invalidData as any)).toThrow();

    // ロールバックされているため、何も作成されていない
    const allMedia = db.findMedia({});
    expect(allMedia).toHaveLength(0);

    db.close();
  });
});
