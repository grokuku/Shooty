# Astre — coquille web (`web/`)

Cible navigateur d'Astre : cœur Rust (`crates/`) compilé en WebAssembly + coquille
TypeScript minimale. **Aucun framework**, **aucun bundler** : modules ES natifs,
`wasm-bindgen --target web`, `tsc` en simple transpilation.

La page affiche la démo « Astre » (ciel étoilé, planète, piste, balise, hangar, navette)
dans un rendu **terminal** WebGL2 : atlas de glyphes `R8` (ASCII + Braille + blocs),
1 `drawArraysInstanced` par image, zoomable aux touches, HUD de performance en DOM.

## Démarrage en trois commandes

```sh
cd web && npm install && npm run build      # wasm (wasm-pack) + TypeScript (tsc)
python3 -m http.server 8000 --directory web # ou : npx serve web
```

Puis ouvrir <http://localhost:8000/>. `npm run build` enchaîne :

1. `wasm-pack build --release --target web ../crates/astre-web --out-dir pkg`
   → `web/pkg/astre_web.js`, `astre_web_bg.wasm`, `astre_web.d.ts` ;
2. `tsc -p tsconfig.json` → `web/dist/*.js` (les `.ts` restent dans `src/`).

> Tout serveur statique convient. `python3 -m http.server` ne connaît pas le type MIME
> `application/wasm` : `wasm-bindgen` retombe alors sur une instanciation par
> `ArrayBuffer` (un avertissement « fallback » peut apparaître en console). Pour servir
> le bon MIME, `npx serve` est préférable.

### Construction manuelle du wasm (si `wasm-pack` est absent)

```sh
cargo build --release --target wasm32-unknown-unknown -p astre-web
# puis, avec le CLI wasm-bindgen de la MÊME version que la crate :
wasm-bindgen --target web --out-dir web/pkg \
  target/wasm32-unknown-unknown/release/astre_web.wasm
```

`wasm-pack` télécharge ce CLI automatiquement. Version épinglée : `wasm-bindgen =
0.2.130` (voir la racine `Cargo.toml`), `wasm-pack 0.15.1`.

## Vérifications

```sh
cargo test --workspace                     # 170+ tests (tous les crates)
cargo clippy --workspace --all-targets -- -D warnings
node web/tests/smoke.mjs                   # ABI wasm en Node, sans navigateur (§H.7)
cd web && npm run typecheck                # tsc --noEmit (typage strict)
```

Vérification navigateur (manuelle, ou scriptée si `chromium` est présent) : ouvrir la
page, la console doit être vide, le HUD doit afficher `BUILD`/`PRES` ≈ 60 et
`RAST`/`RES` non nuls. Les écarts de rendu (police de l'ASCII) sont attendus : les
glyphes Braille et blocs sont procéduraux, donc identiques partout.

## Arborescence

```
web/
  index.html          page plein écran, HUD DOM, panneau d'erreur (aucun framework CSS)
  src/
    main.ts           boot (WebGL2, wasm, timeout), boucle rAF, HUD 4 Hz, localStorage
    presenter.ts      WebGL2 : programme, VAO instancié, atlas R8, letterbox, contexte perdu
    atlas.ts          génération procédurale de l'atlas 32 × 16 slots (fillText / fillRect)
    input.ts          clavier DOM → codes Astre (flèches, ZQSD/WASD, zoom, plein écran)
  tests/smoke.mjs     smoke test de l'ABI wasm sans navigateur
  pkg/                sortie wasm-pack (NON versionnée)
  dist/               sortie tsc (NON versionnée)
```

## Touches

| Touche | Effet |
| --- | --- |
| Flèches, `Z`/`W`/`Q`/`A`/`S`/`D` | orbite (azimut) et élévation |
| `+`, `=`, `_`, pavé `+` | zoom avant (palier de cellule) |
| `-`, pavé `-` | zoom arrière |
| `0` | palier de zoom automatique |
| `R` | recadrage caméra (reset) |
| `P` | HUD DOM (les compteurs de la coquille web) |
| `Tab` | HUD **dans les cellules** (moteur, identique au natif) |
| `M` | bascule ASCII ↔ Braille |
| `Espace` | pause de l'animation |
| `F` | plein écran (sortie par `Échap`, gérée par le navigateur) |

Le zoom est persisté dans `localStorage["astre.cellPx"]` (lecture et écriture
`try/catch` : contexte contraint = zoom de session, jamais une erreur).
