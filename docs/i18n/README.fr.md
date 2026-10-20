# twokey

**Protégez le code contre les modifications de l’IA.**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

twokey bloque les modifications attribuées à l’IA dans les fichiers, fonctions et régions protégés. Les vérifications sont déterministes et n’utilisent aucun modèle d’IA. GitHub gère les revues humaines ; twokey ne gère ni utilisateurs, ni groupes, ni approbations.

## Installation

Depuis une copie locale de ce dépôt. Nécessite Rust 1.90 ou ultérieur, Git et un compilateur C.

```sh
git clone https://github.com/Azazel-Labs/twokey.git
cd twokey
cargo install --path . --locked
```

## Démarrage rapide

Exécutez `twokey init` pour créer la [configuration de départ](../../examples/twokey.toml). Décommentez les règles souhaitées et adaptez leurs chemins. Les changements sont considérés comme assistés par IA par défaut ; les commits entièrement humains déclarent `Human-Authored: true` à la fin du message, après une ligne vide.

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --ai --dry-run
```

Cet aperçu considère les modifications comme produites par une IA et affiche les résultats sans bloquer. Enregistrez la politique dans Git avant d’activer son application.

## Documentation et contributions

Cette page est une introduction traduite. La [documentation anglaise](../../README.md) fait référence. Consultez la [configuration](../configuration.md), l’[intégration GitHub](../github-actions.md) et le [modèle de sécurité](../security.md). Les hooks locaux peuvent être contournés ; une CI de confiance et la protection des branches assurent l’application de la politique. Les corrections de traduction sont bienvenues.

Responsable du projet : Nick Darnell. Distribué sous [CC0 1.0 Universal](../../LICENSE). Les dépendances conservent leurs propres licences.
