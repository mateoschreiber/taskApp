use std::collections::HashMap;

use askama::Template;
use axum::{
    Form, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use chrono::{Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use tower_sessions::Session;
use uuid::Uuid;

use crate::{App, AppError, auth, page};

const FIELDS: [(&str, &str); 6] = [
    ("store", "Tienda"),
    ("supervisor", "Supervisor General"),
    ("zone", "Zona"),
    ("work", "Trabajo realizado"),
    ("intervention", "Tipo de intervención"),
    ("status", "Estado"),
];
const ADD: &str = "__new__";
const PAGE_SIZE: i64 = 20;
const MONTHS: [&str; 12] = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];

type Values = HashMap<String, String>;

#[derive(FromRow, Serialize)]
pub(crate) struct Task {
    pub id: i64,
    pub revision: i64,
    pub activity_date: String,
    pub store: String,
    pub supervisor: String,
    pub zone: String,
    pub work: String,
    pub intervention: String,
    pub status: String,
    pub notes: String,
}

impl Task {
    fn display_date(&self) -> String {
        parse_date(&self.activity_date).map_or_else(
            || self.activity_date.clone(),
            |date| {
                format!(
                    "{} {} {}",
                    date.day(),
                    &MONTHS[date.month0() as usize][..3],
                    date.year()
                )
            },
        )
    }

    fn values(&self) -> Values {
        [
            ("date", self.activity_date.as_str()),
            ("store", &self.store),
            ("supervisor", &self.supervisor),
            ("zone", &self.zone),
            ("work", &self.work),
            ("intervention", &self.intervention),
            ("status", &self.status),
            ("notes", &self.notes),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .chain(std::iter::once((
            "revision".into(),
            self.revision.to_string(),
        )))
        .collect()
    }
}

#[derive(FromRow, Serialize)]
struct OptionRow {
    kind: String,
    label: String,
}

async fn catalog(db: &SqlitePool) -> Result<Vec<OptionRow>, AppError> {
    Ok(
        sqlx::query_as("SELECT kind, label FROM catalog ORDER BY key")
            .fetch_all(db)
            .await?,
    )
}

async fn find_task(db: &SqlitePool, id: i64) -> Result<Option<Task>, AppError> {
    Ok(sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?)
}

#[derive(Template)]
#[template(path = "task.html")]
struct TaskPage {
    csrf: String,
    editing: bool,
    action: String,
    saved: bool,
    date: String,
    notes: String,
    date_error: String,
    notes_error: String,
    has_errors: bool,
    fields: Vec<Choice>,
    revision: String,
    offline: bool,
    conflict: Option<Task>,
}

struct Choice {
    key: &'static str,
    label: &'static str,
    options: Vec<String>,
    selected: String,
    new_value: String,
    error: String,
}

fn value<'a>(values: &'a Values, key: &str) -> &'a str {
    values.get(key).map_or("", String::as_str)
}

async fn form_page(
    app: &App,
    session: &Session,
    id: Option<i64>,
    values: Values,
    errors: Values,
    saved: bool,
    conflict: Option<Task>,
) -> Result<Response, AppError> {
    let csrf = auth::csrf(session).await?;
    let options = catalog(&app.db).await?;
    let has_errors = !errors.is_empty();
    let has_conflict = conflict.is_some();
    let mut response = page(task_page(
        csrf, id, values, errors, saved, options, conflict,
    ))?;
    if has_conflict {
        *response.status_mut() = StatusCode::CONFLICT;
    } else if has_errors {
        *response.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
    }
    Ok(response)
}

fn task_page(
    csrf: String,
    id: Option<i64>,
    values: Values,
    errors: Values,
    saved: bool,
    catalog: Vec<OptionRow>,
    conflict: Option<Task>,
) -> TaskPage {
    let fields = FIELDS
        .iter()
        .map(|&(key, label)| Choice {
            key,
            label,
            options: catalog
                .iter()
                .filter(|option| option.kind == key)
                .map(|option| option.label.clone())
                .collect(),
            selected: value(&values, key).into(),
            new_value: value(&values, &format!("{key}_new")).into(),
            error: value(&errors, key).into(),
        })
        .collect();
    let has_errors = !errors.is_empty();
    TaskPage {
        offline: csrf.is_empty(),
        csrf,
        editing: id.is_some(),
        action: id.map_or_else(|| "/tareas/nueva".into(), |id| format!("/tareas/{id}")),
        saved,
        date: value(&values, "date").into(),
        notes: value(&values, "notes").into(),
        date_error: value(&errors, "date").into(),
        notes_error: value(&errors, "notes").into(),
        has_errors,
        fields,
        revision: value(&values, "revision").into(),
        conflict,
    }
}

pub(crate) async fn offline_page() -> Result<Response, AppError> {
    page(task_page(
        String::new(),
        None,
        Values::new(),
        Values::new(),
        false,
        vec![],
        None,
    ))
}

pub(crate) fn today() -> NaiveDate {
    Utc::now()
        .with_timezone(&chrono_tz::America::Asuncion)
        .date_naive()
}

pub(crate) fn parse_date(date: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .filter(|parsed| {
            date.len() == 10 && !date.starts_with("0000") && parsed.to_string() == date
        })
}

#[derive(Deserialize)]
pub(crate) struct NewQuery {
    saved: Option<i64>,
    tienda: Option<String>,
}

pub(crate) async fn new_task(
    State(app): State<App>,
    session: Session,
    Query(query): Query<NewQuery>,
) -> Result<Response, AppError> {
    if !auth::authenticated(&session).await? {
        return Ok(Redirect::to("/login").into_response());
    }

    let mut values = Values::from([
        ("date".into(), today().to_string()),
        ("supervisor".into(), "FLORENCIA SILVERO".into()),
        ("zone".into(), "INTERIOR".into()),
    ]);
    if let Some(store) = query.tienda {
        values.insert("store".into(), store);
    }
    let mut saved = false;
    if let Some(id) = query.saved
        && let Some(task) = find_task(&app.db, id).await?
    {
        values.insert("date".into(), task.activity_date);
        values.insert("store".into(), task.store);
        saved = true;
    }
    form_page(&app, &session, None, values, Values::new(), saved, None).await
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "La tarea ya no está disponible.").into_response()
}

pub(crate) async fn edit(
    State(app): State<App>,
    session: Session,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    if !auth::authenticated(&session).await? {
        return Ok(Redirect::to("/login").into_response());
    }
    let Some(task) = find_task(&app.db, id).await? else {
        return Ok(not_found());
    };
    form_page(
        &app,
        &session,
        Some(id),
        task.values(),
        Values::new(),
        false,
        None,
    )
    .await
}

pub(crate) async fn create(
    State(app): State<App>,
    session: Session,
    headers: HeaderMap,
    Form(values): Form<Values>,
) -> Result<Response, AppError> {
    save(app, session, headers, None, values).await
}

pub(crate) async fn update(
    State(app): State<App>,
    session: Session,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Form(values): Form<Values>,
) -> Result<Response, AppError> {
    save(app, session, headers, Some(id), values).await
}

async fn save(
    app: App,
    session: Session,
    headers: HeaderMap,
    id: Option<i64>,
    values: Values,
) -> Result<Response, AppError> {
    if !auth::authenticated(&session).await? {
        return Ok(Redirect::to("/login").into_response());
    }
    if !auth::valid_post(&app.auth, &headers, &session, value(&values, "csrf")).await? {
        return Ok(auth::forbidden());
    }

    let revision = value(&values, "revision").parse().ok();
    match persist(&app.db, id, revision, None, &values).await? {
        Save::Invalid(errors) => form_page(&app, &session, id, values, errors, false, None).await,
        Save::Missing => Ok(not_found()),
        Save::Conflict(task) => {
            let mut values = values;
            values.insert("revision".into(), task.revision.to_string());
            form_page(&app, &session, id, values, Values::new(), false, Some(task)).await
        }
        Save::Saved { id: saved_id, .. } => {
            let location = if id.is_some() {
                "/historial?edited=true".into()
            } else {
                format!("/tareas/nueva?saved={saved_id}")
            };
            Ok(Redirect::to(&location).into_response())
        }
    }
}

enum Save {
    Invalid(Values),
    Missing,
    Conflict(Task),
    Saved { id: i64, revision: i64 },
}

async fn persist(
    db: &SqlitePool,
    id: Option<i64>,
    revision: Option<i64>,
    operation: Option<Uuid>,
    values: &Values,
) -> Result<Save, AppError> {
    // Serialize writers so the receipt, version check and catalog changes are atomic.
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let operation = operation.map(|id| id.to_string());
    if let Some(key) = &operation
        && let Some((id, revision)) = sqlx::query_as::<_, (i64, i64)>(
            "SELECT task_id, revision FROM task_mutations WHERE operation = ?",
        )
        .bind(key)
        .fetch_optional(&mut *tx)
        .await?
    {
        return Ok(Save::Saved { id, revision });
    }
    if let Some(id) = id {
        let current: Option<Task> = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        match current {
            None => return Ok(Save::Missing),
            Some(task) if revision != Some(task.revision) => return Ok(Save::Conflict(task)),
            Some(_) => {}
        }
    }

    let mut errors = Values::new();
    let date = value(values, "date");
    if parse_date(date).is_none() {
        errors.insert("date".into(), "Ingresa una fecha válida.".into());
    }
    let notes = value(values, "notes");
    if notes.chars().count() > 2000 || notes.contains('\0') {
        errors.insert(
            "notes".into(),
            "Usa hasta 2.000 caracteres de texto.".into(),
        );
    }

    let mut labels = Vec::with_capacity(FIELDS.len());
    for (kind, _) in FIELDS {
        let selected = value(values, kind);
        let adding = selected == ADD;
        let label = if adding {
            value(values, &format!("{kind}_new")).trim()
        } else {
            selected.trim()
        };
        if label.is_empty()
            || label == ADD
            || label.chars().count() > 160
            || label.chars().any(char::is_control)
        {
            errors.insert(kind.into(), "Selecciona o agrega una opción válida.".into());
            continue;
        }
        let key = label.to_lowercase();
        if adding {
            sqlx::query("INSERT INTO catalog (kind, key, label) VALUES (?, ?, ?) ON CONFLICT (kind, key) DO NOTHING")
                .bind(kind).bind(&key).bind(label).execute(&mut *tx).await?;
        }
        let canonical: Option<String> =
            sqlx::query_scalar("SELECT label FROM catalog WHERE kind = ? AND key = ?")
                .bind(kind)
                .bind(&key)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(canonical) = canonical {
            labels.push(canonical);
        } else {
            errors.insert(
                kind.into(),
                "Selecciona una sugerencia o confirma el nuevo valor.".into(),
            );
        }
    }

    if !errors.is_empty() {
        tx.rollback().await?;
        return Ok(Save::Invalid(errors));
    }
    let sql = if id.is_some() {
        "UPDATE tasks SET activity_date=?, store=?, supervisor=?, zone=?, work=?, intervention=?, status=?, notes=?, revision=revision+1 WHERE id=? RETURNING id, revision"
    } else {
        "INSERT INTO tasks (activity_date, store, supervisor, zone, work, intervention, status, notes) VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id, revision"
    };
    let mut query = sqlx::query_as::<_, (i64, i64)>(sql).bind(date);
    for label in &labels {
        query = query.bind(label);
    }
    query = query.bind(notes);
    if let Some(id) = id {
        query = query.bind(id);
    }
    let (id, revision) = query.fetch_one(&mut *tx).await?;
    if let Some(key) = operation {
        sqlx::query("INSERT INTO task_mutations (operation, task_id, revision) VALUES (?, ?, ?)")
            .bind(key)
            .bind(id)
            .bind(revision)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Save::Saved { id, revision })
}

#[derive(Deserialize)]
pub(crate) struct Mutation {
    operation: String,
    id: Option<i64>,
    revision: Option<i64>,
    values: Values,
}

pub(crate) async fn sync(
    State(app): State<App>,
    session: Session,
    headers: HeaderMap,
    Json(change): Json<Mutation>,
) -> Result<Response, AppError> {
    use serde_json::json;
    if !auth::authenticated(&session).await? {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }
    let csrf = headers
        .get("x-csrf-token")
        .and_then(|token| token.to_str().ok())
        .unwrap_or("");
    if !auth::valid_post(&app.auth, &headers, &session, csrf).await? {
        return Ok(auth::forbidden());
    }
    let Ok(operation) = Uuid::parse_str(&change.operation) else {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    };
    if change.id.is_some() != change.revision.is_some()
        || change.id.is_some_and(|id| id < 1)
        || change.revision.is_some_and(|revision| revision < 1)
    {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }
    Ok(
        match persist(
            &app.db,
            change.id,
            change.revision,
            Some(operation),
            &change.values,
        )
        .await?
        {
            Save::Saved { id, revision } => {
                Json(json!({ "id": id, "revision": revision })).into_response()
            }
            Save::Invalid(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "errors": errors })),
            )
                .into_response(),
            Save::Conflict(task) => {
                (StatusCode::CONFLICT, Json(json!({ "task": task }))).into_response()
            }
            Save::Missing => StatusCode::NOT_FOUND.into_response(),
        },
    )
}

pub(crate) async fn in_range(
    db: &SqlitePool,
    from: NaiveDate,
    through: NaiveDate,
) -> Result<Vec<Task>, AppError> {
    Ok(sqlx::query_as("SELECT * FROM tasks WHERE activity_date BETWEEN ? AND ? ORDER BY activity_date DESC, id DESC")
        .bind(from.to_string()).bind(through.to_string()).fetch_all(db).await?)
}

pub(crate) async fn snapshot(
    State(app): State<App>,
    session: Session,
) -> Result<Response, AppError> {
    if !auth::authenticated(&session).await? {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }
    let date = today();
    let tasks = in_range(&app.db, date - chrono::Days::new(29), date).await?;
    let catalog = catalog(&app.db).await?;
    // Refresh the session only after obtaining an authenticated snapshot.
    session
        .insert("snapshot_at", Utc::now().timestamp())
        .await?;
    Ok(Json(serde_json::json!({
        "date": date.to_string(), "tasks": tasks, "catalog": catalog,
        "csrf": auth::csrf(&session).await?,
    }))
    .into_response())
}

#[derive(Template)]
#[template(path = "today.html")]
struct Today {
    csrf: String,
    date: String,
    tasks: Vec<Task>,
    stores: Vec<StoreCard>,
    selected: String,
    add_url: String,
    total: usize,
    active_stores: usize,
}

struct StoreCard {
    label: String,
    url: String,
    count: usize,
}

#[derive(Deserialize)]
pub(crate) struct TodayQuery {
    tienda: Option<String>,
}

pub(crate) async fn home(
    State(app): State<App>,
    session: Session,
    Query(query): Query<TodayQuery>,
) -> Result<Response, AppError> {
    if !auth::authenticated(&session).await? {
        return Ok(Redirect::to("/login").into_response());
    }
    let date = today();
    let mut tasks = in_range(&app.db, date, date).await?;
    let mut counts = HashMap::new();
    for task in &tasks {
        *counts.entry(task.store.as_str()).or_insert(0) += 1;
    }
    let stores: Vec<_> = catalog(&app.db)
        .await?
        .into_iter()
        .filter(|row| row.kind == "store")
        .map(|row| StoreCard {
            count: counts.get(row.label.as_str()).copied().unwrap_or(0),
            url: format!("/?tienda={}", urlencoding::encode(&row.label)),
            label: row.label,
        })
        .collect();
    let selected = query
        .tienda
        .filter(|name| stores.iter().any(|store| &store.label == name))
        .unwrap_or_default();
    let total = tasks.len();
    let active_stores = counts.len();
    if !selected.is_empty() {
        tasks.retain(|task| task.store == selected);
    }
    let add_url = if selected.is_empty() {
        "/tareas/nueva".into()
    } else {
        format!("/tareas/nueva?tienda={}", urlencoding::encode(&selected))
    };
    page(Today {
        csrf: auth::csrf(&session).await?,
        date: format!(
            "{}, {} de {}",
            [
                "Lunes",
                "Martes",
                "Miércoles",
                "Jueves",
                "Viernes",
                "Sábado",
                "Domingo"
            ][date.weekday().num_days_from_monday() as usize],
            date.day(),
            MONTHS[date.month0() as usize],
        ),
        tasks,
        stores,
        selected,
        add_url,
        total,
        active_stores,
    })
}

#[derive(Template)]
#[template(path = "history.html")]
struct History {
    csrf: String,
    tasks: Vec<Task>,
    number: i64,
    prev: i64,
    next: i64,
    has_next: bool,
    edited: bool,
    from: String,
    through: String,
    pdf: bool,
    error: &'static str,
}

#[derive(Deserialize)]
pub(crate) struct HistoryQuery {
    page: Option<i64>,
    edited: Option<bool>,
}

pub(crate) async fn history(
    State(app): State<App>,
    session: Session,
    Query(query): Query<HistoryQuery>,
) -> Result<Response, AppError> {
    if !auth::authenticated(&session).await? {
        return Ok(Redirect::to("/login").into_response());
    }
    let number = query.page.unwrap_or(1).clamp(1, i64::MAX / PAGE_SIZE);
    let mut tasks =
        sqlx::query_as("SELECT * FROM tasks ORDER BY activity_date DESC, id DESC LIMIT ? OFFSET ?")
            .bind(PAGE_SIZE + 1)
            .bind((number - 1) * PAGE_SIZE)
            .fetch_all(&app.db)
            .await?;
    let has_next = tasks.len() > PAGE_SIZE as usize;
    tasks.truncate(PAGE_SIZE as usize);
    let (from, through) = crate::exports::defaults();
    page(History {
        csrf: auth::csrf(&session).await?,
        tasks,
        number,
        prev: number - 1,
        next: number + 1,
        has_next,
        edited: query.edited.unwrap_or(false),
        from,
        through,
        pdf: false,
        error: "",
    })
}
