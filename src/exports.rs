use std::{io, time::Duration};

use askama::Template;
use axum::{
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use chrono::Datelike;
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook};
use serde::{Deserialize, Serialize};
use tokio::{process::Command, sync::Semaphore};
use tower_sessions::Session;

use crate::{
    App, AppError, auth, page,
    tasks::{self, Task},
};

static EXPORTS: Semaphore = Semaphore::const_new(2);

#[derive(Template)]
#[template(path = "export.html")]
struct ExportPage {
    account_id: i64,
    accounts: Vec<crate::users::Account>,
    selected_account: String,
    csrf: String,
    from: String,
    through: String,
    pdf: bool,
    error: &'static str,
}

#[derive(Deserialize)]
pub(crate) struct Range {
    usuario: Option<String>,
    desde: Option<String>,
    hasta: Option<String>,
    formato: Option<String>,
}

pub(crate) fn defaults() -> (String, String) {
    let today = tasks::today();
    (today.with_day(1).unwrap().to_string(), today.to_string())
}

pub(crate) async fn form(
    State(app): State<App>,
    session: Session,
    Query(range): Query<Range>,
) -> Result<Response, AppError> {
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    if user.scope(range.usuario.as_deref()).is_none() {
        return Ok(auth::forbidden());
    }
    let (from, through) = defaults();
    page(ExportPage {
        account_id: user.id,
        accounts: crate::users::accounts(&app.db, user.admin()).await?,
        selected_account: range.usuario.clone().unwrap_or_default(),
        csrf: auth::csrf(&session).await?,
        from,
        through,
        pdf: false,
        error: "",
    })
}

#[derive(Serialize)]
struct Report {
    period: String,
    responsible: String,
    generated: String,
    tasks: Vec<Task>,
}

pub(crate) async fn download(
    State(app): State<App>,
    session: Session,
    Query(range): Query<Range>,
) -> Result<Response, AppError> {
    let Some(user) = auth::current(&app, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };
    let view = ExportPage {
        account_id: user.id,
        accounts: crate::users::accounts(&app.db, user.admin()).await?,
        selected_account: range.usuario.clone().unwrap_or_default(),
        csrf: auth::csrf(&session).await?,
        from: range.desde.unwrap_or_default(),
        through: range.hasta.unwrap_or_default(),
        pdf: range.formato.as_deref() == Some("pdf"),
        error: "",
    };
    let render_error = |mut view: ExportPage, status, error| {
        view.error = error;
        let mut response = page(view)?;
        *response.status_mut() = status;
        Ok(response)
    };
    let (Some(from), Some(through)) = (
        tasks::parse_date(&view.from),
        tasks::parse_date(&view.through),
    ) else {
        return render_error(
            view,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Selecciona dos fechas válidas.",
        );
    };
    if from > through || !matches!(range.formato.as_deref(), Some("xlsx" | "pdf")) {
        return render_error(
            view,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Revisa el orden de las fechas y selecciona un formato.",
        );
    }
    let Ok(permit) = EXPORTS.try_acquire() else {
        return render_error(
            view,
            StatusCode::SERVICE_UNAVAILABLE,
            "Hay exportaciones en curso. Intenta en unos segundos.",
        );
    };
    let Some(owner) = user.scope(range.usuario.as_deref()) else {
        return Ok(auth::forbidden());
    };
    let rows = tasks::in_range(&app.db, from, through, owner).await?;
    let report = Report {
        period: format!(
            "{} al {}",
            from.format("%d/%m/%Y"),
            through.format("%d/%m/%Y")
        ),
        responsible: match owner {
            None => "Todos los usuarios".to_string(),
            Some(id) if id == user.id => user.name.clone(),
            Some(id) => view
                .accounts
                .iter()
                .find(|account| account.id == id)
                .map(|account| account.name.clone())
                .unwrap_or_else(|| format!("Usuario {id}")),
        },
        generated: tasks::today().format("%d/%m/%Y").to_string(),
        tasks: rows,
    };
    let (extension, content_type, bytes) = if view.pdf {
        let bytes = pdf(report).await?;
        drop(permit);
        ("pdf", "application/pdf", bytes)
    } else {
        let bytes = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            excel(&report)
        })
        .await??;
        (
            "xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            bytes,
        )
    };
    let disposition =
        format!("attachment; filename=\"actividades-{from}-a-{through}.{extension}\"");
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, &disposition),
        ],
        bytes,
    )
        .into_response())
}

fn excel(report: &Report) -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Actividades")?;
    let base = Format::new().set_font_name("Calibri").set_font_size(11);
    let title = base
        .clone()
        .set_bold()
        .set_font_size(14)
        .set_align(FormatAlign::Center);
    let heading = base
        .clone()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0xE30613))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xD9D9D9))
        .set_text_wrap();
    let cell = base
        .clone()
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xD9D9D9))
        .set_align(FormatAlign::Top)
        .set_text_wrap();
    let alternate = cell.clone().set_background_color(Color::RGB(0xF5F5F5));
    let columns = [
        ("Ítem", 7),
        ("Fecha de actividad", 14),
        ("Tienda", 19),
        ("Supervisor General", 20),
        ("Zona", 13),
        ("Trabajo realizado", 27),
        ("Tipo de intervención", 20),
        ("Estado", 18),
        ("Detalle / observaciones", 32),
    ];
    for (index, (label, width)) in columns.into_iter().enumerate() {
        let column = index as u16;
        sheet.set_column_width(column, width)?;
        sheet.write_string_with_format(5, column, label, &heading)?;
    }
    sheet.merge_range(0, 0, 0, 8, "REGISTRO DE ACTIVIDADES", &title)?;
    sheet.write_string_with_format(2, 0, format!("Responsable: {}", report.responsible), &base)?;
    sheet.write_string_with_format(3, 0, format!("Período: {}", report.period), &base)?;
    sheet.write_string_with_format(
        4,
        0,
        format!("Fecha de generación: {}", report.generated),
        &base,
    )?;
    sheet
        .set_landscape()
        .set_paper_size(9)
        .set_print_fit_to_pages(1, 0);
    sheet.set_margins(0.35, 0.35, 0.5, 0.5, 0.2, 0.2);
    sheet.set_repeat_rows(5, 5)?;
    sheet.set_freeze_panes(6, 0)?;
    let last_row = 5 + report.tasks.len() as u32;
    sheet.set_print_area(0, 0, last_row, 8)?;
    sheet.autofilter(5, 0, last_row, 8)?;
    for (index, task) in report.tasks.iter().enumerate() {
        let row = index as u32 + 6;
        let row_format = if index % 2 == 0 { &cell } else { &alternate };
        let date_format = row_format.clone().set_num_format("dd/mm/yyyy");
        sheet.write_number_with_format(row, 0, (index + 1) as f64, row_format)?;
        if let Some(date) =
            tasks::parse_date(&task.activity_date).filter(|date| date.year() >= 1900)
        {
            sheet.write_date_with_format(row, 1, date, &date_format)?;
        } else {
            sheet.write_string_with_format(row, 1, &task.activity_date, row_format)?;
        }
        for (column, value) in [
            &task.store,
            &task.supervisor,
            &task.zone,
            &task.work,
            &task.intervention,
            &task.status,
            &task.notes,
        ]
        .into_iter()
        .enumerate()
        {
            sheet.write_string_with_format(row, (column + 2) as u16, value, row_format)?;
        }
    }
    workbook.save_to_buffer()
}
async fn pdf(report: Report) -> Result<Vec<u8>, AppError> {
    let folder = tempfile::tempdir()?;
    let path = folder.path();
    tokio::fs::write(
        path.join("report.typ"),
        include_str!("../reports/tasks.typ"),
    )
    .await?;
    tokio::fs::write(path.join("report.json"), serde_json::to_vec(&report)?).await?;
    let output = tokio::time::timeout(
        Duration::from_secs(60),
        Command::new("typst")
            .current_dir(path)
            .kill_on_drop(true)
            .args(["compile", "--root", ".", "report.typ", "report.pdf"])
            .output(),
    )
    .await??;
    if !output.status.success() {
        return Err(io::Error::other("Typst no pudo generar el informe").into());
    }
    Ok(tokio::fs::read(path.join("report.pdf")).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Task {
        Task {
            owner_id: Some(1),
            id: 1,
            revision: 1,
            activity_date: "2026-11-10".into(),
            store: "Tienda de prueba".into(),
            supervisor: "María López".into(),
            zone: "Interior".into(),
            work: "Revisión de góndolas y precios".into(),
            intervention: "Seguimiento".into(),
            status: "En proceso".into(),
            notes: "Observación de prueba: revisión completada.".into(),
        }
    }

    #[test]
    fn excel_handles_empty_and_many_records() {
        for count in [0, 1, 300] {
            let report = Report {
                period: "01/11/2026 al 30/11/2026".into(),
                responsible: "Equipo de prueba".into(),
                generated: "28/11/2026".into(),
                tasks: (0..count).map(|_| sample()).collect(),
            };
            let result = excel(&report).unwrap();
            assert!(result.starts_with(b"PK"));
            if count == 1
                && let Ok(dir) = std::env::var("TASKAPP_EXPORT_EXAMPLE_DIR")
            {
                std::fs::write(
                    std::path::Path::new(&dir).join("ejemplo-actividades.xlsx"),
                    result,
                )
                .unwrap();
            }
        }
    }
}
