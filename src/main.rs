mod auth;
mod charts;
mod exports;
mod tasks;
mod users;

use std::{env, error::Error, net::SocketAddr, time::Duration};

use askama::Template;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, StatusCode, header},
    middleware,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use tower_sessions::{Expiry, SessionManagerLayer, cookie, session_store::ExpiredDeletion};
use tower_sessions_sqlx_store::SqliteStore;

#[derive(Clone)]
struct App {
    db: SqlitePool,
    auth: auth::Auth,
}

#[derive(Debug)]
struct AppError;

impl<E: std::error::Error> From<E> for AppError {
    fn from(error: E) -> Self {
        eprintln!("taskApp: {error}");
        Self
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "No se pudo completar la operación. Intenta nuevamente.",
        )
            .into_response()
    }
}

fn page(template: impl Template) -> Result<Response, AppError> {
    Ok(Html(template.render()?).into_response())
}

async fn css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../assets/app.css"),
    )
}

async fn js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../assets/app.js"),
    )
}

macro_rules! asset {
    ($name:ident, $file:literal, $mime:literal) => {
        async fn $name() -> impl IntoResponse {
            (
                [(header::CONTENT_TYPE, $mime)],
                include_bytes!(concat!("../assets/", $file)).as_slice(),
            )
        }
    };
}

asset!(
    manifest,
    "manifest.webmanifest",
    "application/manifest+json"
);
asset!(worker, "sw.js", "text/javascript; charset=utf-8");
asset!(offline_js, "offline.js", "text/javascript; charset=utf-8");
asset!(charts_js, "charts.js", "text/javascript; charset=utf-8");
asset!(charts_css, "charts.css", "text/css; charset=utf-8");
asset!(icon, "icon-192.png", "image/png");
asset!(apple_icon, "icon-180.png", "image/png");
asset!(large_icon, "icon-512.png", "image/png");
asset!(mask_icon, "icon-maskable.png", "image/png");
asset!(favicon, "icon-32.png", "image/png");
asset!(brand, "mark.svg", "image/svg+xml");
asset!(illustration, "jornada.png", "image/png");
asset!(workspace_css, "workspace.css", "text/css; charset=utf-8");
asset!(display_font, "bricolage-latin.woff2", "font/woff2");

async fn headers(request: axum::extract::Request, next: middleware::Next) -> Response {
    let mut response = next.run(request).await;
    for (name, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "same-origin"),
        (
            "content-security-policy",
            "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; manifest-src 'self'; worker-src 'self'; img-src 'self'; font-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
        ),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if env::args().nth(1).as_deref() == Some("hash-password") {
        return auth::print_password_hash();
    }

    let recovery = env::args().nth(1).as_deref() == Some("reset-admin-password");
    let bind: SocketAddr = env::var("TASKAPP_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()?;
    let db = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(env::var("TASKAPP_DB").unwrap_or_else(|_| "taskapp.sqlite".into()))
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5)),
    )
    .await?;
    sqlx::migrate!().run(&db).await?;
    if recovery {
        return auth::reset_admin_password(&db).await;
    }
    let auth = auth::Auth::from_env()?;
    auth::initialize_admin(&db, auth.production).await?;

    let table = if auth.production {
        "production_sessions"
    } else {
        "local_sessions"
    };
    let store = SqliteStore::new(db.clone()).with_table_name(table)?;
    store.migrate().await?;
    tokio::spawn(
        store
            .clone()
            .continuously_delete_expired(Duration::from_secs(300)),
    );
    let sessions = SessionManagerLayer::new(store)
        .with_name(if auth.production {
            "__Host-taskapp"
        } else {
            "taskapp"
        })
        .with_secure(auth.production)
        .with_same_site(cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(cookie::time::Duration::hours(8)));

    let app = Router::new()
        .route("/", get(tasks::home).post(tasks::create))
        .route("/historial", get(tasks::history))
        .route("/graficos", get(charts::dashboard))
        .route("/graficos/sin-conexion", get(charts::offline))
        .route("/api/graficos", get(charts::data))
        .route("/charts.js", get(charts_js))
        .route("/charts.css", get(charts_css))
        .route("/tareas/nueva", get(tasks::new_task).post(tasks::create))
        .route("/tareas/{id}", get(tasks::edit).post(tasks::update))
        .route("/exportar", get(exports::form))
        .route("/exportar/descargar", get(exports::download))
        .route("/api/offline", get(tasks::snapshot))
        .route("/api/tareas/sync", axum::routing::post(tasks::sync))
        .route("/api/tareas/lote", axum::routing::post(tasks::create_batch))
        .route("/usuarios", get(users::list).post(users::create))
        .route("/usuarios/nuevo", get(users::new_page))
        .route("/cuenta", get(users::profile).post(users::update_profile))
        .route(
            "/usuarios/{id}/{action}",
            get(users::edit_page).post(users::update),
        )
        .route(
            "/cuenta/contrasena",
            get(auth::password_page).post(auth::change_password),
        )
        .route("/login", get(auth::login_page).post(auth::login))
        .route("/salir", axum::routing::post(auth::logout))
        .route("/app.css", get(css))
        .route("/workspace.css", get(workspace_css))
        .route("/bricolage-latin.woff2", get(display_font))
        .route("/mark.svg", get(brand))
        .route("/jornada.png", get(illustration))
        .route("/icon-32.png", get(favicon))
        .route("/app.js", get(js))
        .route("/offline.js", get(offline_js))
        .route("/sin-conexion", get(tasks::offline_page))
        .route("/manifest.webmanifest", get(manifest))
        .route("/sw.js", get(worker))
        .route("/icon-180.png", get(apple_icon))
        .route("/icon-192.png", get(icon))
        .route("/icon-512.png", get(large_icon))
        .route("/icon-maskable.png", get(mask_icon))
        .with_state(App { db, auth })
        .layer(sessions)
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(middleware::from_fn(headers));

    let listener = tokio::net::TcpListener::bind(bind).await?;
    println!("taskApp escucha en {}", listener.local_addr()?);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
