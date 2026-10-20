# twokey

**Protege el código frente a cambios de IA.**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

twokey bloquea cambios atribuidos a IA en archivos, funciones y regiones protegidas. Las comprobaciones son deterministas y no utilizan modelos de IA. GitHub gestiona las revisiones humanas; twokey no tiene usuarios, grupos ni un sistema de aprobación.

## Instalación

Desde una copia local de este repositorio. Requiere Rust 1.90 o posterior, Git y un compilador de C.

```sh
git clone https://github.com/Azazel-Labs/twokey.git
cd twokey
cargo install --path . --locked
```

## Primeros pasos

Ejecuta `twokey init` para crear la [configuración inicial](../../examples/twokey.toml). Descomenta las reglas que quieras y adapta sus rutas. Los cambios se consideran asistidos por IA por defecto; los commits completamente humanos declaran `Human-Authored: true` al final del mensaje, después de una línea en blanco.

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --ai --dry-run
```

La vista previa trata los cambios como generados por IA y muestra los resultados sin bloquear. Confirma la política en Git antes de activar su aplicación.

## Documentación y contribuciones

Esta página es una introducción traducida. La [documentación en inglés](../../README.md) es la referencia principal. Consulta la [configuración](../configuration.md), la [integración con GitHub](../github-actions.md) y el [modelo de seguridad](../security.md). Los hooks locales se pueden eludir; la aplicación efectiva requiere CI de confianza y protección de ramas. Las correcciones de traducción son bienvenidas.

Mantenedor: Nick Darnell. Publicado bajo [CC0 1.0 Universal](../../LICENSE). Las dependencias conservan sus propias licencias.
