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

use crate::{App, AppError, page};

static PASSWORD_CHECKS: Semaphore = Semaphore::const_new(4);

#[derive(Clone)]
pub(crate) struct Auth {
    pub production: bool,
    origin: String,
    hash: Arc<str>,
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

pub(crate) fn print_password_hash() -> Result<(), Box<dyn Error>> {
    let password = rpassword::prompt_password("Contraseña nueva: ")?;
    if password.chars().count() < 15 || password.len() > 256 {
        return Err("Usa al menos 15 caracteres y un máximo de 256 bytes.".into());
    }
    if password != rpassword::prompt_password("Repite la contraseña: ")? {
        return Err("Las contraseñas no coinciden.".into());
    }
    println!("{}", hash_password(&password));
    Ok(())
}

impl Auth {
    pub(crate) fn from_env() -> Result<Self, Box<dyn Error>> {
        let production = match env::var("TASKAPP_MODE").as_deref() {
            Ok("production") => true,
            Ok("local") | Err(env::VarError::NotPresent) => false,
            _ => return Err("TASKAPP_MODE debe ser local o production".into()),
        };

        let (origin, hash) = if production {
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
            (origin, hash)
        } else {
            (String::new(), hash_password("admin"))
        };

        Ok(Self {
            production,
            origin,
            hash: hash.into(),
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

pub(crate) async fn authenticated(session: &Session) -> Result<bool, AppError> {
    Ok(session.get("authenticated").await?.unwrap_or(false))
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

pub(crate) async fn login_page(session: Session) -> Result<Response, AppError> {
    if authenticated(&session).await? {
        return Ok(Redirect::to("/").into_response());
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
    let hash = app.auth.hash.clone();
    let valid = form.password.len() <= 256
        && tokio::task::spawn_blocking(move || {
            let _permit = permit;
            verifies(&hash, &form.password)
        })
        .await?;
    if !valid || form.username != "admin" {
        let mut response = page(Login {
            csrf: form.csrf,
            username: &form.username,
            error: "El usuario o la contraseña son incorrectos.",
        })?;
        *response.status_mut() = StatusCode::UNAUTHORIZED;
        return Ok(response);
    }

    session.cycle_id().await?;
    session.remove::<String>("csrf").await?;
    session.insert("authenticated", true).await?;
    Ok(Redirect::to("/").into_response())
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
