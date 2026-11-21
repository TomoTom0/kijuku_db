/**
 * Web GUIサーバー層（Hono）の in-process テスト（TASK-101）。
 *
 * `app.request()` で HTTP リスンせずに endpoint（media 取得系・リストクエリ）と
 * 認証 middleware（未認証拒否・ログイン・logout によるセッション無効化）を検証する。
 */
import { describe, test, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB, MediaInput } from '../../src/index.js';
import { createApp } from '../../src/server/index.js';
import { AuthManager } from '../../src/server/auth.js';
import type { Hono } from 'hono';

const PASSWORD = 'test-password';

// ---- 型ガード（as キャスト・any 不使用でレスポンスJSONを narrow する） ----

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

interface MediaSummary {
  id: number;
  title: string;
  uuid: string;
}

function isMediaArray(value: unknown): value is MediaSummary[] {
  return (
    Array.isArray(value) &&
    value.every(
      (m: unknown): m is MediaSummary =>
        isRecord(m) &&
        typeof m.id === 'number' &&
        typeof m.title === 'string' &&
        typeof m.uuid === 'string'
    )
  );
}

interface TagSummary {
  id: number;
  name: string;
}

function isTagArray(value: unknown): value is TagSummary[] {
  return (
    Array.isArray(value) &&
    value.every(
      (t: unknown): t is TagSummary => isRecord(t) && typeof t.id === 'number' && typeof t.name === 'string'
    )
  );
}

interface AttributeSummary {
  key: string;
  value: string | null;
}

function isAttributeArray(value: unknown): value is AttributeSummary[] {
  return (
    Array.isArray(value) &&
    value.every(
      (a: unknown): a is AttributeSummary =>
        isRecord(a) && typeof a.key === 'string' && (a.value === null || typeof a.value === 'string')
    )
  );
}

interface MediaListBody {
  media: MediaSummary[];
  count: number;
  total: number;
}

function parseMediaListBody(value: unknown): MediaListBody {
  if (!isRecord(value)) throw new Error('list body must be an object');
  if (!isMediaArray(value.media)) throw new Error('list body.media must be Media[]');
  if (typeof value.count !== 'number') throw new Error('list body.count must be number');
  if (typeof value.total !== 'number') throw new Error('list body.total must be number');
  return { media: value.media, count: value.count, total: value.total };
}

interface MediaDetailBody extends MediaSummary {
  tags: TagSummary[];
  attributes: AttributeSummary[];
}

function parseMediaDetailBody(value: unknown): MediaDetailBody {
  if (!isRecord(value)) throw new Error('detail body must be an object');
  if (!isRecord(value.media)) throw new Error('detail body.media must be an object');
  const media = value.media;
  if (
    typeof media.id !== 'number' ||
    typeof media.title !== 'string' ||
    typeof media.uuid !== 'string'
  ) {
    throw new Error('detail body.media fields invalid');
  }
  if (!isTagArray(value.tags)) throw new Error('detail body.tags must be Tag[]');
  if (!isAttributeArray(value.attributes)) {
    throw new Error('detail body.attributes must be MediaAttribute[]');
  }
  return {
    id: media.id,
    title: media.title,
    uuid: media.uuid,
    tags: value.tags,
    attributes: value.attributes,
  };
}

async function readJson(res: Response): Promise<unknown> {
  return res.json();
}

function extractSessionCookie(res: Response): string {
  const setCookie = res.headers.get('set-cookie');
  if (setCookie === null) throw new Error('set-cookie header missing');
  for (const part of setCookie.split(';')) {
    const pair = part.trim();
    if (pair.startsWith('session=')) return pair.slice('session='.length);
  }
  throw new Error('session cookie not found in set-cookie');
}

describe('Web GUI server (Hono)', () => {
  let db: KijukuDB;
  let app: Hono;

  beforeEach(() => {
    db = new KijukuDB(':memory:');
    db.migrate();
    app = createApp(db, new AuthManager(PASSWORD));
  });

  afterEach(() => {
    db.close();
  });

  function createMedia(title: string): MediaSummary {
    const input: MediaInput = { title, media_type: 'comic' };
    const media = db.createMedia(input);
    return { id: media.id, title: media.title, uuid: media.uuid };
  }

  async function login(): Promise<string> {
    const res = await app.request('/api/auth/login', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ password: PASSWORD }),
    });
    expect(res.status).toBe(200);
    const body = await readJson(res);
    expect(isRecord(body) && body.success === true).toBe(true);
    return extractSessionCookie(res);
  }

  function authedGet(uri: string, session: string | null): Promise<Response> {
    const headers: Record<string, string> = {};
    if (session !== null) headers.cookie = `session=${session}`;
    return app.request(uri, { headers });
  }

  test('未認証リクエストは401で拒否される', async () => {
    for (const uri of ['/api/media', '/api/media/1', '/api/media/uuid/some-uuid']) {
      const res = await authedGet(uri, null);
      expect(res.status).toBe(401);
    }
  });

  test('誤ったパスワードではログインできない', async () => {
    const res = await app.request('/api/auth/login', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ password: 'wrong-password' }),
    });
    expect(res.status).toBe(401);
    const body = await readJson(res);
    expect(isRecord(body) && body.error === 'Invalid password').toBe(true);
  });

  test('正しいパスワードでログインしセッションcookieでアクセスできる', async () => {
    const session = await login();
    expect(session.length).toBeGreaterThan(0);

    const res = await authedGet('/api/media', session);
    expect(res.status).toBe(200);
    const body = parseMediaListBody(await readJson(res));
    expect(body.count).toBe(0);
    expect(body.total).toBe(0);
  });

  test('GET /api/media はフィルタ・limit/offset・orderに対応する', async () => {
    createMedia('srv-comic-A');
    createMedia('srv-comic-B');
    createMedia('srv-comic-C');
    const session = await login();

    // 全件（既定ソートは id DESC）
    const res = await authedGet('/api/media', session);
    expect(res.status).toBe(200);
    const body = parseMediaListBody(await readJson(res));
    expect(body.count).toBe(3);
    expect(body.total).toBe(3);
    expect(body.media.map((m) => m.id)).toEqual([3, 2, 1]);

    // ページネーション
    const pagedRes = await authedGet('/api/media?limit=2&offset=1', session);
    const paged = parseMediaListBody(await readJson(pagedRes));
    expect(paged.count).toBe(2);
    expect(paged.total).toBe(3);

    // title フィルタ
    const filteredRes = await authedGet('/api/media?title=srv-comic-B', session);
    const filtered = parseMediaListBody(await readJson(filteredRes));
    expect(filtered.count).toBe(1);
    expect(filtered.media[0]?.title).toBe('srv-comic-B');

    // orderBy / order（昇順）
    const ascRes = await authedGet('/api/media?orderBy=id&order=ASC', session);
    const asc = parseMediaListBody(await readJson(ascRes));
    expect(asc.media.map((m) => m.id)).toEqual([1, 2, 3]);
  });

  test('GET /api/media/:id はmedia・tags・attributesを返す', async () => {
    const media = createMedia('srv-detail');
    const tag = db.createTag('srv-tag');
    db.addTagToMedia(media.id, tag.id);
    db.setMediaAttribute(media.id, 'rating', '5');
    const session = await login();

    const res = await authedGet(`/api/media/${media.id}`, session);
    expect(res.status).toBe(200);
    const body = parseMediaDetailBody(await readJson(res));
    expect(body.id).toBe(media.id);
    expect(body.title).toBe('srv-detail');
    expect(body.tags.map((t) => t.name)).toContain('srv-tag');
    expect(body.attributes.map((a) => a.key)).toContain('rating');

    // 存在しないIDは404
    const notFound = await authedGet('/api/media/999999', session);
    expect(notFound.status).toBe(404);
  });

  test('GET /api/media/uuid/:uuid はuuidでmedia詳細を返す', async () => {
    const media = createMedia('srv-uuid');
    const session = await login();

    const res = await authedGet(`/api/media/uuid/${media.uuid}`, session);
    expect(res.status).toBe(200);
    const body = parseMediaDetailBody(await readJson(res));
    expect(body.uuid).toBe(media.uuid);
    expect(body.id).toBe(media.id);

    // 存在しないuuidは404
    const notFound = await authedGet('/api/media/uuid/00000000-0000-0000-0000-000000000000', session);
    expect(notFound.status).toBe(404);
  });

  test('logoutするとセッションが無効化される', async () => {
    const session = await login();

    const res = await app.request('/api/auth/logout', {
      method: 'POST',
      headers: { cookie: `session=${session}` },
    });
    expect(res.status).toBe(200);

    const after = await authedGet('/api/media', session);
    expect(after.status).toBe(401);
  });
});
