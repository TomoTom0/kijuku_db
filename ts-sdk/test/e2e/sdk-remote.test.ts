/**
 * RemoteKijukuDB 統合テスト（実際のSSH接続）
 *
 * 環境変数:
 * - TEST_SSH_HOST: SSHホスト名（~/.ssh/configに設定されている必要がある）
 * - TEST_REMOTE_DB_PATH: リモートのDBパス（省略時: /tmp/kijuku-test-{timestamp}.db）
 *
 * 実行方法:
 * TEST_SSH_HOST=as5202 bun run test remote.integration.test.ts
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { RemoteKijukuDB } from '../../src/remote.js';
import type { Media } from '../../src/types.js';

const SSH_HOST = process.env.TEST_SSH_HOST;
const REMOTE_DB_PATH = process.env.TEST_REMOTE_DB_PATH || `/tmp/kijuku-test-${Date.now()}.db`;

// SSH_HOSTが設定されていない場合はテストをスキップ
const describeRemote = SSH_HOST ? describe : describe.skip;

describeRemote('RemoteKijukuDB 統合テスト', () => {
  let remoteDb: RemoteKijukuDB;
  let createdMediaIds: number[] = [];

  beforeAll(() => {
    if (!SSH_HOST) {
      throw new Error('TEST_SSH_HOST環境変数が設定されていません');
    }

    remoteDb = new RemoteKijukuDB({
      sshHost: SSH_HOST,
      dbPath: REMOTE_DB_PATH,
    });

    console.log(`SSH接続先: ${SSH_HOST}`);
    console.log(`リモートDB: ${REMOTE_DB_PATH}`);
  });

  afterAll(async () => {
    // プールされた SSH 接続を閉じる（TASK-77）。
    // 未呼び出しだと TCP ソケットがイベントループを保持しプロセスが終了しないため。
    await remoteDb.disconnect();

    // テスト用DBを削除（クリーンアップ）
    if (SSH_HOST) {
      const { Client } = await import('ssh2');
      const SSHConfig = (await import('ssh-config')).default;
      const { readFileSync } = await import('fs');
      const { homedir } = await import('os');
      const { resolve } = await import('path');

      const configPath = resolve(homedir(), '.ssh', 'config');
      const configContent = readFileSync(configPath, 'utf-8');
      const config = SSHConfig.parse(configContent);
      const hostConfig = config.compute(SSH_HOST);

      const client = new Client();

      await new Promise<void>((resolve, reject) => {
        client.on('ready', () => {
          client.exec(`rm -f ${REMOTE_DB_PATH}`, (err) => {
            client.end();
            if (err) reject(err);
            else resolve();
          });
        });

        client.on('error', reject);

        let identityFile = Array.isArray(hostConfig.IdentityFile)
          ? hostConfig.IdentityFile[0]
          : hostConfig.IdentityFile;

        if (typeof identityFile === 'string' && identityFile.startsWith('~/')) {
          identityFile = resolve(homedir(), identityFile.substring(2));
        }

        client.connect({
          host: hostConfig.HostName as string,
          port: parseInt((hostConfig.Port as string) || '22'),
          username: hostConfig.User as string,
          privateKey: readFileSync(identityFile as string),
        });
      });

      console.log(`リモートDBを削除しました: ${REMOTE_DB_PATH}`);
    }
  });

  describe('マイグレーション', () => {
    it('リモートDBをマイグレーションできる', async () => {
      await remoteDb.migrate();
      const version = await remoteDb.getSchemaVersion();
      expect(version).toBe(1);
    });
  });

  describe('メディア操作', () => {
    it('メディアを作成できる', async () => {
      const media = await remoteDb.createMedia({
        title: 'リモートテスト作品',
        media_type: 'comic',
        artist: 'テスト作者',
        description: 'SSH経由で作成されたテストデータ',
      });

      expect(media).toBeDefined();
      expect(media.id).toBeGreaterThan(0);
      expect(media.title).toBe('リモートテスト作品');
      expect(media.artist).toBe('テスト作者');

      createdMediaIds.push(media.id);
    });

    it('IDでメディアを取得できる', async () => {
      const mediaId = createdMediaIds[0];
      const media = await remoteDb.getMedia(mediaId);

      expect(media).toBeDefined();
      expect(media!.id).toBe(mediaId);
      expect(media!.title).toBe('リモートテスト作品');
    });

    it('メディアを更新できる', async () => {
      const mediaId = createdMediaIds[0];

      await remoteDb.updateMedia(mediaId, {
        title: '更新されたタイトル',
        description: '更新されたテストデータ',
      });

      const updated = await remoteDb.getMedia(mediaId);
      expect(updated!.title).toBe('更新されたタイトル');
      expect(updated!.description).toBe('更新されたテストデータ');
    });

    it('メディアを検索できる', async () => {
      const results = await remoteDb.findMedia({ media_type: 'comic' });

      expect(results).toBeDefined();
      expect(Array.isArray(results)).toBe(true);
      expect(results.length).toBeGreaterThan(0);
    });

    it('複数のメディアを一括作成できる', async () => {
      const results = await remoteDb.bulkCreateMedia([
        { title: 'バルク作品1', media_type: 'comic' },
        { title: 'バルク作品2', media_type: 'video' },
        { title: 'バルク作品3', media_type: 'music' },
      ]);

      expect(results).toBeDefined();
      expect(results.length).toBe(3);
      expect(results[0].title).toBe('バルク作品1');
      expect(results[1].title).toBe('バルク作品2');
      expect(results[2].title).toBe('バルク作品3');

      createdMediaIds.push(...results.map(m => m.id));
    });
  });

  describe('タグ操作', () => {
    it('タグを作成できる', async () => {
      const tag = await remoteDb.createTag('リモートタグ');

      expect(tag).toBeDefined();
      expect(tag.id).toBeGreaterThan(0);
      expect(tag.name).toBe('リモートタグ');
    });

    it('タグ名でタグを取得できる', async () => {
      const tag = await remoteDb.getTagByName('リモートタグ');

      expect(tag).toBeDefined();
      expect(tag!.name).toBe('リモートタグ');
    });

    it('全てのタグを取得できる', async () => {
      const tags = await remoteDb.getAllTags();

      expect(tags).toBeDefined();
      expect(Array.isArray(tags)).toBe(true);
      expect(tags.length).toBeGreaterThan(0);
    });

    it('メディアにタグを追加・削除できる', async () => {
      const mediaId = createdMediaIds[0];
      const tag = await remoteDb.getTagByName('リモートタグ');

      // タグを追加
      await remoteDb.addTagToMedia(mediaId, tag!.id);

      // タグを取得
      let mediaTags = await remoteDb.getMediaTags(mediaId);
      expect(mediaTags.length).toBeGreaterThan(0);
      expect(mediaTags.some(t => t.id === tag!.id)).toBe(true);

      // タグを削除
      await remoteDb.removeTagFromMedia(mediaId, tag!.id);
      mediaTags = await remoteDb.getMediaTags(mediaId);
      expect(mediaTags.some(t => t.id === tag!.id)).toBe(false);
    });
  });

  describe('属性操作', () => {
    it('メディアに属性を設定・取得できる', async () => {
      const mediaId = createdMediaIds[0];

      // 属性を設定
      await remoteDb.setMediaAttribute(mediaId, 'test_key', 'test_value');

      // 属性を取得
      const attr = await remoteDb.getMediaAttribute(mediaId, 'test_key');
      expect(attr).toBeDefined();
      expect(attr!.key).toBe('test_key');
      expect(attr!.value).toBe('test_value');
    });

    it('メディアの全属性を取得できる', async () => {
      const mediaId = createdMediaIds[0];

      // 複数の属性を設定
      await remoteDb.setMediaAttribute(mediaId, 'attr1', 'value1');
      await remoteDb.setMediaAttribute(mediaId, 'attr2', 'value2');

      // 全属性を取得
      const attrs = await remoteDb.getMediaAttributes(mediaId);
      expect(attrs.length).toBeGreaterThanOrEqual(2);
    });

    it('メディアの属性を削除できる', async () => {
      const mediaId = createdMediaIds[0];

      // 属性を削除
      await remoteDb.deleteMediaAttribute(mediaId, 'attr1');

      // 削除確認
      const attr = await remoteDb.getMediaAttribute(mediaId, 'attr1');
      expect(attr).toBeNull();
    });

    it('メディアの全属性を削除できる', async () => {
      const mediaId = createdMediaIds[0];

      // 全属性を削除
      await remoteDb.deleteAllMediaAttributes(mediaId);

      // 削除確認
      const attrs = await remoteDb.getMediaAttributes(mediaId);
      expect(attrs.length).toBe(0);
    });
  });

  describe('バックアップ操作', () => {
    it('手動バックアップを作成できる', async () => {
      const path = await remoteDb.backup();
      expect(typeof path).toBe('string');
      expect(path.length).toBeGreaterThan(0);
    });

    it('ラベル付きバックアップを作成できる', async () => {
      const path = await remoteDb.backup('テストバックアップ');
      expect(typeof path).toBe('string');
    });

    it('バックアップ一覧を取得できる', async () => {
      const backups = await remoteDb.listBackups();
      expect(Array.isArray(backups)).toBe(true);
      expect(backups.length).toBeGreaterThan(0);
      expect(backups[0].id).toBeDefined();
      expect(backups[0].path).toBeDefined();
      expect(backups[0].createdAt).toBeInstanceOf(Date);
      expect(backups[0].scope).toBe('manual');
    });

    it('バックアップから復元できる', async () => {
      await remoteDb.backup('復元テスト');
      const restoredPath = await remoteDb.restore({ type: 'latest' });
      expect(typeof restoredPath).toBe('string');

      // 復元後もDBが正常に動作することを確認
      const version = await remoteDb.getSchemaVersion();
      expect(version).toBeGreaterThan(0);
    });
  });

  describe('クリーンアップ', () => {
    it('作成したメディアを削除できる', async () => {
      for (const mediaId of createdMediaIds) {
        await remoteDb.deleteMedia(mediaId);
        const media = await remoteDb.getMedia(mediaId);
        expect(media).toBeNull();
      }
    });
  });

  describe('SSH Session 接続プール（TASK-71・Rust parity）', () => {
    // 専用インスタンスで connectCount 0 から検証する
    const poolDbPath = `${REMOTE_DB_PATH}.pool`;

    it('連続 RPC で Session を再利用し connectCount は増えない', async () => {
      const remote = new RemoteKijukuDB({ sshHost: SSH_HOST!, dbPath: poolDbPath });
      await remote.migrate(); // 初回 RPC で接続確立
      const afterFirst = remote.connectCount;
      expect(afterFirst).toBeGreaterThanOrEqual(1);

      await remote.getSchemaVersion(); // 2 回目は Session 再利用
      expect(remote.connectCount).toBe(afterFirst);
    });

    it('disconnect() 後の RPC は再接続する', async () => {
      const remote = new RemoteKijukuDB({ sshHost: SSH_HOST!, dbPath: poolDbPath });
      await remote.migrate();
      const before = remote.connectCount;

      await remote.disconnect();
      await remote.getSchemaVersion(); // 再接続
      expect(remote.connectCount).toBe(before + 1);
    });
  });
});
