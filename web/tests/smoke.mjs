/**
 * Smoke test de l'ABI wasm **sans navigateur** — §H.7.
 *
 * Usage (après `npm run build:wasm`) :
 *
 * ```sh
 * node web/tests/smoke.mjs
 * ```
 *
 * Il vérifie ce qu'un test de navigateur ne vérifierait pas mieux et moins vite :
 * création/erreurs, géométrie de zoom, bits de `frame`, packing des instances, JSON des
 * compteurs. Aucune dépendance : `node:fs` et l'ESM généré par `wasm-pack`.
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import init, { App } from '../pkg/shooty_web.js';

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '..', 'pkg', 'shooty_web_bg.wasm');

/** Bits de `App::frame` (miroir de `het_web::frame_bits`). */
const RENDERED = 1 << 0;
const SKIPPED = 1 << 1;
const ERROR = 1 << 2;

let failures = 0;

/** Assertion minimale, sans framework. */
function check(label, condition, detail = '') {
  if (condition) {
    console.log(`  ok   ${label}`);
  } else {
    failures += 1;
    console.log(`  FAIL ${label}${detail === '' ? '' : ` — ${detail}`}`);
  }
}

/** Laisse tourner l'horloge réelle (les compteurs ont une fenêtre de 0,5 s). */
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const wasm = await init({ module_or_path: readFileSync(wasmPath) });

console.log('— cycle de vie —');
const app = new App('{"cellPx":10,"tickHz":60,"mode":"braille"}');
check('App::new accepte une configuration JSON', app.atlas_slot_count() === 512);
let threw = false;
try {
  new App('pas du json');
} catch {
  threw = true;
}
check('App::new rejette un document non objet', threw);

console.log('— géométrie —');
app.set_viewport_px(1280, 720);
let bits = app.frame(16.7);
check('frame() rend une image', (bits & RENDERED) === RENDERED, `bits=${bits}`);
check('aucune erreur au premier frame', (bits & ERROR) === 0, app.last_error());

const words = app.info_words();
check('bloc PresentInfo = 16 mots', words === 16, `${words}`);
const info = new Uint32Array(wasm.memory.buffer, app.info_ptr(), words);
const [cellsPtr, cellsLen, cols, rows, cellW, cellH, originX, originY, availW, availH] = info;
check('cellule 10 × 20 (aspect strict 0,5)', cellW === 10 && cellH === 20, `${cellW}×${cellH}`);
check('colonnes × lignes cohérentes', cols === 128 && rows === 36, `${cols}×${rows}`);
check('cellules = cols × rows', cols * rows === 4608, `${cols * rows}`);
check('buffer d’instances = 8 o par cellule', cellsLen === cols * rows * 8, `${cellsLen}`);
check('pointeur dans la mémoire wasm', cellsPtr > 0 && cellsPtr + cellsLen <= wasm.memory.buffer.byteLength);
check('letterbox : origine négative impossible', originX >= 0 && originY >= 0, `${originX},${originY}`);
check('surface vue = canvas device', availW === 1280 && availH === 720);

console.log('— instances —');
const instances = new Uint32Array(wasm.memory.buffer, cellsPtr, cellsLen >>> 2);
let glyphSlots = 0;
for (let i = 0; i < instances.length; i += 2) {
  const slot = (instances[i] >>> 24) | (((instances[i + 1] >>> 31) & 1) << 8);
  if (slot < 512) glyphSlots += 1;
}
check('tous les slots décodés sont < 512', glyphSlots === instances.length / 2, `${glyphSlots}`);
check('des cellules non vides existent', instances.some((w) => w !== 0));

console.log('— zoom —');
app.set_cell_px(8);
check('palier minimal appliqué', app.cell_px() === 8, `${app.cell_px()}`);
for (let i = 0; i < 20; i += 1) app.push_key(43, true); // '+'
check('borne haute 64 après 20 crans', app.cell_px() === 64, `${app.cell_px()}`);
for (let i = 0; i < 40; i += 1) app.push_key(45, true); // '-'
check('borne basse 8 après 40 crans', app.cell_px() === 8, `${app.cell_px()}`);
app.push_key(48, true); // '0' : palier automatique
check('« 0 » revient au palier automatique', app.cell_px() === 10, `${app.cell_px()}`);

console.log('— entrées et erreurs —');
app.push_key(259, true); // flèche gauche
app.push_key(256, true); // flèche haut
app.push_key(0, true); // code inconnu : ignoré
app.push_key(43, false); // relâchement : ignoré
bits = app.frame(33.4);
check('frame après touches : rendu', (bits & RENDERED) === RENDERED, `bits=${bits}`);
check('aucune erreur après touches invalides', (bits & ERROR) === 0, app.last_error());
const nanBits = app.frame(Number.NaN);
check('now_ms = NaN → bit ERROR', (nanBits & ERROR) === ERROR);
check('last_error() renseigné', app.last_error().length > 0);
bits = app.frame(50.1);
check('l’App reste utilisable après erreur', (bits & RENDERED) === RENDERED, `bits=${bits}`);

console.log('— compteurs —');
app.set_viewport_px(1280, 720);
for (let i = 0; i < 40; i += 1) {
  app.frame(100 + i * 16.7);
  await sleep(20);
}
const raw = app.stats_json();
const stats = JSON.parse(raw);
check('stats_json() est du JSON valide', typeof stats === 'object' && stats !== null);
check('JSON sans NaN/Infinity', !/NaN|Infinity/.test(raw));
check('builtFps publié après la fenêtre 0,5 s', stats.builtFps > 0, `${stats.builtFps}`);
check('presentedFps ≤ builtFps', stats.presentedFps <= stats.builtFps + 0.001);
check('cellules rapportées', stats.cells === 4608, `${stats.cells}`);
check('epoch monotone', stats.epoch >= 1);
check('atlasSlots = 512', stats.atlasSlots === 512);

console.log('');
if (failures === 0) {
  console.log('smoke : OK');
} else {
  console.log(`smoke : ${failures} échec(s)`);
  process.exitCode = 1;
}
