/**
 * リモートDB操作のサンプルコード
 *
 * 使用前の準備:
 * 1. ~/.ssh/config にリモートホスト設定を追加
 * 2. .env ファイルに REMOTE_SSH_HOST を設定
 * 3. bun run deploy:local でローカルにバイナリを配置
 * 
 * Note: 外部プロジェクトでは以下のようにインポートしてください:
 * import { RemoteKijukuDB } from 'kijuku-db';
 */

import { RemoteKijukuDB } from '../ts-sdk/src/remote.js';

async function main() {
  // リモートDB接続を作成
  const remoteDb = new RemoteKijukuDB({
    sshHost: process.env.REMOTE_SSH_HOST || 'myserver', // .ssh/configのHost名
    dbPath: '~/.local/share/kijuku/test.db',             // リモートのDBパス
  });

  console.log('リモートDBに接続中...');

  // メディアを作成
  console.log('\n--- メディア作成 ---');
  const media = await remoteDb.createMedia({
    title: 'リモート作品',
    media_type: 'comic',
    artist: 'リモート作者',
    description: 'SSH経由で作成されたメディア',
  });
  console.log('作成されたメディア:', media);

  // メディアを検索
  console.log('\n--- メディア検索 ---');
  const mediaList = await remoteDb.findMedia({});
  console.log(`検索結果: ${mediaList.length}件`);
  mediaList.forEach((m) => {
    console.log(`  - [${m.id}] ${m.title} (${m.media_type})`);
  });

  // タグを作成
  console.log('\n--- タグ作成 ---');
  const tag = await remoteDb.createTag('リモートタグ');
  console.log('作成されたタグ:', tag);

  // メディアにタグを追加
  console.log('\n--- タグ追加 ---');
  await remoteDb.addTagToMedia(media.id, tag.id);
  console.log(`メディア ${media.id} にタグ ${tag.id} を追加しました`);

  // メディアのタグを取得
  console.log('\n--- メディアタグ取得 ---');
  const mediaTags = await remoteDb.getMediaTags(media.id);
  console.log('メディアのタグ:', mediaTags);

  // メディアを更新
  console.log('\n--- メディア更新 ---');
  await remoteDb.updateMedia(media.id, {
    description: '更新された説明文',
  });
  console.log('メディアを更新しました');

  // 更新後のメディアを取得
  const updatedMedia = await remoteDb.getMedia(media.id);
  console.log('更新後のメディア:', updatedMedia);

  console.log('\nリモート操作が完了しました');
}

main().catch(console.error);
