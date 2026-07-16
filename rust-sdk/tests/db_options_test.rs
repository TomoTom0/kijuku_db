// DBOptions / open 挙動のテスト（本番DB保護 P0 Step1: WAL + busy_timeout + readonly skip）
//
// 設計 docs/design/db-protection.md §5.1（prod readonly + no-migrate）・§5.3（WAL + busy_timeout）。

use kijuku_db::{DBOptions, KijukuDB};
use tempfile::TempDir;

/// RW 接続では WAL が有効になる（設計 §5.3）。
#[test]
fn test_rw_open_sets_wal() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("rw.db");
    let db = KijukuDB::open_with_options(
        &db_path,
        DBOptions {
            backup: None,
            ..Default::default()
        },
    )
    .unwrap();
    let conn = db.conn_handle();
    let conn = conn.lock();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
}

/// RW 接続では `DBOptions.timeout` が busy_timeout に反映される。未指定時は 5000ms（設計 §5.3）。
#[test]
fn test_busy_timeout_from_options() {
    let temp = TempDir::new().unwrap();

    // 明示指定: 8000ms
    let db = KijukuDB::open_with_options(
        temp.path().join("busy.db"),
        DBOptions {
            timeout: Some(8000),
            backup: None,
            ..Default::default()
        },
    )
    .unwrap();
    let conn = db.conn_handle();
    let conn = conn.lock();
    let bt: i64 = conn
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap();
    assert_eq!(bt, 8000);
    drop(conn);

    // デフォルト（未指定）: 5000ms
    let db2 = KijukuDB::open_with_options(
        temp.path().join("busy_default.db"),
        DBOptions {
            backup: None,
            ..Default::default()
        },
    )
    .unwrap();
    let c2 = db2.conn_handle();
    let c2 = c2.lock();
    let bt2: i64 = c2
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap();
    assert_eq!(bt2, 5000);
}

/// readonly 接続では WAL を設定せず、書込は拒否される（設計 §5.1・§5.3）。
#[test]
fn test_readonly_skips_wal_and_rejects_writes() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("ro.db");

    // 事前に delete モード（WAL 未設定）のファイルを作成
    {
        let c = rusqlite::Connection::open(&db_path).unwrap();
        c.execute_batch("PRAGMA journal_mode=DELETE; CREATE TABLE _t(x);")
            .unwrap();
        let mode: String = c.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode.to_lowercase(), "delete");
    }

    // readonly で開く
    let db = KijukuDB::open_with_options(
        &db_path,
        DBOptions {
            readonly: true,
            backup: None,
            ..Default::default()
        },
    )
    .unwrap();
    let conn = db.conn_handle();
    let conn = conn.lock();

    // WAL は設定されず delete のまま
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "delete");

    // 書込は拒否される
    let write_result = conn.execute("CREATE TABLE _u(y)", []);
    assert!(
        write_result.is_err(),
        "readonly 接続での書込は失敗すべき（設計 §5.1）"
    );
}
