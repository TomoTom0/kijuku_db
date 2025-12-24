#!/usr/bin/env tsx
/**
 * リモート属性操作のテスト
 */
import { RemoteKijukuDB } from '../src/remote.js';

async function main() {
  console.log('=== リモート属性操作テスト ===\n');

  const remoteDb = new RemoteKijukuDB({
    sshHost: 'as5202',
    dbPath: '~/work/tmp/kijuku_db/test_remote.db',
    binaryPath: '~/.local/kijuku-db/bin/kijuku-cli',
    port: 1022,
  });

  try {
    console.log('1. メディアを作成');
    const media = await remoteDb.createMedia({
      title: 'リモートテストメディア',
      media_type: 'comic',
    });
    console.log(`作成されたメディア: ID=${media.id}, title="${media.title}"`);
    console.log('');

    console.log('2. 属性を設定');
    await remoteDb.setMediaAttribute(media.id, 'test_attr', 'test_value');
    await remoteDb.setMediaAttribute(media.id, 'another_attr', 'another_value');
    console.log('属性を2件設定しました');
    console.log('');

    console.log('3. 特定の属性を取得');
    const attr = await remoteDb.getMediaAttribute(media.id, 'test_attr');
    console.log(`test_attr: ${attr?.value}`);
    console.log('');

    console.log('4. 全ての属性を取得');
    const attrs = await remoteDb.getMediaAttributes(media.id);
    console.log(`属性件数: ${attrs.length}`);
    attrs.forEach(a => console.log(`  ${a.key} = ${a.value} (${a.value_type})`));
    console.log('');

    console.log('5. 特定の属性を削除');
    await remoteDb.deleteMediaAttribute(media.id, 'test_attr');
    const attrsAfterDelete = await remoteDb.getMediaAttributes(media.id);
    console.log(`削除後の属性件数: ${attrsAfterDelete.length}`);
    console.log('');

    console.log('6. 全ての属性を削除');
    await remoteDb.deleteAllMediaAttributes(media.id);
    const attrsAfterDeleteAll = await remoteDb.getMediaAttributes(media.id);
    console.log(`全削除後の属性件数: ${attrsAfterDeleteAll.length}`);
    console.log('');

    console.log('✓ 全てのテストが成功しました');
  } catch (error) {
    console.error('エラー:', error);
    process.exit(1);
  }
}

main();
