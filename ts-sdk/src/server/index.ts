/**
 * Kijuku DB Web GUI Server
 */
import { Hono } from 'hono';
import { getCookie, setCookie, deleteCookie } from 'hono/cookie';
import { serve } from '@hono/node-server';
import type { KijukuDB } from '../index.js';
import { AuthManager, generatePassword } from './auth.js';
import * as path from 'node:path';
import * as fs from 'node:fs';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * サーバーオプション
 */
export interface ServerOptions {
  port?: number;
  password?: string;
  sessionMaxAge?: number;
}

/**
 * Web GUIサーバーを起動
 */
export function startServer(db: KijukuDB, options: ServerOptions = {}): void {
  const port = options.port ?? 40001;
  const password = options.password ?? generatePassword();
  const authManager = new AuthManager(password, options.sessionMaxAge);

  const app = new Hono();

  app.use('*', async (c, next) => {
    const path = c.req.path;

    if (path === '/api/auth/login' || path === '/' || path.startsWith('/static/')) {
      return next();
    }

    const sessionId = getCookie(c, 'session');
    if (!sessionId || !authManager.validateSession(sessionId)) {
      return c.json({ error: 'Unauthorized' }, 401);
    }

    return next();
  });

  app.post('/api/auth/login', async (c) => {
    const body = await c.req.json();
    const sessionId = authManager.authenticate(body.password);

    if (!sessionId) {
      return c.json({ error: 'Invalid password' }, 401);
    }

    setCookie(c, 'session', sessionId, {
      httpOnly: true,
      sameSite: 'Strict',
      maxAge: options.sessionMaxAge ? options.sessionMaxAge / 1000 : 3600,
    });

    return c.json({ success: true });
  });

  app.post('/api/auth/logout', async (c) => {
    const sessionId = getCookie(c, 'session');
    if (sessionId) {
      authManager.logout(sessionId);
    }
    deleteCookie(c, 'session');
    return c.json({ success: true });
  });

  app.get('/api/media', async (c) => {
    const query = c.req.query();
    const filter: Record<string, string> = {};

    if (query.title) filter.title = query.title;
    if (query.artist) filter.artist = query.artist;
    if (query.media_type) filter.media_type = query.media_type;
    if (query.series) filter.series = query.series;

    const limit = query.limit ? parseInt(query.limit, 10) : 50;
    const offset = query.offset ? parseInt(query.offset, 10) : 0;

    const media = db.findMedia(filter, {
      limit,
      offset,
      orderBy: query.orderBy ?? 'id',
      order: (query.order ?? 'DESC') === 'DESC' ? 'DESC' : 'ASC',
    });

    return c.json({ media, count: media.length });
  });

  app.get('/api/media/:id', async (c) => {
    const id = parseInt(c.req.param('id'), 10);
    const media = db.getMedia(id);

    if (!media) {
      return c.json({ error: 'Media not found' }, 404);
    }

    const tags = db.getMediaTags(id);
    const attributes = db.getMediaAttributes(id);

    return c.json({ media, tags, attributes });
  });

  app.get('/', async (c) => {
    const html = getStaticFile('index.html');
    return c.html(html);
  });

  app.get('/static/*', async (c) => {
    const filePath = c.req.path.replace('/static/', '');
    const content = getStaticFile(filePath);

    if (filePath.endsWith('.css')) {
      return c.text(content, 200, { 'Content-Type': 'text/css' });
    } else if (filePath.endsWith('.js')) {
      return c.text(content, 200, { 'Content-Type': 'application/javascript' });
    }

    return c.text(content);
  });

  console.log(`\nKijuku DB Web GUI Server`);
  console.log(`========================`);
  console.log(`URL: http://localhost:${port}`);
  console.log(`Password: ${authManager.getPassword()}`);
  console.log(`\nPress Ctrl+C to stop the server\n`);

  serve({
    fetch: app.fetch,
    port,
  });
}

function getStaticFile(fileName: string): string {
  const staticDir = path.join(__dirname, 'static');
  const filePath = path.join(staticDir, fileName);

  if (!fs.existsSync(filePath)) {
    return 'File not found';
  }

  return fs.readFileSync(filePath, 'utf-8');
}
