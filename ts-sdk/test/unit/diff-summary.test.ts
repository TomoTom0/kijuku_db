/**
 * summarizeDiff（prod/stg 差分要約）の単体テスト
 *
 * 設計 docs/design/db-protection.md §4.4/§15-15・TASK-53。
 * Rust 側 summarize_diff と同じ論理を検証する。computeBackupDiff(current=prod, backup=stg)
 * で stg 編集視点（added=stg新規 等）の BackupDiff を作り、要約する。
 */
import { describe, it, expect } from 'vitest';
import { computeBackupDiff, summarizeDiff } from '../../src/diff.js';
import type { BackupSnapshot, Media, MediaAttribute } from '../../src/types.js';

const T = new Date('2026-01-01T00:00:00Z');

function media(id: number, type: string, artist?: string): Media {
  return {
    id,
    uuid: `uuid-${id}`,
    title: `title-${id}`,
    media_type: type as Media['media_type'],
    flag_exist: true,
    created_at: T,
    updated_at: T,
    ...(artist !== undefined ? { artist } : {}),
  };
}

function attr(mediaId: number, key: string, value: string = `v-${key}`): MediaAttribute {
  return {
    media_id: mediaId,
    key,
    value,
    created_at: T,
    updated_at: T,
  } as MediaAttribute;
}

const emptySnap = (): BackupSnapshot => ({ media: [], tags: [], mediaTags: [], attributes: [], hashes: [] });
const full = { type: 'full' } as const;

describe('summarizeDiff', () => {
  it('totals は5テーブル横断の合計（stg 編集視点）', () => {
    const prod = emptySnap();
    const stg = emptySnap();
    // current=prod, backup=stg: added=stgのみ(3), removed=prodのみ(2)
    prod.media = [media(1, 'comic'), media(2, 'comic')];
    stg.media = [media(1, 'comic'), media(3, 'comic')];
    const diff = computeBackupDiff(prod, stg, { detail: full });
    const summary = summarizeDiff(diff);
    expect(summary.totals.added).toBe(1); // media 3
    expect(summary.totals.removed).toBe(1); // media 2
    expect(summary.totals.changed).toBe(0);
    // counts は diff.summary と同一
    expect(summary.counts).toBe(diff.summary);
  });

  it('media の分布を byType / byFlagExist で集計する', () => {
    const prod = emptySnap();
    const stg = emptySnap();
    prod.media = [media(1, 'comic'), media(2, 'video')];
    stg.media = [media(1, 'comic'), media(3, 'comic')];
    const diff = computeBackupDiff(prod, stg, { detail: full });
    const { distribution } = summarizeDiff(diff);
    expect(distribution.media.byType['comic']).toEqual({ added: 1, removed: 0, changed: 0 });
    expect(distribution.media.byType['video']).toEqual({ added: 0, removed: 1, changed: 0 });
    expect(distribution.media.byFlagExist['true']).toEqual({ added: 1, removed: 1, changed: 0 });
  });

  it('artist が無い場合は unknown に集計し、上位Nを変動件数降順で返す', () => {
    const prod = emptySnap();
    const stg = emptySnap();
    prod.media = [media(1, 'comic')];
    stg.media = [media(1, 'comic'), media(2, 'comic', 'A'), media(3, 'comic', 'A'), media(4, 'comic')];
    const diff = computeBackupDiff(prod, stg, { detail: full });
    const { distribution } = summarizeDiff(diff);
    // added: A×2, unknown×1 → A が変動件数上位
    const top = distribution.media.byArtistTop;
    expect(top.length).toBe(2);
    expect(top[0].artist).toBe('A');
    expect(top[0].counts.added).toBe(2);
  });

  it('attributes の分布を byKey で集計する', () => {
    const prod = emptySnap();
    const stg = emptySnap();
    prod.attributes = [attr(1, 'author', 'old')];
    stg.attributes = [attr(1, 'author', 'new'), attr(2, 'source')];
    const diff = computeBackupDiff(prod, stg, { detail: full });
    const { distribution } = summarizeDiff(diff);
    expect(distribution.attributes.byKey['author']).toEqual({ added: 0, removed: 0, changed: 1 });
    expect(distribution.attributes.byKey['source']).toEqual({ added: 1, removed: 0, changed: 0 });
  });
});
