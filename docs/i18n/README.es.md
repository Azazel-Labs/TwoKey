# twokey

**Aprobación humana para el código que importa.**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

twokey comprueba los cambios de código según la política de tu repositorio. Permite bloquear cambios atribuidos a IA en archivos protegidos o exigir la aprobación de personas concretas. Las comprobaciones son deterministas y no utilizan modelos de IA.

## Instalación

Desde una copia local de este repositorio. Requiere Rust 1.90 o posterior, Git y un compilador de C.

```sh
git clone https://github.com/Azazel-Labs/twokey.git
cd twokey
cargo install --path . --locked
```

## Primeros pasos

Copia el [ejemplo mínimo](../../examples/minimal.toml) a `.twokey.toml` en la raíz de tu repositorio. Sustituye `your-github-login` por el usuario de GitHub del revisor y adapta las rutas. El ejemplo bloquea cambios de IA en la configuración de CI y exige revisión para `/src/auth/**`.

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --ai --dry-run
```

La vista previa trata los cambios como generados por IA y muestra los resultados sin bloquear. Confirma la política en Git antes de activar su aplicación.

## Documentación y contribuciones

Esta página es una introducción traducida. La [documentación en inglés](../../README.md) es la referencia principal. Consulta la [configuración](../configuration.md), la [integración con GitHub](../github-actions.md) y el [modelo de seguridad](../security.md). Los hooks locales se pueden eludir; la aplicación efectiva requiere CI de confianza y protección de ramas. Las correcciones de traducción son bienvenidas.

Mantenedor: Nick Darnell. Publicado bajo [CC0 1.0 Universal](../../LICENSE). Las dependencias conservan sus propias licencias.
