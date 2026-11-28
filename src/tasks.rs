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
    #[serde(skip)]
    pub owner_id: Option<i64>,
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

async fn catalog(db: &SqlitePool, owner: i64) -> Result<Vec<OptionRow>, AppError> {
    Ok(
        sqlx::query_as("SELECT kind, label FROM (SELECT kind, key, label FROM catalog UNION ALL SELECT kind, key, label FROM user_catalog WHERE user_id = ?) ORDER BY key").bind(owner)
            .fetch_all(db)
            .await?,
    )
}

async fn find_task(db: &SqlitePool, id: i64, owner: Option<i64>) -> Result<Option<Task>, AppError> {
    Ok(
        sqlx::query_as("SELECT * FROM tasks WHERE id = ? AND (? IS NULL OR owner_id = ?)")
            .bind(id)
            .bind(owner)
            .bind(owner)
            .fetch_optional(db)
            .await?,
    )
}

#[derive(Template)]
#[template(path = "task.html")]
struct TaskPage {
    account_id: i64,
    batch_limit: usize,
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
    let Some(user) = auth::current(app, session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let options = catalog(&app.db, user.id).await?;
    let has_errors = !errors.is_empty();
    let has_conflict = conflict.is_some();
    let mut view = task_page(csrf, id, values, errors, saved, options, conflict);
    view.account_id = user.id;
    let mut response = page(view)?;
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
        account_id: 0,
        batch_limit: BATCH_LIMIT,
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
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };

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
        && let Some(task) = find_task(&app.db, id, Some(user.id)).await?
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
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let Some(task) = find_task(&app.db, id, user.scope(None).flatten()).await? else {
        return Ok(not_found());
    };
    if task.owner_id != Some(user.id) {
        return page(TaskDetail {
            account_id: user.id,
            csrf: auth::csrf(&session).await?,
            task,
        });
    }
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
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    if !auth::valid_post(&app.auth, &headers, &session, value(&values, "csrf")).await? {
        return Ok(auth::forbidden());
    }

    let revision = value(&values, "revision").parse().ok();
    match persist(&app.db, &user, id, revision, None, &values).await? {
        Save::Unauthorized => Ok(Redirect::to("/login").into_response()),
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
    Unauthorized,
    Invalid(Values),
    Missing,
    Conflict(Task),
    Saved { id: i64, revision: i64 },
}

async fn persist(
    db: &SqlitePool,
    user: &crate::users::User,
    id: Option<i64>,
    revision: Option<i64>,
    operation: Option<Uuid>,
    values: &Values,
) -> Result<Save, AppError> {
    // Serialize writers so the receipt, version check and catalog changes are atomic.
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    if !can_write(&mut tx, user).await? {
        return Ok(Save::Unauthorized);
    }
    let operation = operation.map(|id| id.to_string());
    let result = persist_task(&mut tx, user.id, id, revision, operation.as_deref(), values).await?;
    if matches!(result, Save::Saved { .. }) {
        tx.commit().await?;
    }
    Ok(result)
}

async fn can_write(
    tx: &mut sqlx::SqliteConnection,
    user: &crate::users::User,
) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = ? AND version = ? AND temporary_until IS NULL)")
        .bind(user.id).bind(user.version).fetch_one(tx).await?)
}

async fn persist_task(
    tx: &mut sqlx::SqliteConnection,
    user_id: i64,
    id: Option<i64>,
    revision: Option<i64>,
    operation: Option<&str>,
    values: &Values,
) -> Result<Save, AppError> {
    if let Some(key) = operation
        && let Some((id, revision)) = sqlx::query_as::<_, (i64, i64)>(
            "SELECT task_id, revision FROM task_mutations WHERE user_id = ? AND operation = ?",
        )
        .bind(user_id)
        .bind(key)
        .fetch_optional(&mut *tx)
        .await?
    {
        return Ok(Save::Saved { id, revision });
    }
    if let Some(id) = id {
        let current: Option<Task> =
            sqlx::query_as("SELECT * FROM tasks WHERE id = ? AND owner_id = ?")
                .bind(id)
                .bind(user_id)
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
            sqlx::query("INSERT INTO user_catalog (user_id, kind, key, label) SELECT ?, ?, ?, ? WHERE NOT EXISTS (SELECT 1 FROM catalog WHERE kind = ? AND key = ?) ON CONFLICT (user_id, kind, key) DO NOTHING")
                .bind(user_id).bind(kind).bind(&key).bind(label).bind(kind).bind(&key).execute(&mut *tx).await?;
        }
        let canonical: Option<String> =
            sqlx::query_scalar("SELECT label FROM catalog WHERE kind = ? AND key = ? UNION ALL SELECT label FROM user_catalog WHERE user_id = ? AND kind = ? AND key = ?")
                .bind(kind)
                .bind(&key).bind(user_id).bind(kind).bind(&key)
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
        return Ok(Save::Invalid(errors));
    }
    let sql = if id.is_some() {
        "UPDATE tasks SET activity_date=?, store=?, supervisor=?, zone=?, work=?, intervention=?, status=?, notes=?, revision=revision+1 WHERE id=? AND owner_id=? RETURNING id, revision"
    } else {
        "INSERT INTO tasks (activity_date, store, supervisor, zone, work, intervention, status, notes, owner_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id, revision"
    };
    let mut query = sqlx::query_as::<_, (i64, i64)>(sql).bind(date);
    for label in &labels {
        query = query.bind(label);
    }
    query = query.bind(notes);
    if let Some(id) = id {
        query = query.bind(id);
    }
    let (id, revision) = query.bind(user_id).fetch_one(&mut *tx).await?;
    if let Some(key) = operation {
        sqlx::query("INSERT INTO task_mutations (user_id, operation, task_id, revision) VALUES (?, ?, ?, ?)")
            .bind(user_id).bind(key)
            .bind(id)
            .bind(revision)
            .execute(&mut *tx)
            .await?;
    }
    Ok(Save::Saved { id, revision })
}

// Bound the write transaction and stay below Axum's JSON body limit with full notes.
const BATCH_LIMIT: usize = 100;

#[derive(Deserialize)]
pub(crate) struct TaskBatch {
    operation: String,
    account_id: i64,
    shared: Values,
    tasks: Vec<Values>,
}

#[derive(Serialize, FromRow)]
struct Receipt {
    id: i64,
    revision: i64,
}

pub(crate) async fn create_batch(
    State(app): State<App>,
    session: Session,
    headers: HeaderMap,
    Json(batch): Json<TaskBatch>,
) -> Result<Response, AppError> {
    use serde_json::json;
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let csrf = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if batch.account_id != user.id || !auth::valid_post(&app.auth, &headers, &session, csrf).await?
    {
        return Ok(auth::forbidden());
    }
    if !(1..=BATCH_LIMIT).contains(&batch.tasks.len()) {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }
    let Ok(operation) = Uuid::parse_str(&batch.operation) else {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    };
    let mut tx = app.db.begin_with("BEGIN IMMEDIATE").await?;
    if !can_write(&mut tx, &user).await? {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }
    // Existing receipts suffice. The prefix distinguishes batches from legacy edits/creates.
    let prefix = format!("batch:{operation}:");
    let receipts: Vec<Receipt> = sqlx::query_as("SELECT task_id AS id, revision FROM task_mutations WHERE user_id = ? AND operation GLOB ? ORDER BY operation")
        .bind(user.id).bind(format!("{prefix}*")).fetch_all(&mut *tx).await?;
    if !receipts.is_empty() {
        return Ok(if receipts.len() == batch.tasks.len() {
            Json(json!({ "tasks": receipts })).into_response()
        } else {
            StatusCode::CONFLICT.into_response()
        });
    }

    let mut errors = Vec::new();
    let mut receipts = Vec::new();
    let mut values = batch.shared;
    for (index, task) in batch.tasks.iter().enumerate() {
        for field in [
            "work",
            "work_new",
            "intervention",
            "intervention_new",
            "status",
            "status_new",
            "notes",
        ] {
            values.insert(field.into(), value(task, field).into());
        }
        let operation = format!("{prefix}{index:04}");
        match persist_task(&mut tx, user.id, None, None, Some(&operation), &values).await? {
            Save::Saved { id, revision } => {
                receipts.push(Receipt { id, revision });
                errors.push(Values::new());
            }
            Save::Invalid(fields) => errors.push(fields),
            _ => unreachable!("only inserts are accepted"),
        }
    }
    if errors.iter().any(|fields| !fields.is_empty()) {
        tx.rollback().await?;
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "errors": errors })),
        )
            .into_response());
    }
    tx.commit().await?;
    Ok(Json(json!({ "tasks": receipts })).into_response())
}

#[derive(Deserialize)]
pub(crate) struct Mutation {
    operation: String,
    account_id: i64,
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
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let csrf = headers
        .get("x-csrf-token")
        .and_then(|token| token.to_str().ok())
        .unwrap_or("");
    if !auth::valid_post(&app.auth, &headers, &session, csrf).await? {
        return Ok(auth::forbidden());
    }
    if change.account_id != user.id {
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
            &user,
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
            Save::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
        },
    )
}

pub(crate) async fn in_range(
    db: &SqlitePool,
    from: NaiveDate,
    through: NaiveDate,
    owner: Option<i64>,
) -> Result<Vec<Task>, AppError> {
    Ok(sqlx::query_as("SELECT * FROM tasks WHERE activity_date BETWEEN ? AND ? AND (? IS NULL OR COALESCE(owner_id, 0) = ?) ORDER BY activity_date DESC, id DESC")
        .bind(from.to_string()).bind(through.to_string()).bind(owner).bind(owner).fetch_all(db).await?)
}

pub(crate) async fn snapshot(
    State(app): State<App>,
    session: Session,
) -> Result<Response, AppError> {
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let date = today();
    let tasks = in_range(&app.db, date - chrono::Days::new(29), date, Some(user.id)).await?;
    let catalog = catalog(&app.db, user.id).await?;
    // Refresh the session only after obtaining an authenticated snapshot.
    session
        .insert("snapshot_at", Utc::now().timestamp())
        .await?;
    Ok(Json(serde_json::json!({
        "date": date.to_string(), "tasks": tasks, "catalog": catalog, "account_id": user.id, "admin": user.admin(),
        "csrf": auth::csrf(&session).await?,
    }))
    .into_response())
}

#[derive(Template)]
#[template(path = "today.html")]
struct Today {
    accounts: Vec<crate::users::Account>,
    selected_account: String,
    account_id: i64,
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
}

#[derive(Deserialize)]
pub(crate) struct TodayQuery {
    usuario: Option<String>,
    tienda: Option<String>,
}

pub(crate) async fn home(
    State(app): State<App>,
    session: Session,
    Query(query): Query<TodayQuery>,
) -> Result<Response, AppError> {
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let date = today();
    let Some(owner) = user.scope(query.usuario.as_deref()) else {
        return Ok(auth::forbidden());
    };
    let mut tasks = in_range(&app.db, date, date, owner).await?;
    let mut counts = HashMap::new();
    for task in &tasks {
        *counts.entry(task.store.as_str()).or_insert(0) += 1;
    }
    let own_stores: Vec<_> = catalog(&app.db, user.id)
        .await?
        .into_iter()
        .filter(|row| row.kind == "store")
        .map(|row| row.label)
        .collect();
    let mut labels = own_stores.clone();
    if user.admin() {
        labels.extend(counts.keys().map(|label| (*label).to_owned()));
        labels.sort();
        labels.dedup();
    }
    let selected_account = if user.admin() {
        owner.map(|id| id.to_string()).unwrap_or_default()
    } else {
        String::new()
    };
    let stores: Vec<_> = labels
        .into_iter()
        .map(|label| StoreCard { label })
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
    let add_url = if !own_stores.contains(&selected) {
        "/tareas/nueva".into()
    } else {
        format!("/tareas/nueva?tienda={}", urlencoding::encode(&selected))
    };
    page(Today {
        account_id: user.id,
        accounts: crate::users::accounts(&app.db, user.admin()).await?,
        selected_account,
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
    accounts: Vec<crate::users::Account>,
    selected_account: String,
    account_id: i64,
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
    usuario: Option<String>,
    page: Option<i64>,
    edited: Option<bool>,
}

pub(crate) async fn history(
    State(app): State<App>,
    session: Session,
    Query(query): Query<HistoryQuery>,
) -> Result<Response, AppError> {
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let Some(owner) = user.scope(query.usuario.as_deref()) else {
        return Ok(auth::forbidden());
    };
    let number = query.page.unwrap_or(1).clamp(1, i64::MAX / PAGE_SIZE);
    let mut tasks =
        sqlx::query_as("SELECT * FROM tasks WHERE (? IS NULL OR COALESCE(owner_id, 0) = ?) ORDER BY activity_date DESC, id DESC LIMIT ? OFFSET ?").bind(owner).bind(owner)
            .bind(PAGE_SIZE + 1)
            .bind((number - 1) * PAGE_SIZE)
            .fetch_all(&app.db)
            .await?;
    let has_next = tasks.len() > PAGE_SIZE as usize;
    tasks.truncate(PAGE_SIZE as usize);
    let (from, through) = crate::exports::defaults();
    page(History {
        account_id: user.id,
        accounts: crate::users::accounts(&app.db, user.admin()).await?,
        selected_account: if user.admin() {
            owner.map(|id| id.to_string()).unwrap_or_default()
        } else {
            String::new()
        },
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

#[derive(Template)]
#[template(path = "task_detail.html")]
struct TaskDetail {
    account_id: i64,
    csrf: String,
    task: Task,
}
