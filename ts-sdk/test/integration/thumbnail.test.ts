/**
 * サムネイル操作のテスト
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB } from '../../src/index.js';
import { resolveThumbnailPath } from '../../src/thumbnail.js';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

describe('resolveThumbnailPath', () => {
  test('content を含むパスからサムネイルパスを計算できる', () => {
    const result = resolveThumbnailPath('/data/content/series/vol1', 'abc-uuid');
    expect(result).toBe(path.join('/data', 'cover', 'abc-uuid.jpg'));
  });

  test('content が複数ある場合は最後のものを使用する', () => {
    const result = resolveThumbnailPath('/root/content/sub/content/vol1', 'abc-uuid');
    expect(result).toBe(path.join('/root/content/sub', 'cover', 'abc-uuid.jpg'));
  });

  test('content を含まないパスは undefined を返す', () => {
    const result = resolveThumbnailPath('/data/media/series/vol1', 'abc-uuid');
    expect(result).toBeUndefined();
  });

  test('ルート直下の content は undefined を返す', () => {
    const result = resolveThumbnailPath('/content/vol1', 'abc-uuid');
    expect(result).toBeUndefined();
  });
});

describe('KijukuDB サムネイル操作', () => {
  let db: KijukuDB;
  let tmpDir: string;

  beforeEach(() => {
    db = new KijukuDB(':memory:');
    db.migrate();
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'kijuku-thumb-test-'));
  });

  afterEach(() => {
    db.close();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  describe('checkThumbnail', () => {
    test('path未設定のメディアはskippedになる', () => {
      db.createMedia({ title: 'テスト', media_type: 'comic' });

      const result = db.checkThumbnail({});

      expect(result.total).toBe(1);
      expect(result.skipped).toBe(1);
      expect(result.ok).toBe(0);
      expect(result.details[0].status.type).toBe('skipped');
    });

    test('contentを含まないpathはskippedになる', () => {
      db.createMedia({ title: 'テスト', media_type: 'comic', path: '/data/media/vol1' });

      const result = db.checkThumbnail({});

      expect(result.skipped).toBe(1);
      expect(result.details[0].status.type).toBe('skipped');
    });

    test('サムネイルが正常に存在する場合はokになる', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });
      const coverDir = path.join(tmpDir, 'cover');
      fs.mkdirSync(coverDir, { recursive: true });

      const media = db.createMedia({ title: 'テスト', media_type: 'comic', path: contentDir });
      const expectedThumb = path.join(tmpDir, 'cover', `${media.uuid}.jpg`);
      fs.writeFileSync(expectedThumb, 'dummy');
      db.updateMedia(media.id, { thumbnail_path: expectedThumb });

      const result = db.checkThumbnail({});

      expect(result.ok).toBe(1);
      expect(result.details[0].status.type).toBe('ok');
    });

    test('thumbnail_pathが未設定の場合はmissingになる', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });
      db.createMedia({ title: 'テスト', media_type: 'comic', path: contentDir });

      const result = db.checkThumbnail({});

      expect(result.missing).toBe(1);
      expect(result.details[0].status.type).toBe('missing');
    });

    test('thumbnail_pathが設定されているがファイルが存在しない場合はfileNotFoundになる', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });
      const media = db.createMedia({ title: 'テスト', media_type: 'comic', path: contentDir });
      const expectedThumb = path.join(tmpDir, 'cover', `${media.uuid}.jpg`);
      db.updateMedia(media.id, { thumbnail_path: expectedThumb });

      const result = db.checkThumbnail({});

      expect(result.file_not_found).toBe(1);
      expect(result.details[0].status.type).toBe('fileNotFound');
    });

    test('フィルタで絞り込みができる', () => {
      db.createMedia({ title: 'コミック', media_type: 'comic' });
      db.createMedia({ title: 'ビデオ', media_type: 'video' });

      const result = db.checkThumbnail({ media_type: 'comic' });

      expect(result.total).toBe(1);
      expect(result.details[0].title).toBe('コミック');
    });

    test('空のDBでも正常に動作する', () => {
      const result = db.checkThumbnail({});

      expect(result.total).toBe(0);
      expect(result.ok).toBe(0);
      expect(result.details).toHaveLength(0);
    });
  });

  describe('updateThumbnail', () => {
    test('dry_run=trueの場合はDBを更新しない', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });
      fs.writeFileSync(path.join(contentDir, '001.jpg'), 'dummy');
      const media = db.createMedia({ title: 'テスト', media_type: 'comic', path: contentDir });

      const result = db.updateThumbnail({}, undefined, { dry_run: true });

      expect(result.generated).toBe(1);
      // DB は更新されていない
      const updated = db.getMedia(media.id);
      expect(updated?.thumbnail_path).toBeNull();
    });

    test('001.{ext}が存在しない場合はskippedになる', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });
      // 001.jpg を作らない
      db.createMedia({ title: 'テスト', media_type: 'comic', path: contentDir });

      const result = db.updateThumbnail({}, undefined, { dry_run: true });

      expect(result.skipped).toBe(1);
    });

    test('pathが未設定の場合はskippedになる', () => {
      db.createMedia({ title: 'テスト', media_type: 'comic' });

      const result = db.updateThumbnail({});

      expect(result.skipped).toBe(1);
      expect(result.generated).toBe(0);
    });

    test('既存サムネイルがある場合はalreadyExistsになる', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });
      fs.writeFileSync(path.join(contentDir, '001.jpg'), 'dummy');
      const coverDir = path.join(tmpDir, 'cover');
      fs.mkdirSync(coverDir, { recursive: true });

      const media = db.createMedia({ title: 'テスト', media_type: 'comic', path: contentDir });
      const thumbPath = path.join(tmpDir, 'cover', `${media.uuid}.jpg`);
      fs.writeFileSync(thumbPath, 'dummy');
      db.updateMedia(media.id, { thumbnail_path: thumbPath });

      const result = db.updateThumbnail({}, undefined, { dry_run: true });

      expect(result.already_exists).toBe(1);
    });

    test('force=trueの場合は既存サムネイルも再生成対象になる（dry_run）', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });
      fs.writeFileSync(path.join(contentDir, '001.jpg'), 'dummy');
      const coverDir = path.join(tmpDir, 'cover');
      fs.mkdirSync(coverDir, { recursive: true });

      const media = db.createMedia({ title: 'テスト', media_type: 'comic', path: contentDir });
      const thumbPath = path.join(tmpDir, 'cover', `${media.uuid}.jpg`);
      fs.writeFileSync(thumbPath, 'dummy');
      db.updateMedia(media.id, { thumbnail_path: thumbPath });

      const result = db.updateThumbnail({}, undefined, { dry_run: true, force: true });

      expect(result.generated).toBe(1);
    });

    test('空のDBでも正常に動作する', () => {
      const result = db.updateThumbnail({});

      expect(result.total).toBe(0);
      expect(result.generated).toBe(0);
    });

    test('video: 動画ファイルが存在しない場合はskippedになる', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });
      const videoPath = path.join(contentDir, 'video.mp4');
      // ファイルを作成しない

      db.createMedia({ title: 'テスト動画', media_type: 'video', path: videoPath, extension: 'mp4' });

      const result = db.updateThumbnail({}, undefined, { dry_run: true });

      expect(result.skipped).toBe(1);
      expect(result.generated).toBe(0);
    });

    test('video: 動画ファイルが存在する場合はdry_runでgeneratedになる', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });
      const videoPath = path.join(contentDir, 'video.mp4');
      fs.writeFileSync(videoPath, 'dummy video');

      db.createMedia({ title: 'テスト動画', media_type: 'video', path: videoPath, extension: 'mp4' });

      const result = db.updateThumbnail({}, undefined, { dry_run: true });

      expect(result.generated).toBe(1);
    });

    test('video: 既存サムネイルがある場合はalreadyExistsになる', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });
      const videoPath = path.join(contentDir, 'video.mp4');
      fs.writeFileSync(videoPath, 'dummy video');

      const coverDir = path.join(tmpDir, 'cover');
      fs.mkdirSync(coverDir, { recursive: true });

      const media = db.createMedia({ title: 'テスト動画', media_type: 'video', path: videoPath, extension: 'mp4' });
      const thumbPath = path.join(tmpDir, 'cover', `${media.uuid}.jpg`);
      fs.writeFileSync(thumbPath, 'dummy thumb');
      db.updateMedia(media.id, { thumbnail_path: thumbPath });

      const result = db.updateThumbnail({}, undefined, { dry_run: true });

      expect(result.already_exists).toBe(1);
    });

    test('music: musicはサムネイル対象外でskippedになる', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });
      const musicPath = path.join(contentDir, 'audio.m4a');
      fs.writeFileSync(musicPath, 'dummy audio');

      db.createMedia({ title: 'テスト音楽', media_type: 'music', path: musicPath, extension: 'm4a' });

      const result = db.updateThumbnail({}, undefined, { dry_run: true });

      expect(result.skipped).toBe(1);
      expect(result.details[0].status).toEqual({
        type: 'skipped',
        reason: 'musicはサムネイル対象外',
      });
    });

    test('video: pathにextがない場合でもuuidから解決してgeneratedになる', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });

      const media = db.createMedia({ title: 'テスト動画extなし', media_type: 'video', extension: 'mp4' });
      // pathにextを含めず、実際のファイルは {uuid}.mp4
      const pathWithoutExt = path.join(contentDir, media.uuid);
      const actualFile = `${pathWithoutExt}.mp4`;
      fs.writeFileSync(actualFile, 'dummy video');
      db.updateMedia(media.id, { path: pathWithoutExt });

      const result = db.updateThumbnail({}, undefined, { dry_run: true });

      expect(result.generated).toBe(1);
    });

    test('video: pathにextがなくuuidのファイルもない場合はskippedになる', () => {
      const contentDir = path.join(tmpDir, 'content', 'series', 'vol1');
      fs.mkdirSync(contentDir, { recursive: true });

      const media = db.createMedia({ title: 'テスト動画なし', media_type: 'video', extension: 'mp4' });
      const pathWithoutExt = path.join(contentDir, media.uuid);
      // ファイルを作成しない
      db.updateMedia(media.id, { path: pathWithoutExt });

      const result = db.updateThumbnail({}, undefined, { dry_run: true });

      expect(result.skipped).toBe(1);
    });
  });
});
