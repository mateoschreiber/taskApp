"""Browser and API checks against a disposable taskApp database.

uv run --with playwright --with pypdf tests/task_batches.py target/debug/taskapp.exe
Place Typst on PATH for the PDF check. Set TASKAPP_AXE to axe.min.js to run axe.
"""

from contextlib import closing
from io import BytesIO
from pathlib import Path
from tempfile import TemporaryDirectory
from urllib.parse import urlencode
from uuid import uuid4
from zipfile import ZipFile
import os
import re
import sqlite3
import sys

from playwright.sync_api import expect, sync_playwright
from pypdf import PdfReader

from accounts_http import server


NEW = "/tareas/nueva"
API = "/api/tareas/lote"
ROWS = "[data-task-block]"
ADD = "Agregar otra tarea"


def choose(scope, label, value):
    choice = scope.get_by_role("combobox", name=label, exact=True)
    choice.fill(value)
    choice.press("ArrowDown")
    choice.press("Enter")


def start_task(page, store="ENCARNACIÓN CAMBYRETA"):
    page.goto(NEW)
    expect(page.locator("#store-search")).to_be_visible()
    expect(page.locator(ROWS).first).to_be_hidden()
    choose(page, "Tienda", store)
    expect(page.locator(ROWS).first).to_be_visible()
    return page.locator(ROWS)


def work(row, label):
    choose(row, "Trabajo realizado", label)
    choose(row, "Tipo de intervención", "Intervención de prueba")
    choose(row, "Estado", "Estado de prueba")
    row.get_by_role("textbox", name=re.compile("Detalle")).fill("Observación " + label)


def check(browser, base, db, evidence):
    context = browser.new_context(base_url=base, viewport={"width": 390, "height": 844},
                                  reduced_motion="reduce", is_mobile=True, has_touch=True)
    page = context.new_page()
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.goto("/login")
    page.locator("#username").fill("admin")
    page.locator("#password").fill("admin")
    page.get_by_role("button", name="Entrar").click()
    expect(page.locator("#today-store")).to_be_visible()

    def snapshot():
        return context.request.get("/api/offline").json()

    def assert_rows(count):
        saved = snapshot()["tasks"]
        assert len(saved) == count, saved
        assert len({task["id"] for task in saved}) == count
        return saved

    assert page.locator("#today-store option").first.text_content() == "Todas las tiendas"
    assert all(not re.search(r" \(\d+\)$", label) for label in page.locator("#today-store option").all_text_contents())
    print("T01: selector limpio sin tareas", flush=True)
    rows = start_task(page)
    assert not page.locator("#date").input_value() == ""
    expect(page.locator("#supervisor-search")).to_have_value("FLORENCIA SILVERO")
    expect(page.locator("#zone-search")).to_have_value("INTERIOR")
    print("T03/T04: sucursal primero y valores predeterminados", flush=True)
    work(rows.first, "Trabajo único")
    page.screenshot(path=str(evidence / "una-tarea-390.png"), full_page=True)
    page.get_by_role("button", name="Guardar tarea").click()
    expect(page.locator("#app-message")).to_have_text("Tarea guardada correctamente.")
    assert assert_rows(1)[0]["work"] == "Trabajo único"
    print("T05/T20: alta individual con datos propios", flush=True)

    work(rows.first, "Trabajo uno")
    operations = []
    page.on("request", lambda request: operations.append(request.url) if request.url.endswith(API) else None)
    for index in range(1, 3):
        page.get_by_role("button", name=ADD).click()
        expect(rows.nth(index)).to_be_focused()
        work(rows.nth(index), ["", "Trabajo intermedio", "Trabajo tres"][index])
    expect(rows).to_have_count(3)
    choose(rows.nth(1), "Trabajo realizado", "Trabajo editado")
    expect(rows.first.get_by_role("combobox", name="Trabajo realizado")).to_have_value("Trabajo uno")
    expect(rows.nth(2).get_by_role("combobox", name="Trabajo realizado")).to_have_value("Trabajo tres")
    page.once("dialog", lambda dialog: dialog.dismiss())
    rows.nth(1).get_by_role("button", name="Eliminar tarea 2").click()
    expect(rows).to_have_count(3)
    page.once("dialog", lambda dialog: dialog.accept())
    rows.nth(1).get_by_role("button", name="Eliminar tarea 2").click()
    expect(rows).to_have_count(2)
    expect(rows.nth(1).get_by_role("combobox", name="Trabajo realizado")).to_have_value("Trabajo tres")
    assert not operations
    print("T06–T08: bloques independientes, foco y eliminación protegida", flush=True)

    page.once("dialog", lambda dialog: dialog.dismiss())
    choose(page, "Tienda", "ENCARNACIÓN ENTRADA")
    expect(page.locator("#store-search")).to_have_value("ENCARNACIÓN CAMBYRETA")
    expect(rows.first.get_by_role("combobox", name="Trabajo realizado")).to_have_value("Trabajo uno")
    page.once("dialog", lambda dialog: dialog.dismiss())
    page.get_by_role("navigation", name="Principal").get_by_text("Historial").click(no_wait_after=True)
    assert page.url.endswith(NEW)
    print("T12: confirmación de sucursal y salida protege el borrador", flush=True)

    page.get_by_role("button", name=ADD).click()
    page.get_by_role("button", name="Guardar 3 tareas").click()
    expect(rows.nth(2).get_by_role("combobox", name="Trabajo realizado")).to_be_focused()
    assert not operations
    assert_rows(1)
    work(rows.nth(2), "Trabajo cuatro")
    print("T10: bloque incompleto impide enviar el lote", flush=True)

    for width in [320, 360, 390, 430, 768, 1440]:
        page.set_viewport_size({"width": width, "height": 844 if width < 768 else 900})
        assert page.evaluate("document.documentElement.scrollWidth === innerWidth"), width
        save = page.get_by_role("button", name="Guardar 3 tareas")
        save.scroll_into_view_if_needed()
        assert save.evaluate("(b) => { const r=b.getBoundingClientRect(); return r.height >= 48 && b.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)); }"), width
        page.screenshot(path=str(evidence / f"tres-tareas-{width}.png"), full_page=True)
        if width == 390:
            page.screenshot(path=str(evidence / "guardado-movil-390.png"))
    print("T19: 320, 360, 390, 430, 768 y 1440 px sin desborde ni botón cubierto", flush=True)

    axe = os.getenv("TASKAPP_AXE")
    if axe:
        page.evaluate(Path(axe).read_text(encoding="utf-8"))
        audit = page.evaluate("async () => (await axe.run('.task-form')).violations.map(v => [v.id, v.nodes.map(n => n.target)])")
        assert not audit, audit
        print("Accesibilidad: axe sin infracciones en el formulario", flush=True)

    with page.expect_response(lambda response: response.url.endswith(API)) as posted:
        page.get_by_role("button", name="Guardar 3 tareas").evaluate(
            "button => { button.click(); button.click(); button.click(); }"
        )
    expect(page.locator("#app-message")).to_have_text("3 tareas guardadas correctamente.")
    assert len(operations) == 1
    saved = assert_rows(4)
    assert {task["work"] for task in saved} == {"Trabajo único", "Trabajo uno", "Trabajo tres", "Trabajo cuatro"}
    assert {task["store"] for task in saved} == {"ENCARNACIÓN CAMBYRETA"}
    sent = posted.value.request.post_data_json
    session = snapshot()
    headers = {"X-CSRF-Token": session["csrf"]}
    replay = context.request.post(API, headers=headers, data=sent)
    ids = {item["work"]: item["id"] for item in saved}
    assert posted.value.ok and replay.ok
    assert [receipt["id"] for receipt in replay.json()["tasks"]] == [ids[task["work_new"]] for task in sent["tasks"]]
    assert_rows(4)
    print("T09/T11: tres registros, un envío y reintento idempotente", flush=True)

    invalid = {**sent, "operation": str(uuid4()), "tasks": [sent["tasks"][0], {}]}
    response = context.request.post(API, headers=headers, data=invalid)
    assert response.status == 422 and "work" in response.json()["errors"][1]
    assert_rows(4)

    work(rows.first, "Trabajo recuperar")
    page.get_by_role("button", name=ADD).click()
    work(rows.nth(1), "Trabajo falla")
    with closing(sqlite3.connect(db)) as conn, conn:
        conn.execute("CREATE TRIGGER test_failure BEFORE INSERT ON tasks WHEN NEW.work = 'Trabajo falla' "
                     "BEGIN SELECT RAISE(ABORT, 'test_failure'); END")
    page.get_by_role("button", name="Guardar 2 tareas").click()
    expect(page.locator("[data-task-feedback]")).to_contain_text("Tus datos siguen aquí")
    assert_rows(4)
    expect(rows.first.locator("#work-search")).to_have_value("Trabajo recuperar")
    expect(rows.nth(1).locator("textarea")).to_have_value("Observación Trabajo falla")
    with closing(sqlite3.connect(db)) as conn, conn:
        assert conn.execute("SELECT COUNT(*) FROM user_catalog WHERE label IN ('Trabajo recuperar', 'Trabajo falla')").fetchone() == (0,)
        conn.execute("DROP TRIGGER test_failure")
    page.get_by_role("button", name="Guardar 2 tareas").click()
    expect(page.locator("#app-message")).to_have_text("2 tareas guardadas correctamente.")
    assert_rows(6)
    print("T17: reversión de tareas y catálogo, campos preservados y reintento", flush=True)

    page.goto("/sin-conexion")
    expect(page.locator("#device")).to_be_visible()
    page.evaluate("navigator.serviceWorker.ready")
    context.set_offline(True)
    page.goto(NEW)
    expect(page.locator("#device-form-title")).to_be_visible()
    choose(page, "Tienda", "ENCARNACIÓN ENTRADA")
    work(rows.first, "Trabajo sin señal uno")
    page.get_by_role("button", name=ADD).click()
    work(rows.nth(1), "Trabajo sin señal dos")
    page.get_by_role("button", name="Guardar 2 tareas").click()
    expect(page.locator("#app-message")).to_contain_text("2 tareas guardadas en el dispositivo")
    expect(page.get_by_text("Pendiente de sincronizar", exact=True)).to_be_visible()
    print("T13: lote completo en IndexedDB sin conexión", flush=True)
    context.set_offline(False)
    expect(page.get_by_text("Pendiente de sincronizar", exact=True)).to_have_count(0)
    saved = assert_rows(8)
    assert {item["work"] for item in saved if item["store"] == "ENCARNACIÓN ENTRADA"} == {
        "Trabajo sin señal uno", "Trabajo sin señal dos",
    }
    print("T14: reconexión sincroniza todo el lote", flush=True)

    page.goto("/")
    assert all(not re.search(r" \(\d+\)$", label) for label in page.locator("#today-store option").all_text_contents())
    page.locator("#today-store").select_option("ENCARNACIÓN ENTRADA")
    page.get_by_role("button", name="Filtrar").click()
    expect(page.locator(".work-records li")).to_have_count(2)
    expect(page.locator(".cover-totals")).to_contain_text("8")
    print("T01/T02: filtro, selección y total con datos", flush=True)

    other = browser.new_context(base_url=base)
    other_page = other.new_page()
    other_page.goto("/login")
    other_page.locator("#username").fill("admin")
    other_page.locator("#password").fill("admin")
    other_page.get_by_role("button", name="Entrar").click()
    other_page.goto("/historial")
    for task in saved:
        expect(other_page.get_by_text(task["work"], exact=True)).to_be_visible()
    print("T15: historial completo desde un segundo perfil de navegador", flush=True)

    range_query = urlencode({"desde": session["date"], "hasta": session["date"]})
    other_page.goto("/graficos?" + range_query)
    expect(other_page.locator("#chart-summary")).to_contain_text("8")
    expect(other_page.locator("#chart-grid > section")).to_have_count(5)
    print("T16: total y cinco gráficos con las tareas registradas", flush=True)

    endpoint = "/exportar/descargar?" + range_query + "&formato="
    xlsx = other.request.get(endpoint + "xlsx")
    assert xlsx.ok
    with ZipFile(BytesIO(xlsx.body())) as excel:
        workbook_rows = excel.read("xl/sharedStrings.xml").decode()
    pdf = other.request.get(endpoint + "pdf")
    assert pdf.ok, pdf.status
    pdf_text = " ".join(" ".join(sheet.extract_text().split()) for sheet in PdfReader(BytesIO(pdf.body())).pages)
    for task in saved:
        assert task["work"] in workbook_rows and task["work"] in pdf_text, task["work"]
    print("T18: Excel y PDF contienen los ocho registros", flush=True)

    rows = start_task(page)
    work(rows.first, "Respuesta interrumpida")

    def lose_first_response(route):
        # Commit at the server, then drop the response before it reaches the app.
        response = route.fetch()
        assert response.ok
        route.abort()

    page.route("**" + API, lose_first_response, times=1)
    page.get_by_role("button", name="Guardar tarea").click()
    expect(page.locator("#app-message")).to_contain_text("pendiente de sincronización")
    page.goto("/sin-conexion")
    expect(page.get_by_text("Pendiente de sincronizar", exact=True)).to_have_count(0)
    assert [task["work"] for task in assert_rows(9)].count("Respuesta interrumpida") == 1
    print("T14 adicional: pérdida de respuesta tras confirmar no duplica la tarea", flush=True)

    rows = start_task(page)
    work(rows.first, "Borrador de cambio")
    page.once("dialog", lambda dialog: dialog.accept())
    choose(page, "Tienda", "ENCARNACIÓN ENTRADA")
    expect(rows.first.get_by_role("combobox", name="Trabajo realizado")).to_have_value("")
    expect(page.locator("#store-search")).to_have_value("ENCARNACIÓN ENTRADA")

    cdp = context.new_cdp_session(page)
    cdp.send("Emulation.setCPUThrottlingRate", {"rate": 4})
    perf = page.evaluate("""async () => {
        const button = document.querySelector('[data-task-add]');
        const samples = [];
        for (let i = 0; i < 19; i++) {
            const start = performance.now();
            button.click();
            await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
            samples.push(performance.now() - start);
        }
        return samples.sort((a, b) => a - b);
    }""")
    cdp.detach()
    expect(rows).to_have_count(20)
    print(f"Rendimiento con CPU 4×: mediana {perf[len(perf)//2]:.0f} ms al añadir un bloque hasta el siguiente repintado", flush=True)
    assert not errors, errors
    other.close()
    context.close()


if __name__ == "__main__":
    evidence = Path("target/task-batch-evidence").resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    with TemporaryDirectory() as directory:
        database = Path(directory) / "test.sqlite"
        with server(Path(sys.argv[1]).resolve(), database) as origin, sync_playwright() as playwright:
            browser = playwright.chromium.launch(channel=os.getenv("TASKAPP_BROWSER", "chrome"))
            check(browser, origin, database, evidence)
