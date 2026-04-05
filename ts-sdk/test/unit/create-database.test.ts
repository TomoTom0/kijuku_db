/**
 * createDatabase() 関数の単体テスト
 */
import { describe, it, expect, vi, beforeEach, Mock } from 'vitest';
import { createDatabase } from '../../src/cli.js';
import { KijukuDB, RemoteKijukuDB } from '../../src/index.js';

// KijukuDB と RemoteKijukuDB をモック化
vi.mock('../../src/index.js', () => {
  return {
    KijukuDB: vi.fn(),
    RemoteKijukuDB: vi.fn(),
  };
});

describe('createDatabase', () => {
  beforeEach(() => {
    // 各テストの前にモックをリセット
    vi.clearAllMocks();
  });

  describe('ローカルDB作成', () => {
    it('絶対パスでKijukuDBインスタンスを作成する', () => {
      const dbPath = '/home/user/kijuku.db';
      createDatabase(dbPath);

      expect(KijukuDB).toHaveBeenCalledTimes(1);
      expect(KijukuDB).toHaveBeenCalledWith(dbPath, { verbose: false });
      expect(RemoteKijukuDB).not.toHaveBeenCalled();
    });

    it('相対パスでKijukuDBインスタンスを作成する', () => {
      const dbPath = './kijuku.db';
      createDatabase(dbPath);

      expect(KijukuDB).toHaveBeenCalledTimes(1);
      expect(KijukuDB).toHaveBeenCalledWith(dbPath, { verbose: false });
      expect(RemoteKijukuDB).not.toHaveBeenCalled();
    });

    it('カレントディレクトリのファイルでKijukuDBインスタンスを作成する', () => {
      const dbPath = 'kijuku.db';
      createDatabase(dbPath);

      expect(KijukuDB).toHaveBeenCalledTimes(1);
      expect(KijukuDB).toHaveBeenCalledWith(dbPath, { verbose: false });
      expect(RemoteKijukuDB).not.toHaveBeenCalled();
    });

    it('WindowsドライブレターのパスでKijukuDBインスタンスを作成する', () => {
      const dbPath = 'C:\\Users\\user\\kijuku.db';
      createDatabase(dbPath);

      expect(KijukuDB).toHaveBeenCalledTimes(1);
      expect(KijukuDB).toHaveBeenCalledWith(dbPath, { verbose: false });
      expect(RemoteKijukuDB).not.toHaveBeenCalled();
    });

    it('スラッシュ区切りのWindowsパスでKijukuDBインスタンスを作成する', () => {
      const dbPath = 'C:/Users/user/kijuku.db';
      createDatabase(dbPath);

      expect(KijukuDB).toHaveBeenCalledTimes(1);
      expect(KijukuDB).toHaveBeenCalledWith(dbPath, { verbose: false });
      expect(RemoteKijukuDB).not.toHaveBeenCalled();
    });

    it('日本語を含むパスでKijukuDBインスタンスを作成する', () => {
      const dbPath = './データベース/きじゅく.db';
      createDatabase(dbPath);

      expect(KijukuDB).toHaveBeenCalledTimes(1);
      expect(KijukuDB).toHaveBeenCalledWith(dbPath, { verbose: false });
      expect(RemoteKijukuDB).not.toHaveBeenCalled();
    });
  });

  describe('リモートDB作成', () => {
    it('標準的なリモートパスでRemoteKijukuDBインスタンスを作成する', () => {
      const dbPath = 'as5202:/home/user/kijuku.db';
      createDatabase(dbPath);

      expect(RemoteKijukuDB).toHaveBeenCalledTimes(1);
      expect(RemoteKijukuDB).toHaveBeenCalledWith({
        sshHost: 'as5202',
        dbPath: '/home/user/kijuku.db',
      });
      expect(KijukuDB).not.toHaveBeenCalled();
    });

    it('チルダを含むリモートパスでRemoteKijukuDBインスタンスを作成する', () => {
      const dbPath = 'server:~/.local/share/kijuku/kijuku.db';
      createDatabase(dbPath);

      expect(RemoteKijukuDB).toHaveBeenCalledTimes(1);
      expect(RemoteKijukuDB).toHaveBeenCalledWith({
        sshHost: 'server',
        dbPath: '~/.local/share/kijuku/kijuku.db',
      });
      expect(KijukuDB).not.toHaveBeenCalled();
    });

    it('相対パスを含むリモートパスでRemoteKijukuDBインスタンスを作成する', () => {
      const dbPath = 'myhost:./data/kijuku.db';
      createDatabase(dbPath);

      expect(RemoteKijukuDB).toHaveBeenCalledTimes(1);
      expect(RemoteKijukuDB).toHaveBeenCalledWith({
        sshHost: 'myhost',
        dbPath: './data/kijuku.db',
      });
      expect(KijukuDB).not.toHaveBeenCalled();
    });

    it('ハイフンを含むホスト名でRemoteKijukuDBインスタンスを作成する', () => {
      const dbPath = 'my-server:/var/lib/kijuku.db';
      createDatabase(dbPath);

      expect(RemoteKijukuDB).toHaveBeenCalledTimes(1);
      expect(RemoteKijukuDB).toHaveBeenCalledWith({
        sshHost: 'my-server',
        dbPath: '/var/lib/kijuku.db',
      });
      expect(KijukuDB).not.toHaveBeenCalled();
    });

    it('アンダースコアと数字を含むホスト名でRemoteKijukuDBインスタンスを作成する', () => {
      const dbPath = 'my_server_01:/path/to/db';
      createDatabase(dbPath);

      expect(RemoteKijukuDB).toHaveBeenCalledTimes(1);
      expect(RemoteKijukuDB).toHaveBeenCalledWith({
        sshHost: 'my_server_01',
        dbPath: '/path/to/db',
      });
      expect(KijukuDB).not.toHaveBeenCalled();
    });

    it('日本語を含むリモートパスでRemoteKijukuDBインスタンスを作成する', () => {
      const dbPath = 'server:/home/データベース/きじゅく.db';
      createDatabase(dbPath);

      expect(RemoteKijukuDB).toHaveBeenCalledTimes(1);
      expect(RemoteKijukuDB).toHaveBeenCalledWith({
        sshHost: 'server',
        dbPath: '/home/データベース/きじゅく.db',
      });
      expect(KijukuDB).not.toHaveBeenCalled();
    });
  });

  describe('境界値テスト', () => {
    it('2文字のホスト名でリモートDBを作成する', () => {
      const dbPath = 'ab:/path/to/db';
      createDatabase(dbPath);

      expect(RemoteKijukuDB).toHaveBeenCalledTimes(1);
      expect(RemoteKijukuDB).toHaveBeenCalledWith({
        sshHost: 'ab',
        dbPath: '/path/to/db',
      });
      expect(KijukuDB).not.toHaveBeenCalled();
    });

    it('空文字列でローカルDBを作成する', () => {
      const dbPath = '';
      createDatabase(dbPath);

      expect(KijukuDB).toHaveBeenCalledTimes(1);
      expect(KijukuDB).toHaveBeenCalledWith('', { verbose: false });
      expect(RemoteKijukuDB).not.toHaveBeenCalled();
    });
  });

  describe('戻り値の確認', () => {
    it('ローカルDB作成時にKijukuDBインスタンスを返す', () => {
      const mockInstance = { close: vi.fn() };
      (KijukuDB as unknown as Mock).mockReturnValue(mockInstance);

      const result = createDatabase('./kijuku.db');

      expect(result).toBe(mockInstance);
    });

    it('リモートDB作成時にRemoteKijukuDBインスタンスを返す', () => {
      const mockInstance = { disconnect: vi.fn() };
      (RemoteKijukuDB as unknown as Mock).mockReturnValue(mockInstance);

      const result = createDatabase('server:/path/to/db');

      expect(result).toBe(mockInstance);
    });
  });
});
