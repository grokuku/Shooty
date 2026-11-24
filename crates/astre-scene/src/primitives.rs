//! Primitives de modélisation — port de `demo_scene.c:{plus,minus,dot,radial,tri,quad,
//! box,drum,annulus,loft,section}` (C-REF).
//!
//! Le `Model { T3Frame *f; T3Vec eye, offset; }` du C devient un [`Builder`] qui
//! emprunte le [`Renderer`] : même rôle (un œil + un décalage de modèle courant), mais
//! sans pointeur nu. [`Builder::child`] reproduit `Model engine = m; engine.offset…`
//! (tuyères de la navette).
//!
//! Contrat inchangé : chaque face est éclairée **une fois** par `paint`, puis les deux
//! triangles d'un quad partagent la couleur obtenue (c'est ce qui fait disparaître la
//! couture en diagonale dans le C).

use astre_core::math::Vec3;
use astre_core::raster::Renderer;

use crate::camera::PI;
use crate::lighting::paint;

/// Livrée ivoire de la navette (`ivory` du C).
pub const IVORY: u32 = 0x00E9_E3CA;
/// Accent orange de la navette (`orange` du C).
pub const ORANGE: u32 = 0x00FB_7C3E;
/// Pièces sombres de la navette (`dark` du C).
pub const DARK: u32 = 0x0030_414D;
/// Verrière (`glass` du C).
pub const GLASS: u32 = 0x0035_ABBC;

/// Section transversale d'un fuselage le long de l'axe Z local (`Section` du C).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Section {
    /// Position sur l'axe Z local.
    pub z: f32,
    /// Demi-largeur (rayon X).
    pub rx: f32,
    /// Demi-hauteur (rayon Y).
    pub ry: f32,
    /// Décalage vertical du centre de la section.
    pub y: f32,
}

impl Section {
    /// Construit une section (`{z, rx, ry, y}` du C).
    #[must_use]
    pub const fn new(z: f32, rx: f32, ry: f32, y: f32) -> Self {
        Self { z, rx, ry, y }
    }

    /// Point de la section à l'angle donné — `section` du C.
    #[must_use]
    pub fn at(&self, angle: f32) -> Vec3 {
        Vec3::new(
            self.rx * angle.cos(),
            self.y + self.ry * angle.sin(),
            self.z,
        )
    }
}

/// Point d'un cercle horizontal — `radial` du C.
#[inline]
#[must_use]
pub fn radial(radius: f32, y: f32, angle: f32) -> Vec3 {
    Vec3::new(radius * angle.cos(), y, radius * angle.sin())
}

/// Bâtisseur de géométrie : œil de la caméra, décalage du modèle courant, renderer.
pub struct Builder<'a> {
    renderer: &'a mut Renderer,
    eye: Vec3,
    offset: Vec3,
}

impl<'a> Builder<'a> {
    /// Crée un modèle à l'origine, éclairé depuis `eye`.
    pub fn new(renderer: &'a mut Renderer, eye: Vec3) -> Self {
        Self {
            renderer,
            eye,
            offset: Vec3::ZERO,
        }
    }

    /// Œil de la caméra (utilisé par `paint`).
    #[inline]
    #[must_use]
    pub const fn eye(&self) -> Vec3 {
        self.eye
    }

    /// Décalage courant du modèle.
    #[inline]
    #[must_use]
    pub const fn offset(&self) -> Vec3 {
        self.offset
    }

    /// Remplace le décalage du modèle (`m.offset = V(x,0,z)` du C).
    #[inline]
    pub fn set_offset(&mut self, offset: Vec3) {
        self.offset = offset;
    }

    /// Décalage relatif (`engine.offset.x += …; engine.offset.y -= …` du C).
    #[inline]
    pub fn translate(&mut self, delta: Vec3) {
        self.offset = self.offset + delta;
    }

    /// Sous-modèle partageant le même renderer et le même œil, à un autre décalage.
    ///
    /// Équivalent de `Model engine = m; engine.offset = …` : le parent ne doit plus
    /// être utilisé tant que l'enfant vit (vérifié par l'emprunt).
    pub fn child(&mut self, offset: Vec3) -> Builder<'_> {
        Builder {
            renderer: &mut *self.renderer,
            eye: self.eye,
            offset,
        }
    }

    /// Triangle éclairé (`tri` du C).
    pub fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, color: u32, emission: f32) {
        let shaded = paint(self.eye, self.offset, a, b, c, color, emission);
        self.renderer
            .triangle(a + self.offset, b + self.offset, c + self.offset, shaded);
    }

    /// Quadrilatère convexe en deux triangles, **une seule** évaluation d'éclairage.
    pub fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: u32, emission: f32) {
        let shaded = paint(self.eye, self.offset, a, b, c, color, emission);
        let (a, b, c, d) = (
            a + self.offset,
            b + self.offset,
            c + self.offset,
            d + self.offset,
        );
        self.renderer.triangle(a, b, c, shaded);
        self.renderer.triangle(a, c, d, shaded);
    }

    /// Boîte alignée sur les axes entre deux coins opposés (`box` du C).
    pub fn cuboid(&mut self, a: Vec3, b: Vec3, color: u32) {
        let p = [
            Vec3::new(a.x, a.y, a.z),
            Vec3::new(b.x, a.y, a.z),
            Vec3::new(b.x, a.y, b.z),
            Vec3::new(a.x, a.y, b.z),
            Vec3::new(a.x, b.y, a.z),
            Vec3::new(b.x, b.y, a.z),
            Vec3::new(b.x, b.y, b.z),
            Vec3::new(a.x, b.y, b.z),
        ];
        self.quad(p[4], p[5], p[6], p[7], color, 0.0);
        self.quad(p[0], p[1], p[2], p[3], color, 0.0);
        for i in 0..4 {
            let j = (i + 1) % 4;
            self.quad(p[i], p[j], p[j + 4], p[i + 4], color, 0.0);
        }
    }

    /// Tambour : `n` flancs coniques plus un couvercle en éventail (`drum` du C).
    // Sept paramètres, comme la signature C : les regrouper masquerait la parité.
    #[allow(clippy::too_many_arguments)]
    pub fn drum(&mut self, n: i32, r0: f32, r1: f32, y0: f32, y1: f32, side: u32, lid: u32) {
        for i in 0..n {
            let a = (i as f32 + 0.5) * 2.0 * PI / n as f32;
            let b = (i as f32 + 1.5) * 2.0 * PI / n as f32;
            let p = radial(r1, y1, a);
            let q = radial(r1, y1, b);
            self.quad(radial(r0, y0, a), radial(r0, y0, b), q, p, side, 0.0);
            self.tri(Vec3::new(0.0, y1, 0.0), p, q, lid, 0.0);
        }
    }

    /// Anneau horizontal `n` secteurs, avec saut périodique optionnel (`annulus` du C).
    pub fn annulus(&mut self, inner: f32, outer: f32, y: f32, n: i32, color: u32, period: i32) {
        for i in 0..n {
            if period != 0 && i % period == 0 {
                continue;
            }
            let a = 2.0 * PI * i as f32 / n as f32;
            let b = 2.0 * PI * (i + 1) as f32 / n as f32;
            self.quad(
                radial(inner, y, a),
                radial(outer, y, a),
                radial(outer, y, b),
                radial(inner, y, b),
                color,
                1.0,
            );
        }
    }

    /// Verrière : teinte de reflet (`C(129,226,218)` du C).
    const CANOPY_HIGHLIGHT: u32 = 0x0081_E2DA;

    /// Fuselage lissé par sections successives (`loft` du C).
    ///
    /// `canopy` sélectionne la palette de la verrière (sombre/cyan/verre) au lieu de
    /// la livrée ivoire/orange.
    pub fn loft(&mut self, sections: &[Section], canopy: bool) {
        let count = sections.len();
        if count < 2 {
            return;
        }
        for j in 0..count - 1 {
            for i in 0..12 {
                let a = 2.0 * PI * i as f32 / 12.0;
                let b = 2.0 * PI * (i + 1) as f32 / 12.0;
                let color = if canopy {
                    if j == 0 || j == 2 || j == count - 2 {
                        DARK
                    } else if i == 2 || i == 3 {
                        Self::CANOPY_HIGHLIGHT
                    } else {
                        GLASS
                    }
                } else if (7..=10).contains(&i) {
                    DARK
                } else if i == 0 || i == 5 || j == 1 {
                    ORANGE
                } else {
                    IVORY
                };
                self.quad(
                    sections[j].at(a),
                    sections[j].at(b),
                    sections[j + 1].at(b),
                    sections[j + 1].at(a),
                    color,
                    0.0,
                );
            }
        }
    }
}
