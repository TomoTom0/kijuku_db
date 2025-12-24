/**
 * CLI E2Eテスト（ローカルDB）
 * 実際のCLIバイナリを子プロセスで実行してテスト
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { execSync, spawn } from 'child_process';
import fs from 'fs';
import path from 'path';
import { promisify } from 'util';

const CLI_PATH = path.join(process.cwd(), 'dist', 'cli.js');
const TEST_DIR = path.join(process.cwd(), 'test', 'e2e', 'fixtures');
const TEST_DB = path.join(TEST_DIR, 'test.db');

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

describe('CLI E2E Tests (Local)', () => {
  beforeEach(() => {
    // テストディレクトリを作成
    if (!fs.existsSync(TEST_DIR)) {
      fs.mkdirSync(TEST_DIR, { recursive: true });
    }
    // 既存のテストDBを削除
    if (fs.existsSync(TEST_DB)) {
      fs.unlinkSync(TEST_DB);
    }
  });

  afterEach(() => {
    // テストDBを削除
    if (fs.existsSync(TEST_DB)) {
      fs.unlinkSync(TEST_DB);
    }
  });

  describe('helpコマンド', () => {
    it('ヘルプメッセージを表示できる', () => {
      const result = runCli(['help']);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('Kijuku DB CLI');
      expect(result.stdout).toContain('使い方:');
      expect(result.stdout).toContain('コマンド:');
      expect(result.stdout).toContain('migrate');
      expect(result.stdout).toContain('search');
      expect(result.stdout).toContain('import');
    });

    it('引数なしでヘルプを表示する', () => {
      const result = runCli([]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('Kijuku DB CLI');
    });
  });

  describe('migrateコマンド', () => {
    it('データベースを初期化できる', () => {
      const result = runCli(['migrate', '--db', TEST_DB]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('マイグレーション');
      expect(result.stdout).toContain('完了');
      expect(fs.existsSync(TEST_DB)).toBe(true);
    });

    it('既存のデータベースでもマイグレーションできる', () => {
      // 1回目のマイグレーション
      runCli(['migrate', '--db', TEST_DB]);

      // 2回目のマイグレーション
      const result = runCli(['migrate', '--db', TEST_DB]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('完了');
    });
  });

  describe('importコマンド', () => {
    beforeEach(() => {
      // マイグレーション実行
      runCli(['migrate', '--db', TEST_DB]);
    });

    it('JSONファイルからインポートできる', () => {
      // テストJSONファイルを作成
      const jsonFile = path.join(TEST_DIR, 'test.json');
      const testData = [
        {
          title: 'テストコミック1',
          media_type: 'comic',
          artist: 'テスト作者',
        },
        {
          title: 'テストコミック2',
          media_type: 'comic',
          artist: 'テスト作者2',
        },
      ];
      fs.writeFileSync(jsonFile, JSON.stringify(testData));

      const result = runCli(['import', '--file', jsonFile, '--db', TEST_DB]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('インポート');
      expect(result.stdout).toContain('2件');

      // クリーンアップ
      fs.unlinkSync(jsonFile);
    });

    it('CSVファイルからインポートできる', () => {
      // テストCSVファイルを作成
      const csvFile = path.join(TEST_DIR, 'test.csv');
      const csvData = `title,media_type,artist
テストコミック1,comic,テスト作者1
テストコミック2,comic,テスト作者2`;
      fs.writeFileSync(csvFile, csvData);

      const result = runCli(['import', '--file', csvFile, '--db', TEST_DB]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('インポート');
      expect(result.stdout).toContain('2件');

      // クリーンアップ
      fs.unlinkSync(csvFile);
    });

    it('TSVファイルからインポートできる', () => {
      // テストTSVファイルを作成
      const tsvFile = path.join(TEST_DIR, 'test.tsv');
      const tsvData = `title\tmedia_type\tartist
テストコミック1\tcomic\tテスト作者1
テストコミック2\tcomic\tテスト作者2`;
      fs.writeFileSync(tsvFile, tsvData);

      const result = runCli(['import', '--file', tsvFile, '--db', TEST_DB]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('インポート');
      expect(result.stdout).toContain('2件');

      // クリーンアップ
      fs.unlinkSync(tsvFile);
    });

    it('追加カラムを指定してインポートできる', () => {
      // 追加カラムを含むJSONファイルを作成
      const jsonFile = path.join(TEST_DIR, 'test-with-extra.json');
      const testData = [
        {
          title: 'テストコミック1',
          media_type: 'comic',
          artist: 'テスト作者',
          custom_field: 'カスタム値',
          id_old: '12345',
        },
      ];
      fs.writeFileSync(jsonFile, JSON.stringify(testData));

      const result = runCli([
        'import',
        '--file',
        jsonFile,
        '--db',
        TEST_DB,
        '--additional-columns',
        'custom_field,id_old',
      ]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('インポート');
      expect(result.stdout).toContain('2件の追加属性');

      // クリーンアップ
      fs.unlinkSync(jsonFile);
    });

    it('ファイルが存在しない場合はエラーになる', () => {
      const result = runCli([
        'import',
        '--file',
        'non-existent-file.json',
        '--db',
        TEST_DB,
      ]);

      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain('エラー');
      expect(result.stderr).toContain('見つかりません');
    });

    it('--fileオプションがない場合はエラーになる', () => {
      const result = runCli(['import', '--db', TEST_DB]);

      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain('エラー');
      expect(result.stderr).toContain('--file');
    });

    it('サポートされていないファイル形式の場合はエラーになる', () => {
      // テキストファイルを作成
      const txtFile = path.join(TEST_DIR, 'test.txt');
      fs.writeFileSync(txtFile, 'テストデータ');

      const result = runCli(['import', '--file', txtFile, '--db', TEST_DB]);

      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain('エラー');
      expect(result.stderr).toContain('サポートされていない');

      // クリーンアップ
      fs.unlinkSync(txtFile);
    });
  });

  describe('searchコマンド', () => {
    beforeEach(() => {
      // マイグレーション実行
      runCli(['migrate', '--db', TEST_DB]);

      // テストデータをインポート
      const jsonFile = path.join(TEST_DIR, 'test-data.json');
      const testData = [
        {
          title: 'ドラゴンボール',
          media_type: 'comic',
          artist: '鳥山明',
          series: 'ドラゴンボール',
        },
        {
          title: 'ワンピース',
          media_type: 'comic',
          artist: '尾田栄一郎',
          series: 'ワンピース',
        },
        {
          title: 'NARUTO',
          media_type: 'comic',
          artist: '岸本斉史',
        },
      ];
      fs.writeFileSync(jsonFile, JSON.stringify(testData));
      runCli(['import', '--file', jsonFile, '--db', TEST_DB]);
      fs.unlinkSync(jsonFile);
    });

    it('タイトルで検索できる', () => {
      const result = runCli(['search', '--title', 'ドラゴンボール', '--db', TEST_DB]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('検索結果');
      expect(result.stdout).toContain('ドラゴンボール');
    });

    it('作者で検索できる', () => {
      const result = runCli(['search', '--artist', '尾田栄一郎', '--db', TEST_DB]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('検索結果');
      expect(result.stdout).toContain('ワンピース');
    });

    it('シリーズで検索できる', () => {
      const result = runCli(['search', '--series', 'ドラゴンボール', '--db', TEST_DB]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('検索結果');
      expect(result.stdout).toContain('ドラゴンボール');
    });

    it('タイプで検索できる', () => {
      const result = runCli(['search', '--type', 'comic', '--db', TEST_DB]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('検索結果');
      expect(result.stdout).toContain('3件');
    });

    it('limitオプションが機能する', () => {
      const result = runCli(['search', '--type', 'comic', '--limit', '2', '--db', TEST_DB]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('検索結果');
      expect(result.stdout).toContain('2件');
    });

    it('条件に一致しない場合は0件を返す', () => {
      const result = runCli(['search', '--title', '存在しない作品', '--db', TEST_DB]);

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('0件');
    });
  });

  describe('エラーケース', () => {
    it('不明なコマンドの場合はエラーになる', () => {
      const result = runCli(['unknown-command']);

      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain('エラー');
      expect(result.stderr).toContain('不明なコマンド');
    });
  });
});
