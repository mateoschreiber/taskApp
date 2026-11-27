use std::collections::BTreeMap;

use askama::Template;
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use chrono::{Datelike, Days, NaiveDate};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

use crate::{
    App, AppError, auth, exports, page,
    tasks::{self, Task},
};

#[derive(Template)]
#[template(path = "charts.html")]
struct ChartsPage {
    account_id: i64,
    accounts: Vec<crate::users::Account>,
    selected_account: String,
    csrf: String,
    from: String,
    through: String,
}

fn view(
    csrf: String,
    account_id: i64,
    accounts: Vec<crate::users::Account>,
) -> Result<Response, AppError> {
    let (from, through) = exports::defaults();
    page(ChartsPage {
        account_id,
        accounts,
        selected_account: String::new(),
        csrf,
        from,
        through,
    })
}

pub(crate) async fn dashboard(
    State(app): State<App>,
    session: Session,
) -> Result<Response, AppError> {
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    view(
        auth::csrf(&session).await?,
        user.id,
        crate::users::accounts(&app.db, user.admin()).await?,
    )
}

pub(crate) async fn offline() -> Result<Response, AppError> {
    view(String::new(), 0, Vec::new())
}

#[derive(Deserialize)]
pub(crate) struct Range {
    usuario: Option<String>,
    desde: String,
    hasta: String,
}

pub(crate) async fn data(
    State(app): State<App>,
    session: Session,
    Query(range): Query<Range>,
) -> Result<Response, AppError> {
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let dates = tasks::parse_date(&range.desde).zip(tasks::parse_date(&range.hasta));
    let Some((from, through)) = dates.filter(|(from, through)| from <= through) else {
        return Ok(StatusCode::UNPROCESSABLE_ENTITY.into_response());
    };
    let Some(owner) = user.scope(range.usuario.as_deref()) else {
        return Ok(auth::forbidden());
    };
    let rows = tasks::in_range(&app.db, from, through, owner).await?;
    let charts = summarize(&rows, from, through);
    Ok(
        Json(serde_json::json!({ "charts": charts, "tasks": rows, "account_id": user.id }))
            .into_response(),
    )
}

#[derive(Serialize)]
pub(crate) struct Chart {
    key: &'static str,
    title: &'static str,
    bars: Vec<Bar>,
}

#[derive(Serialize)]
struct Bar {
    label: String,
    count: usize,
    from: Option<String>,
    through: Option<String>,
}

pub(crate) fn summarize(rows: &[Task], from: NaiveDate, through: NaiveDate) -> Vec<Chart> {
    let days = (through - from).num_days() + 1;
    let mut dates = BTreeMap::new();
    for task in rows {
        *dates.entry(task.activity_date.as_str()).or_insert(0) += 1;
    }
    let mut bars = Vec::new();
    let mut start = from;
    while start <= through && !rows.is_empty() {
        let next = match days {
            ..=31 => start.checked_add_days(Days::new(1)),
            32..=90 => start.checked_add_days(Days::new(u64::from(
                7 - start.weekday().num_days_from_monday(),
            ))),
            91..=730 => start
                .with_day(1)
                .and_then(|date| date.checked_add_months(chrono::Months::new(1))),
            _ => NaiveDate::from_ymd_opt(start.year() + 1, 1, 1),
        };
        let end = next
            .and_then(|date| date.pred_opt())
            .unwrap_or(through)
            .min(through);
        let label = match days {
            ..=31 => start.format("%d/%m").to_string(),
            32..=90 => format!("{}–{}", start.format("%d/%m"), end.format("%d/%m")),
            91..=730 => start.format("%m/%Y").to_string(),
            _ => start.format("%Y").to_string(),
        };
        let lower = start.to_string();
        let upper = end.to_string();
        let count = dates
            .range(lower.as_str()..=upper.as_str())
            .map(|(_, count)| count)
            .sum();
        bars.push(Bar {
            label,
            count,
            from: Some(lower),
            through: Some(upper),
        });
        if end == through {
            break;
        }
        start = next.expect("bucket ending before range end has a successor");
    }
    let mut charts = vec![Chart {
        key: "date",
        title: "Evolución de tareas",
        bars,
    }];
    type Dimension = (&'static str, &'static str, fn(&Task) -> &str);
    let dimensions: [Dimension; 4] = [
        ("store", "Tareas por tienda", |task| &task.store),
        ("status", "Distribución por estado", |task| &task.status),
        ("work", "Trabajos más frecuentes", |task| &task.work),
        ("intervention", "Tipos de intervención", |task| {
            &task.intervention
        }),
    ];
    for (key, title, field) in dimensions {
        let mut counts = BTreeMap::new();
        for row in rows {
            *counts.entry(field(row)).or_insert(0) += 1;
        }
        let mut bars: Vec<_> = counts
            .into_iter()
            .map(|(label, count)| Bar {
                label: label.to_owned(),
                count,
                from: None,
                through: None,
            })
            .collect();
        bars.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
        charts.push(Chart { key, title, bars });
    }
    charts
}
