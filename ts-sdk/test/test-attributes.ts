#!/usr/bin/env node
/**
 * 追加属性のテスト
 */
import { KijukuDB } from '../src/index.js';

console.log('=== 追加属性テスト ===\n');

// atara/comics.tsvをインポートしたDBをテスト
const db = new KijukuDB('/tmp/test_atara.db');

console.log('1. 最初のメディアの追加属性を取得');
const media1 = db.getMedia(1);
console.log(`メディア: ${media1?.title} (ID: ${media1?.id})`);

const attrs = db.getMediaAttributes(1);
console.log(`追加属性: ${attrs.length}件`);
attrs.forEach(attr => {
  console.log(`  ${attr.key}: ${attr.value}`);
});
console.log('');

console.log('2. 特定の追加属性を取得');
const idOld = db.getMediaAttribute(1, 'id_old');
console.log(`id_old: ${idOld?.value}`);
console.log('');

console.log('3. 追加属性を持つメディアの数を確認');
const allMedia = db.findMedia({});
let countWithAttrs = 0;
for (const media of allMedia.slice(0, 10)) {
  const attrs = db.getMediaAttributes(media.id);
  if (attrs.length > 0) {
    countWithAttrs++;
  }
}
console.log(`最初の10件中、追加属性を持つメディア: ${countWithAttrs}件`);
console.log('');

console.log('4. 新しい追加属性を設定');
db.setMediaAttribute(1, 'test_attribute', 'テスト値');
const testAttr = db.getMediaAttribute(1, 'test_attribute');
console.log(`設定した属性: ${testAttr?.key} = ${testAttr?.value}`);
console.log('');

console.log('5. 追加属性を更新');
db.setMediaAttribute(1, 'test_attribute', '更新されたテスト値');
const updatedAttr = db.getMediaAttribute(1, 'test_attribute');
console.log(`更新した属性: ${updatedAttr?.key} = ${updatedAttr?.value}`);
console.log('');

console.log('6. 追加属性を削除');
db.deleteMediaAttribute(1, 'test_attribute');
const deletedAttr = db.getMediaAttribute(1, 'test_attribute');
console.log(`削除後の属性: ${deletedAttr === null ? 'null (成功)' : '失敗'}`);

db.close();
console.log('\n=== テスト完了 ===');
