use std::{
    collections::HashMap,
    env,
    error::Error,
    fs,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use askama::Template;
use axum::{
    Form,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use tokio::sync::Semaphore;
use tower_sessions::Session;
use uuid::Uuid;

use crate::{
    App, AppError, page,
    users::{self, User},
};

static PASSWORD_CHECKS: Semaphore = Semaphore::const_new(4);

#[derive(Clone)]
pub(crate) struct Auth {
    pub production: bool,
    origin: String,
    hash: Arc<str>,
    epoch: Uuid,
    attempts: Arc<Mutex<HashMap<IpAddr, (Instant, u8)>>>,
}

fn hash_password(password: &str) -> String {
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).expect("valid salt length");
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("valid Argon2id parameters")
        .to_string()
}

fn verifies(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}

pub(crate) fn valid_password(password: &str) -> bool {
    password.chars().count() >= 15 && password.len() <= 256
}

fn prompt_new_password() -> Result<String, Box<dyn Error>> {
    let password = rpassword::prompt_password("Contraseña nueva: ")?;
    if !valid_password(&password) {
        return Err("Usa al menos 15 caracteres y un máximo de 256 bytes.".into());
    }
    if password != rpassword::prompt_password("Repite la contraseña: ")? {
        return Err("Las contraseñas no coinciden.".into());
    }
    Ok(password)
}

pub(crate) fn print_password_hash() -> Result<(), Box<dyn Error>> {
    println!("{}", hash_password(&prompt_new_password()?));
    Ok(())
}

fn production_bootstrap_hash() -> Result<String, Box<dyn Error>> {
    let hash = fs::read_to_string(env::var("TASKAPP_PASSWORD_HASH_FILE")?)?;
    let hash = hash.trim().to_owned();
    let valid = PasswordHash::new(&hash).is_ok_and(|parsed| {
        parsed.algorithm.as_str() == "argon2id"
            && parsed.version == Some(19)
            && parsed.salt.is_some()
            && parsed.hash.is_some()
            && parsed
                .params
                .get_decimal("m")
                .is_some_and(|m| (19456..=262144).contains(&m))
            && parsed
                .params
                .get_decimal("t")
                .is_some_and(|t| (2..=10).contains(&t))
            && parsed
                .params
                .get_decimal("p")
                .is_some_and(|p| (1..=4).contains(&p))
    });
    if !valid || verifies(&hash, "admin") {
        return Err("Producción requiere un hash Argon2id válido con una contraseña distinta a la de prueba".into());
    }
    Ok(hash)
}

pub(crate) async fn initialize_admin(
    db: &sqlx::SqlitePool,
    production: bool,
) -> Result<(), Box<dyn Error>> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM admin_credentials WHERE id = 1")
            .fetch_optional(db)
            .await?;
    if let Some(hash) = existing {
        if production && verifies(&hash, "admin") {
            return Err(
                "Cambia la contraseña de pruebas antes de usar este volumen en producción.".into(),
            );
        }
        return Ok(());
    }
    let hash = if production {
        production_bootstrap_hash()?
    } else {
        hash_password("admin")
    };
    sqlx::query("INSERT INTO admin_credentials (id, password_hash) VALUES (1, ?) ON CONFLICT(id) DO NOTHING")
        .bind(hash).execute(db).await?;
    Ok(())
}

pub(crate) async fn reset_admin_password(db: &sqlx::SqlitePool) -> Result<(), Box<dyn Error>> {
    let hash = hash_password(&prompt_new_password()?);
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("INSERT INTO admin_credentials (id, password_hash) VALUES (1, ?) ON CONFLICT(id) DO UPDATE SET password_hash = excluded.password_hash")
        .bind(hash).execute(&mut *tx).await?;
    sqlx::query("UPDATE users SET version = version + 1 WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO user_events (actor_id, user_id, action) VALUES (1, 1, 'recuperar')")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    println!("Contraseña de administrador actualizada.");
    Ok(())
}

impl Auth {
    pub(crate) fn from_env() -> Result<Self, Box<dyn Error>> {
        let production = match env::var("TASKAPP_MODE").as_deref() {
            Ok("production") => true,
            Ok("local") | Err(env::VarError::NotPresent) => false,
            _ => return Err("TASKAPP_MODE debe ser local o production".into()),
        };

        let origin = if production {
            let origin = env::var("TASKAPP_ORIGIN")?;
            let uri: Uri = origin.parse()?;
            if uri.scheme_str() != Some("https")
                || uri.authority().is_none()
                || uri
                    .authority()
                    .is_some_and(|authority| authority.as_str().contains('@'))
                || uri.path() != "/"
                || uri.query().is_some()
                || origin.ends_with('/')
            {
                return Err("TASKAPP_ORIGIN debe ser un origen HTTPS sin barra final".into());
            }
            origin
        } else {
            String::new()
        };

        Ok(Self {
            production,
            origin,
            hash: hash_password(&Uuid::new_v4().to_string()).into(),
            epoch: Uuid::new_v4(),
            attempts: Default::default(),
        })
    }

    pub(crate) fn accepts_origin(&self, headers: &HeaderMap) -> bool {
        if headers
            .get("sec-fetch-site")
            .is_some_and(|value| value == "cross-site")
        {
            return false;
        }
        !self.production
            || headers
                .get("origin")
                .is_none_or(|origin| origin == self.origin.as_str())
    }

    fn allow_attempt(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut attempts = self.attempts.lock().expect("login rate limiter");
        attempts.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(300));
        if attempts.len() >= 4096 && !attempts.contains_key(&ip) {
            return false;
        }
        let (_, count) = attempts.entry(ip).or_insert((now, 0));
        if *count >= 8 {
            return false;
        }
        *count += 1;
        true
    }
}

async fn session_user(app: &App, session: &Session) -> Result<Option<User>, AppError> {
    let Some((id, version, epoch)) = session.get::<(i64, i64, Option<String>)>("user").await?
    else {
        return Ok(None);
    };
    Ok(users::by_id(&app.db, id).await?.filter(|user| {
        user.version == version
            && (!user.admin() || epoch.as_deref() == Some(&app.auth.epoch.to_string()))
    }))
}

pub(crate) async fn current(app: &App, session: &Session) -> Result<Option<User>, AppError> {
    Ok(session_user(app, session)
        .await?
        .filter(|user| user.temporary_until.is_none()))
}

async fn start_session(app: &App, session: &Session, user: &User) -> Result<(), AppError> {
    session.cycle_id().await?;
    session.clear().await;
    session
        .insert(
            "user",
            (
                user.id,
                user.version,
                user.admin().then(|| app.auth.epoch.to_string()),
            ),
        )
        .await?;
    Ok(())
}

pub(crate) async fn generate_hash(password: String) -> Result<Option<String>, AppError> {
    let Ok(permit) = PASSWORD_CHECKS.try_acquire() else {
        return Ok(None);
    };
    Ok(Some(
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            hash_password(&password)
        })
        .await?,
    ))
}

pub(crate) async fn csrf(session: &Session) -> Result<String, AppError> {
    if let Some(token) = session.get("csrf").await? {
        return Ok(token);
    }
    let token = Uuid::new_v4().to_string();
    session.insert("csrf", &token).await?;
    Ok(token)
}

pub(crate) async fn valid_post(
    auth: &Auth,
    headers: &HeaderMap,
    session: &Session,
    token: &str,
) -> Result<bool, AppError> {
    let expected: Option<String> = session.get("csrf").await?;
    Ok(auth.accepts_origin(headers) && expected.as_deref() == Some(token))
}

pub(crate) fn forbidden() -> Response {
    (
        StatusCode::FORBIDDEN,
        "La solicitud no es válida. Vuelve a abrir el formulario.",
    )
        .into_response()
}

#[derive(Template)]
#[template(path = "login.html")]
struct Login<'a> {
    csrf: String,
    username: &'a str,
    error: &'a str,
}

#[derive(Deserialize)]
pub(crate) struct Credentials {
    csrf: String,
    username: String,
    password: String,
}

pub(crate) async fn login_page(
    State(app): State<App>,
    session: Session,
) -> Result<Response, AppError> {
    if let Some(user) = session_user(&app, &session).await? {
        if user.temporary_until.is_none() {
            return Ok(Redirect::to("/").into_response());
        }
        if user
            .temporary_until
            .is_some_and(|expiry| expiry > chrono::Utc::now().timestamp())
        {
            return Ok(Redirect::to("/cuenta/contrasena").into_response());
        }
    }
    page(Login {
        csrf: csrf(&session).await?,
        username: "",
        error: "",
    })
}

pub(crate) async fn login(
    State(app): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    session: Session,
    headers: HeaderMap,
    Form(form): Form<Credentials>,
) -> Result<Response, AppError> {
    if !valid_post(&app.auth, &headers, &session, &form.csrf).await? {
        return Ok(forbidden());
    }

    let ip = if app.auth.production {
        // Only cloudflared can reach the app in the production Compose network.
        headers
            .get("cf-connecting-ip")
            .and_then(|value| value.to_str().ok())
            .and_then(|ip| ip.parse().ok())
    } else {
        Some(peer.ip())
    };
    let Some(ip) = ip else { return Ok(forbidden()) };
    if !app.auth.allow_attempt(ip) {
        let mut response = page(Login {
            csrf: form.csrf,
            username: &form.username,
            error: "Demasiados intentos. Intenta en unos minutos.",
        })?;
        *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        response.headers_mut().insert("retry-after", "300".parse()?);
        return Ok(response);
    }

    let Ok(permit) = PASSWORD_CHECKS.try_acquire() else {
        return Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            "Intenta iniciar sesión nuevamente en unos segundos.",
        )
            .into_response());
    };
    let user = users::by_username(&app.db, &form.username.trim().to_ascii_lowercase()).await?;
    let hash = user
        .as_ref()
        .and_then(|user| user.password_hash.as_deref())
        .map_or_else(|| app.auth.hash.clone(), Arc::from);
    let valid = form.password.len() <= 256
        && tokio::task::spawn_blocking(move || {
            let _permit = permit;
            verifies(&hash, &form.password)
        })
        .await?;
    let Some(user) = user.filter(|user| {
        valid
            && user
                .temporary_until
                .is_none_or(|expiry| expiry > chrono::Utc::now().timestamp())
    }) else {
        let mut response = page(Login {
            csrf: form.csrf,
            username: &form.username,
            error: "El usuario o la contraseña son incorrectos.",
        })?;
        *response.status_mut() = StatusCode::UNAUTHORIZED;
        return Ok(response);
    };

    start_session(&app, &session, &user).await?;
    let target = if user.temporary_until.is_some() {
        "/cuenta/contrasena"
    } else {
        "/"
    };
    Ok(Redirect::to(target).into_response())
}

pub(crate) async fn logout(
    State(app): State<App>,
    session: Session,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Result<Response, AppError> {
    if !valid_post(
        &app.auth,
        &headers,
        &session,
        form.get("csrf").map_or("", String::as_str),
    )
    .await?
    {
        return Ok(forbidden());
    }
    session.flush().await?;
    Ok(Redirect::to("/login").into_response())
}

#[derive(Template)]
#[template(path = "password.html")]
struct PasswordPage {
    csrf: String,
    temporary: bool,
    error: &'static str,
}

#[derive(Deserialize)]
pub(crate) struct PasswordChange {
    csrf: String,
    #[serde(default)]
    current: String,
    password: String,
    repeat: String,
}

async fn password_user(app: &App, session: &Session) -> Result<Option<User>, AppError> {
    Ok(session_user(app, session).await?.filter(|user| {
        user.temporary_until
            .is_none_or(|expiry| expiry > chrono::Utc::now().timestamp())
    }))
}

pub(crate) async fn password_page(
    State(app): State<App>,
    session: Session,
) -> Result<Response, AppError> {
    let Some(user) = password_user(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    page(PasswordPage {
        csrf: csrf(&session).await?,
        temporary: user.temporary_until.is_some(),
        error: "",
    })
}

pub(crate) async fn change_password(
    State(app): State<App>,
    session: Session,
    headers: HeaderMap,
    Form(form): Form<PasswordChange>,
) -> Result<Response, AppError> {
    let Some(user) = password_user(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    if !valid_post(&app.auth, &headers, &session, &form.csrf).await? {
        return Ok(forbidden());
    }
    let temporary = user.temporary_until.is_some();
    let invalid = |error| {
        let mut response = page(PasswordPage {
            csrf: form.csrf.clone(),
            temporary,
            error,
        })?;
        *response.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
        Ok(response)
    };
    if !valid_password(&form.password) || form.password != form.repeat {
        return invalid(
            "Usa al menos 15 caracteres y un máximo de 256 bytes. Las contraseñas deben coincidir.",
        );
    }
    let Some(current_hash) = user.password_hash.clone() else {
        return Ok(forbidden());
    };
    let Ok(permit) = PASSWORD_CHECKS.try_acquire() else {
        return Ok(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let hash = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if !temporary && (form.current.len() > 256 || !verifies(&current_hash, &form.current)) {
            return Err("La contraseña actual es incorrecta.");
        }
        if verifies(&current_hash, &form.password) {
            return Err("Elige una contraseña distinta de la anterior.");
        }
        Ok(hash_password(&form.password))
    })
    .await?;
    let hash = match hash {
        Ok(hash) => hash,
        Err(error) => return invalid(error),
    };
    let mut tx = app.db.begin_with("BEGIN IMMEDIATE").await?;
    let changed = sqlx::query(
        "UPDATE users SET password_hash = CASE WHEN id = 1 THEN NULL ELSE ? END,
         temporary_until = NULL, version = version + 1
         WHERE id = ? AND version = ? AND (temporary_until IS NULL OR temporary_until > ?)",
    )
    .bind(&hash)
    .bind(user.id)
    .bind(user.version)
    .bind(chrono::Utc::now().timestamp())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed == 0 {
        return Ok(Redirect::to("/login").into_response());
    }
    if user.admin() {
        sqlx::query("UPDATE admin_credentials SET password_hash = ? WHERE id = 1")
            .bind(hash)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "INSERT INTO user_events (actor_id, user_id, action) VALUES (?, ?, 'cambiar_contrasena')",
    )
    .bind(user.id)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let updated = User {
        version: user.version + 1,
        temporary_until: None,
        ..user
    };
    start_session(&app, &session, &updated).await?;
    if !temporary {
        session
            .insert("profile_notice", "Contraseña actualizada.")
            .await?;
    }
    Ok(Redirect::to(if temporary { "/" } else { "/cuenta" }).into_response())
}
