# twokey

**Laissez l’IA écrire du code. Décidez de ce qu’elle ne peut pas modifier.**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

Un agent corrige un bug en affaiblissant le test. Une refactorisation touche la validation des jetons. Un nettoyage modifie une migration déjà exécutée en production.

**twokey transforme « ne touche pas à ça » en politique vérifiable du dépôt.** Définissez les limites dans `.twokey.toml`, obtenez un retour local et exigez une vérification réussie avant la fusion.

- Protégez des fonctions précises sans figer tout le fichier.
- Autorisez de nouveaux tests tout en bloquant les modifications de l’IA sur les tests existants.
- La CI lit la politique de la branche cible : une PR ne peut pas assouplir ses propres contrôles.
- Les changements sans étiquette sont considérés comme issus de l’IA par défaut, y compris ceux des modèles locaux.

Les contrôles utilisent des motifs de fichiers, des arbres syntaxiques, des marqueurs et des expressions régulières. Aucun appel à un modèle, aucune clé d’API ni envoi de code à un service d’IA. GitHub conserve la gestion des revues et des permissions de fusion.

## Que souhaitez-vous protéger ?

| Confiez à l’IA… | Protégez… | Type de règle |
| --- | --- | --- |
| L’interface de connexion | La validation des jetons et les permissions | Symboles |
| Un correctif et de nouveaux tests | Les tests existants | Symboles : modification/suppression |
| Une nouvelle migration | Les migrations déjà appliquées | Chemins : modification/suppression |
| Des fonctionnalités | La CI, les dépendances et la cryptographie | Chemins |
| Le code environnant | Un calcul ou une constante déjà validés | Marqueurs `twokey:lock` |
| La documentation | Les tirets cadratins et les formulations indésirables | Regex de contenu |

Les exemples de règles sont commentés. Activez ceux dont vous avez besoin et adaptez leurs chemins.

### Réservez la logique d’authentification aux humains

```toml
[[rule]]
id = "token-validation"
kind = "symbol"
paths = ["/src/auth/**"]
symbols = ["verify_token", "Session::verify"]
action = "deny"
reason = "Token validation must be changed by a human."
```

Cette règle bloque les modifications de l’IA sur `verify_token` et `Session::verify`, y compris leur renommage ou suppression. Modifier `greeting()` dans le même fichier peut rester autorisé. Utilisez une règle de chemin pour protéger aussi les imports et les fonctions auxiliaires.

### Ajoutez des tests sans réécrire les anciens

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

Protège les fonctions existantes des modules Rust `tests`. L’ajout de tests est autorisé ; la modification de leurs assertions ou leur suppression est bloquée.

### Verrouillez quelques lignes critiques

```rust
// twokey:lock-begin
pub const MAX_TRANSFER: u64 = 10_000;
// twokey:lock-end
```

Les marqueurs sont activés par défaut et lus depuis la révision de base. Modifier la constante ou supprimer les marqueurs est interdit à l’IA. Aucune règle de chemin supplémentaire n’est nécessaire.

### Appliquez vos conventions de rédaction

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

Bloque les tirets cadratins dans les lignes Markdown ajoutées ou remplacées, y compris pour les changements humains. Les lignes intactes restent inchangées. Omettez `human_action` pour ne viser que l’IA.

Les règles syntaxiques prennent en charge Rust, Python, JavaScript, TypeScript/TSX, Go, Java, C# et Ruby. Les règles de chemins et de contenu fonctionnent avec les autres types de fichiers.

La [politique de départ](../../examples/twokey.toml) contient aussi des exemples pour les migrations, les tests ignorés, les dépendances et les désactivations de linters.

## Installation et démarrage rapide

Utilisez la [GitHub Action pour la CI](../github-actions.md) ou installez la CLI depuis crates.io en local. Cargo nécessite Rust 1.90 ou ultérieur, Git et un compilateur C. Des [binaires et installateurs Windows, Linux et macOS](../../README.md#installation) sont aussi disponibles.

```sh
cargo install twokey --locked
```

Dans le dépôt à protéger, générez la politique, décommentez les règles voulues et adaptez leurs chemins :

```sh
twokey init
```

Les exemples de règles sont commentés. Activez ceux dont vous avez besoin et adaptez leurs chemins.

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --dry-run
```

`--dry-run` affiche les résultats sans bloquer. Enregistrez la politique dans Git avant d’activer son application. Les protections intégrées de la politique et des intégrations, les marqueurs et les restrictions par défaut sur les liens symboliques, sous-modules et changements de mode restent actifs.

### Déclarez un changement entièrement humain

Les changements sont considérés comme issus de l’IA par défaut. La configuration initiale utilise `#Human`, personnalisable avec `[ai].human_marker`. Pour un travail entièrement écrit par une personne :

```toml
[ai]
human_marker = "#Human"
```

```sh
git commit -m "Fix token validation" -m "#Human"
```

Placez le marqueur seul sur une ligne à la fin du message, après une ligne vide. Avec l’éditeur Git, configurez `git config core.commentChar ';'` pour conserver `#Human`. Les signaux explicites d’IA restent prioritaires ; une déclaration humaine ultérieure n’efface pas les modifications d’IA précédentes du même fichier. Cette déclaration ne prouve pas l’origine du code.

## Documentation et contributions

Cette page est une introduction traduite. La [documentation anglaise](../../README.md) fait référence. Consultez la [configuration](../configuration.md), l’[intégration GitHub](../github-actions.md) et le [modèle de sécurité](../security.md). Les hooks locaux peuvent être contournés ; une CI de confiance et la protection des branches assurent l’application de la politique. Les corrections de traduction sont bienvenues.

Responsable du projet : Nick Darnell. Distribué sous [CC0 1.0 Universal](../../LICENSE). Les dépendances conservent leurs propres licences.
