/**
 * 認証とセッション管理
 */
import * as crypto from 'node:crypto';

/**
 * セッション情報
 */
interface Session {
  id: string;
  createdAt: number;
}

/**
 * セッションストア（メモリ内）
 */
class SessionStore {
  private sessions = new Map<string, Session>();

  create(): string {
    const sessionId = crypto.randomBytes(32).toString('hex');
    this.sessions.set(sessionId, {
      id: sessionId,
      createdAt: Date.now(),
    });
    return sessionId;
  }

  exists(sessionId: string): boolean {
    return this.sessions.has(sessionId);
  }

  delete(sessionId: string): void {
    this.sessions.delete(sessionId);
  }

  cleanup(maxAge: number): void {
    const now = Date.now();
    for (const [id, session] of this.sessions.entries()) {
      if (now - session.createdAt > maxAge) {
        this.sessions.delete(id);
      }
    }
  }
}

/**
 * ランダムなパスワードを生成
 */
export function generatePassword(): string {
  const chars = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
  const length = 12;
  let password = '';

  for (let i = 0; i < length; i++) {
    const randomIndex = crypto.randomInt(0, chars.length);
    password += chars[randomIndex];
  }

  return password;
}

/**
 * パスワードをハッシュ化（SHA-256）
 */
export function hashPassword(password: string): string {
  return crypto.createHash('sha256').update(password).digest('hex');
}

/**
 * 認証マネージャー
 */
export class AuthManager {
  private password: string;
  private passwordHash: string;
  private sessionStore: SessionStore;
  private sessionMaxAge: number;

  constructor(password: string, sessionMaxAge: number = 3600000) {
    this.password = password;
    this.passwordHash = hashPassword(password);
    this.sessionStore = new SessionStore();
    this.sessionMaxAge = sessionMaxAge;

    setInterval(() => {
      this.sessionStore.cleanup(this.sessionMaxAge);
    }, 60000);
  }

  getPassword(): string {
    return this.password;
  }

  authenticate(password: string): string | null {
    const hash = hashPassword(password);
    if (hash === this.passwordHash) {
      return this.sessionStore.create();
    }
    return null;
  }

  validateSession(sessionId: string): boolean {
    return this.sessionStore.exists(sessionId);
  }

  logout(sessionId: string): void {
    this.sessionStore.delete(sessionId);
  }
}
