# Rust SDK

Rust SDK（`kijuku-db` クレート）のAPI対応表。機能は TypeScript SDK と同等（メソッド名は snake_case・戻り値は `Result<T>`）。


Rust SDKはTypeScript SDKと同等のAPIを提供します。主な相違点:

## 初期化

```rust
use kijuku_db::{KijukuDB, DBOptions};

// 通常
let db = KijukuDB::open("./data/kijuku.db")?;

// オプション指定
let db = KijukuDB::open_with_options("./data/kijuku.db", DBOptions::default())?;

// インメモリ
let db = KijukuDB::open_in_memory()?;
```

## メソッド対応表

Rust SDKのメソッドはsnake_caseで、戻り値が`Result<T>`で包まれます:

| TypeScript | Rust | 備考 |
|-----------|------|------|
| `new KijukuDB(path)` | `KijukuDB::open(path)` | コンストラクタ → ファクトリメソッド |
| `createMedia(data)` | `create_media(&self, input: &MediaInput)` | |
| `getMedia(id)` | `get_media(&self, id: i64) -> Option<Media>` | |
| `updateMedia(id, data)` | `update_media(&self, id: i64, input: &MediaUpdateInput)` | `Partial<MediaInput>` → `MediaUpdateInput` |
| `deleteMedia(id)` | `delete_media(&self, id: i64)` | |
| `findMedia(filter, opts)` | `find_media(&self, filter: &MediaFilter, options: Option<&QueryOptions>)` | |
| `getDistinctValues(fields, filter)` | `get_distinct_values(&self, fields: &[&str], filter: &MediaFilter)` | |
| `bulkCreateMedia(list)` | `bulk_create_media(&self, data_list: &[MediaInput])` | |
| `bulkDeleteMedia(ids)` | `bulk_delete_media(&self, ids: &[i64])` | |
| `bulkUpdateMedia(updates)` | `bulk_update_media(&self, updates: &[BulkUpdateItem])` | |
| `createTag(name)` | `create_tag(&self, name: &str)` | |
| `getTagByName(name)` | `get_tag_by_name(&self, name: &str) -> Option<Tag>` | |
| `getAllTags()` | `get_all_tags(&self)` | |
| `addTagToMedia(mid, tid)` | `add_tag_to_media(&self, media_id: i64, tag_id: i64)` | |
| `removeTagFromMedia(mid, tid)` | `remove_tag_from_media(&self, media_id: i64, tag_id: i64)` | |
| `getMediaTags(mid)` | `get_media_tags(&self, media_id: i64)` | |
| `getTagUsageStats()` | `get_tag_usage_stats(&self)` | |
| `findUnusedTags()` | `find_unused_tags(&self)` | |
| `setMediaAttribute(mid, k, v, vt)` | `set_media_attribute(&self, media_id: i64, key: &str, value: Option<&str>, value_type: Option<AttributeValueType>)` | |
| `getMediaAttribute(mid, k)` | `get_media_attribute(&self, media_id: i64, key: &str)` | |
| `getMediaAttributes(mid)` | `get_media_attributes(&self, media_id: i64)` | |
| `deleteMediaAttribute(mid, k)` | `delete_media_attribute(&self, media_id: i64, key: &str)` | |
| `deleteAllMediaAttributes(mid)` | `delete_all_media_attributes(&self, media_id: i64)` | |
| `migrate()` | `migrate(&self)` | |
| `getSchemaVersion()` | `get_schema_version(&self) -> Result<i64>` | |
| `getTables()` | `get_tables(&self)` | |
| `getTableInfo(table)` | `get_table_info(&self, table_name: &str)` | |
| `isForeignKeysEnabled()` | `is_foreign_keys_enabled(&self)` | |
| `checkThumbnail(filter, opts)` | `check_thumbnail(&self, filter: &MediaFilter, options: Option<&QueryOptions>)` | |
| `updateThumbnail(filter, opts, to)` | `update_thumbnail(&self, filter: &MediaFilter, options: Option<&QueryOptions>, thumbnail_options: &ThumbnailOptions)` | |
| `updateExist(filter, opts, uo)` | `update_exist(&self, filter: &MediaFilter, options: Option<&QueryOptions>, update_options: &UpdateExistOptions)` | |
| `backup()` | `backup(&self) -> Result<Option<String>>` | |
| `backupWithLabel(label)` | `backup_with_label(&self, label: &str)` | |
| `restore(selector)` | `restore(&mut self, selector: &BackupSelector) -> Result<PathBuf>` | `&mut self` |
| `listBackups()` | `list_backups(&self) -> Result<Vec<BackupInfo>>` | |
| `listPreStashes()` | `list_pre_stashes(&self) -> Result<Vec<BackupInfo>>` | pre-stash 発見経路（§8） |
| `getBackupManager()` | `get_backup_manager(&self) -> Option<&BackupManager>` | |
| `getMediaFromBackup(id, sel)` | `get_media_from_backup(&self, id: i64, selector: &BackupSelector)` | |
| `findMediaFromBackup(f, o, s)` | `find_media_from_backup(&self, filter: &MediaFilter, options: Option<&QueryOptions>, selector: &BackupSelector)` | |
| `getTagByNameFromBackup(n, s)` | `get_tag_by_name_from_backup(&self, name: &str, selector: &BackupSelector)` | |
| `getAllTagsFromBackup(sel)` | `get_all_tags_from_backup(&self, selector: &BackupSelector)` | |
| `getMediaTagsFromBackup(mid, s)` | `get_media_tags_from_backup(&self, media_id: i64, selector: &BackupSelector)` | |
| `getMediaAttributeFromBackup(mid, k, s)` | `get_media_attribute_from_backup(&self, media_id: i64, key: &str, selector: &BackupSelector)` | |
| `getMediaAttributesFromBackup(mid, s)` | `get_media_attributes_from_backup(&self, media_id: i64, selector: &BackupSelector)` | |
| `close()` | `close(self)` | `self`を消費 |
| `transaction(fn)` | `transaction<F, T>(&self, f: F) -> Result<T>` | `FnOnce(&Self) -> Result<T>` |
| `getMediaTagsBulk(ids)` | `get_media_tags_bulk(&self, media_ids: &[i64])` | JOIN 1発（`KijukuBackend` trait・async） |
| `addMediaHash(input)` | `add_media_hash(&self, input: &MediaHashInput)` | |
| `addMediaHashes(inputs)` | `add_media_hashes(&self, inputs: &[MediaHashInput])` | |
| `getMediaHashes(itemUuid)` | `get_media_hashes(&self, item_uuid: &str)` | |
| `getMediaHash(itemUuid, f, t)` | `get_media_hash(&self, item_uuid: &str, filename: &str, time_range: &str)` | |
| `findByContentHash(bytes)` | `find_by_content_hash(&self, hash_bytes: &[u8])` | |
| `deleteMediaHash(itemUuid, f, t)` | `delete_media_hash(&self, item_uuid: &str, filename: &str, time_range: &str)` | |
| `deleteMediaHashes(itemUuid)` | `delete_media_hashes(&self, item_uuid: &str)` | |
| `findDuplicateHashes()` | `find_duplicate_hashes(&self)` | 戻り値 `Vec<(Vec<u8>, i64)>` |
| `computeMediaHash(itemUuid, path, type, duration)` | `compute_media_hash(&self, item_uuid: &str, media_path: &str, media_type: &str, duration_sec: Option<i32>)` | |
| `computeMediaHashes(filter, opts, force)` | `compute_media_hashes(&self, filter: &MediaFilter, options: Option<&QueryOptions>, force: bool)` | |
| `getMediaHashesFromBackup(itemUuid, sel)` | `get_media_hashes_from_backup(&self, item_uuid: &str, selector: &BackupSelector)` | |
| `getMediaHashFromBackup(itemUuid, f, t, sel)` | `get_media_hash_from_backup(&self, item_uuid: &str, filename: &str, time_range: &str, selector: &BackupSelector)` | |
| `findByContentHashFromBackup(bytes, sel)` | `find_by_content_hash_from_backup(&self, hash_bytes: &[u8], selector: &BackupSelector)` | |
| `findDuplicateHashesFromBackup(sel)` | `find_duplicate_hashes_from_backup(&self, selector: &BackupSelector)` | |
| `diffWithBackup(sel, opts)` | `diff_with_backup(&self, selector: &BackupSelector, options: &DiffOptions)` | |
| `diffWithProd(path, opts)` | `diff_with_prod(&self, prod_db_path: &Path, options: &DiffOptions)` | |
| `observe(path, opts)` | `observe(&self, prod_db_path: &Path, options: &ObserveOptions)` | 機械的 promote gate |
| `promote(path, opts, backupOpts)` | `promote(&self, prod_db_path: &Path, observe_options: &ObserveOptions, backup_opts: Option<BackupOptions>)` | gate 不合格時は `PromoteGateFailedError` |
| `setBackupLabel(id, label)` | `set_backup_label(&self, id: &str, label: Option<&str>)` | |
| `setBackupNote(id, note)` | `set_backup_note(&self, id: &str, note: Option<&str>)` | |
| `getBackupMeta(id)` | `get_backup_meta(&self, id: &str)` | |
| `listAuditLogs(filter)` | `list_audit_logs(&self, filter: &AuditLogFilter)` | |
| `getMediaRoot()` | `media_root(&self) -> Option<&str>` | |
| `mediaCp(src, dst, opts)` | `media_cp(&self, src: &str, dst: &str, opts: &FileOpOptions)` | dry-run ファースト |
| `mediaMv(src, dst, opts)` | `media_mv(&self, src: &str, dst: &str, opts: &FileOpOptions)` | |
| `mediaSync(src, dst, opts)` | `media_sync(&self, src: &str, dst: &str, opts: &FileOpOptions)` | |
| `moveToTrash(rel, op, reason)` | `move_to_trash(&self, target_rel: &str, operation: TrashOperation, reason: Option<&str>)` | |
| `listTrash()` | `list_trash(&self)` | |
| `restoreFromTrash(id)` | `restore_from_trash(&self, id: &str) -> Result<PathBuf>` | |
| `purgeTrash(ids, dryRun)` | `purge_trash(&self, ids: Option<&[String]>, dry_run: bool)` | |
| `static replicateDb(src, dst, op)` | `KijukuDB::replicate_db(src: &Path, dst: &Path, op: SyncOp)` | op: `Sync`/`Discard` |

## Rust RemoteKijukuDB

Rust SDKにも`RemoteKijukuDB`が存在し、同じAPIをSSH経由で提供します。詳細はRemoteKijukuDBセクションを参照してください。

---

