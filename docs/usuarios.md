# Usuarios, permisos y recuperación

Abre **Cuenta → Usuarios** como administrador. Puedes crear cuentas, editar el usuario de acceso, elegir una contraseña temporal nueva o eliminar una cuenta. La contraseña temporal vence en 24 horas y debe cambiarse en el primer acceso. Compártela por privado. En **Cuenta → Mi cuenta**, cada persona puede editar su nombre visible y cambiar su contraseña, incluido el administrador.

| Operación | Admin | Cuenta normal |
| --- | --- | --- |
| Administrar cuentas | Sí | — |
| Consultar tareas, gráficos y exportaciones | Todas las cuentas, con filtro en Historial, Gráficos y Exportar | Sus propias tareas |
| Crear, editar y sincronizar tareas | Sus propias tareas | Sus propias tareas |
| Sugerencias | Catálogo inicial y sus valores nuevos | Catálogo inicial y sus valores nuevos |
| Copia local | Hasta 30 días de sus propias tareas | Hasta 30 días de sus propias tareas |

## Datos, credenciales y sesiones

La migración 004 asigna las tareas, sugerencias personalizadas y recibos de sincronización anteriores a admin. El servidor deriva el propietario de la sesión y valida los permisos en cada solicitud. Las cuentas usan identificadores estables, contraseñas Argon2id y una versión de credenciales que invalida las sesiones anteriores al restablecer o cambiar la contraseña. Las sesiones de admin también se renuevan al reiniciar el proceso. Las sesiones del esquema anterior requieren iniciar sesión de nuevo.

Eliminar una cuenta elimina su credencial y sus sugerencias privadas. Las tareas conservan su contenido y pasan a **Cuenta eliminada**. SQLite registra la fecha, el identificador del administrador, el de la cuenta y la operación. Los registros de auditoría carecen de contraseñas y tokens.

La PWA separa las colas por cuenta. Los cambios pendientes anteriores a la migración quedan disponibles para admin después de iniciar sesión con conexión. Al cambiar de cuenta, la aplicación sustituye la copia activa por la de la cuenta autenticada. En un dispositivo sin red, la última copia válida puede seguir disponible durante ocho horas desde su actualización. El servidor rechaza cambios de cuentas eliminadas al reconectar. Cierra sesión antes de compartir el navegador.

## Actualizar Docker y recuperar una copia

1. Antes de actualizar, confirma que los dispositivos sincronizaron sus cambios y construye la nueva imagen. Conserva la imagen anterior para poder revertir.
2. Detén el servicio con `docker compose stop app`. Respalda el contenido completo de `/data` del contenedor detenido en una carpeta privada fuera del repositorio. Incluye cualquier archivo auxiliar de SQLite. Prueba primero la migración en una copia.
3. Ejecuta `docker compose up -d --build` con el volumen existente. Revisa `docker compose logs --tail 50 app` y confirma que admin puede iniciar sesión, que el historial conserva las tareas y que una cuenta normal solo ve las suyas.
4. Para revertir, conserva el volumen migrado y restaura la copia previa en un volumen aparte para arrancar con la imagen anterior. Revisa los cambios posteriores al respaldo antes de volver a habilitar el acceso.

En producción aplica los mismos pasos con `docker compose -f compose.prod.yaml`. Mantén el origen HTTPS y los secretos de Cloudflare y del hash de admin. Para rotar la contraseña de admin, genera un hash con `cargo run --locked -- hash-password`, reemplaza el secreto y recrea el servicio.
