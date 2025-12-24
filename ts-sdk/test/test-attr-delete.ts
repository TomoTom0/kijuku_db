#!/usr/bin/env node
/**
 * 追加属性の削除テスト
 */
import { KijukuDB } from '../src/index.js';

const db = new KijukuDB('/tmp/test_attrs_fresh.db');

// メディアを作成
const media = db.createMedia({
  title: 'テスト',
  media_type: 'comic',
});

console.log('1. 追加属性を設定');
db.setMediaAttribute(media.id, 'attr1', 'value1');
db.setMediaAttribute(media.id, 'attr2', 'value2');

let attrs = db.getMediaAttributes(media.id);
console.log(`設定後: ${attrs.length}件`);
attrs.forEach(a => console.log(`  ${a.key} = ${a.value}`));

console.log('\n2. attr1を削除');
db.deleteMediaAttribute(media.id, 'attr1');

attrs = db.getMediaAttributes(media.id);
console.log(`削除後: ${attrs.length}件`);
attrs.forEach(a => console.log(`  ${a.key} = ${a.value}`));

const deleted = db.getMediaAttribute(media.id, 'attr1');
console.log(`attr1取得結果: ${deleted === null ? 'null (削除成功)' : 'まだ存在 (削除失敗)'}`);

console.log('\n3. 全属性を削除');
db.deleteAllMediaAttributes(media.id);

attrs = db.getMediaAttributes(media.id);
console.log(`全削除後: ${attrs.length}件`);

db.close();
