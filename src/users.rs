use askama::Template;
use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use chrono::Utc;
use serde::Deserialize;
use sqlx::{FromRow, SqlitePool};
use tower_sessions::Session;
use uuid::Uuid;

use crate::{App, AppError, auth, page};

pub(crate) const ADMIN: i64 = 1;

#[derive(Clone, FromRow)]
pub(crate) struct User {
    pub id: i64,
    pub password_hash: Option<String>,
    pub version: i64,
    pub temporary_until: Option<i64>,
}

impl User {
    pub(crate) fn admin(&self) -> bool {
        self.id == ADMIN
    }

    pub(crate) fn scope(&self, selection: Option<&str>) -> Option<Option<i64>> {
        let selection = selection.filter(|value| !value.is_empty());
        match (self.admin(), selection) {
            (true, None) => Some(None),
            (true, Some(id)) => id.parse().ok().filter(|id: &i64| *id >= 0).map(Some),
            (false, None) => Some(Some(self.id)),
            (false, Some(_)) => None,
        }
    }
}

pub(crate) async fn by_username(db: &SqlitePool, username: &str) -> Result<Option<User>, AppError> {
    Ok(sqlx::query_as("SELECT * FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(db)
        .await?)
}

pub(crate) async fn by_id(db: &SqlitePool, id: i64) -> Result<Option<User>, AppError> {
    Ok(sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?)
}

#[derive(FromRow)]
pub(crate) struct Account {
    pub id: i64,
    pub username: String,
    pub name: String,
}

pub(crate) async fn accounts(db: &SqlitePool, admin: bool) -> Result<Vec<Account>, AppError> {
    if !admin {
        return Ok(Vec::new());
    }
    Ok(
        sqlx::query_as("SELECT id, username, name FROM users ORDER BY id")
            .fetch_all(db)
            .await?,
    )
}

#[derive(Template)]
#[template(path = "users.html")]
struct UsersPage {
    account_id: i64,
    csrf: String,
    accounts: Vec<Account>,
    temporary: String,
    issued_for: String,
    username: String,
    name: String,
    error: &'static str,
}

async fn render(
    app: &App,
    session: &Session,
    form: Change,
    temporary: String,
    error: &'static str,
) -> Result<Response, AppError> {
    let mut response = page(UsersPage {
        account_id: ADMIN,
        csrf: auth::csrf(session).await?,
        accounts: accounts(&app.db, true).await?,
        temporary,
        issued_for: form.username.clone(),
        username: form.username,
        name: form.name,
        error,
    })?;
    if !error.is_empty() {
        *response.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
    }
    Ok(response)
}

fn denied(user: Option<&User>) -> Option<Response> {
    match user {
        None => Some(Redirect::to("/login").into_response()),
        Some(user) if !user.admin() => Some(auth::forbidden()),
        Some(_) => None,
    }
}

pub(crate) async fn list(State(app): State<App>, session: Session) -> Result<Response, AppError> {
    if let Some(response) = denied(auth::current(&app, &session).await?.as_ref()) {
        return Ok(response);
    }
    render(&app, &session, Change::default(), String::new(), "").await
}

#[derive(Default, Deserialize)]
pub(crate) struct Change {
    #[serde(default)]
    csrf: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    name: String,
}

pub(crate) async fn create(
    State(app): State<App>,
    session: Session,
    headers: HeaderMap,
    Form(form): Form<Change>,
) -> Result<Response, AppError> {
    modify(app, session, headers, None, "crear", form).await
}

pub(crate) async fn update(
    State(app): State<App>,
    session: Session,
    headers: HeaderMap,
    Path((id, action)): Path<(i64, String)>,
    Form(form): Form<Change>,
) -> Result<Response, AppError> {
    modify(app, session, headers, Some(id), &action, form).await
}

async fn modify(
    app: App,
    session: Session,
    headers: HeaderMap,
    id: Option<i64>,
    action: &str,
    mut form: Change,
) -> Result<Response, AppError> {
    if let Some(response) = denied(auth::current(&app, &session).await?.as_ref()) {
        return Ok(response);
    }
    if !auth::valid_post(&app.auth, &headers, &session, &form.csrf).await? || id == Some(ADMIN) {
        return Ok(auth::forbidden());
    }
    if !matches!(
        (id, action),
        (None, "crear") | (Some(_), "editar" | "restablecer" | "eliminar")
    ) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    if matches!(action, "crear" | "editar") {
        form.username = form.username.trim().to_ascii_lowercase();
        form.name = form.name.trim().to_owned();
        let valid = (3..=32).contains(&form.username.len())
            && form
                .username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            && form.username != "admin"
            && (1..=80).contains(&form.name.chars().count())
            && !form.name.chars().any(char::is_control);
        if !valid {
            return render(
                &app,
                &session,
                form,
                String::new(),
                "Indica un nombre y un usuario válido de 3 a 32 caracteres.",
            )
            .await;
        }
    }

    let mut temporary = String::new();
    let hash = if matches!(action, "crear" | "restablecer") {
        temporary = Uuid::new_v4().simple().to_string();
        let Some(hash) = auth::generate_hash(temporary.clone()).await? else {
            return Ok(StatusCode::SERVICE_UNAVAILABLE.into_response());
        };
        hash
    } else {
        String::new()
    };
    let expiry = Utc::now().timestamp() + 86_400;
    let mut tx = app.db.begin_with("BEGIN IMMEDIATE").await?;
    if matches!(action, "crear" | "editar") {
        let duplicate: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE username = ? AND id <> ?)")
                .bind(&form.username)
                .bind(id.unwrap_or(0))
                .fetch_one(&mut *tx)
                .await?;
        if duplicate {
            tx.rollback().await?;
            return render(
                &app,
                &session,
                form,
                String::new(),
                "Ese usuario ya existe.",
            )
            .await;
        }
    }

    if matches!(action, "restablecer" | "eliminar") {
        let username: Option<String> =
            sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(username) = username else {
            return Ok(StatusCode::NOT_FOUND.into_response());
        };
        if action == "eliminar" && form.username.trim() != username {
            tx.rollback().await?;
            return render(
                &app,
                &session,
                Change::default(),
                String::new(),
                "Confirma el nombre de la cuenta antes de eliminarla.",
            )
            .await;
        }
        form.username = username;
    }

    let changed: Option<i64> = match action {
        "crear" => sqlx::query_scalar("INSERT INTO users (username, name, password_hash, temporary_until) VALUES (?, ?, ?, ?) RETURNING id")
            .bind(&form.username).bind(&form.name).bind(hash).bind(expiry).fetch_optional(&mut *tx).await?,
        "editar" => sqlx::query_scalar("UPDATE users SET username = ?, name = ? WHERE id = ? RETURNING id")
            .bind(&form.username).bind(&form.name).bind(id).fetch_optional(&mut *tx).await?,
        "restablecer" => sqlx::query_scalar("UPDATE users SET password_hash = ?, temporary_until = ?, version = version + 1 WHERE id = ? RETURNING id")
            .bind(hash).bind(expiry).bind(id).fetch_optional(&mut *tx).await?,
        "eliminar" => sqlx::query_scalar("DELETE FROM users WHERE id = ? RETURNING id")
            .bind(id).fetch_optional(&mut *tx).await?,
        _ => unreachable!(),
    };
    let Some(changed) = changed else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    sqlx::query("INSERT INTO user_events (actor_id, user_id, action) VALUES (1, ?, ?)")
        .bind(changed)
        .bind(action)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    if temporary.is_empty() {
        Ok(Redirect::to("/usuarios").into_response())
    } else {
        render(&app, &session, form, temporary, "").await
    }
}
