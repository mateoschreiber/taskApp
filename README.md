# taskApp

Registro privado de trabajos diarios en tienda. El formulario permite guardar un trabajo por vez y agregar opciones a los catálogos. Desde el historial puedes editar los campos de cada tarea.

## En esta PC

```sh
docker compose up --build -d
```

Abre [taskApp](http://127.0.0.1:8080) e inicia sesión con el usuario `admin` y la contraseña `admin`. Esta configuración publica el puerto solo en la interfaz local. La base SQLite persiste en el volumen `tasks`, incluso al recrear el contenedor.

La primera ejecución carga las 33 opciones de `Hoja1` mediante una migración. El historial empieza vacío. El Excel original se conserva como referencia y el contenedor funciona sin acceder a él.

## Tareas, exportación y uso sin conexión

- **Hoy** muestra las tareas de la fecha actual en Paraguay. Elige una tienda y toca **Nueva tarea** para empezar con esa tienda seleccionada. Los campos de catálogo sugieren valores al escribir. Confirma **Agregar «valor»** para crear una opción.
- Desde **Historial → Exportar**, selecciona un rango inclusivo y descarga Excel o PDF con las tareas guardadas de todas las tiendas.
- En Chromium puedes instalar taskApp desde el menú del navegador. En Safari de iPhone, abre **Compartir → Agregar a pantalla de inicio**. La instalación está disponible con HTTPS o desde localhost.
- Tras iniciar sesión con conexión, el dispositivo guarda los catálogos y las tareas de los últimos 30 días. Puedes crear y editar esas tareas sin señal. Los cambios se envían al recuperar conexión con la app abierta o al volver a abrirla. En **Pendientes** puedes revisar cambios, reintentar el envío y resolver conflictos entre dispositivos.
- La copia y los cambios pendientes se guardan en el perfil del navegador. Si vence la sesión local, inicia sesión con conexión para recuperarlos. El historial completo y la exportación requieren conexión. Si ejecutas la app fuera de Docker, instala Typst en el `PATH` para habilitar PDF.

## En el servidor

1. Crea un túnel administrado en Cloudflare y configura el dominio público para que apunte a `http://app:8080`. Habilita la redirección de HTTP a HTTPS para ese dominio. El inicio de sesión se realiza en taskApp.
2. Crea el directorio local `secrets`. Guarda el token del túnel en `secrets/tunnel_token` y genera un hash Argon2id con `cargo run --locked -- hash-password`. El comando pide la contraseña sin mostrarla. Guarda su salida en `secrets/password_hash` como texto UTF-8 sin BOM. Restringe el acceso de cada archivo al operador y al usuario del contenedor correspondiente. La app usa UID 65532.
3. Define `TASKAPP_ORIGIN=https://tu-dominio` en `.env`, sin barra final. Arranca con `docker compose -f compose.prod.yaml up --build -d`. Esta configuración deja la app accesible dentro de la red de Compose y `cloudflared` conecta el dominio con ella.
4. Accede por HTTPS con el usuario `admin` y la contraseña nueva. Antes de mover una instalación existente, detén la app y copia el volumen SQLite al destino.
