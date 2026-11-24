//! Clipping du plan proche — port de la boucle de `t3_triangle` (C-REF).
//!
//! Le C applique un Sutherland–Hodgman 3 -> 4 sommets **en espace vue**, puis rend un
//! éventail `(0, i, i+1)`. La comparaison est stricte sur `>=` : une arête exactement
//! sur `near_z` est **conservée** (§G.8.4), un triangle entièrement derrière disparaît.

use crate::math::Vec3;

/// Nombre maximal de sommets après clipping (un triangle coupé par un plan donne un
/// quadrilatère).
pub const MAX_CLIPPED: usize = 4;

/// Clippe le triangle `v` contre le plan `z >= near_z` et écrit le polygone résultant
/// dans `out` ; rend le nombre de sommets (0 à 4).
///
/// Ordre des sommets identique au C : pour chaque arête `(i+2)%3 -> i`, on émet
/// l'intersection si les deux extrémités changent de côté, puis `q` s'il est devant.
#[must_use]
pub fn near_plane(v: &[Vec3; 3], near_z: f32, out: &mut [Vec3; MAX_CLIPPED]) -> usize {
    let mut n = 0usize;
    for i in 0..3 {
        let p = v[(i + 2) % 3];
        let q = v[i];
        if (p.z >= near_z) != (q.z >= near_z) {
            let t = (near_z - p.z) / (q.z - p.z);
            if let Some(slot) = out.get_mut(n) {
                *slot = Vec3::new(p.x + t * (q.x - p.x), p.y + t * (q.y - p.y), near_z);
            }
            n += 1;
        }
        if q.z >= near_z {
            if let Some(slot) = out.get_mut(n) {
                *slot = q;
            }
            n += 1;
        }
    }
    debug_assert!(n <= MAX_CLIPPED);
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    // Rappel : en **espace vue** du C, `z > 0` est devant la caméra (le plan proche est
    // `z >= near_z`), contrairement à l'axe monde où la caméra regarde vers -Z.
    fn clip(v: [Vec3; 3], near_z: f32) -> Vec<Vec3> {
        let mut out = [Vec3::ZERO; MAX_CLIPPED];
        let n = near_plane(&v, near_z, &mut out);
        out[..n].to_vec()
    }

    #[test]
    fn triangle_in_front_is_untouched() {
        let v = [
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(0.0, 1.0, 1.0),
        ];
        assert_eq!(clip(v, 0.5), v.to_vec());
    }

    #[test]
    fn triangle_behind_disappears() {
        let v = [
            Vec3::new(0.0, 0.0, 0.1),
            Vec3::new(1.0, 0.0, 0.2),
            Vec3::new(0.0, 1.0, 0.3),
        ];
        assert!(clip(v, 0.5).is_empty());
    }

    #[test]
    fn vertices_exactly_on_the_plane_are_kept() {
        // `>=` strict : un triangle entièrement sur near_z est conservé tel quel.
        let v = [
            Vec3::new(0.0, 0.0, 0.5),
            Vec3::new(1.0, 0.0, 0.5),
            Vec3::new(0.0, 1.0, 0.5),
        ];
        assert_eq!(clip(v, 0.5), v.to_vec());
    }

    #[test]
    fn one_vertex_behind_produces_a_quad() {
        let v = [
            Vec3::new(-1.0, -1.0, 1.0),
            Vec3::new(1.0, -1.0, 1.0),
            Vec3::new(0.0, 1.0, -1.0),
        ];
        let out = clip(v, 0.5);
        assert_eq!(out.len(), 4);
        assert!(out.iter().all(|p| p.z >= 0.5 - 1e-6));
        // Les deux intersections sont exactement sur le plan.
        let on_plane = out.iter().filter(|p| (p.z - 0.5).abs() < 1e-6).count();
        assert_eq!(on_plane, 2);
    }

    #[test]
    fn two_vertices_behind_produce_a_triangle_sliver() {
        let v = [
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(0.0, 1.0, 3.0),
        ];
        let out = clip(v, 0.5);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|p| p.z >= 0.5 - 1e-6));
        // Le sommet lointain est intact, les deux autres ont été ramenés sur le plan.
        assert!((out.iter().map(|p| p.z).fold(0.0f32, f32::max) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn vertex_exactly_on_the_plane_with_one_behind_is_degenerate_but_kept() {
        // Le C émet deux fois le sommet sur le plan (`p.z >= near` vrai), l'éventail
        // produit alors des triangles d'aire nulle, rejetés par `raster`.
        let v = [
            Vec3::new(0.0, 0.0, 0.5),
            Vec3::new(1.0, 0.0, 0.5),
            Vec3::new(0.0, 1.0, -2.0),
        ];
        let out = clip(v, 0.5);
        assert_eq!(out.len(), 4);
        assert!(out.iter().all(|p| p.z >= 0.5));
    }
}
