# Despliegue

## Local

Usa la [instalación rápida](../README.md#instalación-rápida) para probar taskApp desde la misma PC. [compose.yaml](../compose.yaml) publica el puerto solo en `127.0.0.1`. La credencial inicial `admin:admin` es exclusiva de una base nueva de pruebas.

## Producción

[compose.prod.yaml](../compose.prod.yaml) conecta taskApp con Cloudflare Tunnel en la red de Compose y prescinde de publicar el puerto de la aplicación en el host.

1. Configura un túnel de Cloudflare para dirigir un dominio al servicio `http://app:8080`. Fuerza HTTPS para ese dominio. Guarda el token en `secrets/tunnel_token`.
2. Ejecuta `cargo run --locked -- hash-password` con Rust disponible. Elige una contraseña de al menos 15 caracteres y hasta 256 bytes. Guarda el hash Argon2id que muestra el comando en `secrets/password_hash`, en UTF-8 sin BOM. Concede permiso de lectura al UID `65532` que usa la aplicación en el contenedor.
3. En `.env`, define `TASKAPP_ORIGIN=https://tareas.ejemplo.com` con el origen HTTPS real, sin barra final.
4. Arranca el servicio y revisa sus registros.

```bash
docker compose -f compose.prod.yaml up --build -d
docker compose -f compose.prod.yaml logs --tail 50 app
```

Entra por el dominio HTTPS como `admin` con la contraseña elegida. El hash del archivo inicializa la credencial cuando la base aún no la tiene. Después, la base conserva la credencial vigente y reemplazar el archivo no la rota. Usa **Cuenta → Mi cuenta** para cambiarla. Si perdiste el acceso, detén la aplicación y ejecuta estos comandos con el volumen del despliegue.

```bash
docker compose -f compose.prod.yaml stop app
docker compose -f compose.prod.yaml run --rm --no-deps app reset-admin-password
docker compose -f compose.prod.yaml up -d app
```

> [!IMPORTANT]
> Conserva la base de trabajo, el Excel original, `.env`, los respaldos y los secretos fuera de Git y de las imágenes. Usa una base de producción separada de la de pruebas. El servidor rechaza arrancar en producción si la credencial almacenada de administrador sigue siendo la de pruebas.

## Variables y archivos

| Configuración | Uso |
| --- | --- |
| `TASKAPP_MODE` | `local` (valor predeterminado) o `production`. Los archivos Compose lo asignan explícitamente. |
| `TASKAPP_PORT` | Puerto publicado por el Compose local en la PC. Predeterminado `8080`. |
| `TASKAPP_BIND` | Dirección de escucha. Predeterminado `127.0.0.1:8080` al ejecutar el binario y `0.0.0.0:8080` en Docker. |
| `TASKAPP_DB` | Ruta de SQLite. Predeterminado `taskapp.sqlite` al ejecutar el binario y `/data/taskapp.sqlite` en Docker. |
| `TASKAPP_ORIGIN` | Origen HTTPS público, obligatorio en producción. |
| `TASKAPP_PASSWORD_HASH_FILE` | Ruta del hash inicial de administrador para una base nueva en producción. Compose monta el secreto en `/run/secrets/password_hash`. |
| `secrets/tunnel_token` | Archivo local que el Compose de producción entrega a `cloudflared`. |

## Respaldo, actualización y reversión

El volumen `tasks` almacena todo `/data`. Compose administra un volumen por proyecto, así que conserva el mismo nombre de proyecto al actualizar. Evita `docker compose down -v` sobre un despliegue con datos.

1. Confirma que los dispositivos sincronizaron sus pendientes. Guarda el commit o la imagen anterior. Construye la nueva imagen antes de detener el servicio si puedes.
2. Detén la app y copia todo `/data` desde el contenedor detenido a un directorio privado y vacío. Incluye los archivos auxiliares de SQLite. Por ejemplo, para un entorno local en Bash:

   ```bash
   backup_dir="$HOME/taskapp-backups/antes-de-actualizar"
   mkdir -p "$backup_dir"
   docker compose stop app
   docker compose cp app:/data/. "$backup_dir"
   ```

3. Prueba la actualización en una copia. Con el volumen habitual, ejecuta `docker compose up --build -d` y revisa los registros, el acceso y el historial. En producción, usa `-f compose.prod.yaml` para estos comandos.
4. Para restaurar, conserva el volumen actual y crea uno vacío en un proyecto separado o con una configuración de Compose que lo monte en `/data`. Con la aplicación detenida, copia el directorio completo al nuevo volumen y asigna los archivos al UID y GID `65532`. Verifica acceso e historial con la copia antes de sustituir el volumen del servicio.
5. Si necesitas revertir una actualización, usa la imagen anterior con el respaldo previo a la migración. Conserva aparte la base que recibió cambios después del respaldo para revisarlos antes de reabrir el acceso.

Consulta la guía oficial de [respaldo y restauración de volúmenes de Docker](https://docs.docker.com/engine/storage/volumes/#back-up-restore-or-migrate-data-volumes) para montar ambos directorios en un contenedor auxiliar.
