/**
 * Clavier → codes Astre — §E.6, §F.5.
 *
 * Règles tenues :
 *
 * - **une seule source de vérité** pour les codes : l'énumération du C
 *   (`test3Dterminal/terminal.h: T3_KEY_UP = 256 …`), reproduite dans
 *   `crates/astre-web/src/lib.rs` (`mod key`) ;
 * - `preventDefault` **ciblé** : uniquement sur les touches réellement consommées, et
 *   seulement si la page a le focus (jamais de blocage global des raccourcis
 *   navigateur) ;
 * - `Ctrl+Z` est **toléré** (on ne l'intercepte pas : l'annulation navigateur reste
 *   disponible, conformément à la consigne) ;
 * - `P` bascule le HUD DOM **localement** (il n'y a pas d'action Rust associée), `F`
 *   demande le plein écran, `Échap` n'est pas intercepté (il quitte le plein écran par
 *   le navigateur lui-même).
 *
 * Les touches d'orbite acceptent les flèches **et** ZQSD/WASD (positions logiques
 * `event.key`, donc indépendantes de la disposition matérielle). Le zoom accepte
 * `+`, `=`, `_` (zoom avant) et `-` (zoom arrière), plus le pavé numérique.
 */

/** Codes de touches spéciales, identiques au C et à `astre-web` (`mod key`). */
export const KEY = {
  UP: 256,
  DOWN: 257,
  RIGHT: 258,
  LEFT: 259,
  TAB: 9,
  ESC: 27,
} as const;

/** Codes de zoom (`+`, `=`, `-`, `0`), interceptés par `astre-web::push_key`. */
export const ZOOM_KEY = {
  PLUS: '+'.charCodeAt(0),
  EQUAL: '='.charCodeAt(0),
  MINUS: '-'.charCodeAt(0),
  ZERO: '0'.charCodeAt(0),
} as const;

/** Rappels de la coquille. */
export interface InputHooks {
  /** Touche à pousser dans Rust (`App::push_key`). */
  onKey(code: number, down: boolean): void;
  /** Bascule du HUD DOM (`P`). */
  onToggleHud(): void;
  /** Demande de plein écran (`F`). */
  onFullscreen(): void;
}

/**
 * Traduit un `KeyboardEvent` en code Astre.
 *
 * Retourne `null` pour une touche non consommée (le navigateur garde son comportement).
 */
export function codeForEvent(event: KeyboardEvent): number | null {
  switch (event.code) {
    case 'NumpadAdd':
      return ZOOM_KEY.PLUS;
    case 'NumpadSubtract':
      return ZOOM_KEY.MINUS;
    case 'ArrowUp':
      return KEY.UP;
    case 'ArrowDown':
      return KEY.DOWN;
    case 'ArrowRight':
      return KEY.RIGHT;
    case 'ArrowLeft':
      return KEY.LEFT;
    case 'Tab':
      return KEY.TAB;
    default:
      break;
  }
  if (event.ctrlKey || event.metaKey || event.altKey) return null;
  const key = event.key;
  switch (key) {
    case 'z':
    case 'Z':
    case 'w':
    case 'W':
      return KEY.UP;
    case 's':
    case 'S':
      return KEY.DOWN;
    case 'q':
    case 'Q':
    case 'a':
    case 'A':
      return KEY.LEFT;
    case 'd':
    case 'D':
      return KEY.RIGHT;
    case '+':
    case '=':
    case '_':
      return ZOOM_KEY.PLUS;
    case '-':
      return ZOOM_KEY.MINUS;
    case '0':
      return ZOOM_KEY.ZERO;
    default:
      break;
  }
  if (key.length === 1) {
    const code = key.charCodeAt(0);
    // `r`/`R` recadre (astre-app), `m`/`M` change de mode, `p` est local (HUD DOM),
    // `f` est local (plein écran). On ne pousse que ce que Rust sait traiter.
    if (key === 'r' || key === 'R') return 'r'.charCodeAt(0);
    if (key === 'm' || key === 'M') return key.charCodeAt(0);
    if (key === ' ' || key === '\t') return key === ' ' ? ' '.charCodeAt(0) : KEY.TAB;
    if (code >= 32 && code <= 126) return code;
  }
  return null;
}

/**
 * Branche les écouteurs clavier sur `target`.
 *
 * `target` est la page (`window`) mais les touches ne sont prises en compte que si
 * `document.hasFocus()` est vrai : une fenêtre en arrière-plan ne consomme rien.
 * Retourne la fonction de détachement (aucun écouteur fuyant).
 */
export function attachInput(target: Window, hooks: InputHooks): () => void {
  const onKeyDown = (event: Event): void => {
    const ke = event as KeyboardEvent;
    if (!document.hasFocus()) return;
    if (ke.ctrlKey && (ke.key === 'z' || ke.key === 'Z')) return; // Ctrl+Z toléré

    switch (ke.key) {
      case 'p':
      case 'P':
        ke.preventDefault();
        hooks.onToggleHud();
        return;
      case 'f':
      case 'F':
        ke.preventDefault();
        hooks.onFullscreen();
        return;
      case 'Escape':
        // Le navigateur quitte le plein écran lui-même : on n'intercepte pas.
        return;
      default:
        break;
    }

    const code = codeForEvent(ke);
    if (code === null) return;
    ke.preventDefault();
    hooks.onKey(code, true);
  };

  const onKeyUp = (event: Event): void => {
    const ke = event as KeyboardEvent;
    const code = codeForEvent(ke);
    if (code === null) return;
    // Pas de `preventDefault` au relâchement : inutile et parfois gênant.
    hooks.onKey(code, false);
  };

  target.addEventListener('keydown', onKeyDown);
  target.addEventListener('keyup', onKeyUp);
  return () => {
    target.removeEventListener('keydown', onKeyDown);
    target.removeEventListener('keyup', onKeyUp);
  };
}
