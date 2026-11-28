# Changelog

Los cambios relevantes se documentan siguiendo [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Unreleased

### Added

- Alta de hasta 100 tareas para la misma sucursal, con campos generales compartidos y un registro por tarea.
- Validación, persistencia atómica e idempotencia para guardar y sincronizar lotes, incluso ante cortes de conexión.
- Administración de usuarios y separación de tareas, sugerencias y colas offline por cuenta.
- Gestión de perfil y contraseña, con recuperación del administrador desde el binario.

### Changed

- Rediseño de Gráficos, con indicadores del período, distribución por estado y acceso al detalle desde las barras.
- Rediseño de las exportaciones: hoja Excel **Actividades** e informe PDF con resumen, registro y observaciones.
- El filtro de sucursales muestra los nombres sin contadores en las opciones.
- Mejoras en la navegación y la disposición de formularios en móvil y escritorio.
- Validación de usuarios simplificada para crear y editar cuentas.

### Fixed

- El menú de sugerencias se abre hacia arriba si falta espacio debajo del campo, por ejemplo al usar el teclado en móvil.
