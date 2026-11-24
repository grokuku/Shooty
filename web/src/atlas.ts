/**
 * Atlas de glyphes — §E.3 / §I.4 décision n°2.
 *
 * 512 slots en grille **32 × 16** (`slot_x = slot & 31`, `slot_y = slot >> 5`), chaque
 * slot mesurant `w × 2w` texels (aspect strict 0,5). La texture est en `R8` (une valeur
 * de couverture par texel) : `0` = fond, `255` = encre. Le fragment shader mélange
 * `mix(bg, fg, texel)`, il n'y a donc **ni alpha ni blending** côté GL.
 *
 * Occupation (contrat d'ABI avec `crates/astre-web/src/instances.rs`) :
 *
 * | Slots | Contenu | Codepoints |
 * | --- | --- | --- |
 * | 0..=95 | ASCII imprimable (`fillText`) | `U+0020..=U+007E` |
 * | 96..=351 | Braille (`fillRect`, 2×4 points) | `U+2800..=U+28FF` |
 * | 352..=383 | Blocs (`fillRect`) | `U+2580..=U+259F` |
 * | 384..=511 | réservé | — |
 *
 * Les glyphes Braille et blocs sont **dessinés procéduralement** sur la grille 2 × 4 de
 * la cellule : c'est ce qui garantit l'alignement exact échantillon ↔ glyphe exigé par
 * §E.2, indépendamment de la police (risque P2 de §I.1).
 */

/** Slots par ligne de l'atlas (constante d'ABI, §E.3). */
export const SLOT_COLS = 32;
/** Lignes de slots de l'atlas (constante d'ABI). */
export const SLOT_ROWS = 16;
/** Nombre total de slots (`32 × 16`). */
export const ATLAS_SLOTS = SLOT_COLS * SLOT_ROWS;
/** Premier slot ASCII (`U+0020`). */
export const ASCII_FIRST = 0;
/** Premier slot Braille (`U+2800`). */
export const BRAILLE_FIRST = 96;
/** Premier slot des blocs (`U+2580`). */
export const BLOCK_FIRST = 352;

/** Pile monospace : aucune police embarquée en v1 (§I.3 ligne 9). */
const FONT_STACK =
  'ui-monospace, "SF Mono", "Cascadia Mono", "DejaVu Sans Mono", "Liberation Mono", monospace';

/** Pixels de l'atlas : couverture `R8` prête pour `texImage2D`. */
export interface AtlasPixels {
  /** Largeur en texels (`32 × cellW`). */
  readonly width: number;
  /** Hauteur en texels (`32 × cellW`, car `16 × 2·cellW`). */
  readonly height: number;
  /** Couverture, `width × height` octets, ligne par ligne. */
  readonly data: Uint8Array;
}

/** Contexte 2D hors écran (OffscreenCanvas si disponible, sinon canvas détaché). */
function makeCanvas(width: number, height: number): {
  ctx: CanvasRenderingContext2D;
  canvas: HTMLCanvasElement | OffscreenCanvas;
} {
  if (typeof OffscreenCanvas !== 'undefined') {
    const canvas = new OffscreenCanvas(width, height);
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    if (ctx === null) throw new Error('atlas : contexte 2D indisponible (OffscreenCanvas)');
    return { ctx: ctx as unknown as CanvasRenderingContext2D, canvas };
  }
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (ctx === null) throw new Error('atlas : contexte 2D indisponible');
  return { ctx, canvas };
}

/** Position `(x, y)` du coin haut-gauche d'un slot, en texels. */
export function slotOrigin(slot: number, cellW: number): { x: number; y: number } {
  const sx = slot % SLOT_COLS;
  const sy = Math.floor(slot / SLOT_COLS);
  return { x: sx * cellW, y: sy * cellW * 2 };
}

/**
 * Construit l'atlas pour une largeur de cellule donnée (pixels device = texels).
 *
 * `cellW` doit être un palier entier (`ZOOM_LEVELS`) : c'est ce qui rend les points
 * Braille carrés (`w/2 × w/2`) et les bords de glyphes alignés sur des texels entiers.
 */
export function buildAtlas(cellW: number): AtlasPixels {
  const cellH = cellW * 2;
  const width = SLOT_COLS * cellW;
  const height = SLOT_ROWS * cellH;
  const { ctx } = makeCanvas(width, height);

  ctx.fillStyle = '#000';
  ctx.fillRect(0, 0, width, height);
  ctx.fillStyle = '#fff';
  ctx.strokeStyle = '#fff';
  ctx.textAlign = 'left';
  ctx.textBaseline = 'alphabetic';

  drawAscii(ctx, cellW, cellH);
  drawBraille(ctx, cellW, cellH);
  drawBlocks(ctx, cellW, cellH);

  const rgba = ctx.getImageData(0, 0, width, height).data;
  const data = new Uint8Array(width * height);
  for (let i = 0, j = 0; i < data.length; i += 1, j += 4) {
    data[i] = rgba[j]!;
  }
  return { width, height, data };
}

/**
 * ASCII `U+0020..=U+007E` dans les slots 0..=95, via `fillText`.
 *
 * Chaque glyphe est **centré** dans son slot (mesure `measureText`) et la taille de
 * police est réduite si l'avance monospace dépasse la largeur de cellule : c'est la
 * parade « police système variable » de §I.1 P2.
 */
function drawAscii(ctx: CanvasRenderingContext2D, cellW: number, cellH: number): void {
  let fontSize = Math.max(6, Math.round(cellH * 0.9));
  ctx.font = `${fontSize}px ${FONT_STACK}`;
  const probe = ctx.measureText('M');
  const advance = probe.width > 0 ? probe.width : cellW;
  if (advance > cellW) {
    fontSize = Math.max(4, Math.floor((fontSize * cellW) / advance));
    ctx.font = `${fontSize}px ${FONT_STACK}`;
  }
  // Ligne de base : le glyphe occupe ~72 % de la hauteur sous la ligne de base.
  const baseline = Math.round((cellH + fontSize * 0.72) / 2);
  for (let code = 0x20; code <= 0x7e; code += 1) {
    if (code === 0x20) continue; // espace : slot 0 volontairement vide
    const slot = ASCII_FIRST + (code - 0x20);
    const { x, y } = slotOrigin(slot, cellW);
    const ch = String.fromCharCode(code);
    const width = ctx.measureText(ch).width;
    const dx = Math.round(x + (cellW - width) / 2);
    ctx.fillText(ch, dx, y + baseline);
  }
}

/**
 * Braille `U+2800..=U+28FF` dans les slots 96..=351.
 *
 * Correspondance bit ↔ point, **identique au C** (`term3d.c:dots[]` =
 * `{1,8,2,16,4,32,64,128}` indexé par `2·rang + colonne`) : le bit `b` allume le point
 * `(colonne, rang)` donné par les tables ci-dessous. Le glyphe est donc exactement la
 * grille 2 × 4 de la cellule, point par point.
 */
function drawBraille(ctx: CanvasRenderingContext2D, cellW: number, cellH: number): void {
  const dotW = cellW / 2;
  const dotH = cellH / 4;
  for (let mask = 0; mask < 256; mask += 1) {
    const slot = BRAILLE_FIRST + mask;
    const { x, y } = slotOrigin(slot, cellW);
    for (let bit = 0; bit < 8; bit += 1) {
      if ((mask & (1 << bit)) === 0) continue;
      const col = BRAILLE_DOT_COL[bit]!;
      const row = BRAILLE_DOT_ROW[bit]!;
      ctx.fillRect(x + col * dotW, y + row * dotH, dotW, dotH);
    }
  }
}

/** Colonne d'un point Braille, indexée par le bit standard (`0..=7`). */
const BRAILLE_DOT_COL = [0, 0, 0, 1, 1, 1, 0, 1];
/** Rangée d'un point Braille, indexée par le bit standard (`0..=7`). */
const BRAILLE_DOT_ROW = [0, 1, 2, 0, 1, 2, 3, 3];

/**
 * Blocs `U+2580..=U+259F` dans les slots 352..=383.
 *
 * Les 16 codepoints que `resolve_cell` peut émettre en v1 (`▘▝▀▖▌▞▛▗▚▐▜▄▙▟█` + espace)
 * sont dessinés **exactement** sur la grille 2 × 4 (masque de 8 bits, même numérotation
 * que `resolve_cell`). Les blocs fractionnaires (`▁▂▃▅▆▇▉…`) ne sont **jamais produits**
 * par le résolveur v1 : ils sont tout de même dessinés, arrondis au huitième de cellule,
 * pour que l'atlas reste complet si un HUD futur les utilise.
 */
function drawBlocks(ctx: CanvasRenderingContext2D, cellW: number, cellH: number): void {
  for (let i = 0; i < 32; i += 1) {
    const code = 0x2580 + i;
    const slot = BLOCK_FIRST + i;
    const { x, y } = slotOrigin(slot, cellW);
    const mask = BLOCK_MASKS.get(code);
    if (mask !== undefined) {
      fillSampleMask(ctx, x, y, cellW, cellH, mask);
      continue;
    }
    const rect = BLOCK_RECTS.get(code);
    if (rect !== undefined) {
      const [rx, ry, rw, rh] = rect;
      ctx.fillRect(
        x + Math.round(rx * cellW),
        y + Math.round(ry * cellH),
        Math.round(rw * cellW),
        Math.round(rh * cellH),
      );
    }
  }
}

/** Masques 2 × 4 (bit `2·rang + colonne`) des blocs alignés sur la grille d'échantillons. */
const BLOCK_MASKS = new Map<number, number>([
  [0x2580, 0x0f], // ▀ upper half
  [0x2584, 0xf0], // ▄ lower half
  [0x2588, 0xff], // █ full
  [0x258c, 0x55], // ▌ left half
  [0x2590, 0xaa], // ▐ right half
  [0x2596, 0x50], // ▖ lower left
  [0x2597, 0xa0], // ▗ lower right
  [0x2598, 0x05], // ▘ upper left
  [0x2599, 0xf5], // ▙ left half + lower right
  [0x259a, 0xa5], // ▚ upper left + lower right
  [0x259b, 0x5f], // ▛ upper half + lower left
  [0x259c, 0xaf], // ▜ upper half + lower right
  [0x259d, 0x0a], // ▝ upper right
  [0x259e, 0x5a], // ▞ upper right + lower left
  [0x259f, 0xfa], // ▟ right half + lower left
  // Trames (jamais émises par `resolve_cell` : approximées sur la grille d'échantillons).
  [0x2591, 0x42], // ░ 25 %
  [0x2592, 0x99], // ▒ 50 %
  [0x2593, 0xbd], // ▓ 75 %
]);

/** Blocs fractionnaires et trames : rectangles normalisés `[x, y, w, h]` (jamais émis en v1). */
const BLOCK_RECTS = new Map<number, [number, number, number, number]>([
  [0x2581, [0, 7 / 8, 1, 1 / 8]], // ▁
  [0x2582, [0, 6 / 8, 1, 2 / 8]], // ▂
  [0x2583, [0, 5 / 8, 1, 3 / 8]], // ▃
  [0x2585, [0, 3 / 8, 1, 5 / 8]], // ▅
  [0x2586, [0, 2 / 8, 1, 6 / 8]], // ▆
  [0x2587, [0, 1 / 8, 1, 7 / 8]], // ▇
  [0x2589, [0, 0, 7 / 8, 1]], // ▉
  [0x258a, [0, 0, 6 / 8, 1]], // ▊
  [0x258b, [0, 0, 5 / 8, 1]], // ▋
  [0x258d, [0, 0, 3 / 8, 1]], // ▍
  [0x258e, [0, 0, 2 / 8, 1]], // ▎
  [0x258f, [0, 0, 1 / 8, 1]], // ▏
  [0x2594, [0, 0, 1, 1 / 8]], // ▔
  [0x2595, [7 / 8, 0, 1 / 8, 1]], // ▕
]);

/**
 * Remplit les sous-cases allumées d'un masque 8 bits (2 colonnes × 4 rangées).
 *
 * Les coordonnées sont des multiples entiers de `cellW/2` et `cellH/4` : avec un palier
 * entier et `cellH = 2·cellW`, ce sont des texels entiers (aucune bavure `NEAREST`).
 */
function fillSampleMask(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  cellW: number,
  cellH: number,
  mask: number,
): void {
  const sw = cellW / 2;
  const sh = cellH / 4;
  for (let row = 0; row < 4; row += 1) {
    for (let col = 0; col < 2; col += 1) {
      if ((mask & (1 << (row * 2 + col))) === 0) continue;
      ctx.fillRect(x + col * sw, y + row * sh, sw, sh);
    }
  }
}
