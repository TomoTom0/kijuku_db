/**
 * 検索・フィルタ機能
 */
import type Database from 'better-sqlite3';
import type { Media, MediaFilter, QueryOptions } from './types.js';

/** 取得可能なフィールド名のホワイトリスト（SQLインジェクション防止） */
export const ALLOWED_DISTINCT_FIELDS: readonly string[] = [
  'title',
  'title_id',
  'artist',
  'artist_id',
  'media_type',
  'series',
  'volume_text',
  'volume_title',
  'magazine',
  'magazine_id',
  'language',
  'source',
  'external_id',
  'artist_en',
  'title_en',
  'chapters',
  'extension',
  'title_pron',
  'artist_pron',
  'series_pron',
];

function addLikeFilter(
  whereClauses: string[],
  params: Record<string, unknown>,
  filterValue: string | undefined,
  dbColumn: string,
  paramName: string
): void {
  if (filterValue !== undefined) {
    whereClauses.push(`m.${dbColumn} LIKE @${paramName}`);
    params[paramName] = `%${filterValue}%`;
  }
}

/**
 * SQLiteの行データをMediaオブジェクトに変換
 */
function rowToMedia(row: any): Media {
  return {
    ...row,
    flag_exist: Boolean(row.flag_exist),
    created_at: new Date(row.created_at),
    updated_at: new Date(row.updated_at),
  };
}

/**
 * フィルタ条件の構築結果
 */
interface FilterConditions {
  /** WHERE句の条件（AND結合済み） */
  condition: string | null;
  /** パラメータ */
  params: Record<string, unknown>;
  /** タグフィルタを使用するか */
  needsTagJoin: boolean;
}

/** パラメータ名カウンターの型 */
interface ParamCounter {
  value: number;
}

/**
 * ユニークなパラメータ名を生成
 * カウンターは参照渡しで、findMediaのスコープ内で管理
 */
function getUniqueParamName(base: string, counter: ParamCounter): string {
  counter.value++;
  return `${base}_${counter.value}`;
}

/**
 * 単一のMediaFilterから条件を構築
 * @param filter - メディアフィルタ
 * @param counter - パラメータ名カウンター（スコープ内で管理）
 */
function buildFilterConditions(filter: MediaFilter, counter: ParamCounter): FilterConditions {
  const whereClauses: string[] = [];
  const params: Record<string, unknown> = {};
  let needsTagJoin = false;

  // 各フィルタ条件を追加
  if (filter.title !== undefined) {
    const paramName = getUniqueParamName('title', counter);
    whereClauses.push(`m.title = @${paramName}`);
    params[paramName] = filter.title;
  }
  if (filter.title_id !== undefined) {
    const paramName = getUniqueParamName('title_id', counter);
    whereClauses.push(`m.title_id = @${paramName}`);
    params[paramName] = filter.title_id;
  }
  if (filter.artist !== undefined) {
    const paramName = getUniqueParamName('artist', counter);
    whereClauses.push(`m.artist = @${paramName}`);
    params[paramName] = filter.artist;
  }
  if (filter.artist_id !== undefined) {
    const paramName = getUniqueParamName('artist_id', counter);
    whereClauses.push(`m.artist_id = @${paramName}`);
    params[paramName] = filter.artist_id;
  }
  if (filter.media_type !== undefined) {
    const paramName = getUniqueParamName('media_type', counter);
    whereClauses.push(`m.media_type = @${paramName}`);
    params[paramName] = filter.media_type;
  }
  if (filter.series !== undefined) {
    const paramName = getUniqueParamName('series', counter);
    whereClauses.push(`m.series = @${paramName}`);
    params[paramName] = filter.series;
  }
  if (filter.source !== undefined) {
    const paramName = getUniqueParamName('source', counter);
    whereClauses.push(`m.source = @${paramName}`);
    params[paramName] = filter.source;
  }
  if (filter.flag_exist !== undefined) {
    const paramName = getUniqueParamName('flag_exist', counter);
    whereClauses.push(`m.flag_exist = @${paramName}`);
    params[paramName] = filter.flag_exist ? 1 : 0;
  }
  if (filter.language !== undefined) {
    const paramName = getUniqueParamName('language', counter);
    whereClauses.push(`m.language = @${paramName}`);
    params[paramName] = filter.language;
  }
  if (filter.magazine !== undefined) {
    const paramName = getUniqueParamName('magazine', counter);
    whereClauses.push(`m.magazine = @${paramName}`);
    params[paramName] = filter.magazine;
  }
  if (filter.magazine_id !== undefined) {
    const paramName = getUniqueParamName('magazine_id', counter);
    whereClauses.push(`m.magazine_id = @${paramName}`);
    params[paramName] = filter.magazine_id;
  }
  if (filter.extension !== undefined) {
    const paramName = getUniqueParamName('extension', counter);
    whereClauses.push(`m.extension = @${paramName}`);
    params[paramName] = filter.extension;
  }
  if (filter.external_id !== undefined) {
    const paramName = getUniqueParamName('external_id', counter);
    whereClauses.push(`m.external_id = @${paramName}`);
    params[paramName] = filter.external_id;
  }

  // 部分一致フィルタ
  if (filter.volume_title !== undefined) {
    const paramName = getUniqueParamName('volume_title', counter);
    whereClauses.push(`m.volume_title LIKE @${paramName}`);
    params[paramName] = `%${filter.volume_title}%`;
  }
  if (filter.title_en !== undefined) {
    const paramName = getUniqueParamName('title_en', counter);
    whereClauses.push(`m.title_en LIKE @${paramName}`);
    params[paramName] = `%${filter.title_en}%`;
  }
  if (filter.artist_en !== undefined) {
    const paramName = getUniqueParamName('artist_en', counter);
    whereClauses.push(`m.artist_en LIKE @${paramName}`);
    params[paramName] = `%${filter.artist_en}%`;
  }

  // id_inフィルタの処理
  // SQLiteのパラメータ数上限（デフォルト999）を考慮してチャンク分割
  if (filter.id_in && filter.id_in.length > 0) {
    // 重複IDを排除（IN句は集合扱いで結果の重複は生じないが、プレースホルダーの
    // 無駄な増加と999件チャンク制限への早期到達を防ぐ）
    const uniqueIds = Array.from(new Set(filter.id_in));
    const CHUNK_SIZE = 999;
    const orClauses: string[] = [];
    for (let i = 0; i < uniqueIds.length; i += CHUNK_SIZE) {
      const chunk = uniqueIds.slice(i, i + CHUNK_SIZE);
      const idParamNames = chunk.map((id) => {
        const paramName = getUniqueParamName('id_in', counter);
        params[paramName] = id;
        return `@${paramName}`;
      });
      orClauses.push(`m.id IN (${idParamNames.join(', ')})`);
    }
    whereClauses.push(`(${orClauses.join(' OR ')})`);
  }

  // exclude_idsフィルタの処理（id_in の逆: NOT IN）
  // チャンク分割された NOT IN 句は AND で結合する
  // （いずれのチャンクにも含まれない = 全体の NOT IN と同義）
  if (filter.exclude_ids && filter.exclude_ids.length > 0) {
    // 重複IDを排除（NOT IN句は集合扱いで結果の重複は生じないが、プレースホルダーの
    // 無駄な増加と999件チャンク制限への早期到達を防ぐ）
    const uniqueIds = Array.from(new Set(filter.exclude_ids));
    const CHUNK_SIZE = 999;
    const andClauses: string[] = [];
    for (let i = 0; i < uniqueIds.length; i += CHUNK_SIZE) {
      const chunk = uniqueIds.slice(i, i + CHUNK_SIZE);
      const idParamNames = chunk.map((id) => {
        const paramName = getUniqueParamName('exclude_ids', counter);
        params[paramName] = id;
        return `@${paramName}`;
      });
      andClauses.push(`m.id NOT IN (${idParamNames.join(', ')})`);
    }
    whereClauses.push(`(${andClauses.join(' AND ')})`);
  }

  // タグフィルタの処理
  if (filter.tag_ids && filter.tag_ids.length > 0) {
    needsTagJoin = true;
    const tagParamNames: string[] = [];
    filter.tag_ids.forEach((tagId) => {
      const paramName = getUniqueParamName('tag_id', counter);
      tagParamNames.push(paramName);
      params[paramName] = tagId;
    });
    const tagPlaceholders = tagParamNames.map((name) => `@${name}`).join(', ');
    whereClauses.push(`mt.tag_id IN (${tagPlaceholders})`);
  }

  // 条件をAND結合
  const condition = whereClauses.length > 0 ? whereClauses.join(' AND ') : null;

  return {
    condition,
    params,
    needsTagJoin,
  };
}

/**
 * WHERE句を構築（OR条件を含む）
 */
function buildWhereClause(
  mainCondition: string | null,
  orConditions: string[]
): string {
  const allConditions: string[] = [];

  // メイン条件を追加
  if (mainCondition) {
    allConditions.push(`(${mainCondition})`);
  }

  // OR条件を追加
  for (const cond of orConditions) {
    allConditions.push(`(${cond})`);
  }

  if (allConditions.length === 0) {
    return '';
  }
  return `WHERE ${allConditions.join(' OR ')}`;
}

/**
 * メディアを検索
 */
export function findMedia(
  db: Database.Database,
  filter: MediaFilter,
  options?: QueryOptions
): Media[] {
  // パラメータカウンターをfindMediaスコープ内で管理
  const counter: ParamCounter = { value: 0 };

  // メインフィルタの条件を構築
  const mainConditions = buildFilterConditions(filter, counter);
  let needsTagJoin = mainConditions.needsTagJoin;
  const allParams: Record<string, unknown> = { ...mainConditions.params };

  // or_filtersの条件を構築
  const orConditions: string[] = [];
  if (filter.or_filters) {
    for (const orFilter of filter.or_filters) {
      const conditions = buildFilterConditions(orFilter, counter);
      if (conditions.needsTagJoin) {
        needsTagJoin = true;
      }
      Object.assign(allParams, conditions.params);
      if (conditions.condition) {
        orConditions.push(conditions.condition);
      }
    }
  }

  // WHERE句の構築
  const whereClause = buildWhereClause(mainConditions.condition, orConditions);

  // FROM句の構築
  const fromClause = needsTagJoin
    ? 'FROM media m INNER JOIN media_tags mt ON m.id = mt.media_id'
    : 'FROM media m';

  // GROUP BY句（タグフィルタ使用時に重複を排除）
  const groupByClause = needsTagJoin ? 'GROUP BY m.id' : '';

  // ORDER BY句の構築
  let orderByClause = '';
  if (options?.sortKeys && options.sortKeys.length > 0) {
    const parts = options.sortKeys.map(sk => `m.${sk.field} ${sk.order ?? 'ASC'}`);
    orderByClause = `ORDER BY ${parts.join(', ')}`;
  }

  // LIMIT/OFFSET句の構築
  let limitClause = '';
  if (options?.limit !== undefined || options?.offset !== undefined) {
    // OFFSETを使う場合、LIMITも必要
    if (options?.limit !== undefined) {
      limitClause = `LIMIT @limit`;
      allParams.limit = options.limit;
    } else {
      // OFFSETのみの場合、LIMITに大きな値を設定
      limitClause = `LIMIT -1`;
    }

    if (options?.offset !== undefined) {
      limitClause += ` OFFSET @offset`;
      allParams.offset = options.offset;
    }
  }

  // SQLクエリの組み立て
  const sql = `
    SELECT m.*
    ${fromClause}
    ${whereClause}
    ${groupByClause}
    ${orderByClause}
    ${limitClause}
  `.trim();

  const stmt = db.prepare(sql);
  const rows = stmt.all(allParams);

  return rows.map(rowToMedia);
}

/**
 * 指定したフィールド群の重複なしの値の組み合わせ一覧を取得する
 *
 * @param db - データベース接続
 * @param fields - 対象フィールド名の配列（ホワイトリストで検証）
 * @param filter - 絞り込み条件
 * @returns 各行が fields と同じ順序のフィールド値（NULL含む）の組み合わせ一覧（昇順）
 */
export function getDistinctValues(
  db: Database.Database,
  fields: string[],
  filter: MediaFilter
): (string | null)[][] {
  if (fields.length === 0) {
    throw new Error('fieldsは1つ以上指定してください');
  }
  for (const field of fields) {
    if (!ALLOWED_DISTINCT_FIELDS.includes(field)) {
      throw new Error(`無効なフィールド: ${field}。使用可能: ${ALLOWED_DISTINCT_FIELDS.join(', ')}`);
    }
  }

  const counter: ParamCounter = { value: 0 };
  const allParams: Record<string, unknown> = {};

  const mainConditions = buildFilterConditions(filter, counter);
  let needsTagJoin = mainConditions.needsTagJoin;
  Object.assign(allParams, mainConditions.params);

  const orConditions: string[] = [];
  if (filter.or_filters) {
    for (const orFilter of filter.or_filters) {
      const conditions = buildFilterConditions(orFilter, counter);
      if (conditions.needsTagJoin) {
        needsTagJoin = true;
      }
      Object.assign(allParams, conditions.params);
      if (conditions.condition) {
        orConditions.push(conditions.condition);
      }
    }
  }

  const filterWhere = buildWhereClause(mainConditions.condition, orConditions);

  const fromClause = needsTagJoin
    ? 'FROM media m INNER JOIN media_tags mt ON m.id = mt.media_id'
    : 'FROM media m';

  const selectCols = fields.map((f) => `m.${f}`).join(', ');
  const orderCols = fields.map((f) => `m.${f} ASC`).join(', ');

  const sql = `
    SELECT DISTINCT ${selectCols}
    ${fromClause}
    ${filterWhere}
    ORDER BY ${orderCols}
  `.trim();

  const stmt = db.prepare(sql);
  const rows = stmt.all(allParams) as Array<Record<string, unknown>>;
  return rows.map((row) => fields.map((f) => (row[f] !== null && row[f] !== undefined ? String(row[f]) : null)));
}
