/**
 * createDatabase() 関数の単体テスト
 *
 * dbPath の local/remote 判定（parseDbPath）に基づき KijukuDB または RemoteKijukuDB を
 * 生成するかを検証。パス解析（host:path 判定・Windows ドライブレター・各種ホスト名形式）の
 * 網羅は parse-db-path.test.ts が担当。ここでは createDatabase の型マッピングを検証する。
 * RemoteKijukuDB は new のみで SSH 接続しない（接続は明示的 connect 呼び出し時）。
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createDatabase, KijukuDB, RemoteKijukuDB } from '../../src/index.js';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

describe('createDatabase', () => {
  let dir: string;

  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kijuku-create-'));
  });

  afterEach(() => {
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it('ローカルパスは KijukuDB インスタンスを返す', () => {
    const db = createDatabase(path.join(dir, 'kijuku.db'));
    expect(db).toBeInstanceOf(KijukuDB);
    expect(db).not.toBeInstanceOf(RemoteKijukuDB);
    (db as KijukuDB).close();
  });

  it('host:path 形式は RemoteKijukuDB インスタンスを返す（new のみ・SSH 接続しない）', () => {
    const db = createDatabase('as5202:/home/user/kijuku.db');
    expect(db).toBeInstanceOf(RemoteKijukuDB);
    expect(db).not.toBeInstanceOf(KijukuDB);
  });

  it('verbose オプションを指定してローカル KijukuDB を作成', () => {
    const db = createDatabase(path.join(dir, 'kijuku.db'), true);
    expect(db).toBeInstanceOf(KijukuDB);
    (db as KijukuDB).close();
  });
});
