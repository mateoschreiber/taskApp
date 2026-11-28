# Usuarios, permisos y recuperación

Abre **Cuenta → Usuarios** como administrador. Puedes crear cuentas, editar el usuario de acceso, elegir una contraseña temporal nueva o eliminar una cuenta. La contraseña temporal vence en 24 horas y debe cambiarse en el primer acceso. Compártela por privado. En **Cuenta → Mi cuenta**, cada persona puede editar su nombre visible y cambiar su contraseña, incluido el administrador.

| Operación | Admin | Cuenta normal |
| --- | --- | --- |
| Administrar cuentas | Sí | — |
| Consultar tareas, gráficos y exportaciones | Todas las cuentas, con filtro en Hoy, Historial, Gráficos y Exportar | Sus propias tareas |
| Crear, editar y sincronizar tareas | Sus propias tareas | Sus propias tareas |
| Sugerencias | Catálogo inicial y sus valores nuevos | Catálogo inicial y sus valores nuevos |
| Copia local | Hasta 30 días de sus propias tareas | Hasta 30 días de sus propias tareas |

## Datos, credenciales y sesiones

La migración 004 asigna las tareas, sugerencias personalizadas y recibos de sincronización anteriores a admin. El servidor deriva el propietario de la sesión y valida los permisos en cada solicitud. Las cuentas usan identificadores estables, contraseñas Argon2id y una versión de credenciales que invalida las sesiones anteriores al restablecer o cambiar la contraseña. Las sesiones de admin también se renuevan al reiniciar el proceso. Las sesiones del esquema anterior requieren iniciar sesión de nuevo.

Eliminar una cuenta elimina su credencial y sus sugerencias privadas. Las tareas conservan su contenido y pasan a **Cuenta eliminada**. SQLite registra la fecha, el identificador del administrador, el de la cuenta y la operación. Los registros de auditoría carecen de contraseñas y tokens.

La PWA separa las colas por cuenta. Los cambios pendientes anteriores a la migración quedan disponibles para admin después de iniciar sesión con conexión. Al cambiar de cuenta, la aplicación sustituye la copia activa por la de la cuenta autenticada. En un dispositivo sin red, la última copia válida puede seguir disponible durante ocho horas desde su actualización. El servidor rechaza cambios de cuentas eliminadas al reconectar. Cierra sesión antes de compartir el navegador.

## Recuperación

Si admin pierde su contraseña, consulta [Despliegue](despliegue.md#producción) para recuperar el acceso. Antes de actualizar Docker o migrar una base, sigue el procedimiento de [respaldo, actualización y reversión](despliegue.md#respaldo-actualización-y-reversión).
