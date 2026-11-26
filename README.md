# Shooty

Jeu en développement. **« Shooty » n'est qu'un nom provisoire** : le nom de travail
du projet, en attendant un titre définitif. Les crates gardent ce préfixe tant que le
nom n'est pas arrêté.

Le moteur de rendu terminal (rasterisation Braille/ASCII, HUD, boucle à pas fixe,
cible WebAssembly) ne vit **pas** ici : il est dans
**[HET](https://github.com/grokuku/HET)**. Ce dépôt ne contient que le jeu — une scène
placeholder pour l'instant, le contenu réel reste à écrire.

L'oracle C du moteur (`test3Dterminal/`, la référence gelée des goldens de parité) est
lui aussi dans HET, et **plus dans ce dépôt** : rien de C n'est compilé ici.

## Démarrage

```sh
cargo run -p shooty-native --release           # jeu dans le terminal
cargo run -p shooty-native -- --help           # options CLI (celles du moteur HET)
cargo run -p shooty-native -- --snapshot --plain --size 80x24   # image ANSI sur stdout
```

Version web (WASM + WebGL2) :

```sh
cd web && npm install && npm run build
python3 -m http.server 8000 --directory .   # puis http://localhost:8000/
```

## Dépendances

Le jeu consomme les crates du moteur via des dépendances `git` épinglées sur une
révision (`rev`) du dépôt HET :

```toml
het-core = { git = "https://github.com/grokuku/HET", rev = "dbac58063dd32997cedbe5b3cee83a2abc37c50d" }
het-app  = { git = "https://github.com/grokuku/HET", rev = "dbac58063dd32997cedbe5b3cee83a2abc37c50d" }
het-term = { git = "https://github.com/grokuku/HET", rev = "dbac58063dd32997cedbe5b3cee83a2abc37c50d" }
het-web  = { git = "https://github.com/grokuku/HET", rev = "dbac58063dd32997cedbe5b3cee83a2abc37c50d" }
```

Pour mettre à jour le moteur, changez la révision dans `Cargo.toml`
(`[workspace.dependencies]`) — c'est le seul endroit à modifier.

Pour développer contre une copie locale de HET **sans** toucher aux manifestes,
créer un `.cargo/config.toml` (gitignoré) — un exemple est fourni dans
[`.cargo/config.toml.exemple`](.cargo/config.toml.exemple) :

```toml
[patch."https://github.com/grokuku/HET"]
het-core = { path = "../HET/crates/het-core" }
het-app  = { path = "../HET/crates/het-app" }
het-term = { path = "../HET/crates/het-term" }
het-web  = { path = "../HET/crates/het-web" }
```

**Limite de Cargo à connaître** : le `[patch]` ne dispense pas Cargo de résoudre la
référence de la dépendance `git` — il télécharge le dépôt et exige que la révision
(`rev`) existe. Celle épinglée dans `Cargo.toml` existe sur le dépôt HET ; le patch
fournit alors le code local à la place. Pour les modifications du moteur, changez la
révision épinglée dans `Cargo.toml` ; le `[patch]` ne sert qu'au développement local.

## Vérifications

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
node web/tests/smoke.mjs        # après npm run build:wasm
```

## Documentation

- Architecture du moteur (et de son oracle C gelé) :
  [HET/docs/ARCHITECTURE.md](https://github.com/grokuku/HET/blob/main/docs/ARCHITECTURE.md)
- Note locale : [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)

## Licence

MIT — voir [LICENSE](LICENSE).
