# Shooty

Jeu en développement.

Le moteur de rendu terminal (rasterisation Braille/ASCII, HUD, boucle à pas fixe,
cible WebAssembly) ne vit **pas** ici : il est dans
**[HET](https://github.com/grokuku/HET)**. Ce dépôt ne contient que le jeu — une scène
placeholder pour l'instant, le contenu réel reste à écrire.

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

Le jeu consomme les crates du moteur via des dépendances `git` épinglées sur un tag :

```toml
het-core = { git = "https://github.com/grokuku/HET", tag = "v0.1.0" }
het-app  = { git = "https://github.com/grokuku/HET", tag = "v0.1.0" }
het-term = { git = "https://github.com/grokuku/HET", tag = "v0.1.0" }
het-web  = { git = "https://github.com/grokuku/HET", tag = "v0.1.0" }
```

Pour développer contre une copie locale de HET **sans** toucher aux manifestes,
cr\u00e9er un `.cargo/config.toml` (gitignor\u00e9) :

```toml
[patch."https://github.com/grokuku/HET"]
het-core = { path = "/chemin/vers/HET/crates/het-core" }
het-app  = { path = "/chemin/vers/HET/crates/het-app" }
het-term = { path = "/chemin/vers/HET/crates/het-term" }
het-web  = { path = "/chemin/vers/HET/crates/het-web" }
```

**Limite de Cargo \u00e0 conna\u00eetre** : le `[patch]` ne dispense pas Cargo de r\u00e9soudre la
r\u00e9f\u00e9rence de la d\u00e9pendance `git` \u2014 il t\u00e9l\u00e9charge le d\u00e9p\u00f4t et exige que le `tag`
(ou la `branch`/le `rev`) existe. Tant que `v0.1.0` n'est pas tagu\u00e9 c\u00f4t\u00e9 HET, il faut
donc aussi pointer les quatre d\u00e9pendances sur une r\u00e9f\u00e9rence existante
(`branch = "main"`), le patch fournissant le code : c'est un \u00e9cart **local et
temporaire**, `tag = "v0.1.0"` reste la forme versionn\u00e9e.

## Vérifications

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
node web/tests/smoke.mjs        # après npm run build:wasm
```

## Documentation

- Architecture du moteur : [HET/docs/ARCHITECTURE.md](https://github.com/grokuku/HET/blob/main/docs/ARCHITECTURE.md)
- Note locale : [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- `test3Dterminal/` : oracle C gelé (référence historique du portage, non compilé par
  le workspace Rust).

## Licence

MIT — voir [LICENSE](LICENSE).
