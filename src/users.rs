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

use crate::{App, AppError, auth, page};

pub(crate) const ADMIN: i64 = 1;

#[derive(Clone, FromRow)]
pub(crate) struct User {
    pub id: i64,
    pub name: String,
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
    Ok(sqlx::query_as("SELECT users.id, users.name, COALESCE(admin_credentials.password_hash, users.password_hash) AS password_hash, users.version, users.temporary_until FROM users LEFT JOIN admin_credentials USING (id) WHERE users.username = ?")
        .bind(username)
        .fetch_optional(db)
        .await?)
}

pub(crate) async fn by_id(db: &SqlitePool, id: i64) -> Result<Option<User>, AppError> {
    Ok(sqlx::query_as("SELECT users.id, users.name, COALESCE(admin_credentials.password_hash, users.password_hash) AS password_hash, users.version, users.temporary_until FROM users LEFT JOIN admin_credentials USING (id) WHERE users.id = ?")
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
        sqlx::query_as("SELECT id, username, COALESCE(NULLIF(name, ''), username) AS name FROM users ORDER BY username")
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
    notice: String,
}

#[derive(Template)]
#[template(path = "user_form.html")]
struct UserForm {
    account_id: i64,
    csrf: String,
    action: String,
    action_url: String,
    title: &'static str,
    username: String,
    target: String,
    error: &'static str,
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
    page(UsersPage {
        account_id: ADMIN,
        csrf: auth::csrf(&session).await?,
        accounts: accounts(&app.db, true).await?,
        notice: session
            .remove::<String>("users_notice")
            .await?
            .unwrap_or_default(),
    })
}

#[derive(Default, Deserialize)]
pub(crate) struct Change {
    #[serde(default)]
    csrf: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    repeat: String,
}

pub(crate) async fn new_page(
    State(app): State<App>,
    session: Session,
) -> Result<Response, AppError> {
    action_page(app, session, None, "crear").await
}

pub(crate) async fn edit_page(
    State(app): State<App>,
    session: Session,
    Path((id, action)): Path<(i64, String)>,
) -> Result<Response, AppError> {
    action_page(app, session, Some(id), &action).await
}

async fn action_page(
    app: App,
    session: Session,
    id: Option<i64>,
    action: &str,
) -> Result<Response, AppError> {
    if let Some(response) = denied(auth::current(&app, &session).await?.as_ref()) {
        return Ok(response);
    }
    match form_page(&app, &session, id, action).await? {
        Ok(form) => page(form),
        Err(status) => Ok(status.into_response()),
    }
}

async fn form_page(
    app: &App,
    session: &Session,
    id: Option<i64>,
    action: &str,
) -> Result<Result<UserForm, StatusCode>, AppError> {
    if id == Some(ADMIN) {
        return Ok(Err(StatusCode::FORBIDDEN));
    }
    let title = match (id, action) {
        (None, "crear") => "Nueva cuenta",
        (Some(_), "editar") => "Editar usuario",
        (Some(_), "restablecer") => "Restablecer contraseña",
        (Some(_), "eliminar") => "Eliminar cuenta",
        _ => return Ok(Err(StatusCode::NOT_FOUND)),
    };
    let target: String = if let Some(id) = id {
        let username = sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&app.db)
            .await?;
        let Some(username) = username else {
            return Ok(Err(StatusCode::NOT_FOUND));
        };
        username
    } else {
        String::new()
    };
    Ok(Ok(UserForm {
        account_id: ADMIN,
        csrf: auth::csrf(session).await?,
        action: action.to_owned(),
        action_url: id.map_or_else(
            || "/usuarios".to_owned(),
            |id| format!("/usuarios/{id}/{action}"),
        ),
        title,
        username: if action == "editar" {
            target.clone()
        } else {
            String::new()
        },
        target,
        error: "",
    }))
}

fn invalid(mut view: UserForm, form: Change, error: &'static str) -> Result<Response, AppError> {
    view.username = form.username;
    view.error = error;
    let mut response = page(view)?;
    *response.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
    Ok(response)
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
    if !auth::valid_post(&app.auth, &headers, &session, &form.csrf).await? {
        return Ok(auth::forbidden());
    }
    let view = match form_page(&app, &session, id, action).await? {
        Ok(view) => view,
        Err(status) => return Ok(status.into_response()),
    };
    if matches!(action, "crear" | "editar") {
        form.username = form.username.trim().to_ascii_lowercase();
        if !(3..=32).contains(&form.username.len())
            || !form
                .username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            || form.username == "admin"
        {
            return invalid(
                view,
                form,
                "Usa de 3 a 32 letras sin acentos, números, guiones o guiones bajos. «admin» está reservado.",
            );
        }
    }
    let hash = if matches!(action, "crear" | "restablecer") {
        if !auth::valid_password(&form.password) || form.password != form.repeat {
            return invalid(
                view,
                form,
                "Usa al menos 15 caracteres y un máximo de 256 bytes. Las contraseñas deben coincidir.",
            );
        }
        let Some(hash) = auth::generate_hash(std::mem::take(&mut form.password)).await? else {
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
            return invalid(view, form, "Ese usuario ya existe.");
        }
    }
    if action == "eliminar" {
        let username: Option<String> =
            sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(username) = username else {
            return Ok(StatusCode::NOT_FOUND.into_response());
        };
        if form.username.trim() != username {
            tx.rollback().await?;
            return invalid(
                UserForm {
                    target: username,
                    ..view
                },
                form,
                "Confirma el nombre de usuario antes de eliminar la cuenta.",
            );
        }
    }

    let (changed, notice): (Option<i64>, &str) = match action {
        "crear" => (sqlx::query_scalar("INSERT INTO users (username, name, password_hash, temporary_until) VALUES (?, '', ?, ?) RETURNING id")
            .bind(&form.username).bind(hash).bind(expiry).fetch_optional(&mut *tx).await?, "Cuenta creada. Comparte la contraseña temporal por privado."),
        "editar" => (sqlx::query_scalar("UPDATE users SET username = ? WHERE id = ? RETURNING id")
            .bind(&form.username).bind(id).fetch_optional(&mut *tx).await?, "Usuario actualizado."),
        "restablecer" => (sqlx::query_scalar("UPDATE users SET password_hash = ?, temporary_until = ?, version = version + 1 WHERE id = ? RETURNING id")
            .bind(hash).bind(expiry).bind(id).fetch_optional(&mut *tx).await?, "Contraseña restablecida. Comparte la contraseña temporal por privado."),
        "eliminar" => (sqlx::query_scalar("DELETE FROM users WHERE id = ? RETURNING id")
            .bind(id).fetch_optional(&mut *tx).await?, "Cuenta eliminada. Sus tareas se conservaron."),
        _ => unreachable!(),
    };
    let Some(changed) = changed else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    sqlx::query("INSERT INTO user_events (actor_id, user_id, action) VALUES (?, ?, ?)")
        .bind(ADMIN)
        .bind(changed)
        .bind(action)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    session.insert("users_notice", notice).await?;
    Ok(Redirect::to("/usuarios").into_response())
}

#[derive(Template)]
#[template(path = "profile.html")]
struct Profile {
    account_id: i64,
    csrf: String,
    name: String,
    notice: String,
    error: &'static str,
}

#[derive(Deserialize)]
pub(crate) struct ProfileChange {
    csrf: String,
    name: String,
}

pub(crate) async fn profile(
    State(app): State<App>,
    session: Session,
) -> Result<Response, AppError> {
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    page(Profile {
        account_id: user.id,
        csrf: auth::csrf(&session).await?,
        name: user.name,
        notice: session
            .remove::<String>("profile_notice")
            .await?
            .unwrap_or_default(),
        error: "",
    })
}

pub(crate) async fn update_profile(
    State(app): State<App>,
    session: Session,
    headers: HeaderMap,
    Form(form): Form<ProfileChange>,
) -> Result<Response, AppError> {
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    if !auth::valid_post(&app.auth, &headers, &session, &form.csrf).await? {
        return Ok(auth::forbidden());
    }
    let name = form.name.trim();
    if !(1..=80).contains(&name.chars().count()) || name.chars().any(char::is_control) {
        let mut response = page(Profile {
            account_id: user.id,
            csrf: form.csrf,
            name: name.to_owned(),
            notice: String::new(),
            error: "Ingresa un nombre visible de 1 a 80 caracteres.",
        })?;
        *response.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
        return Ok(response);
    }
    let changed = sqlx::query("UPDATE users SET name = ? WHERE id = ? AND version = ?")
        .bind(name)
        .bind(user.id)
        .bind(user.version)
        .execute(&app.db)
        .await?
        .rows_affected();
    if changed == 0 {
        return Ok(Redirect::to("/login").into_response());
    }
    session
        .insert("profile_notice", "Nombre visible actualizado.")
        .await?;
    Ok(Redirect::to("/cuenta").into_response())
}
