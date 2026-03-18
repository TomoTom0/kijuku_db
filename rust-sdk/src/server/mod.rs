pub mod auth;

use crate::{KijukuDB, MediaFilter, MediaType, QueryOptions, SortKey, SortOrder};
pub use auth::{generate_password, AuthManager};
use axum::{
    extract::{FromRequestParts, Path, Query, State},
    http::{header, StatusCode},
    middleware,
    response::{Html, IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

static STATIC_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/server/static");

pub struct ServerState {
    db: Arc<Mutex<KijukuDB>>,
    auth: Arc<AuthManager>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    success: bool,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    error: String,
}

#[derive(Deserialize)]
pub struct MediaQuery {
    title: Option<String>,
    artist: Option<String>,
    media_type: Option<String>,
    series: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    #[serde(rename = "orderBy")]
    order_by: Option<String>,
    order: Option<String>,
}

pub struct ServerOptions {
    pub port: u16,
    pub password: Option<String>,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            port: 40001,
            password: None,
        }
    }
}

pub async fn start_server(db: KijukuDB, options: ServerOptions) {
    let password = options.password.unwrap_or_else(generate_password);
    let auth = Arc::new(AuthManager::new(password.clone()));

    // セッションクリーンアップタスクを起動
    let auth_for_cleanup = auth.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            auth_for_cleanup.cleanup_sessions(86400); // 24時間以上古いセッションを削除
        }
    });

    let state = Arc::new(ServerState {
        db: Arc::new(Mutex::new(db)),
        auth: auth.clone(),
    });

    // Protected routes that require authentication
    let protected_routes = Router::new()
        .route("/api/media", get(get_media_list))
        .route("/api/media/:id", get(get_media_detail))
        .layer(middleware::from_fn(auth_middleware));

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/static/*path", get(serve_static))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .merge(protected_routes)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", options.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    println!("\nKijuku DB Web GUI Server");
    println!("========================");
    println!("URL: http://localhost:{}", options.port);
    println!("Password: {}", password);
    println!("\nPress Ctrl+C to stop the server\n");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    println!("\nServer stopped gracefully");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
}

async fn auth_middleware(
    request: axum::extract::Request,
    next: middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    let (mut parts, body) = request.into_parts();

    // ExtensionからStateを取得（クローンして借用を解除）
    let state = parts.extensions.get::<Arc<ServerState>>().unwrap().clone();

    // CookieJarを取得（Infallibleなのでunwrap()が安全）
    let jar = CookieJar::from_request_parts(&mut parts, &()).await.unwrap();

    let request = axum::http::Request::from_parts(parts, body);

    let session_id = jar
        .get("session_id")
        .map(|cookie| cookie.value().to_string());

    if let Some(session_id) = session_id {
        if state.auth.validate_session(&session_id) {
            return Ok(next.run(request).await);
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}

async fn serve_index() -> Html<&'static str> {
    let content = STATIC_DIR
        .get_file("index.html")
        .and_then(|f| f.contents_utf8())
        .unwrap_or("<html><body>Error: index.html not found</body></html>");
    Html(content)
}

async fn serve_static(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    let file = STATIC_DIR.get_file(&path);

    match file {
        Some(file) => {
            let mime_type = if path.ends_with(".js") {
                "application/javascript"
            } else if path.ends_with(".css") {
                "text/css"
            } else if path.ends_with(".html") {
                "text/html"
            } else {
                "application/octet-stream"
            };

            (
                [(header::CONTENT_TYPE, mime_type)],
                file.contents(),
            ).into_response()
        }
        None => (StatusCode::NOT_FOUND, "File not found").into_response(),
    }
}

async fn login(
    State(state): State<Arc<ServerState>>,
    jar: CookieJar,
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<LoginResponse>), StatusCode> {
    if let Some(session_id) = state.auth.authenticate(&payload.password) {
        let cookie = Cookie::build(("session_id", session_id))
            .path("/")
            .http_only(true)
            .build();

        Ok((jar.add(cookie), Json(LoginResponse { success: true })))
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn logout(
    State(state): State<Arc<ServerState>>,
    jar: CookieJar,
) -> (CookieJar, Json<LoginResponse>) {
    if let Some(session_id) = jar.get("session_id") {
        state.auth.logout(session_id.value());
    }

    let mut cookie = Cookie::new("session_id", "");
    cookie.set_path("/");
    cookie.make_removal();

    (jar.remove(cookie), Json(LoginResponse { success: true }))
}

async fn get_media_list(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<MediaQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let media_type = params.media_type.and_then(|s| match s.as_str() {
        "comic" => Some(MediaType::Comic),
        "video" => Some(MediaType::Video),
        "music" => Some(MediaType::Music),
        _ => None,
    });

    let filter = MediaFilter {
        title: params.title,
        artist: params.artist,
        media_type,
        series: params.series,
        ..Default::default()
    };

    let sort_keys = if let Some(ref order_by) = params.order_by {
        let order = match params.order.as_deref() {
            Some("DESC") => SortOrder::Desc,
            _ => SortOrder::Asc,
        };
        vec![SortKey { field: order_by.clone(), order }]
    } else {
        vec![]
    };
    let options = QueryOptions {
        sort_keys,
        limit: params.limit.map(|v| v as i64),
        offset: params.offset.map(|v| v as i64),
    };

    // 全件数を取得（limitとoffsetなし）
    let total = match db.find_media(&filter, None) {
        Ok(all_media) => all_media.len(),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    match db.find_media(&filter, Some(&options)) {
        Ok(media) => {
            let count = media.len();
            Ok(Json(serde_json::json!({
                "media": media,
                "count": count,
                "total": total,
            })))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_media_detail(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match db.get_media(id) {
        Some(media) => {
            let tags = db.get_media_tags(id).unwrap_or_default();
            let attributes = db.get_media_attributes(id).unwrap_or_default();

            Ok(Json(serde_json::json!({
                "media": media,
                "tags": tags,
                "attributes": attributes,
            })))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}
