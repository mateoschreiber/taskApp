use std::{collections::BTreeMap, io, time::Duration};

use askama::Template;
use axum::{
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use chrono::Datelike;
use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook};
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
    csrf: String,
    from: String,
    through: String,
    pdf: bool,
    error: &'static str,
}

#[derive(Deserialize)]
pub(crate) struct Range {
    desde: Option<String>,
    hasta: Option<String>,
    formato: Option<String>,
}

pub(crate) fn defaults() -> (String, String) {
    let today = tasks::today();
    (today.with_day(1).unwrap().to_string(), today.to_string())
}

pub(crate) async fn form(session: Session) -> Result<Response, AppError> {
    if !auth::authenticated(&session).await? {
        return Ok(Redirect::to("/login").into_response());
    }
    let (from, through) = defaults();
    page(ExportPage {
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
    tasks: Vec<Task>,
    states: BTreeMap<String, usize>,
}

pub(crate) async fn download(
    State(app): State<App>,
    session: Session,
    Query(range): Query<Range>,
) -> Result<Response, AppError> {
    if !auth::authenticated(&session).await? {
        return Ok(Redirect::to("/login").into_response());
    }
    let view = ExportPage {
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
    let rows = tasks::in_range(&app.db, from, through).await?;
    if rows.is_empty() {
        return render_error(
            view,
            StatusCode::UNPROCESSABLE_ENTITY,
            "No hay tareas guardadas en este período.",
        );
    }
    let mut states = BTreeMap::new();
    for task in &rows {
        *states.entry(task.status.clone()).or_insert(0) += 1;
    }
    let report = Report {
        period: format!(
            "{} al {}",
            from.format("%d/%m/%Y"),
            through.format("%d/%m/%Y")
        ),
        tasks: rows,
        states,
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
    let disposition = format!("attachment; filename=\"taskapp-{from}-a-{through}.{extension}\"");
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
    let ink = Color::RGB(0x231D20);
    let wine = Color::RGB(0x600F24);
    let red = Color::RGB(0xBC102A);
    let white = Color::White;
    let title = Format::new()
        .set_font_size(20)
        .set_bold()
        .set_font_color(white)
        .set_background_color(wine)
        .set_align(FormatAlign::VerticalCenter);
    let subtitle = Format::new()
        .set_font_color(wine)
        .set_font_size(11)
        .set_background_color(Color::RGB(0xF9E8EA))
        .set_align(FormatAlign::VerticalCenter);
    let heading = Format::new()
        .set_bold()
        .set_background_color(red)
        .set_font_color(white)
        .set_align(FormatAlign::VerticalCenter);
    let body = Format::new()
        .set_font_color(ink)
        .set_text_wrap()
        .set_align(FormatAlign::Top);
    let striped = body.clone().set_background_color(Color::RGB(0xF7F5F2));
    let date_formats = [
        body.clone().set_num_format("dd/mm/yyyy"),
        striped.clone().set_num_format("dd/mm/yyyy"),
    ];

    let summary = workbook.add_worksheet();
    summary
        .set_name("Resumen")?
        .set_screen_gridlines(false)
        .set_tab_color(wine);
    summary.set_column_width(0, 40)?.set_column_width(1, 20)?;
    summary
        .merge_range(0, 0, 0, 1, "Registro de trabajos", &title)?
        .set_row_height(0, 40)?
        .set_row_height(1, 22)?;
    summary.merge_range(1, 0, 1, 1, &report.period, &subtitle)?;
    summary.write_string_with_format(3, 0, "Total de tareas", &heading)?;
    summary.write_number_with_format(3, 1, report.tasks.len() as f64, &heading)?;
    for (index, (label, count)) in report.states.iter().enumerate() {
        let row = index as u32 + 5;
        let style = if index % 2 == 0 { &body } else { &striped };
        summary.write_string_with_format(row, 0, label, style)?;
        summary.write_number_with_format(row, 1, *count as f64, style)?;
    }

    let sheet = workbook.add_worksheet();
    sheet
        .set_name("Tareas")?
        .set_screen_gridlines(false)
        .set_tab_color(red)
        .set_landscape();
    sheet.set_print_fit_to_pages(1, 0);
    sheet.set_freeze_panes(4, 2)?;
    sheet
        .merge_range(0, 0, 0, 7, "Trabajos realizados", &title)?
        .set_row_height(0, 40)?
        .set_row_height(1, 22)?;
    sheet.merge_range(1, 0, 1, 7, &report.period, &subtitle)?;
    sheet.set_row_height(3, 25)?;
    for (index, (label, width)) in [
        ("Fecha de actividad", 19),
        ("Tienda", 33),
        ("Supervisor General", 26),
        ("Zona", 18),
        ("Trabajo realizado", 40),
        ("Tipo de intervención", 32),
        ("Estado", 25),
        ("Detalle / observaciones", 65),
    ]
    .into_iter()
    .enumerate()
    {
        let column = index as u16;
        sheet
            .set_column_width(column, width)?
            .write_string_with_format(3, column, label, &heading)?;
    }
    for (index, task) in report.tasks.iter().enumerate() {
        let row = index as u32 + 4;
        let style = if index % 2 == 0 { &body } else { &striped };
        if let Some(date) =
            tasks::parse_date(&task.activity_date).filter(|date| date.year() >= 1900)
        {
            sheet.write_date_with_format(row, 0, date, &date_formats[index % 2])?;
        } else {
            sheet.write_string_with_format(row, 0, &task.activity_date, style)?;
        }
        for (index, value) in [
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
            sheet.write_string_with_format(row, (index + 1) as u16, value, style)?;
        }
    }
    sheet.autofilter(3, 0, report.tasks.len() as u32 + 3, 7)?;
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
    tokio::fs::write(
        path.join("brand.png"),
        include_bytes!("../assets/icon-192.png"),
    )
    .await?;
    tokio::fs::write(
        path.join("jornada.png"),
        include_bytes!("../assets/jornada.png"),
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
