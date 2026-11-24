# Astre — architecture de la réécriture Rust/WASM

**Statut** : document d'architecture, v1 (décisions actées, prêt à implémenter).
**Périmètre** : le moteur 3D terminal « Astre » et sa déclinaison web.
**Hors périmètre** : `test3Dterminal/quake/` (frontend libretro TyrQuake « Relais de Cendre »), qui reste en C et n'est ni porté ni modifié.
**Langue** : français. Les identifiants de code restent en anglais, sauf les chaînes de HUD qui sont un contrat visuel (voir §G.4).

---

## 0. Conventions de lecture

| Convention | Sens |
| --- | --- |
| **C-REF** | Fait vérifié par lecture du code C de `test3Dterminal/` (référence citée `fichier:symbole`). |
| **ACTÉ** | Décision déjà arbitrée, non rediscutable dans ce document. |
| **ARBITRÉ ICI** | Décision tranchée par ce document, avec justification. |
| **`[à mesurer]`** | Ordre de grandeur plausible, **non mesuré**. Tout chiffre de performance ou de taille non mesuré porte cette marque. |
| Pseudo-code Rust | Illustratif. Aucun fichier de code applicatif n'accompagne ce document : les blocs montrent des signatures et des invariants, pas une implémentation. |

Aucune mesure n'a été faite pour ce document : les chiffres de temps, de débit et de taille binaire sont des estimations dimensionnantes à valider sur machine cible (§H.6, §I).

---

## A. Contexte, existant, objectifs

### A.1 Ce qui existe aujourd'hui (C-REF)

Le dépôt `/projects/Shooty` contient un unique dossier de travail `test3Dterminal/`, projet C99 sans dépendance hors `libm`, compilé par un `Makefile` (`CFLAGS = -O2 -std=c99 -Wall -Wextra -Wpedantic`, `LDLIBS = -lm`). Cible `make` → binaire `astre` ; `make check` → `tests/render` (seul test) ; cibles Windows via `mingw32-make astre.exe`.

| Fichier | Lignes | Rôle |
| --- | --- | --- |
| `term3d.h` | 44 | API publique : `T3Vec`, `T3Color`, `T3Cell`, `T3Camera`, `T3Mode`, `T3Anchor`, `T3Sky`. |
| `term3d.c` | 249 | Moteur : `t3_begin`, `t3_triangle`, `t3_resolve`, `t3_fill`, `t3_text`, `t3_import_rgb565`, `resolve_cell`. |
| `terminal.h` | 20 | API backend : `T3Palette`, codes de touches, `t3_present`, `t3_snapshot`. |
| `terminal.c` | 327 | Termios/console Win32, encodeur ANSI différentiel, palettes RGB/256/plain. |
| `demo.c` | 257 | Boucle live, options CLI, HUD (`overlay`), moniteur de perf, orbite. |
| `demo_scene.c` | 215 | Scène : ciel étoilé + planète, navette, piste, balises, hangar. |
| `tests/render.c` | ~63 | Assertions profondeur / clipping / glyphes / ancrage HUD. |

Faits structurants relevés dans le C, à préserver ou à traiter explicitement au portage :

1. **Grille d'échantillons 2 × 4 par cellule.** `t3_resize` fixe `w = cols*2`, `h = rows*4` (`term3d.c:t3_resize`). Une `T3Frame` alloue `8·cols·rows` couleurs (`samples`) **et** `8·cols·rows` profondeurs `float` (`depth`).
2. **Le ciel est évalué par échantillon**, pas par cellule : `t3_begin` boucle sur `w × h` et appelle le callback `T3Sky` pour chaque échantillon (`term3d.c:t3_begin`). À 240 × 72 cellules cela fait 480 × 288 = **138 240** évaluations de ciel par image, contre 17 280 appels à `resolve_cell` — le ciel est le premier poste de coût du moteur actuel.
3. **Profondeur en `1/z`** : `project` renvoie `iz = 1/z`, `raster` interpole `iz` par barycentriques et garde le maximum (`iz > depth[i]`). Test de profondeur strict, donc « plus proche gagne », indépendant de l'ordre de soumission (vérifié par `tests/render.c:visibility`).
4. **Triangles double-face** : `raster` inverse deux sommets si `area < 0`, avec rejet de `|area| < 0.00001f`.
5. **Clipping du plan proche** en espace vue, Sutherland–Hodgman à 3→4 sommets, `near_z` borné à `max(0.001f, near_z)` (`term3d.c:t3_triangle`).
6. **Résolution cellule** (`term3d.c:resolve_cell`) : on trouve `p[hi]` = échantillon le plus éloigné de `p[0]`, puis `p[lo]` = plus éloigné de `p[hi]` ; deux groupes par distance à `p[lo]`/`p[hi]`, seuil de séparation `160` si la cellule contient de la géométrie (`surface`, dérivé de `depth > 0`), `1500` sinon (ciel). Le groupe 1 forme un masque de 8 bits (bit `i` = échantillon `i`, parcours ligne par ligne de gauche à droite).
7. **Table ASCII** de 19 `Shape {char c; uint8_t mask}` choisie par distance de Hamming minimale, **inversion autorisée** (`d > 4 ⇒ d = 8-d`) ; le champ `mask` sert alors de booléen « inversé » — champ à double sens, piège de portage (§G.5).
8. **Glyphes Braille** : `dots[] = {1,8,2,16,4,32,64,128}` mappe l'échantillon `2j+col` sur le bit standard du point braille ; inversion si la grappe d'encre dépasse 4 échantillons. **Blocs** : table de 16 codepoints utilisée seulement si les deux bandes de 2 lignes sont identiques (`!((mask ^ (mask>>2)) & 0x33)`), index `(mask&3) | ((mask>>2)&12)`.
9. **Boucle live = temps variable.** `demo.c:live` mesure `dt` réel, borne à 0,12 s, et **dort** pour tenir `--fps` (`wait_frame(1.0/fps - (end-start))`). La simulation (angle d'orbite, horloge `seconds`) avance de ce `dt` variable : le résultat n'est pas reproductible d'une exécution à l'autre.
10. **Snapshot déterministe** déjà présent : `--snapshot [--size WxH] [--time s] [--plain]` rend une image figée et l'écrit en ANSI sur `stdout` (`demo.c:render` + `terminal.c:t3_snapshot`). C'est **le** point d'ancrage des tests de non-régression.
11. **Encodeur ANSI différentiel** : `t3_present` compare à un buffer `previous`, saute les cellules égales, regroupe les cellules modifiées contiguës en un seul positionnement curseur (`\033[y;xH`), réémet une couleur seulement si la clé change, et force un redraw complet au changement de taille ou de palette. `equal_cell` ignore `fg` quand `glyph == ' '`.
12. **Bornes de resize** dans le C : chaque dimension `1..=4096`, sans plafond de surface — `t3_resize(4096, 4096)` demanderait 16,7 M cellules, soit ≈ 1 Gio d'échantillons + profondeurs. Le C se contente d'échouer si `malloc` rend `NULL`.
13. **État global** : `static Shape lookup[256]` (cache ASCII dans `t3_resolve`), `static volatile sig_atomic_t quitting/suspending` (Unix), `static volatile LONG quitting` (Win32). Tout cela disparaît dans la version Rust (§C.5, « jamais de global »).
14. `relay.c` (Quake) réutilise `t3_import_rgb565` ; hors périmètre, **non porté**.

### A.2 Objectifs de la v1

| # | Objectif |
| --- | --- |
| O1 | Rendre la **même image** que le C de référence (parité mesurée par golden tests, §H.3). |
| O2 | Pivoter à l'écran sans terminal, dans un navigateur, avec un rendu **terminal** (atlas monospace, ASCII + Braille + blocs), aspect de cellule strict. |
| O3 | Conserver le terminal natif comme cible de première classe, à partir du **même cœur**. |
| O4 | Rendre le timing **déterministe** (pas fixe + interpolation) pour que le rendu soit testable et reproductible. |
| O5 | Zéro `web-sys` / zéro `js-sys` côté Rust : le GPU et le DOM sont pilotés par une coquille TypeScript mince. |

### A.3 Non-objectifs (v1)

- **Multi-surfaces / split-screen** : l'API les admet (`SurfaceView`), l'implémentation n'en produit qu'une.
- **Quake / libretro** : `test3Dterminal/quake/` reste en C, non compilé par le workspace Rust.
- **Audio, réseau, sauvegarde de partie.**
- **Rendu 3D « lisse »** (hors glyphes) : le contrat visuel est le caractère de terminal.
- **Publication sur crates.io** : action manuelle ultérieure, hors périmètre v1. Le blocage « absence de licence » est levé : licence **MIT** actée le 2026-11-24, `LICENSE` créé à la racine (§I.3, §I.4).

### A.4 Contraintes dures (ACTÉ)

| Contrainte | Valeur | Conséquence |
| --- | --- | --- |
| Budget cellules | **1 000 000** maximum | Garde-fou mémoire et GPU ; le zoom est **clampé**, jamais une erreur (§E). |
| Cellule (`Cell`) | `glyph u32`, `fg`/`bg` `0x00RRGGBB`, `flags u16` | 16 o avec padding explicite (§C.5). |
| Échantillons | 2 × 4 par cellule | Parité avec le C, et **alignement exact** des glyphes sur la grille d'échantillons (§E.3). |
| Double buffer | **terminal uniquement** | Le web ré-upload tout ; seule la cible ANSI garde `previous`. |
| Dirty tiles | 8 × 4 cellules | Unité de diff/patch, indépendante de la surface. |
| Resize | politique `Preserve` \| `Clear` + `epoch++` | Pas de sémantique implicite, pas d'état global. |
| Cibles | features **interdites** pour choisir la cible | Le choix se fait par **crate** (binaire natif vs cdylib), jamais par `cfg(feature)`. |
| `astre-core` | aucune dépendance plateforme | `no_std` + `libm` uniquement. |
| `astre-web` | pas de `web-sys` | ABI C/pointeurs minimale, TS fait le reste. |
| WebGL2 | **1 draw call** instancié, atlas `R8`, instances de **8 o** | `fg` 24 bits + `slot` 8 bits, `bg` 24 bits + `flags` 8 bits. |
| Aspect de cellule | **strict** en v1 (`w/h = 0,5`) | Rend les points Braille **carrés** et le ladder de zoom entier (§E.2). |

---

## B. Découpage : 6 crates + une coquille TS

### B.1 Le triptyque

```
            ┌─────────────────────────────────────────────┐
            │  astre-core   math · raster · clip · resolve │   no_std + libm
            │               CellBuffer · HUD · metrics     │   aucune I/O
            └───────────────┬─────────────────────────────┘
                            │
            ┌───────────────▼─────────────┐
            │  astre-scene   caméra, orbite, ciel, navette, décor │
            └───────────────┬─────────────┘
                            │
            ┌───────────────▼──────────────────────────────┐
            │  astre-app   Sim à pas fixe · Renderer · HUD  │
            │              Presenter · EventSource · CLI    │
            └──────┬────────────────────────┬──────────────┘
                   │                        │
      ┌────────────▼───────────┐   ┌────────▼─────────────┐
      │ astre-term  crossterm  │   │ astre-web  cdylib    │
      │ + encodeur ANSI diff   │   │ wasm-bindgen, zéro   │
      │ (Presenter natif)      │   │ web-sys              │
      └────────────┬───────────┘   └────────┬─────────────┘
                   │                        │
      ┌────────────▼───────────┐   ┌────────▼─────────────┐
      │ astre-native  binaire  │   │  web/  TS : loader,  │
      │ CLI, --snapshot        │   │  presenter WebGL2,   │
      └────────────────────────┘   │  atlas, input        │
                                   └──────────────────────┘
```

Règle de dépendance, vérifiée par un test (§H.1) : `core ← scene ← app ← {term, web}`, `native → {app, term}`. **`astre-core` ne dépend de personne** ; `astre-web` ne dépend pas de `astre-term` ; `astre-term` ne dépend pas de `astre-scene`.

### B.2 Les six crates

| Crate | Type | Dépendances | Contenu |
| --- | --- | --- | --- |
| `astre-core` | `lib`, `no_std` (+`libm`) | aucune | `math` (`Vec3`, `clamp`, `mix`), `raster` (projection, edge, depth, clipping), `resolve` (cellules, tables ASCII/Braille/blocs), `cell` (`Cell`, `CellRect`, `CellBuffer`, dirty tiles), `hud` (`Anchor`, `text`, `fill`), `metrics`. |
| `astre-scene` | `lib`, `no_std` | `astre-core` | `Camera`/`orbit`, `Sky` (dégradé + planète + étoiles), `lighting` (`paint`, brume), primitives (`tri`, `quad`, `box`, `drum`, `annulus`, `loft`), `world` (terrain, piste, balises, hangar, navette), `Scene::compose`. |
| `astre-app` | `lib` | `core`, `scene` | `Sim` (pas fixe), `Renderer` (possède un `CellBuffer`), trait `Presenter`, trait `EventSource`, `Input` (touches décodées → actions), `Options` (parseur CLI), `App` (orchestration), `Metrics` (compteurs + fenêtre glissante). |
| `astre-term` | `lib` | `core`, `app`, `crossterm`, `libc`(unix) | Session terminal (raw mode, écran alterné, taille, touches), **encodeur ANSI différentiel** (parité octet à octet avec le C), palettes `Rgb`/`Ansi256`/`Plain`. Implémente `Presenter` et `EventSource` (mode pull). |
| `astre-native` | `bin` | `app`, `term` | `main`, CLI (`--snapshot`, `--size`, `--time`, `--mode`, `--palette`, `--fps`, `--tick-hz`, `--cell-aspect`, `--no-hud`, `--plain`), mode headless. |
| `astre-web` | `cdylib` | `app` | Exports `#[wasm_bindgen]` : cycle de vie, `frame(now_ms)`, pointeurs mémoire, `resize`, `zoom`, `push_key`, métriques, erreurs. **Aucun appel DOM.** |

### B.3 La coquille TypeScript (`web/`)

| Module | Rôle | Taille visée |
| --- | --- | --- |
| `loader.ts` | `import init from '…/astre_web.js'`, gestion d'erreur explicite, `WebGL2` detection, montage du canvas. | ~80 lignes |
| `presenter.ts` | Implémentation WebGL2 du `Presenter` : programme, VAO instancié, `bufferSubData`, 1 `drawArraysInstanced`, viewport/letterbox, gestion perte de contexte. | ~250 lignes |
| `atlas.ts` | Génération procédurale de l'atlas `R8`, rebuild débouncé, `fillText` (ASCII) + `fillRect` (Braille/blocs). | ~200 lignes |
| `input.ts` | Clavier → codes Astre, `preventDefault` ciblé, molette, `ResizeObserver`, visibilité d'onglet. | ~120 lignes |
| `main.ts` | `requestAnimationFrame`, câblage, overlay d'erreur DOM minimal. | ~60 lignes |

`web/` ne connaît **ni** la 3D, **ni** la résolution de cellules, **ni** le timing : il ne fait que (1) appeler `app.frame(now)`, (2) pousser des événements, (3) dessiner le buffer d'instances que Rust lui donne.

### B.4 Arborescence cible

```
/                          Cargo.toml (workspace virtuel) · Cargo.lock · rust-toolchain.toml
  crates/astre-core/
  crates/astre-scene/
  crates/astre-app/
  crates/astre-term/
  crates/astre-native/
  crates/astre-web/
  web/                     package.json · tsconfig.json · vite.config.ts · src/*
  docs/ARCHITECTURE.md
  test3Dterminal/          C de référence — INCHANGÉ, sert d'oracle (§H.3)
  .github/workflows/ci.yml
```

### B.5 Interdits structurels

1. **Pas de `cfg(feature = "wasm"/"native")` pour choisir une cible.** Les cibles diffèrent par le crate terminal (`astre-native` vs `astre-web`) ; une seule feature autorisée dans tout le workspace, `std-math` sur `astre-core` (bascule `libm` → `std` pour le binaire natif), et elle est **purement une optimisation**, jamais un choix de comportement.
2. **Pas de `web-sys`, `js-sys`, `wasm-bindgen-futures`** dans le workspace (test d'arbre de dépendances en CI, §F.9).
3. **Pas d'état global mutable** : aucune `static mut`, aucun `OnceLock` sur le chemin chaud. La table ASCII est calculée **à la compilation** (`const fn`, §C.5.6).
4. **Pas de `panic!` atteignable depuis une frontière** (CLI, wasm, présentation) : erreurs typées, `Result` (§F.8).
5. **Pas de `unwrap`/`expect` hors tests** ; `#![deny(clippy::unwrap_used, clippy::expect_used)]` au niveau workspace.

---

## C. Contrats publics

Les contrats ci-dessous sont la frontière stable du moteur. Ils sont volontairement **petits** : tout ce qui n'y est pas est privé et peut changer.

### C.1 Types fondamentaux (`astre-core::cell`, `astre-core::math`)

```rust
// 16 o, layout explicite, indépendant de l'ordre des champs par rustc.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub glyph: u32,     // 0..=0x10FFFF ; 0 = vide (jamais produit, voir C.5.4)
    pub fg: u32,        // 0x00RRGGBB
    pub bg: u32,        // 0x00RRGGBB
    pub flags: u16,     // cf. FLAG_* ci-dessous
    pub _pad: u16,      // padding explicite : size_of::<Cell>() == 16
}

pub const FLAG_UNDERLINE: u16 = 1 << 0;
pub const FLAG_BOLD:      u16 = 1 << 1;
pub const FLAG_REVERSE:   u16 = 1 << 2;
pub const FLAG_WIDE:      u16 = 1 << 3;  // futur : glyphes double-largeur (CJK)
// bits 4..=15 réservés ; le présentateur terminal les ignore, le web n'envoie
// que l'octet bas (cf. F.4).

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CellRect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelSize { pub w: u32, pub h: u32 }   // pixels DEVICE

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceView {
    pub origin_px: (i32, i32),   // coin haut-gauche de la zone dans le canvas
    pub cell_px:   PixelSize,    // taille d'une cellule en pixels device
    // v1 : une seule surface, égale à la grille entière.
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode { Ascii, Braille }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Palette { Rgb, Ansi256, Plain }
```

`Color` n'est **pas** un type distinct : `fg`/`bg` sont des `u32` packés `0x00RRGGBB`, comme l'exige l'instance GPU de 8 o. Les helpers de `astre-core::math` (`rgb`, `mix`, `trunc_u8`) portent les sémantiques exactes du C (§G.5).

### C.2 `Renderer` — le `T3Frame` de Rust

`Renderer` est l'agrégat « cible de rastérisation + `CellBuffer` », analogue exact de `T3Frame` en C, mais avec les responsabilités séparées.

```rust
pub trait Renderer {
    /// (Re)dimensionne la cible. No-op si (cols, rows) inchangés (comme le C,
    /// qui ne réalloue que sur changement). Échoue hors bornes/budget (C.5.3).
    fn resize(&mut self, cols: i32, rows: i32) -> Result<(), BufferError>;

    /// Démarre une image. `viewport` = sous-rectangle de cellules écrit
    /// (v1 : toute la grille, ce qui autorise les sous-vues futures).
    /// `sky` est évalué PAR ÉCHANTILLON (parité C, cf. A.1.2).
    fn begin<S: Sky + ?Sized>(&mut self, camera: Camera, viewport: CellRect, sky: &mut S);

    /// Triangle double-face ; profondeur testée en 1/z, clipping plan proche.
    fn triangle(&mut self, a: Vec3, b: Vec3, c: Vec3, color: u32);

    /// Convertit les échantillons en cellules (ASCII ou Braille).
    fn resolve(&mut self, mode: Mode);

    fn cells(&self) -> &CellBuffer;
    fn cells_mut(&mut self) -> &mut CellBuffer;   // HUD, overlays
}
```

Sémantique héritée du C, contractuelle :

- `begin` remet `depth` à zéro sur le viewport ; sans ciel, `samples` à noir.
- `begin` normalise la caméra (`near_z ≥ 0.001`, `fov_y ∈ [0.05, 3.05]`, `cell_aspect ∈ [0.1, 3.0]`) et précalcule `fx`, `fy`, `right`, `up`, `forward`, avec les **deux** replis du C : `up_world` parallèle à la visée → `right = (1,0,0)` ; `eye == target` → base orthonormée arbitraire sans NaN.
- L'ordre de soumission des triangles n'influence jamais l'image (le test de profondeur est strict sur `1/z`).
- `resolve` est idempotent : deux `resolve` consécutifs avec le même `mode` donnent le même `CellBuffer` (le C le vérifie implicitement via `tests/render.c:formats`).

### C.3 `Presenter` — la sortie

```rust
pub struct PresentConfig {
    pub palette: Palette,            // terminal
    pub dpr: f32,                    // web : devicePixelRatio
    pub requested_cell_px: Option<PixelSize>, // web : zoom demandé (None = auto)
    pub letterbox_bg: u32,           // web : couleur des zones mortes
    pub alt_screen: bool,            // terminal : écran alterné
}

pub trait Presenter {
    type Error: core::fmt::Debug;

    /// Idempotent ; appelé au démarrage et à chaque changement de config
    /// (palette, DPR, zoom). Ne fait jamais de rendu.
    fn configure(&mut self, cfg: PresentConfig) -> Result<(), Self::Error>;

    /// Surface allouable en pixels DEVICE, ou `None` si le présentateur n'a pas
    /// de notion de pixel (terminal). Sert au calcul du zoom (§E.2).
    fn available_px(&self) -> Option<PixelSize>;

    /// Présente l'image. `&mut [SurfaceView]` : le présentateur peut réordonner,
    /// clamper ou ignorer des surfaces (v1 : exactement une).
    /// Retourne `Result` : ajout arbitré ici, nécessaire pour « pas de panic
    /// traversant » (§F.8).
    fn present(&mut self, surfaces: &mut [SurfaceView], m: &FrameMetrics)
        -> Result<PresentOutcome, Self::Error>;
}

pub enum PresentOutcome {
    Presented { bytes: u32 },
    Skipped,                  // rien de modifié (dirty tiles vides)
    Lost,                     // contexte GPU perdu / terminal disparu
}
```

Deux implémentations, jamais trois :

| Cible | Implémentation | Mécanisme |
| --- | --- | --- |
| Terminal | `astre-term::AnsiPresenter` (Rust) | Diff sur `previous`, écriture ANSI. |
| Web | `web/src/presenter.ts` (TypeScript) | `bufferSubData` + 1 `drawArraysInstanced`. |

Conséquence architecturale assumée : **le `Presenter` web n'est pas une implémentation Rust du trait**. `astre-web` expose la donnée (pointeur, longueur, géométrie de grille) et le TS fait le GPU. C'est la seule façon de tenir « zéro `web-sys` ».

### C.4 `EventSource` — la source d'événements

```rust
pub enum InputEvent {
    Key { code: KeyCode, mods: Mods, down: bool },
    Resize { avail_px: Option<PixelSize>, dpr: f32 },
    Zoom { delta: i8 },          // -1 / +1 depuis le clavier ou la molette
    Focus { visible: bool },
    Quit,
}

pub trait EventSource {
    /// Remplit `out` sans bloquer. Retourne le nombre d'événements produits.
    fn poll(&mut self, out: &mut VecDeque<InputEvent>) -> usize;
}
```

`astre-term` est **pull** (l'app appelle `poll`, comme `t3_terminal_key` qui rend `-1` si la file est vide). Le web est **push** : le TS appelle `app.push_key(...)` / `app.push_resize(...)` au moment de l'événement DOM, ce qui alimente le `VecDeque<InputEvent>` **interne** à Rust ; `poll` sert alors uniquement à vider cette file dans le même chemin de code. Un seul code de consommation d'entrée pour les deux cibles, sans `if cfg!`.

### C.5 `CellBuffer`, resize, dirty tiles

C'est le contrat le plus sollicité : il doit être compréhensible en 30 secondes et impossible à mal utiliser.

#### C.5.1 Structure

```rust
pub struct CellBuffer {
    cells:  Vec<Cell>,     // len() == stride * rows
    cols:   i32,           // 1..=4096
    rows:   i32,           // 1..=4096
    stride: i32,           // >= cols, explicite, jamais implicite
    epoch:  u32,           // incrémenté à CHAQUE resize effectif
    dirty:  DirtyTiles,    // bitfield de tuiles 8x4
    tiles_x: u16,          // ceil(cols / 8)
    tiles_y: u16,          // ceil(rows / 4)
}
```

- **`stride` explicite** : `index(x, y) = (y as usize) * (stride as usize) + (x as usize)`. Aucune fonction ne suppose `stride == cols`. Raison : `SurfaceView` et les sous-vues futures peuvent écrire dans une bande verticale d'un buffer plus large, et un présentateur peut vouloir aligner les rangées (padding) sans toucher aux appelants.
- **Accès** : `get(x, y) -> Option<&Cell>` et `get_mut` **bornés** ; `row(y) -> &[Cell]` de longueur `cols`. Pas d'`Index` public : les dépassements doivent être visibles dans les types.
- **`epoch`** : monotone, jamais remis à zéro, jamais global. Un présentateur qui met en cache une géométrie (l'atlas web, la taille du terminal) compare l'`epoch` pour invalider sa copie ; c'est le remplacement propre des `static` du C.

#### C.5.2 Bornes et budget

```rust
pub const MAX_DIM:   i32 = 4096;         // parité avec t3_resize (C-REF)
pub const MAX_CELLS: u32 = 1_000_000;    // ACTÉ
pub const TILE_W:    i32 = 8;
pub const TILE_H:    i32 = 4;            // 32 cellules par tuile
```

| Condition | Résultat |
| --- | --- |
| `cols < 1` ou `rows < 1` | `Err(BufferError::Dimension)` (parité C : `t3_resize(0, 9) == false`) |
| `cols > 4096` ou `rows > 4096` | `Err(BufferError::Dimension)` |
| `cols * rows > 1_000_000` | `Err(BufferError::Budget { cells, max })` |
| `(cols, rows)` identiques | `Ok(())`, **aucune** réallocation, **aucun** `epoch++` (parité C) |

**Le budget n'est pas une erreur d'exécution** : dans le produit, l'appelant a déjà clampé (le zoom web borne la taille de cellule pour tenir sous 1 M, §E.2). Une erreur `Budget` remontée est donc un bug d'appelant, pas un cas utilisateur.

Mémoire consommée, à titre dimensionnant `[à mesurer]` :

| Objet | 4 800 cellules (120×40) | 17 280 cellules (240×72) | 1 000 000 cellules (plafond) |
| --- | --- | --- | --- |
| `CellBuffer` (16 o/cellule) | 75 Kio | 264 Kio | 15,3 Mio |
| `previous` terminal (double buffer) | 75 Kio | 264 Kio | 15,3 Mio |
| Échantillons : `samples` 4 o + `depth` 4 o, 8/cellule | 38 400 × 8 = 300 Kio | 138 240 × 8 = 1 080 Kio | 8 000 000 × 8 = 61 Mio |
| **Total moteur** | **≈ 0,45 Mio** | **≈ 1,6 Mio** | **≈ 92 Mio** |

Conclusion documentée : le plafond de 1 M cellules est un garde-fou anti-`malloc`-fou (le C, lui, laissait faire jusqu'à ≈ 1 Gio), **pas** une cible d'usage. Les cibles réalistes sont 5 k à 20 k cellules.

#### C.5.3 Resize : `Preserve` vs `Clear`

```rust
pub enum ResizePolicy { Preserve, Clear }

/// Politique explicite, jamais déduite. `epoch += 1` dans TOUS les cas où
/// (cols, rows) ou la politique change effectivement la surface.
pub fn resize(&mut self, cols: i32, rows: i32, policy: ResizePolicy) -> Result<u32, BufferError>;
```

| Politique | Effet | Usage |
| --- | --- | --- |
| `Clear` | Tout le buffer devient `Cell { glyph: ' ', fg: bg: 0, flags: 0 }`. | Démarrage, changement de scène, snapshot. |
| `Preserve` | Recopie la zone commune `min(cols, old_cols) × min(rows, old_rows)` ligne par ligne (le `stride` peut changer), le reste devient vide. | Redimensionnement de fenêtre en cours de jeu : évite le flash noir. |

Le C **n'avait pas** de `Preserve` (il `malloc`-ait des buffers neufs et laissait `cells` non initialisé, l'image étant réécrite à chaque `resolve`). En Rust, l'initialisation est obligatoire (`Cell` n'a pas de « non initialisé »), donc `Clear` = `fill`, `Preserve` = `copy_within` + `fill` du reste.

Dans les deux cas : `epoch += 1`, **toutes** les tuiles marquées sales, et la capacité allouée est **réutilisée** si suffisante (`Vec::clear` + `resize_with`) pour éviter la fuite par fragmentation sur redimensionnements répétés (§H.5).

#### C.5.4 Invariants (assertions en `debug_assert!` + tests)

| # | Invariant |
| --- | --- |
| I1 | `1 <= cols <= 4096`, `1 <= rows <= 4096`, `cols * rows <= 1_000_000`. |
| I2 | `stride >= cols` **et** `cells.len() == stride * rows`. |
| I3 | Toute case hors `[0, cols) × [0, rows)` mais dans `[0, stride) × [0, rows)` est du padding, jamais lu par un présentateur. |
| I4 | `glyph` est toujours un scalaire Unicode valide et **jamais `0`** : les producteurs (resolve, HUD) n'émettent que `' '`, ASCII imprimable 32..127, Braille `U+2800..=U+28FF`, blocs `U+2580..=U+259F`. |
| I5 | `fg`/`bg` ont leurs 8 bits hauts à zéro : `color & 0xFF00_0000 == 0`. |
| I6 | `epoch` est strictement croissant sur la vie du buffer et n'est jamais réutilisé. |
| I7 | `resolve` ne modifie ni `cols`, `rows`, `stride`, ni `epoch` ; il marque les tuiles correspondant aux cellules effectivement modifiées. |
| I8 | Aucune opération ne lit de cellule « au-delà de `cols` » : le padding n'influence jamais l'image. |

#### C.5.5 Dirty tiles 8 × 4

- Tuile = 8 colonnes × 4 lignes = 32 cellules ; `tiles_x = ceil(cols/8)`, `tiles_y = ceil(rows/4)`.
- Stockage : un `Vec<u64>` de `ceil(tiles_x*tiles_y / 64)` mots, plus un compteur `dirty_count`.
- Toute écriture via `cells_mut()` est **paresseuse** : le `CellBuffer` expose `mark_dirty(rect: CellRect)` ; le HUD (`hud::text`, `hud::fill`) et `resolve` marquent les tuiles touchées. Écrire directement dans `iter_mut_raw()` (réservé aux tests) marque **tout** le buffer.
- Utilisation par présentateur :
  - **terminal** : itère les tuiles sales, fusionne les cellules sales contiguës d'une ligne en une seule séquence de positionnement curseur (reproduction de l'optimisation `contiguous` du C), et saute les tuiles propres ;
  - **web** : si `dirty_count == 0` → `PresentOutcome::Skipped` (aucun appel GL) ; sinon ré-upload complet (le patch partiel de sous-plage est une optimisation documentée mais **non retenue en v1**, §I.4).
- À 240 × 72 : 30 × 18 = **540 tuiles**. Le compteur `dirty_tiles` est exposé dans `FrameMetrics` pour l'overlay debug (§D.6).

#### C.5.6 Tables de résolution : `const`, pas `static`

Le C construisait `static Shape lookup[256]` à la première utilisation (état global non thread-safe, et bug latent si `lookup[0].c` devenait `0` — il ne le devient pas par chance, cf. A.1.7). En Rust :

```rust
const ASCII_LOOKUP: [AsciiShape; 256] = build_ascii_lookup(); // const fn, évalué au compile
// AsciiShape { ch: u8, inverted: bool }  — deux champs distincts, plus de
// double sens du champ `mask` du C.
```

Bénéfice : zéro état, zéro branche au premier appel, table testable unitairement (les 256 entrées sont un golden trivial, §H.2).

#### C.5.7 Récapitulatif des garanties du `CellBuffer`

1. `size_of::<Cell>() == 16` et le layout est vérifié par un test.
2. Aucune lecture/écriture hors bornes possible depuis l'API publique.
3. `resize` est le **seul** chemin qui change `cols`/`rows`/`stride`, et il incrémente toujours `epoch`.
4. Aucune donnée globale : deux `CellBuffer` sont totalement indépendants (test en parallèle, §H.1).
5. Aucune allocation sur le chemin chaud (`resolve` + HUD réutilisent les buffers existants) : critère vérifié par un test qui compte les allocations (`#[global_allocator]` compteur, §H.5).

---

## D. Boucle et timing

### D.1 Modèle : simulation à pas fixe, rendu interpolé

Le C avançait la simulation d'un `dt` variable borné à 0,12 s (`demo.c:live`), ce qui rend toute image non reproductible hors `--snapshot --time`. La réécriture impose un **pas fixe** (ACTÉ), avec interpolation au rendu.

```rust
pub struct Sim {
    tick_hz: u32,          // 10..=240, défaut 60, configurable (--tick-hz)
    dt: f32,               // 1.0 / tick_hz
    accumulator: f32,
    prev: SceneState,      // état au tick N-1 (pour l'interpolation)
    curr: SceneState,      // état au tick N
    ticks: u64,
    dropped: u64,
}

const MAX_CATCHUP: u32 = 4;      // pas max rattrapés par frame
const MAX_FRAME_DT: f32 = 0.25;  // clamp anti-spirale (onglet en arrière-plan)

impl Sim {
    fn advance(&mut self, frame_dt: f32) -> (u32 /*steps*/, f32 /*alpha*/) {
        let dt = frame_dt.min(MAX_FRAME_DT);
        self.accumulator += dt;
        let mut steps = 0;
        while self.accumulator >= self.dt && steps < MAX_CATCHUP {
            self.prev = self.curr;
            self.curr.tick(self.dt);          // orbite, horloge, bob de la navette
            self.accumulator -= self.dt;
            steps += 1;
            self.ticks += 1;
        }
        if steps == MAX_CATCHUP && self.accumulator >= self.dt {
            // On abandonne le retard : mieux vaut une image en retard qu'une
            // spirale de la mort. Compteur exposé (sim_dropped).
            self.dropped += 1;
            self.accumulator = 0.0;
        }
        (steps, self.accumulator / self.dt)
    }
}
```

Le rendu consomme `SceneState::lerp(prev, curr, alpha)` — interpolation linéaire de l'azimut/élévation/rayon de la caméra et du `bob` de la navette. Points délicats, contractuels :

- **L'azimut doit être interpolé en angle court** (`±π`), sinon un tour complet produit un balayage inverse pendant une frame.
- `tick` ne touche **jamais** au raster ni au `CellBuffer` ; `render` ne modifie **jamais** l'état de simulation. Test : une frame rendue avec `alpha` constant donne une image identique quelle que soit la fréquence d'appel (§H.4).
- Un `tick_hz` différent de 60 ne change **pas** la trajectoire, seulement sa granularité : `Sim` est déterministe pour un `(tick_hz, nombre de ticks)` donné. C'est ce qui rend les golden tests possibles sans horloge murale.

### D.2 Cadencement : pas de verrou vsync, pas de sommeil

ARBITRÉ ICI : on **ne dort pas** pour tenir une cadence, contrairement à `demo.c:wait_frame`. Le rythme est donné par l'ordonnanceur :

| Cible | Cadenceur | Conséquence |
| --- | --- | --- |
| Web | `requestAnimationFrame` | 60/120 Hz selon l'écran ; `now` = timestamp rAF (ms, même base pour toutes les frames). |
| Natif | boucle `poll()` terminal + `sleep(jusqu'au prochain tick)` | Le terminal n'a pas de vsync ; on dort jusqu'au prochain **tick** (1/60 s), pas jusqu'à une « frame » cible. |

Pourquoi pas de verrou vsync : le coût de présentation web est dominé par le raster + resolve Rust ; verrouiller ajouterait une latence de 1 à 2 frames pour un gain nul (un seul draw call ne peut pas « déchirer »). Côté natif, `--fps` du C devient **un plafond de rafraîchissement optionnel** (`--fps 0` = illimité) et non plus l'horloge de la simulation.

### D.3 Thread principal (rAF) vs Worker + OffscreenCanvas

| Critère | Main thread + rAF | Worker + OffscreenCanvas |
| --- | --- | --- |
| Coût du présentateur | 1 `bufferSubData` + 1 draw | identique |
| Où tourne le wasm | déjà le thread principal | doit être **chargé dans le worker** (le wasm est monothread par instance) |
| Entrées clavier | directes | à proxifier (`postMessage` par événement) |
| `ResizeObserver` | direct | à proxifier |
| Latence d'entrée | 1 frame | 1 frame + 1 aller-retour |
| Complexité | faible | + transfert de contrôle du canvas, + protocole de messages, + repli si `transferControlToOffscreen` absent |
| Gain attendu | — | **faible** `[à mesurer]` : le pipeline est 1 draw call, le coût est dans le wasm qui tourne de toute façon sur un thread |

**ARBITRÉ ICI : thread principal + rAF en v1.** Le worker n'apporterait rien tant que le coût Rust reste sous le budget d'une frame (~8 ms `[à mesurer]` à 240 × 72) ; il ne deviendrait intéressant que si l'on visait > 30 000 cellules ou plusieurs canvas. Le découpage est prêt pour la bascule : `astre-web` n'a aucune dépendance DOM, et `web/src/main.ts` isole la boucle dans une fonction `runFrame(now)`; déplacer `main.ts`+`presenter.ts` dans un worker ne demanderait pas de toucher à Rust.

### D.4 Une frame, dans l'ordre

```
frame(now):
  1. frame_dt = now - last_now                     (clamp D.1)
  2. events = event_source.poll(&mut queue)        (push web / pull terminal)
  3. dispatch(events) → camera, mode, zoom, quit
  4. (steps, alpha) = sim.advance(frame_dt)        [t_sim]
  5. state = SceneState::lerp(prev, curr, alpha)
  6. renderer.begin(state.camera, full_rect, &mut sky)      [t_begin]
  7. scene.compose(&mut renderer, &state)                   [t_raster]
  8. renderer.resolve(mode)                                 [t_resolve]
  9. hud.draw(renderer.cells_mut(), &overlay_state)         [t_hud]
 10. presenter.present(&mut surfaces, &metrics)             [t_present]
 11. metrics.observe(...)                                    (fenêtre 0,5 s)
```

Règle : `present` est **le seul** point de sortie. Rien entre 6 et 9 ne connaît le backend.

### D.5 Compteurs : « produit » vs « présenté »

C'est la distinction que le C ne faisait pas (`fps` y était « images écrites sur le terminal »).

| Compteur | Définition | Utilité |
| --- | --- | --- |
| `sim_hz` | ticks de simulation par seconde | santé du pas fixe ; doit valoir `tick_hz`. |
| `sim_dropped` | rattrapages abandonnés (D.1) | alerte de surcharge de la simulation. |
| `built_fps` | frames **produites** par `frame()` (raster + resolve + HUD) | capacité du moteur. |
| `presented_fps` | frames **réellement présentées** (`Presented`, hors `Skipped`) | ce que l'œil voit. |
| `skipped` | frames `Skipped` d'affilée max | détection d'une boucle qui ne change rien (état figé). |
| `alpha` | fraction de tick en attente | interpolation effective. |

Sur un écran 120 Hz avec `tick_hz = 60`, `built_fps = presented_fps = 120` et `sim_hz = 60` : c'est **normal**, et c'est l'intérêt de l'interpolation. Sur un écran 60 Hz, `built == presented == sim == 60`. L'overlay doit afficher les deux, jamais un seul « FPS » ambigu.

### D.6 Temps par phase et overlay debug

```rust
pub struct FrameMetrics {
    pub tick_hz: u32,
    pub sim_ticks: u32,
    pub sim_dropped: u32,
    pub sim_us: u32,
    pub begin_us: u32,
    pub raster_us: u32,      // t_raster − t_begin
    pub resolve_us: u32,
    pub hud_us: u32,
    pub upload_us: u32,      // web : bufferSubData ; terminal : encodage ANSI
    pub draw_us: u32,        // web : drawArraysInstanced ; terminal : write()
    pub frame_us: u32,
    pub cells: u32,
    pub dirty_tiles: u32,
    pub zoom_level: u8,
    pub epoch: u32,
    pub built_fps: f32,
    pub presented_fps: f32,
}
```

- Mesure : `Instant::now()` natif ; `performance.now()` côté web, **collecté par Rust** via le `now_ms` passé à `frame()` pour l'horloge de frame, et par instrumentation interne pour les phases. Les phases GPU (`draw_us`) sont mesurées en temps CPU d'appel GL, pas en temps GPU (pas de `EXT_disjoint_timer_query` en v1).
- Les compteurs sont lissés sur une fenêtre de 0,5 s (parité avec `demo.c:live`) et figés quand la scène est en pause (le C le faisait déjà).
- **Overlay debug** : touche `P` (parité C) puis, en supplément, `F3` pour les compteurs bruts. Rendu **dans les cellules** via `astre-core::hud`, jamais en DOM/GL texte : l'overlay est donc identique sur les deux cibles et testable par golden. Contenu à largeur ≥ 46 : `SIM 60.0  BUILD 61.2  PRES 61.2`, `RAST 4.10  RES 0.83  UP 0.11  DRAW 0.09 ms`, `CELLS 17280  TILES 540/540  ZOOM 8  EPOCH 3`.
- Coût de l'overlay négligeable par construction (≤ 3 lignes, hors chemin de resolve).

---

## E. Zoom (taille de cellule en pixels)

### E.1 Ce que le zoom change, et ce qu'il ne change pas

Le zoom web agit sur `cell_px` (taille d'une cellule en **pixels device**), donc sur `cols × rows`, donc sur la quantité de travail. Il ne change **pas** `cell_aspect` : l'aspect reste strict 0,5 en v1 (ACTÉ). Corollaire mathématique exploité : une cellule de largeur `w` et hauteur `2w` découpe ses 2 × 4 sous-échantillons en blocs de `w/2 × w/2` pixels **carrés** — c'est ce qui rend les points Braille non déformés et l'atlas exactement aligné.

### E.2 Paliers discrets en pixels entiers (ARBITRÉ ICI, contre le continu)

```
cell_px_w ∈ { 4, 6, 8, 10, 12, 16, 20, 24, 32 }         (9 paliers, niveau 0..=8)
cell_px_h  = 2 × cell_px_w                                (aspect strict 0,5)
dot_px     = cell_px_w / 2  ∈ { 2, 3, 4, 5, 6, 8, 10, 12, 16 }
```

Justification du **discret entier** contre le continu :

1. **Alignement échantillon ↔ glyphe.** L'atlas dessine chaque point Braille / quadrant par `fillRect` sur la grille 2 × 4 de la cellule. Un alignement exact n'existe que si chaque sous-case tombe sur un rectangle entier de texels **et** de pixels. Avec un facteur continu (par exemple `cell_px_w = 7,43`), `w/2 = 3,715` : les points ne tombent plus sur des frontières de pixels, `NEAREST` crée des points inégaux (3 px / 4 px) et le Braille devient visuellement bruité. C'est exactement le défaut que l'atlas procédural cherche à éviter.
2. **Rastérisation stable.** Les bords de l'atlas sont des multiples de `w/2` ; sans cela, les glyphes sont décalés d'une fraction de pixel et l'image « tremble » d'un palier à l'autre.
3. **Coût borné et connu.** 9 paliers ⇒ 9 tailles de grille possibles par viewport, 9 tailles d'atlas, 9 jeux de golden testables. Le continu produirait une infinité de configurations non testables.
4. **DPR.** Un palier entier en pixels **device** reste entier à DPR 1, 1,25, 1,5, 2, 3. Un palier exprimé en CSS px deviendrait fractionnaire dès DPR 1,5 (ex. 12,5 px CSS = 18,75 px device) : c'est la raison pour laquelle la borne et les paliers sont en device, et que le letterbox absorbe le reste (§E.4).
5. **Budget.** Le palier le plus fin (`w = 4`) maximise le nombre de cellules ; le clamp du budget s'y exprime simplement (§E.5).

Paliers et grilles, à titre indicatif (viewport 16:9) :

| Niveau | `cell_px` | Grille @ 1920 × 1080 device | Cellules | `dot_px` |
| --- | --- | --- | --- | --- |
| 0 | 4 × 8 | 480 × 135 | 64 800 | 2 |
| 1 | 6 × 12 | 320 × 90 | 28 800 | 3 |
| 2 | 8 × 16 | 240 × 67 | 16 080 | 4 |
| 3 | 10 × 20 | 192 × 54 | 10 368 | 5 |
| 4 | 12 × 24 | 160 × 45 | 7 200 | 6 |
| 5 | 16 × 32 | 120 × 33 | 3 960 | 8 |
| 6 | 20 × 40 | 96 × 27 | 2 592 | 10 |
| 7 | 24 × 48 | 80 × 22 | 1 760 | 12 |
| 8 | 32 × 64 | 60 × 16 | 960 | 16 |

*(1920/4 = 480 ; 1080/8 = 135. Les valeurs sont des ordres de grandeur de dimensionnement, pas des mesures.)*

### E.3 L'atlas suit le palier

L'atlas est **généré pour le palier courant**, pas une fois pour toutes : au niveau `w`, chaque slot fait `w × 2w` texels et la texture `R8` fait `(32·w) × (32·w)` (ACTÉ le 2026-11-24).

| Niveau | Texels / slot | Slot | Texture (`R8`) | Mémoire |
| --- | --- | --- | --- | --- |
| 2 (`w = 8`) | 8 × 16 | point = 4 × 4 | 256 × 256 | 64 Kio |
| 5 (`w = 16`) | 16 × 32 | point = 8 × 8 | 512 × 512 | 256 Kio |
| 8 (`w = 32`) | 32 × 64 | point = 16 × 16 | 1024 × 1024 | 1 Mio |

Grille d'atlas : **32 slots en largeur × 16 en hauteur = 512 slots** (`slot = 0..511`), d'où `slot_x = slot & 31`, `slot_y = slot >> 5`. À la taille nominale (`w = 16`), la texture fait donc 512 × 512.

**Interprétation actée le 2026-11-24** : « 512 slots = grille de 32 colonnes × 16 lignes, chaque slot de `w × 2w` texels » (aspect strict 0,5). La formulation ci-dessus est **exactement** cette décision : aucune divergence, aucune constante à corriger, l'ambiguïté « slot de 32 × 16 texels » est définitivement écartée. Conséquence pratique : `slot_x = slot & 31`, `slot_y = slot >> 5` et `uv = (vec2(slot_x, slot_y) + corner) * (1.0/vec2(32.0, 16.0))` sont figés (§F.4) ; `atlas.ts` garde ces constantes centralisées au cas où.

Occupation des 512 slots :

| Plage | Contenu | Slots |
| --- | --- | --- |
| 0..=95 | ASCII imprimable `U+0020..=U+007E` via `fillText` | 96 |
| 96..=351 | Braille `U+2800..=U+28FF` via `fillRect` | 256 |
| 352..=383 | Blocs `U+2580..=U+259F` via `fillRect` | 32 |
| 384..=511 | réservé (curseur, glyphes de HUD futurs, cadratins) | 128 |

Point important, source de bugs si oublié : les 32 codepoints de `U+2580..=U+259F` incluent des blocs fractionnaires au huitième (`▁▂▃▅▆▇`, `▉▊▋▍▎▏`) qui **ne tombent pas** sur la grille 2 × 4. La v1 ne les produit jamais (`resolve_cell` n'en émet que 16, tous alignés : `▘▝▀▖▌▞▛▗▚▐▜▄▙▟█` + espace) ; ils sont tout de même dessinés dans l'atlas, avec un arrondi au demi-point, et marqués dans le code comme « non utilisés par le résolveur v1 ». Le test §H.2 vérifie que le résolveur n'émet **que** des slots alignés.

Rebuild : **débouncé à ~60 ms**, déclenché par changement de palier de zoom, de DPR, ou de disponibilité de police. Un rebuild = 384 `fillRect`/`fillText` + 1 `texImage2D` de 64 Kio à 1 Mio `[à mesurer : 2–20 ms]`. Pendant un rebuild, l'ancienne texture reste liée (double atlas ou `texSubImage2D` par slot) : jamais de frame avec un atlas incomplet.

### E.4 Bornes en pixels device, letterbox, DPR

```rust
pub struct ZoomConfig {
    pub levels: &'static [u32],     // E.2 — pixels device (largeur de cellule)
    pub aspect: f32,                // 0.5 en v1
    pub min_cell_px: u32,           // 4
    pub max_cell_px: u32,           // 32
    pub budget_cells: u32,          // 1_000_000
}
```

Algorithme de calcul de la grille (source unique de vérité, dans `astre-core`, testé unitairement) :

```
fn fit(avail_px: PixelSize, dpr: f32, requested_level: Option<u8>, cfg) -> Grid:
    // 1. palier effectif
    level = requested_level.unwrap_or(auto_level(avail_px))
    level = clamp(level, 0, levels.len()-1)

    // 2. descente de palier si le budget est dépassé (jamais d'erreur, §E.5)
    while cols(avail, level) * rows(avail, level) > budget_cells and level > 0:
        level -= 1

    // 3. géométrie
    w = levels[level]; h = round(w * 1.0 / cfg.aspect)
    cols = max(1, avail_px.w / w)
    rows = max(1, avail_px.h / h)

    // 4. letterbox : la grille occupe cols*w × rows*h, centrée
    used  = (cols * w, rows * h)
    origin = ((avail_px.w - used.0) / 2, (avail_px.h - used.1) / 2)
    return Grid { level, cell_px: (w, h), cols, rows, origin, leftover: avail_px - used }
```

- `auto_level` choisit le palier dont la hauteur de cellule approche 16–20 px device (confort de lecture d'un monospace) : `auto = argmin |h - 20|`, borné par le budget.
- **Division entière** (`avail / cell`) ⇒ reste non nul en général ⇒ **zone morte (letterbox)** : bandes de `≤ w-1` px horizontalement et `≤ h-1` verticalement, remplies par `letterbox_bg` (`gl.clear` de la zone hors viewport + `scissor` sur la grille). Jamais de mise à l'échelle fractionnaire du glyphe : c'est ce qui garantit `NEAREST` sans bavure.
- **DPR = pixels device** : `avail_px = (css_w × dpr, css_h × dpr)` arrondi au plus proche, `dpr` lu sur `window.devicePixelRatio` et re-lu à chaque `ResizeObserver` (le DPR change au zoom navigateur ou au déplacement d'écran). Le canvas est dimensionné en device (`canvas.width = avail_px.w`) et stylé en CSS (`style.width = css_w px`) : aucune mise à l'échelle implicite du navigateur.
- Effet DPR concret : à DPR 2, un palier `w = 8` donne des cellules de 4 × 8 **CSS** px (petites mais nettes) ; l'auto-palier compense en descendant d'un ou deux crans.

### E.5 Clamp, jamais erreur

Le budget de 1 M cellules est une **borne du produit**, pas une erreur : si le palier demandé dépasse le budget, on **remonte d'un palier** (`while … level > 0`) jusqu'à rentrer dedans. Si même le palier 0 dépasse (canvas > ~4 K device de large en cellules de 4 px, cas théorique), on garde le palier 0 et on **réduit `cols`/`rows`** au budget, en centrant : l'image est alors letterboxée davantage, sans erreur, sans `panic`, sans allocation refusée. Un `Err` ne peut venir que d'un appelant qui viole `MAX_DIM`, c'est-à-dire d'un bug (§C.5.2).

### E.6 Raccourcis et entrées

| Entrée | Effet | Note |
| --- | --- | --- |
| `+` / `=` | palier +1 (cellules plus grosses) | parité C, mais sur le zoom et non la caméra (§E.7) |
| `-` | palier −1 | idem |
| `0` | retour au palier auto | nouveau |
| Molette | **dolly caméra** (rayon) | préserve la sémantique `+`/`-` du C |
| `Ctrl`/`⌘` + molette | zoom cellule (palier) | geste navigateur attendu |
| Pincement (trackpad) | équivalent `Ctrl` + molette | fourni par le navigateur sous forme de `wheel` avec `ctrlKey` |
| `R` | reset caméra (parité C) | n'affecte pas le zoom |

`preventDefault` obligatoire sur `+`, `-`, `0`, molette, flèches, `Espace`, `Tab`, `F3` : sinon le navigateur zoome la page (Ctrl+molette) ou fait défiler. Le zoom de **page** du navigateur reste possible (on ne bloque que la molette sur le canvas).

### E.7 Changement assumé par rapport au C

Dans le C, `+`/`-` changeaient le **rayon de la caméra** (`demo.c:keypress`, `radius ∈ [3.4, 32]`). En web, deux notions de « zoom » coexistaient ; on tranche : `+`/`-` = zoom **d'affichage** (taille de cellule, nouvelle notion propre au web), dolly caméra = molette (et, sur clavier, `Z`/`X` en secours accessibilité). Le natif conserve `+`/`-` = rayon **et** ajoute `Ctrl +`/`Ctrl -` = zoom de palette de caractères si un jour un terminal exposait sa taille de cellule. Documenté pour éviter la surprise aux utilisateurs du C.

### E.8 Pas de `localStorage` obligatoire

Le zoom n'est **pas** persisté en v1 (ARBITRÉ ICI). Justifications :

1. **Rien à persister qui ne soit dérivable** : le palier auto se recalcule du viewport et du DPR ; un palier forcé est une préférence de session, pas un état de jeu.
2. **Déterminisme des tests** : un état persistant rend les captures et les goldens dépendants du profil du navigateur ; il faudrait une clé de version, une validation, un fallback — trois sources de bugs pour un confort marginal.
3. **Vie privée / contextes contraints** : `localStorage` lève dans certains iframes (`file://`, sandbox sans `allow-same-origin`) et en navigation privée de plusieurs navigateurs ; le code devrait gérer l'exception. Un réglage qui marche « une fois sur deux » est pire que pas de réglage.
4. **Coût d'ajout ultérieur nul** : `PresentConfig.requested_cell_px` est déjà un `Option` fourni par la coquille ; persister reviendrait à lire une clé dans `input.ts` avant le premier `configure`. Aucun changement de Rust. À réévaluer après la v1, en `try/catch` et avec une clé versionnée.

---

## F. Pont WASM / JavaScript

### F.1 Empaquetage

- `astre-web` est une `cdylib` (`crate-type = ["cdylib"]`) avec `wasm-bindgen`.
- Build : `wasm-pack build crates/astre-web --target web --release` → `pkg/astre_web.js` + `astre_web_bg.wasm` + `astre_web.d.ts`. `--target web` (ES module + `init()`) est le seul mode retenu : `bundler` masque le chargement asynchrone, `nodejs` ne sert qu'aux tests (§H.7).
- `wasm-bindgen` est la **seule** dépendance d'`astre-web`. Versions `wasm-bindgen` et `wasm-pack` épinglées (`Cargo.lock` + `package.json` + CI), parce qu'un désaccord de version produit des erreurs d'import à l'exécution difficiles à lire.
- Profil release : `opt-level = "s"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = "debuginfo"`. Taille attendue **`[à mesurer]` : 150–400 Kio de `.wasm` brut, 60–150 Kio en brotli.** Repère : `astre-core` + `astre-scene` + `astre-app` font ~2 000 lignes de Rust sans dépendance lourde ; la plus grosse part variable est le formateur de flottants et le `core::fmt` des erreurs.

### F.2 Surface exposée : 1 appel par frame maximum

```rust
#[wasm_bindgen]
pub struct App { /* Sim, Renderer, buffers, queue, grid, metrics */ }

#[wasm_bindgen]
impl App {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<App, JsError>;

    // ─── 1 seul appel par frame ───────────────────────────────────────────
    pub fn frame(&mut self, now_ms: f64) -> Result<u32, JsError>;  // bitflags

    // ─── événements (jamais dans la boucle rAF) ───────────────────────────
    pub fn push_key(&mut self, code: u32, mods: u8, down: bool) -> Result<(), JsError>;
    pub fn push_zoom(&mut self, delta: i8) -> Result<(), JsError>;
    pub fn push_resize(&mut self, css_w: f64, css_h: f64, dpr: f32) -> Result<(), JsError>;
    pub fn push_focus(&mut self, visible: bool);

    // ─── géométrie pour le présentateur TS ────────────────────────────────
    pub fn cells_ptr(&self) -> u32;
    pub fn cells_len_bytes(&self) -> u32;
    pub fn grid_cols(&self) -> u32;
    pub fn grid_rows(&self) -> u32;
    pub fn cell_px_w(&self) -> u32;
    pub fn cell_px_h(&self) -> u32;
    pub fn origin_px_x(&self) -> i32;
    pub fn origin_px_y(&self) -> i32;
    pub fn avail_px_w(&self) -> u32;
    pub fn avail_px_h(&self) -> u32;
    pub fn epoch(&self) -> u32;
    pub fn metrics_ptr(&self) -> u32;      // FrameMetrics #[repr(C)]
    pub fn last_error_ptr(&self) -> u32;   // C-string UTF-8, NUL-terminal
}
```

`frame` renvoie des bits : `1 = image produite`, `2 = image présentable (dirty non vide)`, `4 = simulation a abandonné du retard`, `8 = demande de quitter`. Le TS les lit pour décider d'appeler le présentateur.

**Règle contractuelle : un seul appel wasm par frame.** Tous les autres exports sont événementiels. Un test (§H.7) compte les appels à `frame` par seconde et vérifie qu'aucun autre export n'est appelé dans la boucle rAF.

### F.3 Transfert de la mémoire : vue sur `WebAssembly.Memory` (ARBITRÉ ICI)

Deux options étaient ouvertes : (a) une vue JS sur `WebAssembly.Memory`, (b) un `bufferSubData` alimenté par un tableau copié côté Rust.

Retenu : **(a) vue sur `WebAssembly.Memory`, `bufferSubData` avec `byteOffset`**, zéro copie.

```ts
// presenter.ts — à chaque frame présentable
const mem = wasmModule.memory;                       // handle gardé après init()
const ptr = app.cells_ptr();
const len = app.cells_len_bytes();
// La vue DOIT être recréée si la mémoire a grandi (ArrayBuffer détaché).
let view = new Uint8Array(mem.buffer, ptr, len);
gl.bindBuffer(gl.ARRAY_BUFFER, instanceVbo);
gl.bufferSubData(gl.ARRAY_BUFFER, 0, view);
gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, app.grid_cols() * app.grid_rows());
```

Justification chiffrée `[à mesurer]` : à 240 × 72, 17 280 cellules × 8 o = **138 240 o par frame** (≈ 135 Kio, ≈ 138 Ko décimaux). Une copie JS supplémentaire coûterait une allocation + un `memcpy` de 135 Kio, soit de l'ordre de 10–30 µs : faible en absolu, mais (b) ajoute aussi un `Vec<u8>` sérialisé par `wasm-bindgen` (copie *supplémentaire* et allocation par frame). (a) supprime les deux et laisse Rust écrire directement dans le buffer que le GPU lit.

**Pièges documentés, avec parades :**

1. **`ArrayBuffer` détaché après croissance mémoire.** Toute croissance de `WebAssembly.Memory` invalide les vues existantes. Parade : ne jamais mettre la vue en cache au-delà d'une frame ; la recréer systématiquement (`new Uint8Array(...)` par frame est gratuit), et re-lire `mem.buffer` si `buffer.byteLength` a changé.
2. **Pointeur invalide après `resize`.** `cells_ptr()` change quand le `Vec<Cell>` réalloue. Le TS **ne doit pas** mémoriser le pointeur : il l'appelle chaque frame (coût : un appel wasm trivial en plus — on l'inclut dans les getters déjà appelés une fois par frame, ce qui reste « 1 frame call + N getters » ; pour tenir la règle stricte, `frame()` **écrit** la géométrie et les pointeurs dans un bloc `PresentInfo` `#[repr(C)]` en mémoire, que le TS lit d'un coup via `metrics_ptr`/`info_ptr` : **c'est la solution retenue**, une seule lecture structurée par frame).
3. **`memory` non exposé.** Il faut garder l'objet rendu par `init()` (`const m = await init(); m.memory`). Ne pas compter sur `wasm_web_bg.wasm` importé directement.

Conséquence : `FrameMetrics` et `PresentInfo` sont `#[repr(C)]`, à plat (pas d'`enum` avec données, pas de `String`), pour être lus par `DataView` sans sérialisation.

### F.4 Instance GPU : 8 octets par cellule (ACTÉ)

```rust
#[repr(C)]
pub struct InstanceWord(pub u32);
// word0 = fg(24 bits bas) | slot(8 bits hauts)     => fg 0x00RRGGBB | slot << 24
// word1 = bg(24 bits bas) | flags(8 bits hauts)    => bg 0x00RRGGBB | (flags & 0xFF) << 24
```

- `Cell` (16 o) → instance (8 o) : projection **lossy assumée** des `flags` (`u16` → 8 bits ; le présentateur terminal, lui, garde les 16 bits). Les 4 bits définis (§C.1) tiennent dans un octet, le reste est réservé.
- Taille à 240 × 72 : 138 240 o `[à mesurer]` ; au plafond de 1 M cellules : 8 Mio, ré-uploadés intégralement en v1 (choix §I.4).
- Le shader dérive les UV du slot : `slot_x = slot & 31`, `slot_y = slot >> 5`, `uv = (vec2(slot_x, slot_y) + corner) * (1.0/vec2(32.0, 16.0))`.
- État GL : **pas** de blending (l'alpha du glyphe est mélangé dans le fragment shader : `mix(bg, fg, a)`), **pas** de depth test, **pas** de cull, `NEAREST` en min/mag, `CLAMP_TO_EDGE`, scissor = grille. Un seul programme, un seul VAO, un seul `drawArraysInstanced(TRIANGLE_STRIP, 0, 4, n)`. Coût CPU attendu `[à mesurer] : < 0,2 ms`.

### F.5 Clavier : DOM → `VecDeque` Rust

- `input.ts` mappe `KeyboardEvent.code`+`key` sur un `KeyCode` stable (u32) : ASCII pour les caractères imprimables (`code == 32..126`), plages hautes pour les touches spéciales (`KEY_UP = 256`, `DOWN, RIGHT, LEFT`, `TAB = 0x0B`, `F3`, `ESC`), **parité exacte avec l'énumération C** (`terminal.h: T3_KEY_UP = 256…`). Le mapping est une table pure, testable, une seule source.
- Chaque `keydown`/`keyup` appelle `app.push_key(code, mods, down)` **immédiatement** (hors rAF) ; Rust empile dans un `VecDeque<InputEvent>` borné (256 : au-delà, les événements les plus anciens sont fusionnés/abandonnés avec compteur, pour ne jamais croître sans fin si le navigateur déborde de répétitions).
- La **répétition de touche** est laissée au navigateur (`event.repeat`), comme le C laissait `wRepeatCount` à la console. Une répétition par frame maximum : Rust coalesce les `down` identiques consécutifs en un seul événement par frame (une touche maintenue = 1 action, indépendamment du débit clavier).
- Sur `blur`/`visibilitychange`, `push_focus(false)` : Rust vide la file et relâche toutes les touches logiques. Sans cela, une touche maintenue pendant un changement d'onglet reste « enfoncée » à jamais (bug classique).
- `preventDefault` uniquement sur la liste de §E.6 **quand le canvas a le focus** : on ne casse ni les raccourcis navigateur ni l'accessibilité.

### F.6 `ResizeObserver`, DPR

- `ResizeObserver` sur le conteneur du canvas → `push_resize(css_w, css_h, devicePixelRatio)`. On observe le **conteneur**, pas le canvas (le canvas est dimensionné par le code : s'observer soi-même crée une boucle).
- Debounce : **aucun** côté TS. Rust reçoit chaque événement, recalcule la grille (clamp §E.5) et ne déclenche une réallocation + `epoch++` que si `(cols, rows)` change réellement. Un drag de fenêtre produit donc des appels peu coûteux et un seul redimensionnement effectif par taille distincte.
- `dpr` re-lu à chaque événement : le zoom navigateur change `devicePixelRatio` sans changer la taille CSS ; sans cette relecture la grille resterait calée sur l'ancien DPR (image floue).
- Au premier frame, `push_resize` est appelé avec les dimensions mesurées **avant** `frame()` (sinon une frame à 1 × 1 cellule est présentée).

### F.7 Chargement asynchrone et erreurs explicites

```ts
// loader.ts
export async function boot(canvas: HTMLCanvasElement): Promise<Runtime> {
  if (!canvas.getContext("webgl2")) throw new BootError("WEBGL2_UNAVAILABLE", …);
  const mod = await init();                       // rejette si le .wasm 404 / MIME faux
  const app = new mod.App();                      // peut lever (erreur d'init Rust)
  const atlas = await Atlas.build(gl, levelFor(app));
  const presenter = new Presenter(gl, atlas);
  return new Runtime(app, presenter, mod.memory);
}
```

- Trois causes d'échec distinctes, trois messages **en français** dans un panneau DOM (pas d'`alert`) : `WEBGL2_UNAVAILABLE` (« Ce navigateur n'expose pas WebGL2. Le moteur fonctionne aussi en terminal natif : `./astre`. »), `WASM_FETCH_FAILED` (URL + statut HTTP), `BOOT_FAILED` (message Rust). Pas de spinner infini : `init()` est borné par un `AbortController` + timeout de 15 s.
- Le panneau est du DOM statique (un `div` stylé) : aucune dépendance, aucune frame gaspillée.
- Rien n'est rendu avant que le premier `configure` + `ResizeObserver` aient fourni une grille valide.

### F.8 Pas de `panic` traversant la frontière

- `panic = "abort"` : un panic wasm **arrête** l'instance (trap). Il ne « remonte » pas en exception JS récupérable. La règle n'est donc pas « attraper », c'est **« ne jamais paniquer »** :
  1. tous les exports publics retournent `Result<_, JsError>` (converti en `throw` JS par `wasm-bindgen`) ;
  2. `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]` sur `astre-core`/`astre-app` ;
  3. toute entrée numérique externe (`now_ms`, `dpr`, `css_w/h`, `code`, `delta`) est **validée et bornée** avant usage (`now_ms.is_finite()`, `dpr ∈ [0.5, 4]`, tailles ≤ 16384) ;
  4. `debug_assert!` pour les invariants internes, jamais pour des entrées utilisateur ;
  5. `console_error_panic_hook` **uniquement** en build debug, pour le diagnostic ; jamais en release (taille + coût).
- Côté natif : `main` retourne `ExitCode`, les erreurs CLI impriment le même texte qu'aujourd'hui et rendent `2` (parité C : `demo.c:main`, options invalides → 2, échec de rendu → 1).
- `last_error_ptr()` expose le dernier message d'erreur Rust pour que le TS n'ait pas à parser une exception.

### F.9 Build et CI

```
cargo test --workspace                     # core, scene, app, term (golden compris)
cargo clippy --workspace -- -D warnings
cargo fmt --check
cargo build --release -p astre-native
wasm-pack build crates/astre-web --target web --release
npm ci --prefix web && npm run build --prefix web        # tsc + vite
node crates/astre-web/tests/node-smoke.mjs               # wasm-bindgen-test ou smoke
# vérification d'interdits :
cargo tree -p astre-web | grep -q web-sys && exit 1 || true
cargo tree -p astre-core | grep -qE 'crossterm|wasm-bindgen' && exit 1 || true
cargo run -q -p astre-native -- --snapshot --time 8 --size 120x40 --plain | diff - golden/ascii-120x40.txt
```

- Deux « garde-fous de dépendances » ci-dessus sont des tests à part entière (§H.1) : ils matérialisent §B.5.
- La CI ne fait **pas** tourner de navigateur (§H.6).
- Taille du bundle surveillée : échec si `astre_web_bg.wasm` > 600 Kio brut (seuil `[à mesurer]`, à ajuster après la première mesure réelle).

---

## G. Migration C → Rust, fichier par fichier

### G.1 Table de correspondance

| C (fichier:symbole) | Rust (crate::module) | Notes de portage |
| --- | --- | --- |
| `term3d.h: T3Vec`, `t3_vec`, `t3_rgb`, `t3_mix`, `clampf` | `astre-core::math::{Vec3, rgb, mix, clamp}` | `clampf` → `f32::clamp` mais **attention NaN** : `fminf/fmaxf` du C ne propagent pas NaN comme `f32::clamp` (qui panique sur bornes NaN et retourne NaN sur valeur NaN). Écrire `clamp_sat` explicite pour conserver la sémantique C. |
| `term3d.c: t3_create/t3_destroy/t3_resize/t3_cols/t3_rows/t3_cells` | `astre-core::cell::CellBuffer` + `astre-app::Renderer` | Le couple allocation/accès de `T3Frame` se scinde en `CellBuffer` (sortie) et `RasterTarget` (`samples`+`depth`, privé). Bornes et no-op sur taille identique conservés (§C.5.2). |
| `term3d.c: t3_begin` | `astre-core::raster::begin` + `Renderer::begin` | Garder l'ordre exact des opérations : clamp de `near_z`, base `forward/right/up` avec les **deux** replis, calcul de `fx`/`fy`, `memset depth`, puis boucle ciel par échantillon. |
| `term3d.c: view/project/edge/raster` | `astre-core::raster::{view, project, edge, raster_triangle}` | `Projected {x, y, iz}` → struct identique en `f32`. Rejet `abs(area) < 1e-5`, inversion des sommets, bbox en `f32` puis cast `i32` **comme le C** (le C fait `fmaxf(0, ceilf(...))` puis `(int)`). |
| `term3d.c: t3_triangle` (clipping) | `astre-core::clip::near_plane` | Sutherland–Hodgman 3→4 exactement, puis éventail `(0, i, i+1)`. Conserver le test `(p.z >= near_z) != (q.z >= near_z)` (comparaison stricte, ordre des sommets `(i+2)%3, i`). |
| `term3d.c: shapes[]`, `bits`, `distance`, `resolve_cell`, `t3_resolve` | `astre-core::resolve::{ASCII_LOOKUP, distance, resolve_cell, resolve_all}` | `Shape{char c, uint8_t mask}` → `AsciiShape{ch: u8, inverted: bool}` (fin du double sens du champ). Tables braille/blocs en `const [u32; N]`. La boucle 2 × 4 devient indexée explicitement, sans arithmétique `size_t`. |
| `term3d.c: t3_fill`, `t3_text` | `astre-core::hud::{fill, text, Anchor}` | `T3Anchor` → `enum Anchor {TopLeft, TopRight, BottomLeft, BottomRight}`. Conserver `c >= 32 && c < 127` sinon `'?'`, et le calcul `y = rows - dy - 1` pour les ancres basses. |
| `term3d.c: t3_import_rgb565` | **non porté** | Utilisé par `quake/relay.c` uniquement (hors périmètre). À reprendre si Relais de Cendre est porté un jour ; noter que le C y tronque `width`/`height` par `fminf` avec des rapports 4:3 — comportement à re-spécifier le jour venu. |
| `terminal.c: t3_terminal_open/close/running/size/key` (Unix) | `astre-term::session` via `crossterm` | `crossterm::terminal::{enable_raw_mode, EnterAlternateScreen}`, `event::poll/read`. **Écarts à gérer** : `SIGTSTP` (`Ctrl-Z`) était intercepté et rejoué à la main (`raise(SIGSTOP)` + restauration du mode écran) ; `crossterm` ne le fait pas → garder un handler `libc` sous `cfg(unix)` et reproduire la séquence « quitter l'écran alterné → stop → ré-entrer → forcer un redraw complet ». |
| `terminal.c: t3_terminal_open/key` (Win32) | `astre-term::session` (Windows) | `crossterm` gère l'entrée console et `WriteConsoleW`. Points à préserver : conversion **par caractère complet** (jamais couper un UTF-8 multi-octets en frontière de buffer — le C le faisait explicitement), et `ENABLE_VIRTUAL_TERMINAL_PROCESSING` sur la sortie. |
| `terminal.c: utf8`, `nearest_cube`, `indexed`, `ink`, `emit`, `equal_cell`, `t3_present` | `astre-term::ansi::{utf8, indexed, ink, emit, equal_cell, present}` | **Parité octet à octet obligatoire** (golden §H.4) : mêmes séquences, même ordre (bg puis fg), même optimisation de couleur « seulement si la clé change », même `equal_cell` (fg ignoré si `glyph == ' '`), même fusion des cellules contiguës, même redraw complet sur changement de taille/palette. |
| `terminal.c: t3_snapshot` | `astre-term::snapshot` + `astre-native --snapshot` | Mêmes lignes, `\033[0m` en fin de ligne hors mode `plain`, `\n` final. Le cas Win32 « sortie console » du C (bascule de mode du handle) devient inutile : on écrit dans un `Write`, et `astre-native` redirige. |
| `demo.c: Options`, `number`, `dimensions`, `usage`, `main` | `astre-app::cli::{Options, parse}` + `astre-native::main` | Mêmes options, mêmes bornes (`--fps 1..120`, `--cell-aspect 0.1..3`, `--size 1..4096`, `--time 0..86400`), mêmes messages et mêmes codes de sortie (2 pour une option invalide, 1 pour un échec de rendu). Voir §G.6 pour le choix d'analyseur. |
| `demo.c: Orbit`, `reset_camera`, `camera_for`, `limit` | `astre-app::orbit` + `astre-scene::camera` | Constantes à conserver : `azimuth 1.01`, `elevation 0.30`, `radius 14.2`, `fov_y 0.79`, `near_z 0.15`, `radius ∈ [3.4, 32]`, `elevation ∈ [0.03, 1.42]`, pas `0.075` (azimut) / `0.045` (élévation), orbite auto `0.095 rad/s`, recentrage `target.y += 0.118·radius·cos(elevation)`. |
| `demo.c: keypress` | `astre-app::input::apply` | Mêmes touches ; `+`/`-` traités selon §E.7. |
| `demo.c: overlay` | `astre-app::overlay` | Mêmes chaînes (contrat visuel), mêmes paliers de largeur (`w >= 32`, `46`, `63`, `86`, `h >= 20`, `h >= 3`). C'est le seul endroit où des accents sont volontairement absents : le C n'utilisait que de l'ASCII imprimable dans le HUD. |
| `demo.c: now`, `wait_frame`, `live` | `astre-app::app::App::frame` + `astre-core::time` | Disparaît : remplacé par le pas fixe (§D). L'horloge devient un paramètre injecté (`now_ms`) pour les tests. |
| `demo_scene.c: hash2`, `sky`, `paint`, `tri`, `quad`, `box`, `drum`, `annulus`, `terrain`, `pad`, `loft`, `ship`, `beacon`, `draw_port` | `astre-scene::{sky, lighting, primitives, world}` | Portage purement mécanique, à constantes identiques. `draw_port` → `Scene::compose`. `Model { frame, eye, offset }` → `Builder<'_>` qui emprunte le `Renderer`. |
| `tests/render.c: visibility/formats/hud` | `astre-core` tests d'intégration + goldens | Chaque `assert` du C devient un test nommé (§H.2). |
| `terminal.c` : `t3_present` comme `Presenter` | `astre-term::AnsiPresenter` | Le terminal est le seul backend à garder un `previous` (double buffer), conformément à ACTÉ. |
| `quake/relay.c` | **non porté** | Hors périmètre. |

### G.2 Découpage des fichiers, pas seulement des symboles

| Module Rust | Origine | Taille visée |
| --- | --- | --- |
| `astre-core/src/math.rs` | extraits de `term3d.c` + `demo_scene.c` (`dot`, `cross`, `unit`) | ~200 l. |
| `astre-core/src/raster.rs` | `t3_begin` (partie géométrie) + `view/project/edge/raster` | ~320 l. |
| `astre-core/src/clip.rs` | clipping de `t3_triangle` | ~90 l. |
| `astre-core/src/resolve.rs` | `shapes`, `bits`, `distance`, `resolve_cell`, `t3_resolve` | ~300 l. |
| `astre-core/src/cell.rs` | nouveau (`T3Frame` scindé) | ~350 l. |
| `astre-core/src/hud.rs` | `t3_fill`, `t3_text` | ~150 l. |
| `astre-scene/src/*` | `demo_scene.c` éclaté (sky / lighting / primitives / world) | ~600 l. |
| `astre-app/src/*` | `demo.c` éclaté (app / sim / orbit / input / overlay / cli / metrics) | ~800 l. |
| `astre-term/src/*` | `terminal.c` éclaté (session / ansi / palette / snapshot) | ~700 l. |
| `astre-web/src/lib.rs` | nouveau (ABI) | ~400 l. |

### G.3 Difficultés `f32`, indices, NaN — la liste des pièges vérifiés

1. **Littéraux `f64` invisibles.** En Rust, `0.5`, `1.0`, `0.29` sont des `f64`. Le C mélangeait déjà les deux : `demo_scene.c:normal` écrit `float k = 1 / sqrtf(dot(n,n))` — `1` est un `int`, promu en `double`, division **en double** puis conversion en `f32`. `term3d.c:unit` écrit au contraire `1.0f / sqrtf(...)` (division en `f32`). Porter les deux tels quels (ou documenter un écart assumé) sinon les goldens divergent sur les couleurs d'un ou deux niveaux.
2. **Troncature `f32` → `u8`, pas arrondi.** Le C affecte des `float` à des `uint8_t` (ex. `t3_mix`, `resolve_cell` : `T3Color bg = {sum[0][0]/count[0]}` où `sum`/`count` sont des `int`). En Rust, `as u8` **sature** et tronque vers zéro, ce qui coïncide ici, mais toute valeur négative (possible via `t3_rgb(-5, …)` avant clamp, ou `light-band` dans le ciel) donnerait `0` en Rust et un comportement indéfini en C. Écrire un helper `trunc_u8_sat(f32) -> u8` **nommé** et l'utiliser partout ; c'est aussi ce qui rend le code auditable.
3. **Division entière de couleurs.** `sum[0][0]/count[0]` est une division entière (troncature). Utiliser `i32` en Rust puis `as u8`, jamais `f32` : sinon ±1 d'écart sur beaucoup de cellules.
4. **L'indice `count[0] >= 1`.** Invariant non documenté dans le C : `p[lo]` ne peut jamais tomber dans le groupe 1, donc `count[0] ≠ 0` et la division n'est jamais `0/0`. À conserver **et** à asserter (`debug_assert!(count[0] >= 1)`), sinon un jour une simplification du seuil rend la moyenne NaN.
5. **Indices négatifs.** `t3_fill` et `t3_text` reçoivent des coordonnées potentiellement négatives et les bornent (`fmaxf(0,x)`, `x+i >= 0`). En Rust, `usize` interdit les négatifs : garder des `i32` dans les signatures publiques du HUD et convertir après bornage, dans un seul endroit.
6. **`mask ^= 255` et `group << i`.** Détail du C : après inversion, `mask` est complémenté puis re-testé. C'est un `u8` (débordement tronqué) : en Rust, `mask ^ 0xFF` sur `u8` est correct, mais un `u32` changerait le résultat. Typer `mask: u8` fermement.
7. **`(int)fmaxf(0, y)` sur un `int`.** Le C passe un `int` à `fmaxf` (conversion implicite) : aucune perte, mais le portage doit **éviter** de reproduire l'aller-retour float inutile ; utiliser `y.max(0)` en `i32`.
8. **NaN dans la caméra.** `eye == target` donne `forward = (0,0,0)`, puis `right` non normalisable → repli `(1,0,0)` (via `dot(horizontal,horizontal) > 1e-10f`). `fov_y` et `cell_aspect` sont clampés, `near_z ≥ 0.001`. Conséquence : **aucun NaN ne sort** du `begin` du C. Le portage doit conserver ces trois garde-fous et ajouter, en debug, une assertion `camera.is_finite()` à l'entrée. Un `Vec3` NaN propagerait sinon jusqu'aux couleurs (`as u8` d'un NaN = 0 en Rust, indéfini en C : divergence silencieuse).
9. **`1e-20f` et `1e-10f`.** Garder la même constante : ils déterminent si un vecteur dégénéré est normalisé à `(0,0,0)` ou rejeté.
10. **`atan2f(x, z)` et `hash2`.** Utilisés par le ciel (étoiles) : sensibles au `libm`. Voir §G.7 (déterminisme).
11. **`%` sur entiers négatifs.** `(int)floorf(sx)` peut être négatif, puis `hash2(x, y)` caste en `u32` : le C convertit un `int` négatif en `uint32_t` (modulo 2³²). En Rust, `x as u32` fait exactement la même chose (conversion *wrapping*), mais `x.rem_euclid()` non : garder `as u32`.
12. **Ordre d'évaluation des sommes de couleurs.** `sum[group][0] += p[i].r` en `int`, puis division : porter en `i32` et sommer dans l'ordre 0..8. Un `f32` donnerait des écarts.
13. **`static Shape lookup[256]` initialisé paresseusement.** Le C teste `!lookup[0].c` comme drapeau ; `lookup[0]` devient `{' ', 0}` (c = 32 ≠ 0) par construction, donc le cache fonctionne. C'est fragile et non thread-safe : en Rust, table `const` (§C.5.6), et le drapeau disparaît.
14. **`T3Cell` = `{uint32_t glyph; T3Color fg, bg;}` = 12 o**, alors que la cible Rust fait 16 o (padding explicite). Ne pas chercher la parité de taille : la parité est **sémantique** (glyphe + deux couleurs), et les goldens comparent des triplets, pas des octets de `Cell`.

### G.4 Chaînes de HUD : contrat visuel

Les libellés de `demo.c:overlay` (`" ASTRE "`, `"RELAIS 07  /  AIRE DE VOL"`, `"ORBITE AUTO"`, `"Fleches  Orbite   +/-  Zoom   Espace  Pause   M  Rendu   P  Perf"`, `"NX-04"`, `"AZ %03d   ALT %04.1f m   DIST %04.1f m"`, l'écran d'aide) sont **sans accents** parce que le C n'autorisait que l'ASCII imprimable dans `t3_text`. Le portage les conserve **à l'identique** : ce sont eux que comparent les goldens texte, et les changer casserait la comparaison C/Rust pour un gain nul. Une éventuelle version accentuée serait une option (`--hud-utf8`) après la v1, avec son propre golden.

### G.5 Ce qu'on ne porte *pas* tel quel (améliorations actées)

| C | Rust | Raison |
| --- | --- | --- |
| `cells` non initialisé après `malloc` | toujours initialisé | sécurité ; `resolve` couvre toute la grille, donc pas de changement visible |
| `memset(depth)` complet à chaque `begin` | idem (sur le viewport, pas au-delà) | parité + petite économie sur les sous-vues |
| `static` de cache / signaux | état dans la struct / handler `cfg(unix)` | §B.5 |
| `wait_frame` (sommeil) | pas de sommeil | §D.2 |
| `dt` variable | pas fixe + interpolation | §D.1 |
| `+`/`-` = rayon caméra | `+`/`-` = zoom cellule, molette = rayon | §E.7 |
| `--fps` = horloge | `--fps` = plafond de rafraîchissement | §D.2 |

### G.6 `clap` vs `std::env` : recommandation

**Recommandé : `std::env` + un parseur maison** (~120 lignes, `astre-app::cli`), pas `clap`.

| Critère | Maison | `clap` (derive) |
| --- | --- | --- |
| Dépendances transitives | 0 | ~7–9 crates `[à mesurer]` |
| Taille binaire ajoutée | ~0 | de l'ordre de 100–300 Kio `[à mesurer]` |
| Messages d'erreur | rédigés à la main, **identiques au C** | reformatés, différents |
| Code de sortie | contrôlé (2 comme le C) | à forcer via `exit_code` |
| Aide | statique, en français, comme aujourd'hui | générée, en anglais par défaut |
| Coût de maintenance | faible (12 options) | faible |
| Bénéfice | parité + zéro dépendance | complétions shell, sous-commandes |

Arguments décisifs : (1) l'identité du projet est « zéro dépendance hors `libm` » — l'introduire pour 12 options serait disproportionné ; (2) la parité des messages et du code de sortie est **testable** (§H.5) et sert de golden CLI ; (3) le parseur est partagé avec `astre-native` uniquement (le web ne parse rien), donc il n'a aucune raison d'être générique. Bascule vers `clap` justifiée seulement si le CLI gagne des sous-commandes (`serve`, `bench`, `replay`).

### G.7 Déterminisme `f32` et `libm`

- Le C appelle `sqrtf`, `tanf`, `sinf`, `cosf`, `atan2f`, `floorf`, `ceilf`, `fabsf`, `fminf`, `fmaxf`, `lroundf`. En Rust natif, `f32::sqrt` est une instruction IEEE (exacte, identique partout), mais `sin/cos/tan/atan2` passent par `libm` de la plateforme : **résultats non garantis identiques** entre x86-64, aarch64, wasm et MSVC.
- **ARBITRÉ ICI** : `astre-core` est `no_std` et dépend de la crate `libm` (implantation Rust pure). Tous les appels trigonométriques passent par `libm::sinf/cosf/tanf/atan2f/floorf/ceilf/…`. Conséquences : même résultat bit à bit sur toutes les cibles (le code est le même), au prix d'un ralentissement des fonctions trigonométriques `[à mesurer : facteur 1–3 sur `sinf`/`atan2f`]`. La feature `std-math` bascule vers `std` **pour le seul binaire natif** et n'est jamais activée pour le web → deux jeux de goldens si elle est activée ; par défaut **désactivée**, y compris en natif, pour garder une seule référence.
- `sqrtf` peut venir de l'instruction (IEEE) dans les deux cas : garder `libm::sqrtf` par uniformité de code, le compilateur le replie sur l'instruction.
- **Opérations fusionnées** : Rust n'émet pas de FMA automatiquement (`fma` est un appel explicite) ; le C non plus sans `-ffast-math`. Les deux pipelines sont donc « sans contraction » : cohérents.
- **Pas de `fast-math`, pas de `-C target-feature=+fma`** pour la cible de référence des goldens.

### G.8 Ce que la migration ne doit pas casser

Le C a des comportements non écrits mais réels, à conserver explicitement :

1. `t3_resize` **no-op** si même taille (les buffers — et donc le contenu — survivent) : le `Renderer::resize` doit être un no-op, pas un `Clear`.
2. `t3_begin` appelé **sans** `t3_resize` préalable travaille sur l'ancienne grille (le démo appelle `resize` à chaque frame ; la bibliothèque, non).
3. `t3_resolve` peut être appelé plusieurs fois avec des modes différents sur la même image (le test `formats` le fait) : `resolve` ne consomme pas les échantillons.
4. Un triangle entièrement derrière `near_z` disparaît (test `visibility`), mais une arête exactement sur `near_z` est **conservée** (`>=`).
5. `t3_text` tronque à `cols` mais écrit à partir d'un `x` négatif possible (test `hud` : `t3_text(TOP_LEFT, -2, 0, "ABCDE")` affiche `C` en colonne 0).
6. `t3_present` renvoie `false` en cas d'échec d'écriture **sans** mettre à jour `previous` : la frame suivante réessaie donc tout. En Rust, `PresentOutcome`/`Err` doit préserver cette propriété (ne pas marquer les tuiles propres avant succès).

---

## H. Tests

### H.1 Unitaires `astre-core` et `astre-scene`

| Test | Vérifie |
| --- | --- |
| `cell_layout_is_16_bytes` | `size_of::<Cell>() == 16`, `align_of == 4`, offsets exacts. |
| `math_clamp_nan` | `clamp_sat` reproduit `fminf/fmaxf` sur NaN et infinis. |
| `buffer_bounds` | 0, −1, 4097, budget 1 000 001 → `Err` ; taille identique → `Ok` sans `epoch++` ; `stride >= cols` ; `len == stride*rows`. |
| `buffer_resize_preserve` | contenu commun conservé ligne à ligne ; le reste vide ; `epoch` strictement croissant. |
| `buffer_dirty_tiles` | écriture en `(7,3)` marque la tuile 0 ; en `(8,0)` marque la tuile 1 ; `dirty_count` exact ; `clear_dirty` remet à zéro. |
| `two_buffers_independent` | deux buffers modifiés en parallèle (threads) restent indépendants (pas de global). |
| `raster_depth_order` | les deux ordres de soumission donnent la même image (parité `tests/render.c:visibility`). |
| `raster_near_clip` | 2 sommets derrière `near_z` → petite surface visible ; `z = near_z` exactement → conservé ; tout derrière → rien. |
| `raster_degenerate` | triangle d'aire nulle ignoré ; triangle d'aire < 1e-5 ignoré. |
| `raster_double_sided` | envers/surface même couleur. |
| `resolve_lookup_snapshot` | les 256 entrées de `ASCII_LOOKUP` comparées à une table de référence (golden texte, 256 lignes) — c'est la traduction exacte de la boucle d'initialisation du C. |
| `resolve_all_masks_ascii` | pour chacun des 256 masques, le glyphe émis ∈ ASCII 32..127 et l'inversion correspond à la référence C. |
| `resolve_block_alignment` | sur 4 096 cellules synthétiques aléatoires, le résolveur n'émet **que** les 16 codepoints de blocs alignés (jamais `▁▂▃…`). |
| `resolve_braille_bits` | motif d'échantillons `(2j+col)` → bit braille standard attendu, pour les 256 motifs. |
| `resolve_count0_nonzero` | invariant §G.3.4 sur un million de cellules pseudo-aléatoires (déterministe, PRNG xorshift semé). |
| `hud_anchors` | les 4 ancres à 40 × 20 et 23 × 9 (parité `tests/render.c:hud`), `dx` négatif, troncature, `'?'` hors ASCII. |
| `scene_determinism` | `Scene::compose` avec `t` fixe produit 3 fois le même `CellBuffer` (hash identique). |
| `dependency_guards` | `cargo tree` : `astre-core` sans `crossterm`/`wasm-bindgen` ; `astre-web` sans `web-sys`. |

### H.2 Golden `CellBuffer` : référence C et régénération

Deux niveaux de golden, complémentaires :

**(a) Golden C ↔ Rust (parité de portage).** On utilise le C **inchangé** comme oracle.

```
# 1. oracle C (jamais modifié, jamais recompilé par la CI Rust)
cd test3Dterminal && make astre && \
  ./astre --snapshot --size 120x40 --time 8 --mode braille --plain > /tmp/c-braille-120x40.txt
# 2. candidat Rust
cargo run -q -p astre-native -- --snapshot --size 120x40 --time 8 --mode braille --plain \
  > /tmp/r-braille-120x40.txt
# 3. comparaison stricte du texte (plain = glyphes sans couleurs)
diff -u /tmp/c-braille-120x40.txt /tmp/r-braille-120x40.txt
```

- Matrices de cas, minimale mais non triviale : `{ascii, braille} × {120x40, 240x72, 23x9} × {--time 0, 8}` = 12 snapshots. `--time` étant déterministe dans le C (`orbit.seconds = start`, `azimuth += fmod(start*0.095, 2π)`), ces 12 images sont reproductibles.
- Le mode `--plain` compare les **glyphes** (l'information la plus fragile : résolution, tables, blocs). Un second passage en `--palette rgb` compare les **séquences ANSI** complètes (§H.4).
- Pour les couleurs, la comparaison est **tolérante** (§I.5) : on parse les séquences et on autorise ±1 par canal et ≤ 0,5 % de cellules divergentes ; les glyphes, eux, sont comparés strictement.

**(b) Golden Rust ↔ Rust (non-régression).** Une fois (a) vert, on fige des goldens **du** Rust, plus riches : hash du `CellBuffer` complet (glyphes + couleurs + flags) pour chaque cas, stocké en texte (`cols×rows` puis une ligne par rangée encodée en hex). Avantages : diffs lisibles en revue, pas de binaire.

Procédure de régénération (documentée en tête de chaque fichier golden) :

```
cargo run -p astre-native -- --regen-golden  # hors CI, écrit golden/*.txt
git diff golden/                             # la revue humaine est OBLIGATOIRE
```

Règles :
- La régénération **n'est jamais** un moyen de faire passer la CI : un diff golden non nul est une régression à expliquer, ou une modification de référence **avec** levée explicite du gel du C et v2 des goldens (§I.4).
- Le C est **GELÉ — acté le 2026-11-24** : `test3Dterminal/` est l'oracle de parité **immuable** du portage ; aucune modification ultérieure n'y est admise (sources C, `Makefile`, `tests/`, carte `quake/id1/maps/relay.bsp` inclus). Un tag/commit est noté dans `docs/` comme référence d'oracle. Si quelqu'un modifie `test3Dterminal/`, les goldens (a) deviennent invalides et la CI doit le dire (test qui vérifie un hash des sources C listées). Une évolution du comportement se ferait par une v2 des goldens **et** une décision explicite de lever le gel, jamais par une retouche silencieuse (§I.4).

### H.3 Resize, zoom, clamp

| Test | Vérifie |
| --- | --- |
| `fit_budget_clamp` | pour 20 tailles de viewport × 4 DPR × 9 paliers, `cols*rows <= 1_000_000` **toujours**, `level` recule si nécessaire, jamais d'`Err`. |
| `fit_letterbox` | `origin + cells*cell_px <= avail_px`, reste < `cell_px`, centrage exact (différence ≤ 1 px entre les deux marges). |
| `fit_auto_level` | monote en `avail_px` : un viewport plus grand ne choisit jamais un palier plus grossier. |
| `zoom_clamps_at_edges` | `push_zoom(+1)` 20 fois reste au palier max ; `-1` 20 fois au palier min ; aucune erreur, `epoch` stable si la grille ne change pas. |
| `dpr_fractional` | `dpr = 1,25` : `cell_px` reste entier, le letterbox absorbe ; la somme reste ≤ `avail_px`. |
| `resize_storm` | 2 000 `push_resize` aléatoires (déterministes) : aucune allocation après la 50ᵉ (compteur §H.5), `epoch` monotone, image finale identique à un resize direct vers la même taille avec `Clear`. |

### H.4 Diff terminal ANSI

Testé avec un `PresentTarget: Write` factice (pas de terminal réel), ce qui rend les assertions exactes :

1. **Séquences exactes** : première image → `\033[?1049h`, `\033[?25l`, `\033[2J` puis positionnements + couleurs + glyphes ; dernière → `\033[0m\033[?25h\033[?1049l` (parité `terminal.c:enter_screen/leave_screen`).
2. **Diff minimal** : deux `CellBuffer` qui diffèrent d'une cellule → les octets émis contiennent **une** séquence de positionnement et **une** cellule.
3. **Fusion contiguë** : trois cellules adjacentes modifiées → un seul `\033[y;xH`.
4. **Couleur mémorisée** : deux cellules voisines de même `bg` → une seule séquence `48`.
5. **`fg` ignoré sur espace** : `equal_cell` (parité §G.1).
6. **Redraw complet** sur changement de taille **et** de palette.
7. **Palette 256** : `nearest_cube` et la bascule cube/gris reproduites à l'identique sur 4 096 couleurs (golden texte).
8. **Échec d'écriture** : un `Write` qui renvoie `Err` → `previous` **non** mis à jour, la frame suivante réémet tout (§G.8.6).
9. **UTF-8** : un glyphe Braille se sérialise en 3 octets `E2 A0 xx` ; jamais coupé entre deux appels d'écriture (parité `terminal.c:utf8`).

### H.5 Snapshot natif headless, CLI, allocations

- **Headless** : `astre --snapshot` ne touche ni `isatty` ni `termios` ; testé en CI (pas de TTY).
- **CLI** : 12 tests de parité — option inconnue, `--fps 0`, `--size 0x9`, `--size 4097x1`, `--cell-aspect 0.05`, `--time -1`, `--plain` sans `--snapshot` → message + code 2 ; `--help` → code 0.
- **Allocations** : `#[global_allocator]` compteur dans un test d'intégration ; sur 100 frames à grille constante, **zéro** allocation après la première (couvre §C.5.7.5 et la fuite §I.6).
- **RSS** : test `#[ignore]` (manuel) qui resize 5 000 fois et compare `/proc/self/statm` — non bloquant en CI, mais documenté comme vérification de release.

### H.6 Navigateur en CI : question tranchée

**Recommandation : pas de Playwright/Puppeteer en CI pour la v1.** Justification :

1. **Ce qui serait testé n'est pas le risque principal.** Le code TS du présentateur fait ~250 lignes : un `bufferSubData` et un `drawArraysInstanced`. Ses modes de panne (état GL, mauvais UV, mauvais stride d'instance) sont **visuellement** évidents et se détectent en une ouverture manuelle. Le risque réel est dans le wasm (raster, resolve, timing) — déjà couvert sans navigateur.
2. **Le coût est réel et récurrent.** Playwright télécharge un navigateur (~150–400 Mo selon la cible), exige un environnement GPU ou SwiftShader/ANGLE, et l'obtention d'un contexte WebGL2 *headless* est notoirement instable en conteneur (drapeaux `--use-gl=swiftshader`, `--enable-unsafe-swiftshader`, échecs silencieux qui rendent `getContext('webgl2')` nul). On paierait plusieurs minutes de CI et une catégorie de flakiness pour un test dont la valeur est faible.
3. **Les captures sont non déterministes par construction** : l'atlas est rendu avec la police système (`fillText`) ; deux images entre un runner Linux et un poste macOS n'ont pas le même rendu de glyphe. Tout test de pixel serait condamné au `pixelmatch` tolérant, donc peu discriminant.
4. **Ce qui reste testable sans navigateur l'est déjà** : `wasm-bindgen-test` en Node (§H.7) couvre l'ABI, les pointeurs, `resize`/`zoom`/`push_key`, les erreurs, et le packing des instances (comparé à une fonction de référence Rust).

Contre-proposition, si un jour le besoin est prouvé (une régression WebGL constatée) : **un seul** job optionnel, en `workflow_dispatch` (jamais bloquant), qui ouvre une page et vérifie (a) `getContext('webgl2') != null`, (b) `drawArraysInstanced` appelé une fois (instrumentation du contexte), (c) un hash du buffer d'instances (qui, lui, est déterministe). Pas de comparaison d'image.

### H.7 Tests du pont WASM (sans navigateur)

- `wasm-bindgen-test` en cible Node : `App::new`, `push_resize`, `push_zoom` (clamps), `push_key` (coalescence des répétitions, vidage sur perte de focus), `frame` (bits de retour, monotonie de `epoch`), `last_error` après entrée invalide (`dpr = NaN`, `css_w = -1`).
- **Packing des instances** : `Cell → InstanceWord` testé contre une implémentation de référence écrite séparément (fg 24 bits + slot 8, bg 24 bits + flags 8) sur 10 000 cellules pseudo-aléatoires.
- **Comptage d'appels** : instrumenter le wrapper JS généré et vérifier qu'une frame = 1 `frame()` + 0 autre export (règle §F.2).
- **Pointeurs** : après un `resize` qui réalloue, `cells_ptr` a changé et `cells_len_bytes == cols*rows*8` (ou `*2` mots).

---

## I. Risques et choix ouverts

### I.1 Risques priorisés

| P | Risque | Impact | Mitigation |
| --- | --- | --- | --- |
| **P1** | **Divergence C ↔ Rust** sur les couleurs/glyphes (troncature `f32→u8`, `1/` en `f64`, ordre des sommes) | Les goldens échouent en masse, on ne sait plus si le portage est juste | §G.3 en checklist ; golden glyphes stricts en premier ; helper `trunc_u8_sat` unique et testé ; comparer d'abord `--plain` (glyphes) puis les couleurs. |
| **P1** | **Déterminisme `f32` inter-plateformes** (trigonométrie `libm`) | Golden instable entre runners | `libm` pur dans `astre-core`, `std-math` désactivée par défaut ; tolérance ±1 canal / ≤0,5 % cellules ; goldens stockés en texte et non en pixels. |
| **P1** | **Budget 1 M cellules × 8 o = 8 Mio d'instances ré-uploadés/frame** | Débit PCIe/`bufferSubData` en hausse, frame longue | v1 : ré-upload complet ; à 20 k cellules (138 Kio) le coût est négligeable. Seuil d'alerte mesuré → patch par tuiles sales (sous-plages `bufferSubData`) si > 1 Mio/frame. |
| **P2** | **WebGL2 indisponible** (vieux navigateur, GPU bloqué, conteneur) | Aucune image | Détection à l'init + message explicite + renvoi vers le terminal natif. Pas de fallback Canvas2D (§I.2). |
| **P2** | **Perte de contexte WebGL** (mobile, reset GPU, longue inactivité) | Écran figé, `gl` invalide | `webglcontextlost` (annuler `preventDefault`), `webglcontextrestored` → reconstruire programme, VAO, VBO et **atlas**, forcer `epoch`/redraw complet. Test manuel documenté. |
| **P2** | **Police monospace variable / absente** | Glyphes mal alignés, atlas différent par machine, captures non reproductibles | Pile `ui-monospace, "SF Mono", "Cascadia Mono", "DejaVu Sans Mono", monospace` ; mesurer `measureText` et **centrer** dans le slot ; `letterSpacing = 0` ; les glyphes Braille/blocs n'utilisent **pas** la police (fillRect) donc restent alignés quoi qu'il arrive ; option ultérieure : police embarquée (sous-ensemble de 96 glyphes, ~20–40 Kio woff2 `[à mesurer]`). |
| **P2** | **Fuites sur resize répétés** (Vec réalloués, atlas reconstruit) | Croissance mémoire dans le temps, sur mobile notamment | Capacité réutilisée (`Vec::with_capacity` au max vu), `epoch`-guard sur l'atlas, test §H.3 `resize_storm` + compteur d'allocations ; test RSS manuel en release. |
| **P2** | **Performance du ciel par échantillon** : 138 240 évals/frame à 240 × 72 (déjà le poste dominant en C) | Frame > 8 ms sur portable | Mesurer d'abord (§D.6). Optimisations candidates, dans l'ordre : (a) évaluer le ciel par échantillon **pair** (×2×2) puis duplication douce — **casse la parité golden**, donc derrière un drapeau et après avoir figé une v2 des goldens ; (b) précalculer l'étoile/planète hors ligne ; (c) éviter `atan2f` en étoiles (remplacer par un hash de direction quantifiée). **Aucune** de ces optimisations n'entre en v1. |
| **P3** | **Panic traversant la frontière wasm** (trap irrécupérable) | Onglet mort sans message | `Result` partout, `deny(clippy::unwrap_used)`, validation des entrées, `panic = "abort"` + hook debug seulement (§F.8). |
| **P3** | **`wasm-bindgen` désynchronisé** (CLI vs crate) | Erreur d'import incompréhensible au boot | Versions épinglées (`Cargo.lock`, `package.json`, `rust-toolchain.toml`), job CI dédié au build wasm seul. |
| **P3** | **`ArrayBuffer` détaché** après croissance mémoire | Exception TS aléatoire en cours de session | Recréer la vue chaque frame ; ne jamais cacher `cells_ptr` (§F.3). |
| **P3** | **Portage terminal régressif** (raw mode, `Ctrl-Z`, Windows) | Terminal cassé à la sortie, echo activé | Parité octet à octet testée ; garder le handler `SIGTSTP` explicite ; checklist manuelle Windows (Windows Terminal + `cmd.exe`) avant release. |
| ~~**P3**~~ | **Ambiguïté « 512 slots 32 × 16 »** | Atlas mal dimensionné, UV faux | **Acté le 2026-11-24, risque soldé** (§E.3, §I.4) : grille 32 colonnes × 16 lignes = 512 slots, slot `w × 2w` texels. Le code isole la constante dans `atlas.ts` pour qu'un changement reste local. |
| ~~**P4**~~ | **Licence absente** | Impossible de publier / redistribuer légalement | **Acté le 2026-11-24, risque soldé** : licence MIT, `LICENSE` (copyright 2026 `Holaf`) à la racine (§I.3, §I.4). |
| **P4** | **`test3Dterminal/` modifié un jour** | Oracle invalidé, goldens menteurs | **Gel acté le 2026-11-24** (§I.4) : aucune modification ultérieure du C, oracle de parité immuable. Garde-fou maintenu : hash des sources C dans un test, procédure de regen explicite (§H.2). |

### I.2 Fallback Canvas2D : recommandation chiffrée

**Recommandation : ne pas l'implémenter.** Chiffrage du chemin de secours, à 240 × 72 = 17 280 cellules :

| Poste | Estimation `[à mesurer]` |
| --- | --- |
| `fillRect` de fond | 17 280 appels/frame |
| `drawImage` du glyphe | 17 280 appels/frame (ou `fillText` : 17 280 appels + `font` à re-régler) |
| Coût d'appel Canvas2D (petites opérations) | 0,5–2 µs → **17–70 ms/frame** |
| Cadence atteignable | **≈ 14–60 fps**, avec un plancher réaliste plutôt vers 20–30 fps |
| Mémoire de l'atlas CPU (`ImageBitmap`) | identique au GPU (0,25–1 Mo), mais en bitmap |
| Code supplémentaire | ~250 lignes TS + un second chemin de test à maintenir |
| Valeur utilisateur | faible : les navigateurs sans WebGL2 sont marginaux, et le terminal natif existe |

Pour ~250 lignes et une deuxième implémentation de présentation à maintenir (donc deux fois plus de surface de bug pour un rendu approximatif), on couvrirait un cas d'usage que le projet sert déjà autrement. **Décision : détection + message explicite + lien vers le natif.** Si un besoin réel apparaît (kiosque sur machine sans GPU), la porte est ouverte : `presenter.ts` est derrière une interface, un `presenter2d.ts` s'y substituerait sans toucher à Rust, avec un plafond de cellules réduit (≈ 4 000) pour tenir la cadence.

### I.3 Choix ouverts et recommandation par défaut

| # | Choix ouvert | Options | **Recommandation par défaut** | Justification courte |
| --- | --- | --- | --- | --- |
| 1 | **Emplacement du workspace** | (a) racine du dépôt, à côté de `test3Dterminal/` ; (b) sous-dossier `rust/` | **(a) racine**, `Cargo.toml` virtuel + `crates/*` + `web/` | Cargo exige un manifeste pour `cargo test --workspace` à la racine ; `test3Dterminal/` n'étant pas un projet Cargo, il n'y a aucun conflit ; les chemins `crates/…` et `web/` restent stables pour la CI et `wasm-pack`. L'option (b) ajouterait un `cd` à chaque commande et un `paths` dans `package.json`. |
| 2 | **Nom du binaire** | `astre`, `astre-native`, `shooty` | **`astre`** | Parité avec le C et la doc existante (`./astre`, `.\astre.exe`) ; le nom de crate `astre-native` reste distinct du nom de binaire (`[[bin]] name = "astre"`). |
| 3 | **Licence** (tranchée) | MIT, Apache-2.0, double, propriétaire | **MIT** — **acté le 2026-11-24** ; `LICENSE` MIT (copyright 2026 `Holaf`) créé à la racine | Décision du titulaire des droits (compte GitHub `grokuku`, dépôt `grokuku/Shooty`) : la question « qui décide » est close. Le `LICENSE` couvre le dépôt ; `test3Dterminal/quake/TyrQuake-LICENSE.txt` (GPLv2) conserve sa licence d'origine et ne contamine pas `astre-core` tant que Quake reste hors périmètre et non lié. À répercuter dans les manifestes : `license = "MIT"` dans les `Cargo.toml` du workspace (au lieu de `MIT OR Apache-2.0`), `"license": "MIT"` dans `web/package.json`. |
| 4 | **Fallback Canvas2D** | garder / jeter | **Jeter** (§I.2) | 17–70 ms/frame estimés, +250 lignes, deuxième chemin de test. |
| 5 | **Tolérance des goldens couleur** | stricte / ±1 / ±2 | **glyphes stricts ; couleurs ±1 par canal, ≤ 0,5 % de cellules divergentes** | Couvre les écarts `libm`/troncature sans masquer une vraie régression (une erreur de table ou de masque change des milliers de cellules). |
| 6 | **`libm` dans `astre-core`** | dépendre de `libm` / utiliser `std` | **`libm`**, `std-math` désactivée par défaut (§G.7) | Déterminisme inter-cibles ; coût trigonométrique à mesurer, acceptable sur 138 k évals/frame. |
| 7 | **Fuites sur resize** | rien / capacité réutilisée / pool | **capacité réutilisée** + test d'allocations + test RSS manuel | Simple, couvre les deux causes (Vec, atlas) sans pool. |
| 8 | **WebGL2 indisponible** | fallback 2D / message / redirection | **message explicite + renvoi terminal natif** | §I.2. |
| 9 | **Police monospace variable** | système + centrage / embarquée | **système + centrage mesuré**, police embarquée seulement si le rendu ASCII se révèle trop incohérent | Les glyphes Braille/blocs (le cœur visuel) sont en `fillRect`, indépendants de la police ; embarquer une police ajoute 20–40 Kio et une licence de police. |
| 10 | **Persistance du zoom** | `localStorage` / rien | **rien** (§E.8) | Dérivable, non déterministe, exceptions en iframe/privé. |
| 11 | **Thread du rendu** | main / Worker | **main thread + rAF** (§D.3) | 1 draw call ; le coût est dans le wasm, déplacer ne gagne rien tant qu'on reste sous 8 ms. |
| 12 | **Patch d'instances** | complet / tuiles sales | **complet en v1** | 138 Kio/frame à la cible ; le patch ajoute un chemin de code et des tests pour un gain non mesuré. |

### I.4 Décisions actées (2026-11-24)

Trois choix étaient ouverts en fin de rédaction ; ils sont tranchés par le titulaire des droits et deviennent des contraintes du projet, pas des recommandations.

| # | Décision actée le 2026-11-24 | Portée dans le document | Conséquences |
| --- | --- | --- | --- |
| 1 | **Licence MIT.** `LICENSE` MIT à la racine du dépôt, copyright 2026 `Holaf` (compte GitHub du dépôt `grokuku/Shooty`). Le blocage « licence absente » est levé. | §A.3, §I.1 (P4 licence, soldé), §I.3 ligne 3 | `license = "MIT"` dans les `Cargo.toml` du workspace, `"license": "MIT"` dans `web/package.json` ; `test3Dterminal/quake/` garde sa licence GPLv2 d'origine et reste non lié. |
| 2 | **Atlas = grille 32 × 16 slots.** 512 slots, `slot = 0..511`, `slot = slot_y·32 + slot_x` ; chaque slot fait `w × 2w` texels (aspect strict 0,5), d'où une texture `R8` de `(32·w) × (32·w)`. | §E.3 (formulation déjà conforme, marquée ACTÉ), §F.4, §I.1 (P3 ambiguïté, soldé) | `slot_x = slot & 31`, `slot_y = slot >> 5`, `uv = (vec2(slot_x, slot_y) + corner) * (1/vec2(32, 16))`. L'alternative « slot de 32 × 16 texels » est écartée. |
| 3 | **`test3Dterminal/` est GELÉ.** Il devient l'oracle de parité **immuable** des golden tests : aucune modification ultérieure du C (sources, `Makefile`, `tests/`, carte `relay.bsp`). | §H.2 (gel marqué ACTÉ), §I.1 (P4 « C modifié », garde-fou maintenu) | Le hash des sources C listées reste un test obligatoire ; toute évolution de comportement passe par une v2 des goldens et une levée explicite du gel, jamais par une retouche du C. |

---

### I.5 Amendements de la Phase 3 (implémentation web)

La Phase 3 (cible WASM + coquille `web/`) a été implémentée et vérifiée. Six points du
document se sont révélés faux, incomplets ou trop coûteux à tenir tels quels. Ils sont
amendés ici, avec leur justification ; le reste du document reste la référence.

| # | Point amendé | Constat d'implémentation | Amendement retenu |
| --- | --- | --- | --- |
| 1 | **Instance de 8 o : `slot` sur 8 bits** (§F.4) | Incompatible avec les **512 slots** fixés par §E.3 / §I.4 : 8 bits n'adressent que 256 slots. En release, `slot << 24` déborde silencieusement et les slots 256..=511 (Braille haut, blocs) se projettent sur des slots ASCII — glyphes parasites constatés à la première capture d'écran. | `slot` occupe **9 bits** : les 8 bits hauts de `word0` **plus le bit 31 de `word1`**. Les `flags` gardent leurs **4 bits définis** dans les bits 24..=27 de `word1`. Les 8 o/instance et les 24 bits par couleur sont préservés. Shader : `slot = (word0 >> 24) | ((word1 >> 31) << 8)`. |
| 2 | **Paliers de zoom** `{4,6,8,10,12,16,20,24,32}` (§E.2) | À `4 px`, `1920 × 1080` demande 64 800 cellules — coût du ciel par échantillon (§I.1 P2) déraisonnable pour une démo. | Paliers `{8,10,12,14,16,20,24,32,40,48,64}` (11 paliers), palier auto = hauteur de cellule 20 px device. La propriété « palier entier en pixels device » (justification du discret, §E.2.1–4) est conservée. Constante unique : `astre_core::zoom::ZOOM_LEVELS`. |
| 3 | **Pas de `localStorage`** (§E.8) | Le zoom d'affichage est une préférence utilisateur stable d'une session à l'autre ; sa non-persistance a été jugée frustrante pour un rendu zoomable. | Zoom persisté sous `localStorage["astre.cellPx"]`, **avec** les précautions de §E.8.3 : lecture/écriture en `try/catch`, validation (entier `4..=512`), valeur invalide ignorée, aucune dépendance de Rust à la persistance. |
| 4 | **`Instant::now()` comme horloge** (§D.6, implicite) | `std::time::Instant::now()` **trappe** (`unreachable`) sur `wasm32-unknown-unknown` : la PAL `std` de cette cible n'a pas d'horloge. Tout `astre-app` tel quel était inutilisable depuis le wasm. | Horloge **injectée** : `astre_app::clock::Clock` (`now_us() -> u64`), avec `StdClock` (natif, `Instant`), `NullClock` (snapshots déterministes) et `PlatformClock` = `performance.now()` importé par `wasm-bindgen` côté wasm (ni `web-sys` ni `js-sys`). `Metrics` date sa fenêtre en microsecondes monotones fournies par l'appelant. |
| 5 | **API `App::new()` sans argument** (§F.2) | La coquille a besoin de transmettre le zoom persisté et la cadence avant la première frame ; `push_resize`/`push_zoom` avant `frame` ne portaient pas la préférence. | `App::new(config_json: &str)` avec un JSON **plat** (`{"cellPx":16,"tickHz":60,"mode":"braille","hud":true}`), parsé sans `serde` (parseur maison ~120 l., clés inconnues ignorées, aucune valeur refusée). `stats_json()` remplace les getters multiples pour le HUD DOM (appel à 4 Hz, hors rAF). |
| 6 | **`panic` traversant / `catch_unwind`** (§F.8) | `panic = "abort"` : un panic wasm trappe l'instance, `catch_unwind` n'a rien à attraper (et `wasm32-unknown-unknown` ne déroule pas les piles). | Stratégie « ne jamais paniquer » : entrées validées et bornées, aucun `unwrap`/`expect`, erreurs **récupérables** (`last_error()`, bit `ERROR`, état sain conservé, frame suivante normale). Documenté dans `crates/astre-web/src/lib.rs`. |

Non amendé, confirmé : **thread principal + `requestAnimationFrame`**, pas de Worker
(§D.3 — le coût est dans le wasm, un Worker ajouterait un protocole de messages pour un
gain nul à cette échelle) ; **pas de `web-sys`/`js-sys`** ; **pas de repli Canvas2D**
(§I.2) ; **ré-upload complet** des instances en v1 (§I.4 décision n°12) ; **C gelé**.

Chiffres mesurés en Phase 3 (`[à mesurer]` levés pour ces points) : `.wasm` release
**114 316 o** (~112 Kio, soit bien en dessous des 150–400 Kio estimés, `opt-level = 3`,
`lto = "thin"`, symbols strippés par `wasm-pack`) ; atlas `R8` de 64 Kio à 1 Mio selon le
palier ; à 128 × 36 cellules, `raster` ≈ 1,1 ms et `resolve` ≈ 0,8 ms sur la machine de
développement **en rendu logiciel** (SwiftShader), `built_fps ≈ 60` en GPU réel.

---

## Résumé

### Sections

| § | Contenu |
| --- | --- |
| **A** | Contexte : le C de référence (7 fichiers, 1 119 lignes, grille 2 × 4, profondeur 1/z, ciel par échantillon, boucle à `dt` variable), objectifs v1, non-objectifs, contraintes dures (1 M cellules, `Cell` 16 o, 8 o/instance, aspect strict). |
| **B** | 6 crates (`core` ← `scene` ← `app` ← `term`/`web`, + `native`) et la coquille TS `web/` ; règles de dépendance et interdits structurels (pas de feature pour la cible, pas de `web-sys`, pas de global). |
| **C** | Contrats : `Cell`/`CellRect`/`SurfaceView`/`FrameMetrics`, `Renderer{resize, begin, triangle, resolve, cells, cells_mut}`, `Presenter{configure, available_px, present}`, `EventSource` (push web / pull terminal), `CellBuffer` (stride explicite, dirty 8 × 4, `Preserve`/`Clear`, `epoch`, 8 invariants). |
| **D** | Pas fixe (`tick_hz` configurable, `MAX_CATCHUP = 4`, clamp 0,25 s), interpolation (angle court), pas de verrou vsync ni sommeil, main thread + rAF arbitré, compteurs `sim_hz`/`built_fps`/`presented_fps` et ms par phase, overlay `P`/`F3` dans les cellules. |
| **E** | Zoom = 9 paliers entiers en pixels device (`4…32`, hauteur = 2 × largeur → points Braille carrés), atlas régénéré par palier (256² à 1024², `R8`), letterbox, DPR, clamp de budget sans erreur, raccourcis, pas de `localStorage`. |
| **F** | `wasm-pack --target web`, 1 appel `frame` par frame, vue sur `WebAssembly.Memory` + `bufferSubData` (zéro copie, pièges de détachement/pointeur), instance 8 o, clavier → `VecDeque`, `ResizeObserver`, erreurs explicites, `Result` sans panic, build CI avec garde-fous de dépendances, taille wasm `[à mesurer]`. |
| **G** | Correspondance fichier → module avec les vrais symboles C, 14 pièges `f32`/indices/NaN vérifiés, `std::env` plutôt que `clap`, `libm` pour le déterminisme, 6 comportements implicites du C à préserver. |
| **H** | Unitaires core, golden `CellBuffer` contre le C (oracle gelé) + procédure de régénération, resize/zoom/clamp, diff ANSI octet à octet, snapshot headless + allocations, pas de Playwright en CI (justifié), tests wasm en Node. |
| **I** | 17 risques priorisés, chiffrage du fallback Canvas2D, 12 choix ouverts avec recommandation par défaut, 3 décisions actées le 2026-11-24 (§I.4). |

### Les 5 décisions les plus engageantes

1. **Le `Presenter` web n'est pas du Rust.** `astre-web` n'expose que des pointeurs et de la géométrie ; le GPU est piloté par `web/src/presenter.ts`. C'est ce qui permet de tenir « zéro `web-sys` », mais cela crée une frontière ABI qu'il faudra traiter avec le même soin qu'une API publique (§C.3, §F.2–F.4).
2. **Pas fixe + interpolation, et plus aucun sommeil.** La simulation devient déterministe et testable (oracle reproductible), au prix d'un écart assumé avec le C (qui avançait au `dt` réel et dormait pour tenir `--fps`). Tout le reste du timing (compteurs, overlay) en découle (§D).
3. **La parité avec le C est un critère de recette, pas une intention.** Goldens texte sur 12 cas, glyphes stricts, couleurs tolérantes ±1 ; le C est gelé comme oracle. Cela contraint les choix d'implémentation (`f32` partout, `trunc_u8_sat`, ordre des sommes, `libm`) et rend le portage vérifiable (§G.3, §H.2).
4. **Le zoom est un ladder discret en pixels device, lié à l'atlas.** Il garantit l'alignement exact échantillon ↔ glyphe, rend les points Braille carrés, borne le coût à 9 configurations testables et rend le clamp de budget trivial. Le continu, plus « naturel », aurait cassé l'alignement et multiplié les configurations (§E).
5. **`astre-core` est `no_std` + `libm`, sans état global.** C'est ce qui rend le déterminisme possible, les tests parallèles sûrs, et interdit structurellement les raccourcis du C (`static lookup`, signaux globaux). C'est aussi la contrainte qui coûte le plus cher en discipline (pas d'`unwrap`, pas de `web-sys`, pas de feature de cible) (§B.5, §C.5.6, §G.7).

### Questions restantes pour l'utilisateur

Les trois premières questions sont **closes depuis le 2026-11-24** (§I.4) ; les suivantes restent ouvertes.

1. **Licence** : qui est titulaire des droits sur `test3Dterminal/` (aucun fichier de licence, ni racine ni dans le dossier), et sous quelle licence veut-on placer le portage Rust ? → **Tranché le 2026-11-24** : titulaire `Holaf`, licence **MIT**, `LICENSE` créé à la racine (§I.4).
2. **« 512 slots 32 × 16 »** : confirmez-vous la lecture « grille de 32 × 16 slots = 512, chaque slot `w × 2w` texels » (§E.3) ? L'alternative serait « slot de 32 × 16 texels », incompatible avec l'aspect strict 0,5 sans transposition. → **Acté le 2026-11-24** : oui, grille 32 × 16 slots, slot `w × 2w` texels (§E.3, §I.4).
3. **Le C reste-t-il gelé** comme oracle de parité, ou prévoyez-vous de le faire évoluer (ce qui rendrait les goldens C obsolètes et forcerait à figer un tag) ? → **Acté le 2026-11-24** : oui, `test3Dterminal/` est GELÉ, oracle de parité immuable (§H.2, §I.4).
4. **Écart de raccourcis assumé** : `+`/`-` passent de « rayon caméra » (C) à « zoom d'affichage » (web), la molette prenant le dolly. Confirmez-vous, y compris pour le binaire natif (§E.7) ?
5. **Cible de performance** : quelle cadence et quelle taille de grille doivent être « confortables » sur la machine de référence (portable, navigateur) ? Cela décide s'il faut envisager l'optimisation du ciel (§I.1, P2) dès la v1 ou après.
6. **Terminal natif en v1** : `crossterm` est-il accepté comme unique dépendance de `astre-term` (il apporte ~10 crates transitives `[à mesurer]`), ou préférez-vous conserver l'implémentation `termios`/`WriteConsoleW` écrite à la main pour rester à zéro dépendance ?
7. **Golden visuel de référence** : souhaitez-vous que `apercu.png` (l'image du README C) serve de référence visuelle de la v1 web, ou le rendu web peut-il légitimement différer (police, échelle) dès lors que la géométrie et les glyphes sont conformes ?
