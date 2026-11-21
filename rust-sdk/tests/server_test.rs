//! Web GUIサーバー層（axum）の in-process テスト（TASK-101）。
//!
//! `tower::ServiceExt::oneshot` で TCP リスンせずに Router へ直接リクエストを投げ、
//! 主要 endpoint（media 取得系・リストクエリ）と認証 middleware
//! （未認証拒否・ログイン・logout によるセッション無効化）の回帰を保障する。

#![allow(deprecated)] // KijukuDB の同期 API（server 層と同じ扱い）

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    response::Response,
    Router,
};
use kijuku_db::server::{build_app, AuthManager, ServerState};
use kijuku_db::{AttributeValueType, KijukuDB, MediaInput, MediaType};
use std::sync::Arc;
use tower::ServiceExt;

const PASSWORD: &str = "test-password";

/// migrate 済み :memory: DB からテスト用 Router を組み立てる。
fn app_for(db: KijukuDB) -> Router {
    build_app(Arc::new(ServerState::new(
        db,
        Arc::new(AuthManager::new(PASSWORD.to_string())),
    )))
}

/// テスト用 Router（データ空の :memory: DB・migrate 済み）。
fn test_app() -> Router {
    let db = KijukuDB::open_in_memory().expect("open :memory: db");
    db.migrate().expect("migrate");
    app_for(db)
}

/// テスト用の media を1件作成して返す。
fn create_media(db: &KijukuDB, title: &str) -> kijuku_db::Media {
    db.create_media(&MediaInput {
        title: title.to_string(),
        media_type: MediaType::Comic,
        ..Default::default()
    })
    .expect("create_media")
}

/// 正しいパスワードでログインし、session_id クッキーの値を返す。
async fn login(app: &Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "password": PASSWORD }).to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK, "ログインは成功する");

    let set_cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .expect("login 応答に Set-Cookie がある")
        .to_str()
        .unwrap();
    set_cookie
        .split(';')
        .next()
        .and_then(|pair| pair.strip_prefix("session_id="))
        .expect("Set-Cookie に session_id が含まれる")
        .to_string()
}

/// GET リクエスト（session_id クッキー付き）を送る。
async fn get(app: &Router, uri: &str, session_id: Option<&str>) -> Response {
    let mut builder = Request::builder().uri(uri);
    if let Some(session_id) = session_id {
        builder = builder.header(header::COOKIE, format!("session_id={session_id}"));
    }
    app.clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

/// レスポンスボディを JSON としてパースする。
async fn body_json(res: Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn media_endpoints_reject_missing_session() {
    let app = test_app();
    for uri in ["/api/media", "/api/media/1", "/api/media/uuid/some-uuid"] {
        let res = get(&app, uri, None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{uri} は未認証で 401");
    }
}

#[tokio::test]
async fn login_rejects_wrong_password() {
    let app = test_app();
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "password": "wrong-password" }).to_string(),
        ))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "誤パスワードは 401");
}

#[tokio::test]
async fn login_success_sets_session_cookie_and_allows_access() {
    let app = test_app();

    let session_id = login(&app).await;
    assert!(!session_id.is_empty(), "session_id が発行される");

    let res = get(&app, "/api/media", Some(&session_id)).await;
    assert_eq!(res.status(), StatusCode::OK, "認証済みで /api/media が通る");

    let body = body_json(res).await;
    assert_eq!(body["count"], 0, "空 DB では count=0");
    assert_eq!(body["total"], 0, "空 DB では total=0");
}

#[tokio::test]
async fn media_list_supports_filter_limit_offset_and_order() {
    let db = KijukuDB::open_in_memory().unwrap();
    db.migrate().unwrap();
    create_media(&db, "srv-comic-A");
    create_media(&db, "srv-comic-B");
    create_media(&db, "srv-comic-C");
    let app = app_for(db);
    let session_id = login(&app).await;

    // 全件
    let res = get(&app, "/api/media", Some(&session_id)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["count"], 3);
    assert_eq!(body["total"], 3);

    // ページネーション（limit/offset）
    let res = get(&app, "/api/media?limit=2&offset=1", Some(&session_id)).await;
    let body = body_json(res).await;
    assert_eq!(body["count"], 2, "limit=2");
    assert_eq!(body["total"], 3, "total は全件数");

    // title フィルタ
    let res = get(&app, "/api/media?title=srv-comic-B", Some(&session_id)).await;
    let body = body_json(res).await;
    assert_eq!(body["count"], 1, "title フィルタで1件");
    assert_eq!(body["media"][0]["title"], "srv-comic-B");

    // order_by / order（id DESC で降順）
    let res = get(
        &app,
        "/api/media?orderBy=id&order=DESC",
        Some(&session_id),
    )
    .await;
    let body = body_json(res).await;
    let ids: Vec<_> = body["media"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![3, 2, 1], "orderBy=id&order=DESC で降順");
}

#[tokio::test]
async fn media_detail_by_id_returns_media_tags_attributes() {
    let db = KijukuDB::open_in_memory().unwrap();
    db.migrate().unwrap();
    let media = create_media(&db, "srv-detail");
    let tag = db.create_tag("srv-tag").unwrap();
    db.add_tag_to_media(media.id, tag.id).unwrap();
    db.set_media_attribute(media.id, "rating", Some("5"), Some(AttributeValueType::Integer))
        .unwrap();
    let app = app_for(db);
    let session_id = login(&app).await;

    let res = get(&app, &format!("/api/media/{}", media.id), Some(&session_id)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["media"]["id"], media.id);
    assert_eq!(body["media"]["title"], "srv-detail");
    assert_eq!(body["tags"][0]["name"], "srv-tag", "タグが付随する");
    assert_eq!(body["attributes"][0]["key"], "rating", "属性が付随する");

    // 存在しない ID は 404
    let res = get(&app, "/api/media/999999", Some(&session_id)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn media_detail_by_uuid_roundtrip() {
    let db = KijukuDB::open_in_memory().unwrap();
    db.migrate().unwrap();
    let media = create_media(&db, "srv-uuid");
    let app = app_for(db);
    let session_id = login(&app).await;

    let res = get(
        &app,
        &format!("/api/media/uuid/{}", media.uuid),
        Some(&session_id),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["media"]["uuid"], media.uuid, "uuid で取得できる");
    assert_eq!(body["media"]["id"], media.id);

    // 存在しない uuid は 404
    let res = get(
        &app,
        "/api/media/uuid/00000000-0000-0000-0000-000000000000",
        Some(&session_id),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn logout_invalidates_session() {
    let app = test_app();
    let session_id = login(&app).await;

    // logout はクッキー付きで呼ぶ
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header(header::COOKIE, format!("session_id={session_id}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 同一セッションでは以降拒否される
    let res = get(&app, "/api/media", Some(&session_id)).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "logout 後は 401");
}
