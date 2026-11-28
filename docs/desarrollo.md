# Desarrollo

Instala Rust con Cargo, rustfmt y Clippy. El [Dockerfile](../Dockerfile) compila con Rust 1.100 y usa el conjunto de dependencias de [Cargo.lock](../Cargo.lock). Si ejecutas el binario fuera de Docker, instala Typst y deja `typst` en el `PATH` para generar PDF. El contenedor usa Typst 0.14.2 e incluye Liberation Sans.

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
cargo test --locked
```

Para el arranque local, usa `cargo run --locked`. El binario escucha por defecto en `127.0.0.1:8080` y crea `taskapp.sqlite` si no existe. Define `TASKAPP_BIND` y `TASKAPP_DB` para usar un puerto libre y una base de prueba propia. Las plantillas y los recursos se incorporan al binario.

## Pruebas de integración

Las dos suites arrancan el binario en un puerto disponible con una base SQLite temporal. Ejecútalas después de `cargo build --locked`. Necesitas Python 3. En Linux, usa `python3` si el comando `python` no está disponible. En Windows, usa `target/debug/taskapp.exe` y, si tienes el lanzador de Windows, puedes invocar Python con `py -3`.

```bash
python tests/accounts_http.py target/debug/taskapp
```

[accounts_http.py](../tests/accounts_http.py) usa solo la biblioteca estándar. Comprueba credenciales, permisos entre cuentas, persistencia y recuperación del administrador.

[task_batches.py](../tests/task_batches.py) requiere Playwright, pypdf, un navegador compatible y Typst en el `PATH`. La suite comprueba el alta, los lotes, los reintentos, el trabajo offline, el historial, los gráficos y las exportaciones. Por defecto, usa Chrome instalado. Si ejecutas con Edge, establece `TASKAPP_BROWSER=msedge`.

```bash
python -m pip install playwright pypdf
python -B tests/task_batches.py target/debug/taskapp
```

Alternativamente, si usas [uv](https://docs.astral.sh/uv/), puedes ejecutar la suite con dependencias temporales. Ejemplo en Windows:

```powershell
uv run --with playwright --with pypdf python -B tests/task_batches.py target/debug/taskapp.exe
```

Para incluir la comprobación de accesibilidad de los formularios, instala `axe-core` en un directorio temporal y define `TASKAPP_AXE` con la ruta a su `axe.min.js` antes de ejecutar la suite. Estas herramientas son para desarrollo.

## Capturas y muestras

La suite de lotes escribe capturas en `target/task-batch-evidence/`. Para preparar imágenes de publicación, usa una instancia aislada con la función `server` de la suite HTTP y datos ficticios. Inicia sesión en un contexto nuevo del navegador. Carga varias tiendas ficticias y tareas con fechas recientes, distintos trabajos y estados. Sustituye también los nombres y opciones del catálogo que se vean en pantalla.

Captura **Hoy** y **Gráficos** a 1440 × 900 en escritorio y 390 × 844 en móvil. Para **Nueva tarea**, completa dos bloques con datos de ejemplo y captura el formulario. Descarga el PDF y el XLSX mediante **Historial → Exportar**. Renderiza la página del PDF con un visor y la hoja **Actividades** con una aplicación de hojas de cálculo. Guarda solo las imágenes optimizadas en [media/](media/). Los datos, descargas y archivos intermedios quedan fuera de Git.
