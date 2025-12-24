/**
 * CLI E2Eテスト（リモートDB - SSH接続）
 * 実際のCLIバイナリを子プロセスで実行してテスト
 *
 * 環境変数:
 * - TEST_SSH_HOST: SSHホスト名（~/.ssh/configに設定されている必要がある）
 * - TEST_REMOTE_DB_PATH: リモートのDBパス（省略時: /tmp/kijuku-test-{timestamp}.db）
 *
 * 実行方法:
 * TEST_SSH_HOST=as5202 bun test test/e2e/cli-remote.test.ts
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from 'vitest';
import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';

const CLI_PATH = path.join(process.cwd(), 'dist', 'cli.js');
const TEST_DIR = path.join(process.cwd(), 'test', 'e2e', 'fixtures');

const SSH_HOST = process.env.TEST_SSH_HOST;
const REMOTE_DB_PATH = process.env.TEST_REMOTE_DB_PATH || `/tmp/kijuku-test-${Date.now()}.db`;

// SSH_HOSTが設定されていない場合はテストをスキップ
const describeRemote = SSH_HOST ? describe : describe.skip;

/**
 * CLIコマンドを実行
 */
function runCli(args: string[]): {
  stdout: string;
  stderr: string;
  exitCode: number;
} {
  try {
    const result = execSync(`node ${CLI_PATH} ${args.join(' ')}`, {
      encoding: 'utf-8',
      cwd: process.cwd(),
      env: { ...process.env },
    });
    return {
      stdout: result,
      stderr: '',
      exitCode: 0,
    };
  } catch (error: any) {
    return {
      stdout: error.stdout?.toString() || '',
      stderr: error.stderr?.toString() || '',
      exitCode: error.status || 1,
    };
  }
}

/**
 * リモートDBのパス指定（host:path形式）
 */
function getRemoteDbPath(): string {
  return `${SSH_HOST}:${REMOTE_DB_PATH}`;
}

describeRemote('CLI E2E Tests (Remote)', () => {
  const remoteDbPath = getRemoteDbPath();

  beforeAll(() => {
    if (!SSH_HOST) {
      throw new Error('TEST_SSH_HOST環境変数が設定されていません');
    }

    console.log(`SSH接続先: ${SSH_HOST}`);
    console.log(`リモートDB: ${REMOTE_DB_PATH}`);
    console.log(`DB指定: ${remoteDbPath}`);

    // テストディレクトリを作成
    if (!fs.existsSync(TEST_DIR)) {
      fs.mkdirSync(TEST_DIR, { recursive: true });
    }
  });

  afterAll(async () => {
    // テスト用リモートDBを削除（クリーンアップ）
    if (SSH_HOST) {
      try {
        const { Client } = await import('ssh2');
        const SSHConfig = (await import('ssh-config')).default;
        const { readFileSync } = await import('fs');
        const { homedir } = await import('os');
        const { resolve } = await import('path');

        const configPath = resolve(homedir(), '.ssh', 'config');
        const configContent = readFileSync(configPath, 'utf-8');
        const config = SSHConfig.parse(configContent);

        const hostConfig = config.compute(SSH_HOST);
        const conn = new Client();

        await new Promise<void>((resolve, reject) => {
          conn
            .on('ready', () => {
              conn.exec(`rm -f ${REMOTE_DB_PATH}`, (err, stream) => {
                if (err) {
                  reject(err);
                  return;
                }

                stream
                  .on('close', () => {
                    conn.end();
                    resolve();
                  })
                  .on('error', reject);
              });
            })
            .on('error', reject)
            .connect({
              host: hostConfig.HostName || SSH_HOST,
              port: hostConfig.Port ? parseInt(hostConfig.Port, 10) : 22,
              username: hostConfig.User || process.env.USER,
              privateKey: hostConfig.IdentityFile ? readFileSync(resolve(homedir(), '.ssh', hostConfig.IdentityFile)) : undefined,
            });
        });

        console.log(`リモートDBを削除しました: ${REMOTE_DB_PATH}`);
      } catch (error) {
        console.error('リモートDBの削除に失敗:', error);
      }
    }
  });

  describe('migrateコマンド', () => {
    it('リモートDBを初期化できる', () => {
      const result = runCli(['migrate', '--db', remoteDbPath]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('マイグレーション');
      expect(result.stdout).toContain('完了');
    }, 30000); // タイムアウトを30秒に設定

    it('既存のリモートDBでもマイグレーションできる', () => {
      // 2回目のマイグレーション
      const result = runCli(['migrate', '--db', remoteDbPath]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('完了');
    }, 30000);
  });

  describe('importコマンド', () => {
    beforeEach(async () => {
      // マイグレーション実行（各テストの前に実行）
      runCli(['migrate', '--db', remoteDbPath]);
    }, 30000);

    it('JSONファイルからリモートDBにインポートできる', () => {
      // テストJSONファイルを作成
      const jsonFile = path.join(TEST_DIR, 'test-remote.json');
      const testData = [
        {
          title: 'リモートテストコミック1',
          media_type: 'comic',
          artist: 'リモート作者1',
        },
        {
          title: 'リモートテストコミック2',
          media_type: 'comic',
          artist: 'リモート作者2',
        },
      ];
      fs.writeFileSync(jsonFile, JSON.stringify(testData));

      const result = runCli(['import', '--file', jsonFile, '--db', remoteDbPath]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('インポート');
      expect(result.stdout).toContain('2件');

      // クリーンアップ
      fs.unlinkSync(jsonFile);
    }, 60000); // タイムアウトを60秒に設定

    it('CSVファイルからリモートDBにインポートできる', () => {
      // テストCSVファイルを作成
      const csvFile = path.join(TEST_DIR, 'test-remote.csv');
      const csvData = `title,media_type,artist
リモートコミック1,comic,リモート作者1
リモートコミック2,comic,リモート作者2`;
      fs.writeFileSync(csvFile, csvData);

      const result = runCli(['import', '--file', csvFile, '--db', remoteDbPath]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('インポート');
      expect(result.stdout).toContain('2件');

      // クリーンアップ
      fs.unlinkSync(csvFile);
    }, 60000);

    it('追加カラムを指定してリモートDBにインポートできる', () => {
      // 追加カラムを含むJSONファイルを作成
      const jsonFile = path.join(TEST_DIR, 'test-remote-extra.json');
      const testData = [
        {
          title: 'リモートコミック',
          media_type: 'comic',
          artist: 'リモート作者',
          custom_field: 'カスタム値',
          id_old: '98765',
        },
      ];
      fs.writeFileSync(jsonFile, JSON.stringify(testData));

      const result = runCli([
        'import',
        '--file',
        jsonFile,
        '--db',
        remoteDbPath,
        '--additional-columns',
        'custom_field,id_old',
      ]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('インポート');
      expect(result.stdout).toContain('2件の追加属性');

      // クリーンアップ
      fs.unlinkSync(jsonFile);
    }, 60000);
  });

  describe('searchコマンド', () => {
    beforeAll(() => {
      // マイグレーション実行
      runCli(['migrate', '--db', remoteDbPath]);

      // テストデータをインポート
      const jsonFile = path.join(TEST_DIR, 'test-remote-data.json');
      const testData = [
        {
          title: 'リモートドラゴンボール',
          media_type: 'comic',
          artist: '鳥山明',
          series: 'リモートドラゴンボール',
        },
        {
          title: 'リモートワンピース',
          media_type: 'comic',
          artist: '尾田栄一郎',
          series: 'リモートワンピース',
        },
      ];
      fs.writeFileSync(jsonFile, JSON.stringify(testData));
      runCli(['import', '--file', jsonFile, '--db', remoteDbPath]);
      fs.unlinkSync(jsonFile);
    }, 60000);

    it('リモートDBでタイトル検索ができる', () => {
      const result = runCli(['search', '--title', 'リモートドラゴンボール', '--db', remoteDbPath]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('検索結果');
      expect(result.stdout).toContain('リモートドラゴンボール');
    }, 30000);

    it('リモートDBで作者検索ができる', () => {
      const result = runCli(['search', '--artist', '尾田栄一郎', '--db', remoteDbPath]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('検索結果');
      expect(result.stdout).toContain('リモートワンピース');
    }, 30000);

    it('リモートDBでタイプ検索ができる', () => {
      const result = runCli(['search', '--type', 'comic', '--db', remoteDbPath]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('検索結果');
      expect(result.stdout).toContain('2件');
    }, 30000);

    it('リモートDBでlimitオプションが機能する', () => {
      const result = runCli(['search', '--type', 'comic', '--limit', '1', '--db', remoteDbPath]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('検索結果');
      expect(result.stdout).toContain('1件');
    }, 30000);
  });

  describe('エラーケース', () => {
    it('無効なSSHホストの場合はエラーになる', () => {
      const invalidDbPath = 'invalid-host-9999:/tmp/test.db';
      const result = runCli(['migrate', '--db', invalidDbPath]);

      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain('エラー');
    }, 30000);
  });
});
