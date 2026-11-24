# Relais de Cendre

Un FPS solo pour terminal. Traverse la galerie, utilise les piliers pour te mettre à couvert et rejoins le portique lumineux en haut des marches. Le portique termine la traversée et relance la carte. [Voir la vidéo de la démo](relais-de-cendre.mp4)

## Jouer

Depuis la racine du projet, lance **`.\quake\relay.exe` dans Windows Terminal** ou **`./quake/relay` dans Linux ou WSL**. Les exécutables fournis ciblent x86-64. Garde-les à côté de la bibliothèque TyrQuake et du dossier `id1`. Une fenêtre de 120 × 40 caractères ou plus offre un bon compromis pour distinguer les adversaires.

| Commande | Action |
| --- | --- |
| ↑ / ↓ ou Z / S (W / S en QWERTY) | Avancer / reculer |
| ← / → | Tourner |
| Q / D (A / D en QWERTY) | Pas latéral |
| Espace / F | Tirer / sauter |
| E | Changer d’arme |
| M | Basculer entre Braille et ASCII |
| R / X | Recommencer la carte / quitter |

La démo démarre directement dans l’arène en Braille couleur, avec une cible de 60 images/s. Elle est muette. Le clavier du terminal fournit des frappes et des répétitions. Chaque frappe de déplacement prolonge le mouvement de 170 ms. La santé et les munitions apparaissent sur le tableau de bord du jeu.

## Fonctionnement et sources

TyrQuake exécute la simulation et produit une image RVB565. Astre l’échantillonne à raison de huit points par cellule, conserve les proportions de l’image et présente les cellules modifiées au terminal. Le dossier contient les bibliothèques Linux et Windows de TyrQuake ainsi que leur source au commit `67db338ff4d27fdbcead61d4e54938d758ae62cb`. [Projet TyrQuake](https://github.com/libretro/tyrquake) · [Licence du moteur](TyrQuake-LICENSE.txt)

Les ressources de jeu et la logique QuakeC proviennent de LibreQuake v0.09-beta, édition lite. Les textures architecturales et la carte du relais sont générées pour cette démo. [Version de LibreQuake](https://github.com/lavenderdotpet/LibreQuake/releases/tag/v0.09-beta) · [Licences et crédits des ressources](id1/docs/README-IMPORTANT-LICENCE-INFO)

Pour recompiler depuis la racine, utiliser `make quake/relay` sous Linux ou `mingw32-make quake/relay.exe` sous Windows avec MinGW-w64. La commande `--snapshot` produit une image ANSI fixe et fonctionne également avec une sortie redirigée.

Pour reconstruire la carte, exécuter `python3 quake/build_arena.py` avec les outils `qbsp`, `vis` et `light` d’ericw-tools dans le PATH. Le script `build_compat.py` crée `id1/pak2.pak` à partir des deux archives de LibreQuake. Il complète la table des couleurs lumineuses, normalise les indicateurs de visibilité des modèles BSP et fournit la configuration de démarrage. [ericw-tools v0.18.1](https://github.com/ericwa/ericw-tools/releases/tag/v0.18.1)
