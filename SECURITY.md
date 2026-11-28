# Seguridad y privacidad

## Reportar una vulnerabilidad

El reporte privado de GitHub está desactivado. El propietario tiene pendiente habilitar un canal privado. Si ya tienes contacto privado con quien mantiene taskApp, comparte allí un resumen, la versión o commit, el impacto y los pasos mínimos para verificar el problema con datos ficticios.

Evita publicar detalles de vulnerabilidades en issues, discusiones o pull requests. Excluye credenciales, tokens, datos operativos y copias de SQLite. Consulta la [configuración de reporte privado en GitHub](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository).

## Operación

- Usa HTTPS y una contraseña exclusiva para producción. Limita `admin:admin` a una base local de pruebas. Entrega contraseñas temporales por un canal privado.
- Mantén tokens, hashes, bases y respaldos fuera de Git, de las imágenes Docker y de los enlaces compartidos. Restringe el acceso al volumen y a sus copias según los permisos de tu equipo.
- Cierra sesión antes de ceder un navegador compartido. La PWA puede conservar tareas propias y cambios pendientes en IndexedDB. Verifica la sincronización antes de cerrar sesión o borrar el almacenamiento del navegador.
- Respalda el volumen completo con la aplicación detenida y verifica la restauración en una copia. Trata las exportaciones con el mismo cuidado que los datos originales.

Consulta [Despliegue](docs/despliegue.md) para HTTPS, secretos y respaldos, y [Usuarios](docs/usuarios.md) para permisos y recuperación.
