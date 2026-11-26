<p align="center">
  <img src="assets/icon-192.png" alt="" width="76" height="76">
</p>

<h1 align="center">taskApp</h1>

<p align="center">Registro privado de trabajos diarios en tienda.</p>

<p align="center">
  <a href="#puesta-en-marcha">Puesta en marcha</a> ·
  <a href="#uso-diario">Uso diario</a> ·
  <a href="#instalación-y-uso-sin-conexión">Instalación y uso sin conexión</a> ·
  <a href="#despliegue-en-servidor">Despliegue en servidor</a>
</p>

## Puesta en marcha

Con Docker y Docker Compose, ejecuta este comando desde la raíz del repositorio.

```sh
docker compose up --build -d
```

Abre [taskApp en esta PC](http://127.0.0.1:8080). Para pruebas locales, el usuario y la contraseña son `admin`. El puerto se publica en `127.0.0.1`. Si el `8080` está ocupado, define `TASKAPP_PORT` en `.env` y usa ese puerto en la URL.

Al crear la base, las migraciones cargan **33 opciones iniciales** de la hoja `Hoja1` del Excel fuente. El historial comienza vacío. Las tareas se guardan en SQLite, dentro del volumen `tasks`, y se conservan al recrear el contenedor.

## Uso diario

| Dónde | Qué puedes hacer |
| --- | --- |
| **Hoy** | Consultar las tareas del día en horario de Paraguay, filtrar por tienda y abrir el formulario con esa tienda preseleccionada. |
| **Nueva tarea** | Registrar un trabajo por vez. Escribe para buscar en los catálogos y selecciona **Agregar «valor»** cuando necesites una opción nueva. |
| **Historial** | Consultar y editar los registros. Desde **Exportar**, elige las fechas y el formato. Se incluyen ambas fechas y los registros guardados de todas las tiendas. |

**Excel** incluye las hojas **Resumen** y **Tareas**, con fechas nativas, filtros y observaciones. **PDF** genera un informe A4 con resumen por estado, fichas agrupadas por fecha y páginas numeradas.

## Instalación y uso sin conexión

Instala la PWA desde el menú de Chromium. En Safari para iPhone, abre **Compartir → Agregar a pantalla de inicio**. Para instalarla, usa la URL HTTPS del servidor o abre la app desde localhost.

Al iniciar sesión con conexión, se prepara una copia local de los catálogos y de las tareas de los últimos 30 días. Puedes crear tareas y editar las que estén en esa copia. El historial completo y la exportación requieren conexión.

Los cambios se envían al recuperar la conexión con la app abierta o al volver a abrirla. En **Pendientes** puedes reintentar el envío y comparar tu edición con la versión del servidor si hubo un conflicto. Si la sesión vence, vuelve a iniciar sesión para recuperar la cola. El cierre de sesión limpia la copia del dispositivo y, si quedan cambios pendientes, pide confirmación antes de descartarlos.

## Despliegue en servidor

1. En Cloudflare Tunnel, configura el dominio público para que apunte a `http://app:8080`. Habilita la redirección a HTTPS. El inicio de sesión se realiza en taskApp.
2. Guarda el token del túnel en `secrets/tunnel_token`. Genera el hash Argon2id con `cargo run --locked -- hash-password` y guarda únicamente la salida del comando en `secrets/password_hash`, como UTF-8 sin BOM. La contraseña debe tener al menos 15 caracteres. Da permiso de lectura al usuario del contenedor que consume cada secreto. El contenedor de taskApp usa el UID `65532`.
3. Define `TASKAPP_ORIGIN=https://tu-dominio` en `.env`, sin barra final. Levanta el entorno de producción con este comando.

   ```sh
   docker compose -f compose.prod.yaml up --build -d
   ```

4. Abre el dominio por HTTPS e inicia sesión como `admin` con la contraseña nueva. La app escucha dentro de la red de Compose y Cloudflare Tunnel conecta el dominio con ella.

> [!IMPORTANT]
> El Excel fuente, `.env`, los secretos y la base de trabajo se conservan fuera de Git. Para trasladar una instalación, detén la app y copia el volumen completo de SQLite.

## Desarrollo

El servidor usa **Rust, Axum y Askama**. **SQLite** guarda los registros y sus revisiones. La PWA usa **IndexedDB** para las copias y los cambios pendientes. Las exportaciones se generan en el servidor con **rust_xlsxwriter** y **Typst**. La imagen de Docker ya incluye Typst. Si ejecutas la app directamente con Cargo, agrega Typst al `PATH` para habilitar PDF.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```
