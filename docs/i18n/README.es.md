# twokey

**Deja que la IA escriba código. Decide qué no puede cambiar.**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

Un agente corrige un fallo debilitando el test. Una refactorización toca la validación de tokens. Una limpieza modifica una migración que ya se ejecutó en producción.

**twokey convierte «no toques eso» en una política verificable del repositorio.** Define los límites en `.twokey.toml`, recibe avisos durante el desarrollo y exige que la comprobación pase antes de fusionar.

- Protege funciones concretas sin bloquear todo el archivo.
- Permite nuevos tests y bloquea cambios de IA en los existentes.
- La CI lee la política de la rama de destino: una PR no puede relajar sus propias comprobaciones.
- Los cambios sin etiquetas se consideran de IA por defecto, incluidos los de modelos locales.

Las comprobaciones usan patrones de archivos, árboles sintácticos, marcadores y expresiones regulares. No requieren llamadas a modelos, claves de API ni enviar código a un servicio de IA. GitHub sigue gestionando las revisiones y los permisos de fusión.

## ¿Qué quieres proteger?

| Deja que la IA trabaje en… | Protege… | Tipo de regla |
| --- | --- | --- |
| La interfaz de acceso | La validación de tokens y los permisos | Símbolos |
| Un arreglo y nuevos tests | Los tests existentes | Símbolos: modificar/eliminar |
| Una nueva migración | Las migraciones ya aplicadas | Rutas: modificar/eliminar |
| Funcionalidades | CI, dependencias y criptografía | Rutas |
| El código circundante | Un cálculo o una constante revisados | Marcadores `twokey:lock` |
| Documentación | Rayas largas o expresiones no deseadas | Regex de contenido |

Las reglas de ejemplo están comentadas. Activa las que necesites y adapta sus rutas.

### Reserva la autenticación para cambios humanos

```toml
[[rule]]
id = "token-validation"
kind = "symbol"
paths = ["/src/auth/**"]
symbols = ["verify_token", "Session::verify"]
action = "deny"
reason = "Token validation must be changed by a human."
```

Esta regla bloquea cambios de IA en `verify_token` y `Session::verify`, incluidos su renombrado y eliminación. Una edición de `greeting()` en el mismo archivo puede pasar. Usa una regla de ruta si también necesitas proteger imports y funciones auxiliares.

### Añade tests sin reescribir los existentes

```toml
[[rule]]
id = "existing-rust-tests"
kind = "symbol"
paths = ["**/*.rs"]
symbols = ["tests::*"]
on = ["modified", "deleted"]
action = "deny"
reason = "Add regression tests; preserve existing test behavior."
```

Protege las funciones existentes de los módulos Rust `tests`. Permite añadir tests; bloquea cambiar sus aserciones o eliminarlos.

### Bloquea unas líneas críticas

```rust
// twokey:lock-begin
pub const MAX_TRANSFER: u64 = 10_000;
// twokey:lock-end
```

Los marcadores están activados por defecto y se leen desde la revisión base. Cambiar la constante o borrar los marcadores se bloquea para la IA. No hace falta otra regla de ruta.

### Aplica tus convenciones de escritura

```toml
[[rule]]
id = "no-em-dashes"
kind = "content"
paths = ["*.md"]
pattern = '\x{2014}'
lines = "added"
action = "deny"
human_action = "deny"
reason = "Use a comma, colon, parentheses, or a separate sentence."
```

Bloquea las rayas largas en líneas Markdown añadidas o sustituidas, incluso en cambios humanos. Las líneas intactas no se comprueban. Omite `human_action` para aplicarlo solo a la IA.

Las reglas sintácticas admiten Rust, Python, JavaScript, TypeScript/TSX, Go, Java, C# y Ruby. Las reglas de rutas y contenido funcionan con otros archivos.

Más ejemplos en la [política inicial](../../examples/twokey.toml): migraciones, omisiones de tests, dependencias y supresiones de linters.

## Instalación y primeros pasos

Usa la [GitHub Action para CI](../github-actions.md) o instala la CLI desde crates.io para trabajar localmente. Cargo requiere Rust 1.90 o posterior, Git y un compilador de C. También hay [binarios e instaladores para Windows, Linux y macOS](../../README.md#installation).

```sh
cargo install twokey --locked
```

En el repositorio que quieres proteger, genera la política, descomenta las reglas deseadas y adapta sus rutas:

```sh
twokey init
```

Las reglas de ejemplo están comentadas. Activa las que necesites y adapta sus rutas.

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --dry-run
```

`--dry-run` informa sin bloquear. Guarda la política en Git antes de activar su aplicación. La protección interna de la política y sus integraciones, los marcadores y las restricciones predeterminadas de enlaces simbólicos, submódulos y cambios de modo siguen activas.

### Declara un cambio completamente humano

Los cambios se consideran de IA por defecto. La configuración inicial usa `#Human`; puedes cambiarlo con `[ai].human_marker`. Para trabajo escrito íntegramente por una persona:

```toml
[ai]
human_marker = "#Human"
```

```sh
git commit -m "Fix token validation" -m "#Human"
```

Pon el marcador en su propia línea, al final del mensaje, después de una línea en blanco. Si usas el editor de Git, configura `git config core.commentChar ';'` para conservar `#Human`. Las señales explícitas de IA prevalecen; una declaración humana posterior no borra cambios de IA anteriores en el mismo archivo. La declaración no es una prueba de autoría.

## Documentación y contribuciones

Esta página es una introducción traducida. La [documentación en inglés](../../README.md) es la referencia principal. Consulta la [configuración](../configuration.md), la [integración con GitHub](../github-actions.md) y el [modelo de seguridad](../security.md). Los hooks locales se pueden eludir; la aplicación efectiva requiere CI de confianza y protección de ramas. Las correcciones de traducción son bienvenidas.

Mantenedor: Nick Darnell. Publicado bajo [CC0 1.0 Universal](../../LICENSE). Las dependencias conservan sus propias licencias.
