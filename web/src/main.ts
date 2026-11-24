/**
 * Coquille web d'Astre — §B.3, §D.3, §F.7.
 *
 * Rôle volontairement mince : (1) charger le wasm, (2) appeler `App.frame(now)` à
 * chaque `requestAnimationFrame`, (3) dessiner le buffer d'instances que Rust lui donne,
 * (4) pousser les événements. La coquille ne connaît **ni** la 3D, **ni** la résolution
 * de cellules, **ni** le timing : tout cela vit dans `astre-web`.
 *
 * Pas de bundler : modules ES natifs, `wasm-bindgen --target web`, `tsc` en simple
 * transpilation. Aucun framework CSS ni JS.
 */

import init, { App } from '../pkg/astre_web.js';
import type { InitOutput } from '../pkg/astre_web.js';
import { buildAtlas } from './atlas.js';
import { attachInput } from './input.js';
import { Presenter, readInfo } from './presenter.js';

/** Bits de retour de `App::frame` (miroir de `astre_web::frame_bits`). */
const FRAME_RENDERED = 1 << 0;
const FRAME_ERROR = 1 << 2;

/** Clé de persistance du zoom (décision de la Phase 3, écart assumé avec §E.8). */
const STORAGE_CELL_PX = 'astre.cellPx';
/** Rebuild d'atlas débouncé (§E.3). */
const ATLAS_DEBOUNCE_MS = 60;
/** Période de rafraîchissement du HUD DOM (4 Hz, hors rAF). */
const HUD_INTERVAL_MS = 250;
/** Délai maximal du chargement wasm. */
const BOOT_TIMEOUT_MS = 15_000;

/** Compteurs lus de `stats_json()` (sous-ensemble utilisé par le HUD). */
interface Stats {
  cols: number;
  rows: number;
  cellPx: number;
  cellPxH: number;
  cells: number;
  dirtyTiles: number;
  epoch: number;
  tickHz: number;
  builtFps: number;
  presentedFps: number;
  rasterMs: number;
  resolveMs: number;
}

/**
 * Point d'entrée. Toute erreur de démarrage produit un panneau DOM en français, jamais
 * un `alert` ni un écran vide (§F.7).
 */
async function boot(): Promise<void> {
  const canvas = document.getElementById('view');
  const stage = document.getElementById('stage');
  const hud = document.getElementById('hud');
  if (!(canvas instanceof HTMLCanvasElement) || stage === null || hud === null) {
    showError('BOOT_FAILED', 'La page est incomplète : canvas, conteneur ou HUD introuvable.');
    return;
  }

  // 1. WebGL2 : pas de repli Canvas2D (§I.2, décision actée).
  const gl = canvas.getContext('webgl2', {
    alpha: false,
    antialias: false,
    depth: false,
    stencil: false,
    premultipliedAlpha: false,
    preserveDrawingBuffer: false,
    powerPreference: 'high-performance',
  });
  if (gl === null) {
    showError(
      'WEBGL2_UNAVAILABLE',
      "Ce navigateur n'expose pas WebGL2. Le moteur Astre fonctionne aussi en terminal " +
        "natif : ./astre (crate astre-native). Aucun repli Canvas2D n'est fourni.",
    );
    return;
  }

  // 2. Chargement du wasm, borné dans le temps.
  let wasm: InitOutput;
  try {
    wasm = await withTimeout(init(), BOOT_TIMEOUT_MS, 'WASM_FETCH_FAILED');
  } catch (error) {
    showError('WASM_FETCH_FAILED', describe(error));
    return;
  }

  // 3. Application Rust : le zoom persisté (localStorage) entre par la configuration.
  let app: App;
  try {
    app = new App(JSON.stringify(configFromStorage()));
  } catch (error) {
    showError('BOOT_FAILED', describe(error));
    return;
  }

  // 4. Présentateur + atlas (rebuild débouncé sur changement de palier / de DPR).
  //
  // Piège vérifié à la main : `ensureAtlas` est appelée à **chaque frame** tant que
  // l'atlas n'est pas à jour. Ré-armer le timer à chaque appel (sans mémoriser la
  // cible déjà programmée) annule le débounce 60 ms avant qu'il n'expire à 60 Hz : le
  // timer ne se déclenche jamais et le canvas reste noir. D'où `wantedCellW`.
  let atlasTimer: number | null = null;
  let wantedCellW = 0;
  let presenter: Presenter;
  const buildAtlasNow = (cellW: number): void => {
    try {
      presenter.setAtlas(buildAtlas(cellW), cellW);
      persistCellPx(cellW);
    } catch (error) {
      console.error('atlas : reconstruction impossible', error);
    }
  };
  const ensureAtlas = (cellW: number): void => {
    if (cellW <= 0 || cellW === presenter.atlasCellPx) return;
    // Premier atlas : construction immédiate, pas de frame noire au démarrage.
    if (presenter.atlasCellPx === 0) {
      buildAtlasNow(cellW);
      return;
    }
    // Déjà programmé pour ce palier : ne pas repousser l'échéance.
    if (atlasTimer !== null && cellW === wantedCellW) return;
    wantedCellW = cellW;
    if (atlasTimer !== null) window.clearTimeout(atlasTimer);
    atlasTimer = window.setTimeout(() => {
      atlasTimer = null;
      const target = wantedCellW;
      wantedCellW = 0;
      buildAtlasNow(target);
    }, ATLAS_DEBOUNCE_MS);
  };
  presenter = new Presenter(gl, canvas, () => {
    // Contexte GL restauré : programme, VAO, VBO recréés ; l'atlas est à refaire.
    ensureAtlas(app.cell_px());
  });

  // 5. Taille du canvas en pixels device, puis première grille AVANT la première frame
  //    (sinon une frame à 1 × 1 cellule serait présentée, §F.6).
  const applySize = (): void => {
    const dpr = window.devicePixelRatio > 0 ? window.devicePixelRatio : 1;
    const rect = stage.getBoundingClientRect();
    const cssW = Math.max(1, rect.width);
    const cssH = Math.max(1, rect.height);
    const deviceW = Math.max(1, Math.round(cssW * dpr));
    const deviceH = Math.max(1, Math.round(cssH * dpr));
    canvas.width = deviceW;
    canvas.height = deviceH;
    canvas.style.width = `${cssW}px`;
    canvas.style.height = `${cssH}px`;
    app.set_viewport_px(deviceW, deviceH);
  };
  const observer = new ResizeObserver(applySize);
  observer.observe(stage);
  applySize();
  watchDpr(applySize);

  // 6. Clavier (hors boucle rAF, §F.5).
  attachInput(window, {
    onKey: (code, down) => {
      app.push_key(code, down);
    },
    onToggleHud: () => {
      hud.classList.toggle('hidden');
    },
    onFullscreen: () => {
      void toggleFullscreen();
    },
  });

  // 7. Boucle : `requestAnimationFrame`, pas de verrou 60 Hz (§D.2, §D.3).
  let lastHudUpdate = 0;
  let lastUploadUs = 0;
  let lastDrawUs = 0;

  const runFrame = (now: number): void => {
    const bits = app.frame(now);
    if ((bits & FRAME_ERROR) !== 0) {
      showError('RUNTIME_ERROR', app.last_error());
      return;
    }
    if ((bits & FRAME_RENDERED) !== 0) {
      const info = readInfo(wasm.memory, app.info_ptr());
      // L'atlas doit correspondre au palier courant : rebuild débouncé si besoin.
      if (info.cellW !== presenter.atlasCellPx) ensureAtlas(info.cellW);
      const timings = presenter.draw(info, wasm.memory);
      lastUploadUs = timings.uploadUs;
      lastDrawUs = timings.drawUs;
    }
    if (now - lastHudUpdate >= HUD_INTERVAL_MS) {
      lastHudUpdate = now;
      updateHud(hud, app, lastUploadUs, lastDrawUs);
    }
    requestAnimationFrame(runFrame);
  };
  requestAnimationFrame(runFrame);
}

/** Met à jour le HUD DOM (4 Hz) : fps, ms par phase, cellules, rappel des touches. */
function updateHud(hud: HTMLElement, app: App, uploadUs: number, drawUs: number): void {
  let stats: Stats;
  try {
    stats = JSON.parse(app.stats_json()) as Stats;
  } catch {
    return;
  }
  const upload = (uploadUs / 1000).toFixed(2);
  const draw = (drawUs / 1000).toFixed(2);
  hud.textContent = [
    `SIM ${stats.tickHz.toFixed(1)}  BUILD ${stats.builtFps.toFixed(1)}  PRES ${stats.presentedFps.toFixed(1)}`,
    `RAST ${stats.rasterMs.toFixed(2)}  RES ${stats.resolveMs.toFixed(2)}  UP ${upload}  DRAW ${draw} ms`,
    `CELLS ${stats.cells}  TILES ${stats.dirtyTiles}  ZOOM ${stats.cellPx}×${stats.cellPxH}  EPOCH ${stats.epoch}`,
    '+/- zoom  R reset  P HUD  F plein écran',
  ].join('\n');
}

/** Configuration de démarrage : zoom persisté (si présent) + valeurs par défaut. */
function configFromStorage(): Record<string, number | string> {
  const config: Record<string, number | string> = { tickHz: 60, mode: 'braille' };
  const stored = readStoredCellPx();
  if (stored !== null) config['cellPx'] = stored;
  return config;
}

/**
 * Zoom persisté (`localStorage["astre.cellPx"]`).
 *
 * Écart assumé avec §E.8, qui ne voulait **rien** persister : la Phase 3 demande la
 * persistance. D'où les précautions de §E.8.3 : lecture/écriture en `try/catch` (iframe
 * `file://`, navigation privée), validation stricte (entier borné) et valeur invalide
 * ignorée.
 */
function readStoredCellPx(): number | null {
  try {
    const raw = window.localStorage.getItem(STORAGE_CELL_PX);
    if (raw === null) return null;
    const value = Number.parseInt(raw, 10);
    if (!Number.isFinite(value) || value < 4 || value > 512) return null;
    return value;
  } catch {
    return null;
  }
}

/** Écrit le zoom courant, silencieusement (§E.8.3). */
function persistCellPx(cellPx: number): void {
  try {
    window.localStorage.setItem(STORAGE_CELL_PX, String(cellPx));
  } catch {
    /* contexte contraint : le zoom reste fonctionnel pour la session. */
  }
}

/**
 * Surveille `devicePixelRatio` via `matchMedia('(resolution: Xdppx)')` : un zoom
 * navigateur ou un changement d'écran ne déclenche pas de `ResizeObserver`, mais change
 * le DPR (§E.4, §F.6). La requête est ré-armée après chaque changement.
 */
function watchDpr(onChange: () => void): void {
  const dpr = window.devicePixelRatio > 0 ? window.devicePixelRatio : 1;
  const query = window.matchMedia(`(resolution: ${dpr}dppx)`);
  const handler = (): void => {
    query.removeEventListener('change', handler);
    onChange();
    watchDpr(onChange);
  };
  query.addEventListener('change', handler);
}

/** Plein écran (F) ; `Échap` est géré par le navigateur. */
async function toggleFullscreen(): Promise<void> {
  try {
    if (document.fullscreenElement === null) {
      await document.documentElement.requestFullscreen();
    } else {
      await document.exitFullscreen();
    }
  } catch (error) {
    console.warn('plein écran refusé', error);
  }
}

/** Course contre une promesse, avec message d'erreur explicite. */
function withTimeout<T>(promise: Promise<T>, timeoutMs: number, code: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => {
      reject(new Error(`${code} : délai de ${timeoutMs} ms dépassé`));
    }, timeoutMs);
    promise.then(
      (value) => {
        window.clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        window.clearTimeout(timer);
        reject(error instanceof Error ? error : new Error(describe(error)));
      },
    );
  });
}

/** Message lisible d'une valeur rejetée. */
function describe(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}

/**
 * Panneau d'erreur DOM (aucun `alert`, §F.7) : titre en français, code, détail, et
 * rappel que le terminal natif reste disponible.
 */
function showError(code: string, detail: string): void {
  let panel = document.getElementById('error');
  if (panel === null) {
    panel = document.createElement('div');
    panel.id = 'error';
    document.body.appendChild(panel);
  }
  const title = document.createElement('h1');
  title.textContent = 'Astre — démarrage impossible';
  const message = document.createElement('p');
  message.textContent = `[${code}] ${detail}`;
  const hint = document.createElement('p');
  hint.className = 'hint';
  hint.textContent =
    'Le moteur fonctionne aussi en terminal natif : cargo run -p astre-native --release';
  panel.replaceChildren(title, message, hint);
  console.error(`[${code}] ${detail}`);
}

// Aucune rejection non gérée ne doit rester silencieuse.
window.addEventListener('unhandledrejection', (event) => {
  showError('BOOT_FAILED', describe(event.reason));
});

void boot();
