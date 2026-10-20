# twokey

**Lass KI Code schreiben. Entscheide, was sie nicht ändern darf.**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

Ein Agent behebt einen Fehler, indem er den Test abschwächt. Ein Refactoring verändert die Tokenprüfung. Beim Aufräumen wird eine Migration bearbeitet, die bereits in Produktion gelaufen ist.

**twokey macht aus „Fass das nicht an“ eine überprüfbare Repository-Richtlinie.** Lege die Grenzen in `.twokey.toml` fest, erhalte lokal Rückmeldung und verlange eine erfolgreiche Prüfung vor dem Merge.

- Schütze einzelne Funktionen, ohne die ganze Datei zu sperren.
- Erlaube neue Tests und blockiere KI-Änderungen an bestehenden Tests.
- Die CI liest die Richtlinie aus dem Zielbranch: Ein PR kann seine eigenen Prüfungen nicht abschwächen.
- Änderungen ohne Kennzeichnung gelten standardmäßig als KI, auch bei lokalen Modellen.

Die Prüfungen verwenden Dateimuster, Syntaxbäume, Marker und reguläre Ausdrücke. Keine Modellaufrufe, API-Schlüssel oder Code-Uploads an einen KI-Dienst. Reviews und Merge-Berechtigungen bleiben bei GitHub.

## Was möchtest du schützen?

| Überlasse der KI… | Schütze… | Regeltyp |
| --- | --- | --- |
| Die Anmeldeoberfläche | Tokenprüfung und Berechtigungen | Symbole |
| Fehlerbehebungen und neue Tests | Bestehende Tests | Symbole: Ändern/Löschen |
| Eine neue Migration | Bereits angewendete Migrationen | Pfade: Ändern/Löschen |
| Neue Funktionen | CI, Abhängigkeiten und Kryptografie | Pfade |
| Den umgebenden Code | Eine geprüfte Berechnung oder Konstante | `twokey:lock`-Marker |
| Dokumentation | Lange Gedankenstriche und unerwünschte Formulierungen | Inhaltsregex |

Die Beispielregeln sind auskommentiert. Aktiviere die gewünschten Regeln und passe ihre Pfade an.

### Authentifizierungslogik bleibt menschliche Arbeit

```toml
[[rule]]
id = "token-validation"
kind = "symbol"
paths = ["/src/auth/**"]
symbols = ["verify_token", "Session::verify"]
action = "deny"
reason = "Token validation must be changed by a human."
```

Diese Regel blockiert KI-Änderungen an `verify_token` und `Session::verify`, auch Umbenennungen und Löschungen. Eine Änderung an `greeting()` in derselben Datei kann erlaubt bleiben. Schütze das gesamte Modul per Pfadregel, wenn auch Imports und Hilfsfunktionen unverändert bleiben sollen.

### Neue Tests hinzufügen, bestehende bewahren

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

Schützt bestehende Funktionen in Rust-Modulen namens `tests`. Neue Tests sind erlaubt; Änderungen an bestehenden Assertions und das Löschen von Tests werden blockiert.

### Einige kritische Zeilen sperren

```rust
// twokey:lock-begin
pub const MAX_TRANSFER: u64 = 10_000;
// twokey:lock-end
```

Marker sind standardmäßig aktiv und werden aus der Basisrevision gelesen. KI darf weder die Konstante ändern noch die Marker entfernen. Eine zusätzliche Pfadregel ist dafür nicht nötig.

### Schreibkonventionen automatisch prüfen

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

Blockiert lange Gedankenstriche in hinzugefügten oder ersetzten Markdown-Zeilen, auch bei menschlichen Änderungen. Unberührte Zeilen bleiben unbeachtet. Ohne `human_action` gilt die Regel nur für KI.

Syntaxregeln unterstützen Rust, Python, JavaScript, TypeScript/TSX, Go, Java, C# und Ruby. Pfad- und Inhaltsregeln funktionieren auch mit anderen Dateitypen.

Weitere Beispiele für Migrationen, übersprungene Tests, Abhängigkeiten und Linter-Unterdrückungen findest du in der [Startkonfiguration](../../examples/twokey.toml).

## Installation und Schnellstart

Nutze die [GitHub Action für CI](../github-actions.md) oder installiere die CLI lokal über crates.io. Cargo benötigt Rust 1.90 oder neuer, Git und einen C-Compiler. Auch [fertige Binärdateien und Installer für Windows, Linux und macOS](../../README.md#installation) stehen bereit.

```sh
cargo install twokey --locked
```

Erzeuge im zu schützenden Repository die Richtlinie, aktiviere die gewünschten Regeln und passe ihre Pfade an:

```sh
twokey init
```

Die Beispielregeln sind auskommentiert. Aktiviere die gewünschten Regeln und passe ihre Pfade an.

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --dry-run
```

`--dry-run` zeigt Ergebnisse an, ohne zu blockieren. Committe die Richtlinie vor der Durchsetzung. Der eingebaute Schutz der Richtlinie und ihrer Integrationen, Marker sowie die Standardbeschränkungen für Symlinks, Submodule und Modusänderungen bleiben aktiv.

### Vollständig menschliche Änderungen kennzeichnen

Änderungen gelten standardmäßig als KI. Die Startkonfiguration verwendet `#Human`; über `[ai].human_marker` kannst du den Marker ändern. Für vollständig menschlich geschriebenen Code:

```toml
[ai]
human_marker = "#Human"
```

```sh
git commit -m "Fix token validation" -m "#Human"
```

Setze den Marker auf eine eigene Zeile am Ende der Commit-Nachricht, nach einer Leerzeile. Konfiguriere bei Verwendung des Git-Editors `git config core.commentChar ';'`, damit Git `#Human` beibehält. Explizite KI-Signale haben Vorrang; ein späterer menschlicher Commit hebt frühere KI-Änderungen an derselben Datei nicht auf. Der Marker ist eine Erklärung, kein Nachweis der Urheberschaft.

## Dokumentation und Beiträge

Diese Seite ist eine übersetzte Einführung. Maßgeblich ist die [englische Dokumentation](../../README.md). Siehe [Konfiguration](../configuration.md), [GitHub-Integration](../github-actions.md) und [Sicherheitsmodell](../security.md). Lokale Hooks können umgangen werden; zur Durchsetzung sind vertrauenswürdige CI und geschützte Branches nötig. Übersetzungskorrekturen sind willkommen.

Maintainer: Nick Darnell. Veröffentlicht unter [CC0 1.0 Universal](../../LICENSE). Abhängigkeiten behalten ihre jeweiligen Lizenzen.
