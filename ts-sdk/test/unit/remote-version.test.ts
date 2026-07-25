/**
 * リモートバイナリ自動デプロイのバージョン比較ロジックのテスト（TASK-69）。
 * Rust `parse_semver`/`needs_deploy` と同一セマンティクス（parity 検証）。
 *
 * 設計: クライアント接続時に getServerVersion でリモート CLI バージョンを取得し、
 * `local > remote`（厳密大なり）の場合のみデプロイ。ダウングレード保護・フェイルセーフ付き。
 */
import { describe, it, expect } from 'vitest';
import { parseSemver, needsDeploy } from '../../src/remote.js';

describe('parseSemver', () => {
  it('parses MAJOR.MINOR.PATCH', () => {
    expect(parseSemver('0.2.2')).toEqual([0, 2, 2]);
  });

  it('zero-pads short forms', () => {
    expect(parseSemver('0.2')).toEqual([0, 2, 0]);
    expect(parseSemver('1')).toEqual([1, 0, 0]);
  });

  it('rejects too many parts', () => {
    expect(parseSemver('0.2.2.1')).toBeNull();
  });

  it('rejects non-numeric / negative', () => {
    expect(parseSemver('0.x.2')).toBeNull();
    expect(parseSemver('0.2.-1')).toBeNull();
  });
});

describe('needsDeploy', () => {
  it('deploys when remote is unknown (auto-recovery)', () => {
    expect(needsDeploy('0.2.2', null)).toBe(true);
  });

  it('deploys on upgrade', () => {
    expect(needsDeploy('0.2.2', '0.2.1')).toBe(true);
    expect(needsDeploy('1.0.0', '0.2.2')).toBe(true);
  });

  it('skips on equal', () => {
    expect(needsDeploy('0.2.2', '0.2.2')).toBe(false);
  });

  it('protects downgrade (local older than remote)', () => {
    expect(needsDeploy('0.2.1', '0.2.2')).toBe(false);
    expect(needsDeploy('0.2.2', '1.0.0')).toBe(false);
  });

  it('fails safe on parse error', () => {
    expect(needsDeploy('0.2.2', 'garbage')).toBe(true);
    expect(needsDeploy('0.2.2', '0.2.2.1')).toBe(true);
  });

  it('handles remote short form (zero-pad compare)', () => {
    expect(needsDeploy('0.2.2', '0.2')).toBe(true);
  });
});
