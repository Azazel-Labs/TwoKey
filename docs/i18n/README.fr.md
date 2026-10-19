# twokey

**Une approbation humaine pour le code qui compte.**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

twokey vérifie les modifications du code selon la politique de votre dépôt. Il peut bloquer les modifications attribuées à une IA dans les fichiers protégés ou exiger une approbation de personnes désignées. Les vérifications sont déterministes et ne font appel à aucun modèle d’IA.

## Installation

Depuis une copie locale de ce dépôt. Nécessite Rust 1.90 ou ultérieur, Git et un compilateur C.

```sh
git clone https://github.com/Azazel-Labs/twokey.git
cd twokey
cargo install --path . --locked
```

## Démarrage rapide

Copiez l’[exemple minimal](../../examples/minimal.toml) dans `.twokey.toml` à la racine de votre dépôt. Remplacez `your-github-login` par le compte GitHub du réviseur et adaptez les chemins. L’exemple bloque les modifications de la CI attribuées à une IA et exige une revue pour `/src/auth/**`.

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --ai --dry-run
```

Cet aperçu considère les modifications comme produites par une IA et affiche les résultats sans bloquer. Enregistrez la politique dans Git avant d’activer son application.

## Documentation et contributions

Cette page est une introduction traduite. La [documentation anglaise](../../README.md) fait référence. Consultez la [configuration](../configuration.md), l’[intégration GitHub](../github-actions.md) et le [modèle de sécurité](../security.md). Les hooks locaux peuvent être contournés ; une CI de confiance et la protection des branches assurent l’application de la politique. Les corrections de traduction sont bienvenues.

Responsable du projet : Nick Darnell. Distribué sous [CC0 1.0 Universal](../../LICENSE). Les dépendances conservent leurs propres licences.
