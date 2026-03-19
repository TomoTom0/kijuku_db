/**
 * マイグレーション機能のテスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB } from '../../src/index.js';
import fs from 'fs';
import path from 'path';

describe('Migration', () => {
  let testDbPath: string;

  beforeEach(() => {
    // 各テストで一時的なDBファイルを作成
    testDbPath = path.join('/tmp', `test-kijuku-${Date.now()}.db`);
  });

  afterEach(() => {
    // テスト後にDBファイルを削除
    if (fs.existsSync(testDbPath)) {
      fs.unlinkSync(testDbPath);
    }
    // WALファイルも削除
    const walPath = `${testDbPath}-wal`;
    const shmPath = `${testDbPath}-shm`;
    if (fs.existsSync(walPath)) fs.unlinkSync(walPath);
    if (fs.existsSync(shmPath)) fs.unlinkSync(shmPath);
  });

  test('新しいDBでマイグレーションを実行できる', () => {
    const db = new KijukuDB(testDbPath);

    // マイグレーションを実行
    db.migrate();

    // スキーマバージョンが3であることを確認
    const version = db.getSchemaVersion();
    expect(version).toBe(5);

    db.close();
  });

  test('マイグレーション後にテーブルが作成される', () => {
    const db = new KijukuDB(testDbPath);
    db.migrate();

    // テーブルの存在を確認するヘルパーメソッドを使用
    // （実装時に追加予定）
    const tables = db.getTables();

    expect(tables).toContain('schema_version');
    expect(tables).toContain('media');
    expect(tables).toContain('tags');
    expect(tables).toContain('media_tags');
    expect(tables).toContain('media_attributes');

    db.close();
  });

  test('マイグレーション済みのDBで再度実行してもエラーにならない', () => {
    const db = new KijukuDB(testDbPath);

    // 1回目のマイグレーション
    db.migrate();
    const version1 = db.getSchemaVersion();

    // 2回目のマイグレーション（冪等性の確認）
    db.migrate();
    const version2 = db.getSchemaVersion();

    // バージョンが変わらないことを確認
    expect(version1).toBe(version2);
    expect(version2).toBe(5);

    db.close();
  });

  test('インメモリDBでマイグレーションを実行できる', () => {
    const db = new KijukuDB(':memory:');

    db.migrate();

    const version = db.getSchemaVersion();
    expect(version).toBe(5);

    db.close();
  });

  test('マイグレーション前にgetSchemaVersionを呼ぶと0を返す', () => {
    const db = new KijukuDB(testDbPath);

    const version = db.getSchemaVersion();
    expect(version).toBe(0);

    db.close();
  });

  test('外部キー制約が有効化されている', () => {
    const db = new KijukuDB(testDbPath);
    db.migrate();

    // 外部キー制約の確認
    const fkEnabled = db.isForeignKeysEnabled();
    expect(fkEnabled).toBe(true);

    db.close();
  });
});
