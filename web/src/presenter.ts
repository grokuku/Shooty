/**
 * Présentateur WebGL2 — §C.3, §F.3, §F.4.
 *
 * Layout d'instance (8 o, §F.4 **amendé en Phase 3**) :
 *
 * ```text
 * word0 = fg(24) | slot[7:0] << 24
 * word1 = bg(24) | flags(4) << 24 | slot[8] << 31
 * ```
 *
 * Le 9e bit de slot est indispensable : l'atlas compte 512 slots (§E.3) alors que 8
 * bits n'en adressent que 256. Le détail de l'arbitrage est dans
 * `crates/het-web/src/instances.rs`.
 *
 * Un seul programme, un seul VAO, **un seul `drawArraysInstanced`** par image :
 * quatre sommets (`TRIANGLE_STRIP`) et une instance de 8 octets par cellule. Aucune
 * copie par cellule : le buffer d'instances est lu **en place** dans la mémoire wasm
 * (`new Uint32Array(memory.buffer, ptr, len/4)`) puis téléversé par un unique
 * `bufferData`/`bufferSubData`.
 *
 * État GL (volontairement minimal) : pas de blending (le mélange se fait dans le
 * fragment shader), pas de depth test, pas de cull, `NEAREST` en min/mag,
 * `CLAMP_TO_EDGE`, `scissor` sur la grille. Le letterbox est un `clear` du canvas
 * complet avant le dessin de la grille (`origin_px` venant de `fit`, §E.4).
 *
 * Perte de contexte (`webglcontextlost` / `webglcontextrestored`, risque P2 de §I.1) :
 * le présentateur se marque `lost`, ignore les frames, puis reconstruit programme/VAO/
 * VBO **et redemande un atlas** via `onAtlasNeeded` (la texture, elle, appartient au
 * contexte perdu).
 */

import { ATLAS_SLOTS, SLOT_COLS, SLOT_ROWS } from './atlas.js';
import type { AtlasPixels } from './atlas.js';

/** Géométrie lue dans le bloc `PresentInfo` de `het-web` (16 mots de 32 bits). */
export interface PresentInfo {
  /** Adresse du buffer d'instances dans la mémoire wasm (octets). */
  cellsPtr: number;
  /** Longueur du buffer d'instances (octets). */
  cellsLen: number;
  cols: number;
  rows: number;
  cellW: number;
  cellH: number;
  originX: number;
  originY: number;
  availW: number;
  availH: number;
  epoch: number;
  level: number;
  frameIndex: number;
  flags: number;
}

/** Décalages (en mots) du bloc `PresentInfo`, contrat d'ABI avec `abi.rs`. */
const INFO_WORD = {
  cellsPtr: 0,
  cellsLen: 1,
  cols: 2,
  rows: 3,
  cellW: 4,
  cellH: 5,
  originX: 6,
  originY: 7,
  availW: 8,
  availH: 9,
  epoch: 10,
  level: 11,
  frameIndex: 13,
  flags: 14,
} as const;

/**
 * Lit le bloc `PresentInfo`.
 *
 * La vue est **recréée à chaque frame** : toute croissance de la mémoire wasm détache
 * l'`ArrayBuffer` précédent (§F.3, piège n°1) et un pointeur mémorisé devient faux dès
 * qu'un `Vec` réalloue (§F.3, piège n°2).
 */
export function readInfo(memory: WebAssembly.Memory, ptr: number): PresentInfo {
  const view = new Uint32Array(memory.buffer, ptr, 16);
  return {
    cellsPtr: view[INFO_WORD.cellsPtr]!,
    cellsLen: view[INFO_WORD.cellsLen]!,
    cols: view[INFO_WORD.cols]!,
    rows: view[INFO_WORD.rows]!,
    cellW: view[INFO_WORD.cellW]!,
    cellH: view[INFO_WORD.cellH]!,
    originX: view[INFO_WORD.originX]! | 0,
    originY: view[INFO_WORD.originY]! | 0,
    availW: view[INFO_WORD.availW]!,
    availH: view[INFO_WORD.availH]!,
    epoch: view[INFO_WORD.epoch]!,
    level: view[INFO_WORD.level]!,
    frameIndex: view[INFO_WORD.frameIndex]!,
    flags: view[INFO_WORD.flags]!,
  };
}

/** Temps CPU du présentateur, en microsecondes (compteurs `upload_us`/`draw_us`). */
export interface PresentTimings {
  uploadUs: number;
  drawUs: number;
}

const VERTEX_SHADER = `#version 300 es
layout(location = 0) in vec2 aCorner;   // 0/1 dans la cellule
layout(location = 1) in uvec2 aCell;    // word0 = fg|slot[7:0]<<24, word1 = bg|flags<<24|slot[8]<<31

uniform vec2 uCanvas;   // taille du canvas, en pixels device
uniform vec2 uOrigin;   // coin haut-gauche de la grille, en pixels device
uniform vec2 uCell;     // taille d'une cellule, en pixels device
uniform vec2 uGrid;     // nombre de colonnes, de lignes

out vec2 vUV;
flat out vec3 vFg;
flat out vec3 vBg;

void main() {
  int cols = int(uGrid.x);
  int id = gl_InstanceID;
  ivec2 cell = ivec2(id % cols, id / cols);

  uint word0 = aCell.x;
  uint word1 = aCell.y;
  uint slot = (word0 >> 24u) | (((word1 >> 31u) & 1u) << 8u);
  uint fg = word0 & 0x00FFFFFFu;
  uint bg = word1 & 0x00FFFFFFu;

  vFg = vec3(float((fg >> 16u) & 255u), float((fg >> 8u) & 255u), float(fg & 255u)) / 255.0;
  vBg = vec3(float((bg >> 16u) & 255u), float((bg >> 8u) & 255u), float(bg & 255u)) / 255.0;

  vec2 px = uOrigin + uCell * (vec2(cell) + aCorner);
  gl_Position = vec4(px.x / uCanvas.x * 2.0 - 1.0, 1.0 - px.y / uCanvas.y * 2.0, 0.0, 1.0);

  vec2 slotOrigin = vec2(float(slot % ${SLOT_COLS}u), float(slot / ${SLOT_COLS}u));
  vUV = (slotOrigin + aCorner) * vec2(1.0 / ${SLOT_COLS}.0, 1.0 / ${SLOT_ROWS}.0);
}
`;

const FRAGMENT_SHADER = `#version 300 es
precision mediump float;

uniform sampler2D uAtlas;

in vec2 vUV;
flat in vec3 vFg;
flat in vec3 vBg;
out vec4 outColor;

void main() {
  float coverage = texture(uAtlas, vUV).r;
  outColor = vec4(mix(vBg, vFg, coverage), 1.0);
}
`;

/** Rappels de cycle de vie du présentateur (§I.1, risque P2 : perte de contexte). */
export interface PresenterHooks {
  /** Le contexte a été restauré (ou créé) et **aucun atlas** n'est en place : il faut
   *  en reconstruire un (`web/main.ts` le fait en appelant `setAtlas`). */
  onAtlasNeeded: () => void;
  /** `webglcontextlost` : plus aucun dessin possible jusqu'à la restauration. */
  onContextLost?: () => void;
  /** `webglcontextrestored` : programme/VAO/VBO reconstruits, atlas à refaire. */
  onContextRestored?: () => void;
}

/** Présentateur WebGL2 (une instance par canvas). */
export class Presenter {
  private readonly gl: WebGL2RenderingContext;
  private readonly canvas: HTMLCanvasElement;
  private readonly hooks: PresenterHooks;

  private program: WebGLProgram | null = null;
  private vao: WebGLVertexArrayObject | null = null;
  private cornerBuffer: WebGLBuffer | null = null;
  private instanceBuffer: WebGLBuffer | null = null;
  private instanceBytes = 0;
  private texture: WebGLTexture | null = null;
  private atlasCellW = 0;
  private atlasWidth = 0;
  private atlasHeight = 0;

  private locCanvas: WebGLUniformLocation | null = null;
  private locOrigin: WebGLUniformLocation | null = null;
  private locCell: WebGLUniformLocation | null = null;
  private locGrid: WebGLUniformLocation | null = null;
  private locAtlas: WebGLUniformLocation | null = null;

  /** `true` après une perte de contexte : plus aucun appel GL n'a de sens. */
  private lost = false;

  constructor(gl: WebGL2RenderingContext, canvas: HTMLCanvasElement, hooks: PresenterHooks) {
    this.gl = gl;
    this.canvas = canvas;
    this.hooks = hooks;
    canvas.addEventListener('webglcontextlost', (event) => {
      event.preventDefault();
      this.lost = true;
      this.hooks.onContextLost?.();
    });
    canvas.addEventListener('webglcontextrestored', () => {
      this.lost = false;
      this.initResources();
      this.hooks.onContextRestored?.();
      this.hooks.onAtlasNeeded();
    });
    this.initResources();
  }

  /** `true` si un atlas a été **réellement** téléversé sur le GPU. */
  get hasAtlas(): boolean {
    return this.texture !== null && this.atlasCellW > 0;
  }

  /** Largeur de cellule pour laquelle l'atlas courant a été construit. */
  get atlasCellPx(): number {
    return this.atlasCellW;
  }

  /** `true` si le contexte GL est perdu (aucun dessin possible). */
  get isLost(): boolean {
    return this.lost;
  }

  /** (Re)construit programme, VAO et buffers — après création ou restauration du contexte. */
  private initResources(): void {
    const gl = this.gl;
    const program = linkProgram(gl, VERTEX_SHADER, FRAGMENT_SHADER);
    if (program === null) return;
    this.program = program;

    this.locCanvas = gl.getUniformLocation(program, 'uCanvas');
    this.locOrigin = gl.getUniformLocation(program, 'uOrigin');
    this.locCell = gl.getUniformLocation(program, 'uCell');
    this.locGrid = gl.getUniformLocation(program, 'uGrid');
    this.locAtlas = gl.getUniformLocation(program, 'uAtlas');

    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    this.vao = vao;

    // Attribut 0 : les quatre coins d'un quad (TRIANGLE_STRIP).
    const cornerBuffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, cornerBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    this.cornerBuffer = cornerBuffer;

    // Attribut 1 : l'instance (2 mots de 32 bits non signés).
    const instanceBuffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, instanceBuffer);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribIPointer(1, 2, gl.UNSIGNED_INT, 8, 0);
    gl.vertexAttribDivisor(1, 1);
    this.instanceBuffer = instanceBuffer;
    this.instanceBytes = 0;

    gl.disable(gl.BLEND);
    gl.disable(gl.DEPTH_TEST);
    gl.disable(gl.CULL_FACE);
    gl.disable(gl.SCISSOR_TEST);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);

    // La texture survit à la perte de contexte si on la recrée : on la laisse au
    // prochain `setAtlas` (l'ancienne est invalide).
    this.texture = null;
    this.atlasCellW = 0;
    this.atlasWidth = 0;
    this.atlasHeight = 0;
  }

  /**
   * Installe un nouvel atlas `R8` et retourne `true` si le téléversement a **réussi**.
   *
   * Pendant un rebuild (débouncé côté `atlas.ts`), l'ancienne texture reste liée : une
   * frame n'est jamais dessinée avec un atlas incomplet (§E.3).
   *
   * Un échec (contexte perdu dans l'intervalle, allocation refusée, `texImage2D` en
   * erreur) est **signalé par le retour `false`** : c'est ce qui autorise l'appelant à
   * réessayer. Le bug d'origine consistait à mémoriser `atlasCellW` même quand
   * `createTexture`/`texImage2D` échouaient : la coquille croyait alors l'atlas à jour,
   * ne réessayait plus jamais, et le canvas restait noir sans le moindre message.
   */
  setAtlas(pixels: AtlasPixels, cellW: number): boolean {
    if (this.lost) return false;
    const gl = this.gl;
    if (this.texture === null) {
      const texture = gl.createTexture();
      if (texture === null) {
        console.error(
          'presenter : createTexture() a échoué (contexte GL perdu ou mémoire GPU épuisée)',
        );
        return false;
      }
      this.texture = texture;
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    } else {
      gl.bindTexture(gl.TEXTURE_2D, this.texture);
    }
    // Un échec antérieur a pu laisser une erreur en attente : on repart d'un état propre.
    // Drain borné : sur un contexte perdu, `getError()` peut rendre `CONTEXT_LOST_WEBGL`
    // indéfiniment — boucler sans borne figerait la page.
    for (let i = 0; i < 8 && gl.getError() !== gl.NO_ERROR; i += 1) {
      /* vide la file d'erreurs */
    }
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.R8,
      pixels.width,
      pixels.height,
      0,
      gl.RED,
      gl.UNSIGNED_BYTE,
      pixels.data,
    );
    const error = gl.getError();
    if (error !== gl.NO_ERROR) {
      console.error(
        `presenter : téléversement de l'atlas refusé (GL error 0x${error.toString(16)})`,
      );
      return false;
    }
    this.atlasWidth = pixels.width;
    this.atlasHeight = pixels.height;
    this.atlasCellW = cellW;
    return true;
  }

  /**
   * Dessine l'image décrite par `info`, le buffer d'instances étant lu en place dans
   * `memory` (zéro copie, §F.3).
   */
  draw(info: PresentInfo, memory: WebAssembly.Memory): PresentTimings {
    const gl = this.gl;
    const timings: PresentTimings = { uploadUs: 0, drawUs: 0 };
    if (this.lost || this.program === null || this.vao === null || this.texture === null) {
      return timings;
    }
    const instances = info.cols * info.rows;
    if (instances <= 0 || info.cellsLen === 0) return timings;

    const canvasW = info.availW;
    const canvasH = info.availH;
    const gridW = info.cols * info.cellW;
    const gridH = info.rows * info.cellH;
    if (canvasW === 0 || canvasH === 0 || gridW === 0 || gridH === 0) return timings;

    gl.bindVertexArray(this.vao);
    gl.useProgram(this.program);

    // 1. Letterbox : tout le canvas en couleur de fond, puis `scissor` sur la grille.
    const bg = letterboxColor();
    gl.disable(gl.SCISSOR_TEST);
    gl.clearColor(bg[0], bg[1], bg[2], 1);
    gl.clear(gl.COLOR_BUFFER_BIT);

    // WebGL compte depuis le bas : conversion depuis le repère DOM (haut-gauche).
    const glX = info.originX;
    const glY = canvasH - (info.originY + gridH);
    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(glX, glY, gridW, gridH);
    // Le viewport reste **le canvas entier** : le vertex shader projette déjà des pixels
    // device vers un NDC calculé sur `uCanvas` (la surface complète). Réduire le viewport
    // à la boîte de la grille appliquait la transformation **deux fois** (NDC écrasé dans
    // la boîte) : la grille était alors comprimée de `gridW/availW` et `gridH/availH` et
    // décalée de `origin` en trop, soit jusqu'à une cellule de dérive quand
    // `availH mod cellH` est grand. Conséquence visible : les frontières de texels de
    // l'atlas ne tombaient plus sur des pixels entiers (la dernière ligne/colonne de
    // chaque cellule échantillonnait le voisin), mesuré à 2,2 % de pixels faux à
    // `cellW = 64` et 11,2 % à `cellW = 8`. Mesuré après correctif : 0 %.
    // Le `scissor` suffit à borner le dessin à la grille (letterbox hors grille).
    gl.viewport(0, 0, canvasW, canvasH);

    // 2. Téléversement des instances, en une fois.
    const t0 = performance.now();
    const words = info.cellsLen >>> 2;
    const source = new Uint32Array(memory.buffer, info.cellsPtr, words);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceBuffer);
    if (info.cellsLen !== this.instanceBytes) {
      gl.bufferData(gl.ARRAY_BUFFER, source, gl.DYNAMIC_DRAW);
      this.instanceBytes = info.cellsLen;
    } else {
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, source);
    }
    // Le VAO garde la liaison de l'attribut 1 : re-lier le buffer ne suffit pas à
    // garantir l'association sur certains pilotes, on la refait.
    gl.vertexAttribIPointer(1, 2, gl.UNSIGNED_INT, 8, 0);
    const t1 = performance.now();
    timings.uploadUs = Math.round((t1 - t0) * 1000);

    // 3. Uniformes + texture, puis l'unique appel de dessin.
    gl.uniform2f(this.locCanvas, canvasW, canvasH);
    gl.uniform2f(this.locOrigin, info.originX, info.originY);
    gl.uniform2f(this.locCell, info.cellW, info.cellH);
    gl.uniform2f(this.locGrid, info.cols, info.rows);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.uniform1i(this.locAtlas, 0);

    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, instances);
    const t2 = performance.now();
    timings.drawUs = Math.round((t2 - t1) * 1000);

    gl.disable(gl.SCISSOR_TEST);
    return timings;
  }

  /** Taille de l'atlas courant, pour le diagnostic. */
  get atlasSize(): { width: number; height: number } {
    return { width: this.atlasWidth, height: this.atlasHeight };
  }

  /** Libère les ressources GL (le canvas et le contexte survivent). */
  dispose(): void {
    const gl = this.gl;
    if (this.texture !== null) gl.deleteTexture(this.texture);
    if (this.instanceBuffer !== null) gl.deleteBuffer(this.instanceBuffer);
    if (this.cornerBuffer !== null) gl.deleteBuffer(this.cornerBuffer);
    if (this.vao !== null) gl.deleteVertexArray(this.vao);
    if (this.program !== null) gl.deleteProgram(this.program);
    this.texture = null;
    this.instanceBuffer = null;
    this.cornerBuffer = null;
    this.vao = null;
    this.program = null;
    this.instanceBytes = 0;
    this.atlasCellW = 0;
  }

  /** Nombre de slots de l'atlas (diagnostic/HUD). */
  static get slotCount(): number {
    return ATLAS_SLOTS;
  }

  /** Canvas associé (diagnostic). */
  get target(): HTMLCanvasElement {
    return this.canvas;
  }
}

/** Couleur des zones mortes du letterbox (`#05070a`, même fond que la page). */
function letterboxColor(): [number, number, number] {
  return [0x05 / 255, 0x07 / 255, 0x0a / 255];
}

/** Compile et lie un programme ; `null` (et message console) en cas d'échec. */
function linkProgram(
  gl: WebGL2RenderingContext,
  vertexSource: string,
  fragmentSource: string,
): WebGLProgram | null {
  const vertex = compileShader(gl, gl.VERTEX_SHADER, vertexSource);
  const fragment = compileShader(gl, gl.FRAGMENT_SHADER, fragmentSource);
  if (vertex === null || fragment === null) return null;
  const program = gl.createProgram();
  if (program === null) return null;
  gl.attachShader(program, vertex);
  gl.attachShader(program, fragment);
  gl.linkProgram(program);
  gl.deleteShader(vertex);
  gl.deleteShader(fragment);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    console.error('presenter : échec de liaison du programme', gl.getProgramInfoLog(program));
    gl.deleteProgram(program);
    return null;
  }
  return program;
}

/** Compile un shader ; `null` (et message console) en cas d'échec. */
function compileShader(gl: WebGL2RenderingContext, kind: number, source: string): WebGLShader | null {
  const shader = gl.createShader(kind);
  if (shader === null) return null;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    console.error('presenter : échec de compilation du shader', gl.getShaderInfoLog(shader));
    gl.deleteShader(shader);
    return null;
  }
  return shader;
}
