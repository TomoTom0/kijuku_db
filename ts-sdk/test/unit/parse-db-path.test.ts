/**
 * parseDbPath() 関数の単体テスト
 */
import { describe, it, expect } from 'vitest';
import { parseDbPath } from '../../src/config.js';

describe('parseDbPath', () => {
  describe('リモートパス（host:path形式）', () => {
    it('標準的なリモートパスをパースできる', () => {
      const result = parseDbPath('as5202:/home/user/kijuku.db');
      expect(result.isRemote).toBe(true);
      expect(result.sshHost).toBe('as5202');
      expect(result.remotePath).toBe('/home/user/kijuku.db');
      expect(result.localPath).toBeUndefined();
    });

    it('チルダを含むリモートパスをパースできる', () => {
      const result = parseDbPath('server:~/.local/share/kijuku/kijuku.db');
      expect(result.isRemote).toBe(true);
      expect(result.sshHost).toBe('server');
      expect(result.remotePath).toBe('~/.local/share/kijuku/kijuku.db');
    });

    it('相対パスを含むリモートパスをパースできる', () => {
      const result = parseDbPath('myhost:./data/kijuku.db');
      expect(result.isRemote).toBe(true);
      expect(result.sshHost).toBe('myhost');
      expect(result.remotePath).toBe('./data/kijuku.db');
    });

    it('ハイフンやアンダースコアを含むホスト名を処理できる', () => {
      const result = parseDbPath('my-server_01:/var/lib/kijuku.db');
      expect(result.isRemote).toBe(true);
      expect(result.sshHost).toBe('my-server_01');
      expect(result.remotePath).toBe('/var/lib/kijuku.db');
    });

    it('数字を含むホスト名を処理できる', () => {
      const result = parseDbPath('server123:/path/to/db');
      expect(result.isRemote).toBe(true);
      expect(result.sshHost).toBe('server123');
      expect(result.remotePath).toBe('/path/to/db');
    });
  });

  describe('ローカルパス', () => {
    it('絶対パス（Unix）をパースできる', () => {
      const result = parseDbPath('/home/user/kijuku.db');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe('/home/user/kijuku.db');
      expect(result.sshHost).toBeUndefined();
      expect(result.remotePath).toBeUndefined();
    });

    it('相対パスをパースできる', () => {
      const result = parseDbPath('./kijuku.db');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe('./kijuku.db');
    });

    it('カレントディレクトリのファイルをパースできる', () => {
      const result = parseDbPath('kijuku.db');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe('kijuku.db');
    });

    it('親ディレクトリの参照を含むパスをパースできる', () => {
      const result = parseDbPath('../data/kijuku.db');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe('../data/kijuku.db');
    });
  });

  describe('Windowsドライブレター', () => {
    it('C:で始まるパスをローカルパスとして扱う', () => {
      const result = parseDbPath('C:\\Users\\user\\kijuku.db');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe('C:\\Users\\user\\kijuku.db');
    });

    it('D:で始まるパスをローカルパスとして扱う', () => {
      const result = parseDbPath('D:\\data\\kijuku.db');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe('D:\\data\\kijuku.db');
    });

    it('C:/スラッシュ区切りのパスをローカルパスとして扱う', () => {
      const result = parseDbPath('C:/Users/user/kijuku.db');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe('C:/Users/user/kijuku.db');
    });
  });

  describe('エッジケース', () => {
    it('空文字列を処理できる', () => {
      const result = parseDbPath('');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe('');
    });

    it('コロンのみを含むパスを処理できる', () => {
      const result = parseDbPath(':');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe(':');
    });

    it('複数のコロンを含むパスを処理できる（最初のコロンで分割）', () => {
      const result = parseDbPath('server:/path/with:colon/file.db');
      expect(result.isRemote).toBe(true);
      expect(result.sshHost).toBe('server');
      expect(result.remotePath).toBe('/path/with:colon/file.db');
    });

    it('スペースを含むパスを処理できる', () => {
      const result = parseDbPath('/home/user/My Documents/kijuku.db');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe('/home/user/My Documents/kijuku.db');
    });

    it('スペースを含むリモートパスを処理できる', () => {
      const result = parseDbPath('server:/home/user/My Documents/kijuku.db');
      expect(result.isRemote).toBe(true);
      expect(result.sshHost).toBe('server');
      expect(result.remotePath).toBe('/home/user/My Documents/kijuku.db');
    });

    it('特殊文字を含むパスを処理できる', () => {
      const result = parseDbPath('./データベース/きじゅく.db');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe('./データベース/きじゅく.db');
    });

    it('特殊文字を含むリモートパスを処理できる', () => {
      const result = parseDbPath('server:/home/データベース/きじゅく.db');
      expect(result.isRemote).toBe(true);
      expect(result.sshHost).toBe('server');
      expect(result.remotePath).toBe('/home/データベース/きじゅく.db');
    });

    it('URLのようなパスを処理できる（プロトコル部分は2文字以上なのでリモート扱い）', () => {
      const result = parseDbPath('http://example.com/kijuku.db');
      expect(result.isRemote).toBe(true);
      expect(result.sshHost).toBe('http');
      expect(result.remotePath).toBe('//example.com/kijuku.db');
    });
  });

  describe('境界値テスト', () => {
    it('1文字のホスト名はローカルパスとして扱われる（Windowsドライブレター対策）', () => {
      const result = parseDbPath('C:/path/to/db');
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe('C:/path/to/db');
    });

    it('2文字のホスト名はリモートパスとして扱われる', () => {
      const result = parseDbPath('ab:/path/to/db');
      expect(result.isRemote).toBe(true);
      expect(result.sshHost).toBe('ab');
      expect(result.remotePath).toBe('/path/to/db');
    });

    it('非常に長いホスト名を処理できる', () => {
      const longHost = 'a'.repeat(255);
      const result = parseDbPath(`${longHost}:/path/to/db`);
      expect(result.isRemote).toBe(true);
      expect(result.sshHost).toBe(longHost);
      expect(result.remotePath).toBe('/path/to/db');
    });

    it('非常に長いパスを処理できる', () => {
      const longPath = '/home/' + 'a'.repeat(1000) + '/kijuku.db';
      const result = parseDbPath(longPath);
      expect(result.isRemote).toBe(false);
      expect(result.localPath).toBe(longPath);
    });
  });
});
