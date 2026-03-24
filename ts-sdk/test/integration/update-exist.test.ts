/**
 * updateExist（flag_exist更新）のテスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB } from '../../src/index.js';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

describe('updateExist', () => {
  let db: KijukuDB;
  let tmpDir: string;

  beforeEach(() => {
    db = new KijukuDB(':memory:');
    db.migrate();
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'kijuku-update-exist-'));
  });

  afterEach(() => {
    db.close();
    fs.rmSync(tmpDir, { recursive: true });
  });

  describe('video / music（ファイル単体）', () => {
    test('ファイルが削除された場合 flag_exist = false に更新される', () => {
      // path を指定して createMedia すると flag_exist が自動で true になる
      const filePath = path.join(tmpDir, 'test.mp4');
      fs.writeFileSync(filePath, '');

      const media = db.createMedia({
        title: 'テスト動画',
        media_type: 'video',
        path: filePath,
      });
      expect(db.getMedia(media.id)!.flag_exist).toBe(true);

      // ファイルを削除してから updateExist を実行
      fs.unlinkSync(filePath);

      const result = db.updateExist({}, undefined, { dry_run: false });
      expect(result.updated).toBe(1);
      expect(db.getMedia(media.id)!.flag_exist).toBe(false);

      fs.unlinkSync(result.detail_file);
    });

    test('path が null のメディアは flag_exist = false のまま変化しない', () => {
      db.createMedia({ title: 'パスなし', media_type: 'video' });

      const result = db.updateExist({}, undefined, { dry_run: false });
      expect(result.updated).toBe(0);

      fs.unlinkSync(result.detail_file);
    });
  });

  describe('comic（ディレクトリ）', () => {
    test('001.jpg を削除した場合 flag_exist = false に更新される', () => {
      const comicDir = path.join(tmpDir, 'comic1');
      fs.mkdirSync(comicDir);
      fs.writeFileSync(path.join(comicDir, '001.jpg'), '');

      const media = db.createMedia({
        title: 'コミック',
        media_type: 'comic',
        path: comicDir,
      });
      expect(db.getMedia(media.id)!.flag_exist).toBe(true);

      // 001.jpg を削除
      fs.unlinkSync(path.join(comicDir, '001.jpg'));

      const result = db.updateExist({}, undefined, { dry_run: false });
      expect(result.updated).toBe(1);
      expect(db.getMedia(media.id)!.flag_exist).toBe(false);

      fs.unlinkSync(result.detail_file);
    });
  });

  describe('dry_run', () => {
    test('dry_run: true の場合 DB は更新されない', () => {
      const filePath = path.join(tmpDir, 'dry.mp4');
      fs.writeFileSync(filePath, '');

      const media = db.createMedia({
        title: 'テスト',
        media_type: 'video',
        path: filePath,
      });
      // ファイルを削除すると flag_exist は true → false になる想定
      fs.unlinkSync(filePath);

      const result = db.updateExist({}, undefined, { dry_run: true });
      // updated はカウントされるが DB は変わらない
      expect(result.updated).toBe(1);
      expect(db.getMedia(media.id)!.flag_exist).toBe(true);

      fs.unlinkSync(result.detail_file);
    });
  });

  describe('戻り値', () => {
    test('total は対象メディア件数', () => {
      db.createMedia({ title: 'A', media_type: 'video', path: '/nonexistent/a.mp4' });
      db.createMedia({ title: 'B', media_type: 'video', path: '/nonexistent/b.mp4' });

      const result = db.updateExist({}, undefined, { dry_run: false });
      expect(result.total).toBe(2);

      fs.unlinkSync(result.detail_file);
    });

    test('detail_file に全件の詳細が書き出される', () => {
      db.createMedia({ title: 'A', media_type: 'video', path: '/nonexistent/a.mp4' });

      const result = db.updateExist({}, undefined, { dry_run: false });
      expect(fs.existsSync(result.detail_file)).toBe(true);

      const detail = JSON.parse(fs.readFileSync(result.detail_file, 'utf-8'));
      expect(detail).toHaveLength(1);
      expect(detail[0].title).toBe('A');

      fs.unlinkSync(result.detail_file);
    });

    test('updated_ids は 1000 件以下の場合インラインで返る', () => {
      const filePath = path.join(tmpDir, 'v.mp4');
      fs.writeFileSync(filePath, '');
      db.createMedia({ title: 'V', media_type: 'video', path: filePath, flag_exist: false });

      const result = db.updateExist({}, undefined, { dry_run: false });
      expect(result.updated_ids).not.toBeNull();
      expect(result.updated_ids_file).toBeNull();

      fs.unlinkSync(result.detail_file);
    });
  });
});
