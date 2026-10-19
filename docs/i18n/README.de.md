# twokey

**Menschliche Freigabe für wichtigen Code.**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

twokey prüft Codeänderungen anhand der Richtlinien deines Repositorys. Es kann KI zugeordnete Änderungen an geschützten Dateien blockieren oder die Freigabe bestimmter Personen verlangen. Die Prüfungen sind deterministisch und verwenden kein KI-Modell.

## Installation

Aus einer lokalen Kopie dieses Repositorys. Benötigt Rust 1.90 oder neuer, Git und einen C-Compiler.

```sh
git clone https://github.com/Azazel-Labs/twokey.git
cd twokey
cargo install --path . --locked
```

## Schnellstart

Kopiere das [minimale Beispiel](../../examples/minimal.toml) als `.twokey.toml` in das Stammverzeichnis deines Repositorys. Ersetze `your-github-login` durch den GitHub-Benutzernamen der prüfenden Person und passe die Pfade an. Das Beispiel blockiert KI-Änderungen an der CI-Konfiguration und verlangt eine Prüfung für `/src/auth/**`.

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --ai --dry-run
```

Die Vorschau behandelt Änderungen als KI-generiert und zeigt Ergebnisse an, ohne zu blockieren. Committe die Richtlinie, bevor du ihre Durchsetzung aktivierst.

## Dokumentation und Beiträge

Diese Seite ist eine übersetzte Einführung. Maßgeblich ist die [englische Dokumentation](../../README.md). Siehe [Konfiguration](../configuration.md), [GitHub-Integration](../github-actions.md) und [Sicherheitsmodell](../security.md). Lokale Hooks können umgangen werden; zur Durchsetzung sind vertrauenswürdige CI und geschützte Branches nötig. Übersetzungskorrekturen sind willkommen.

Maintainer: Nick Darnell. Veröffentlicht unter [CC0 1.0 Universal](../../LICENSE). Abhängigkeiten behalten ihre jeweiligen Lizenzen.
