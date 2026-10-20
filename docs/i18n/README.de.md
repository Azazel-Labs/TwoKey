# twokey

**Geschützten Code vor KI-Änderungen bewahren.**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

twokey blockiert KI-zugeordnete Änderungen an geschützten Dateien, Funktionen und Bereichen. Die Prüfungen sind deterministisch und verwenden keine KI-Modelle. Menschliche Reviews verwaltet GitHub; twokey hat keine Benutzer, Gruppen oder Freigaben.

## Installation

Aus einer lokalen Kopie dieses Repositorys. Benötigt Rust 1.90 oder neuer, Git und einen C-Compiler.

```sh
git clone https://github.com/Azazel-Labs/twokey.git
cd twokey
cargo install --path . --locked
```

## Schnellstart

Erstelle mit `twokey init` die [Startkonfiguration](../../examples/twokey.toml). Entferne die Kommentarzeichen bei den gewünschten Regeln und passe ihre Pfade an. Änderungen gelten standardmäßig als KI-gestützt; vollständig menschliche Commits deklarieren `Human-Authored: true` am Ende der Commit-Nachricht nach einer Leerzeile.

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --ai --dry-run
```

Die Vorschau behandelt Änderungen als KI-generiert und zeigt Ergebnisse an, ohne zu blockieren. Committe die Richtlinie, bevor du ihre Durchsetzung aktivierst.

## Dokumentation und Beiträge

Diese Seite ist eine übersetzte Einführung. Maßgeblich ist die [englische Dokumentation](../../README.md). Siehe [Konfiguration](../configuration.md), [GitHub-Integration](../github-actions.md) und [Sicherheitsmodell](../security.md). Lokale Hooks können umgangen werden; zur Durchsetzung sind vertrauenswürdige CI und geschützte Branches nötig. Übersetzungskorrekturen sind willkommen.

Maintainer: Nick Darnell. Veröffentlicht unter [CC0 1.0 Universal](../../LICENSE). Abhängigkeiten behalten ihre jeweiligen Lizenzen.
