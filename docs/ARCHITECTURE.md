# Shooty — architecture

**Statut** : pointeur. La spécification d'architecture du moteur a déménagé avec lui.

Le moteur (rasterisation terminal, résolution Braille/ASCII, HUD, boucle à pas fixe,
cible WebAssembly) vit désormais dans le dépôt **HET** :

- <https://github.com/grokuku/HET>
- Spécification complète : <https://github.com/grokuku/HET/blob/main/docs/ARCHITECTURE.md>

Shooty ne contient plus que le **jeu** : une scène (`crates/shooty-game`), un binaire
natif (`crates/shooty-native`) et une coquille web (`crates/shooty-web` + `web/`), tous
branchés sur les crates `het-*` publiées par HET (dépendances `git` + tag `v0.1.0`).

L'ancienne version de ce document (spécification d'`astre-core`/`astre-app`, désormais
`het-core`/`het-app`) reste consultable dans l'historique Git de ce dépôt.
`test3Dterminal/` conserve l'oracle C gelé à titre de référence historique ; il n'est
plus le moteur du jeu.
