# openRTS

Un jeu de stratégie en temps réel (RTS) en 3D, jouable à la souris comme au doigt,
écrit en Rust avec le moteur [Bevy](https://bevyengine.org) 0.20.

Le design du jeu est décrit dans [GAME_DESIGN.md](GAME_DESIGN.md).

## Organisation du dépôt

| Dossier | Rôle |
| --- | --- |
| `crates/rts_engine` | Le moteur générique : entrées, caméra, sélection, joueurs (`players`), file des ordres (`orders`), simulation, collisions, navigation (grille, A\*), bâtiments (pose, chantiers), économie (`economy` : gisements, récolte, production, population, interface), rendu (`graphics`). Aucun contenu de jeu. |
| `crates/rts_game` | Le jeu : unités, bâtiments, factions, cartes, gisements (`garde.rs`, `sylves.rs`, `collectif.rs`, `map.rs`, `base.rs`, `base_sylves.rs`, `gisements.rs`). Utilise `rts_engine`. |

## Compiler et lancer

Il faut Rust stable (installé avec [rustup](https://rustup.rs)), puis :

```sh
cargo run
```

La première compilation de Bevy prend plusieurs minutes ; les suivantes sont rapides.
Sous Linux, il faut aussi les dépendances système de Bevy
(voir le [guide Bevy pour Linux](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)).

## Commandes (ordinateur)

| Action | Commande |
| --- | --- |
| Sélectionner une unité | Clic gauche |
| Sélection en rectangle | Glisser au clic gauche |
| Ajouter / retirer de la sélection | Maj + clic ou Maj + rectangle |
| Déplacer la sélection | Clic droit sur le sol |
| Récolter (Techniciens ou Pousses sélectionnés) | Clic droit sur un gisement de Cristal, ou sur un Extracteur (Calice chez les Sylves) terminé |
| Point de ralliement (bâtiment producteur sélectionné) | Clic droit sur le sol, ou sur un gisement (les Techniciens produits y récoltent directement) |
| Produire (Quartier général ou Caserne sélectionné) | Touche de l'unité ou bouton du panneau en bas : T (Technicien, QG) ; S (Sentinelle), V, L, F (Caserne, grisés tant que leur bâtiment n'existe pas) |
| Annuler une unité en file | Clic sur sa case dans la file (remboursée) |
| Annuler un chantier sélectionné | X ou bouton « Annuler » (remboursé) |
| Faire défiler la caméra | Flèches ou souris au bord de l'écran |
| Zoomer | Molette ou trackpad |
| Test de charge (300 unités) | F3 |
| Construire (un Technicien ou une Pousse sélectionné) | B puis la touche du bâtiment, ou un bouton de la barre en bas de l'écran |
| Poser le bâtiment | Clic gauche (Maj + clic : en poser un autre) |
| Annuler la pose | Clic droit ou Échap |
| Libérer la souris | Échap (un clic dans le jeu la reprend ; pendant une pose, le premier Échap annule la pose) |
| Quitter | Cmd + Q (macOS), Alt + F4 (Windows, Linux) |

Le jeu s'ouvre en plein écran. Pour que les bords d'écran servent au défilement
sans déclencher les coins actifs, la barre de menus ou le Dock de macOS, le curseur
du système est verrouillé et le jeu dessine le sien, qui ne peut pas sortir de l'écran.

Les unités ne se traversent pas : elles se bousculent, écartent les unités
immobiles de leur chemin et s'arrêtent en groupe serré à l'arrivée. Elles
contournent les bâtiments (grille de navigation de 1 m et A\*) ; les grosses
unités évitent les passages trop étroits pour elles. Les unités volantes
passent au-dessus de tout et ne gênent que les autres volantes.

### Bâtiments de la Garde

| Touche (après B) | Bâtiment | Empreinte | Prérequis |
| --- | --- | --- | --- |
| Q | Quartier général | 7 × 7 m | — |
| H | Module d'habitat | 3 × 3 m | — |
| E | Extracteur | 4 × 4 m | — (pile sur une source d'Éther libre) |
| C | Caserne | 5 × 6 m | Quartier général |
| T | Tourelle | 2 × 2 m | Caserne |

Les raccourcis sont des lettres tapées : ils suivent ce qui est écrit sur les
touches, en AZERTY comme en QWERTY. En mode pose, un fantôme suit le pointeur,
aligné sur la grille : vert si l'emplacement est libre, rouge sinon. Le
Technicien le plus proche se rend sur le chantier et y reste : le bâtiment
monte du sol tant qu'il travaille. Le coût est débité à la pose (refus et
message « Ressources insuffisantes » sinon) et remboursé si l'on annule le
chantier. L'Extracteur s'aimante sur la source d'Éther la plus proche ;
ailleurs, son fantôme est rouge (« Doit être posé sur une source »).

### Bâtiments des Sylves (`OPENRTS_FACTION=sylves`)

| Touche (après B) | Bâtiment | Empreinte | Prérequis | Rôle |
| --- | --- | --- | --- | --- |
| A | Arbre-mère | 7 × 7 m | — | Cœur de base, dépôt, produit les Pousses (P), 15 de population |
| S | Souche nourricière | 3 × 3 m | — | 8 de population |
| C | Calice | 4 × 4 m | — (pile sur une source d'Éther libre) | Récolte de l'Éther |
| R | Ronceraie | 5 × 5 m | Arbre-mère | Produit les Ronciers (R) ; Sourcier (S) et Lanceur de spores (L) grisés |
| E | Épineux | 2 × 2 m | Ronceraie | Défense fixe |

Les Sylves construisent autrement : **la Pousse devient le bâtiment**. Avec
des Pousses sélectionnées, B puis une lettre (ou la barre de boutons) ouvre
la pose, comme pour la Garde. Le coût est débité à la pose et
l'emplacement est promis (cadre vert au sol) à la Pousse libre la plus
proche ; elle s'y rend, s'enracine et disparaît (sa population est libérée).
Le chantier pousse ensuite tout seul, sans échafaudage : le bâtiment grandit
du sol. Chaque Pousse ne fait qu'un bâtiment (« Aucun ouvrier libre » sinon).
Donner un autre ordre à la Pousse avant qu'elle arrive annule la promesse,
remboursée en entier ; annuler un chantier qui pousse (X) rend 75 % du coût
et la Pousse ressort de terre.

### Économie

Deux ressources, aux noms **provisoires** : le **Cristal** (principale,
grappes bleu glacier en ligne derrière le Quartier général) et l'**Éther**
(rare, failles violettes sur les flancs, récoltées par un Extracteur). Les
compteurs en haut à droite montrent les stocks et la population
(utilisée / disponible).

- Les Techniciens vont au gisement, extraient (2,8 s pour 5 Cristal, 2 s pour
  4 Éther), portent leur charge (visible) au Quartier général le plus proche,
  et recommencent. Un seul Technicien à la fois par gisement : les autres
  attendent leur tour ou passent au voisin libre ; deux à trois par gisement
  le saturent. Un gisement vidé disparaît, ses Techniciens passent au suivant.
- Production : un bâtiment producteur sélectionné affiche ses boutons et sa
  file (5 au plus, payée à la mise en file, progression de la première
  unité). Les unités produites sortent vers le point de ralliement.
- Population : chaque unité en occupe (1 à 4), le Quartier général en
  fournit 15 et chaque Module d'habitat 8, dans la limite d'un plafond
  global (200, réglable avec `OPENRTS_POP_MAX`). Au-delà : « Population maximale ».

La partie démarre avec 50 Cristal, un Quartier général, deux Modules
d'habitat, les gisements et quatre Techniciens qui récoltent déjà. En
Sylves : un Arbre-mère, deux Souches nourricières et quatre Pousses (elles
rapportent à l'Arbre-mère, et récoltent l'Éther au pied d'un Calice).

Tous les ordres (déplacer, récolter, poser, produire, rallier, annuler)
passent par une file appliquée au pas de simulation suivant : c'est la base
du futur multijoueur en lockstep (voir `crates/rts_engine/src/orders.rs`).

## Rendu et réglages graphiques

Le moteur gère le rendu (module `graphics`) : HDR, tone mapping filmique (ACES),
bloom sur les pièces lumineuses, ombres en cascade recadrées sur la zone visible,
brouillard proportionnel au zoom, éclairage d'ambiance par image (IBL) calculé au
démarrage, lumière principale et contre-jours colorés, particules d'ambiance.
Les couleurs (lumières, ciel, brouillard) viennent d'une `Ambiance` fournie par le
jeu ; l'apparence des unités est décrite en données, pièce par pièce
(`ModelBuilder`), avec une `Palette` de matériaux par faction.

Deux niveaux de qualité (`GraphicsQuality`) :

| Qualité | Contenu | Cible |
| --- | --- | --- |
| Haute (défaut) | HDR + bloom, MSAA ×4, 3 cascades d'ombre filtrées en douceur | Ordinateur |
| Basse | Sans HDR ni bloom, FXAA, 1 cascade d'ombre, 3 fois moins de particules | iPad, machines modestes |

## Variables de développement

| Variable | Effet |
| --- | --- |
| `OPENRTS_WINDOWED=1` | Fenêtre au lieu du plein écran, souris libre |
| `OPENRTS_FACTION=sylves` | Faction affichée : `garde` (défaut ; `coalition` accepté), `sylves` ou `collectif` (ambiance, sol, unités, test de charge) |
| `OPENRTS_QUALITY=bas` | Qualité graphique Basse (`haut` par défaut) |
| `OPENRTS_STRESS=1` | Lance le test de charge (300 unités) au démarrage |
| `OPENRTS_ZOOM=12` | Distance de départ de la caméra (10 à 70 m) |
| `OPENRTS_DEMO=buildings` | Garde : scène de démonstration des bâtiments (les cinq bâtiments, un chantier en cours, un fantôme vert et un rouge, un groupe qui contourne la Caserne au bout de 11 s) |
| `OPENRTS_FACTION=sylves OPENRTS_DEMO=buildings` | Sylves : les cinq bâtiments, un Calice et la ligne de Cristal récoltés par des Pousses, une Ronceraie en pleine croissance, un fantôme vert et un rouge, une Pousse sélectionnée qui part s'enraciner au bout de 10,5 s (emplacement promis en vert) |
| `OPENRTS_FOCUS=x,z` | La caméra vise ce point du sol au démarrage |
| `OPENRTS_DEMO=economy` | Garde : l'économie en marche (douze Techniciens sur la ligne de Cristal, trois à un Extracteur, Quartier général sélectionné avec trois Techniciens en file et son point de ralliement, compteurs) |
| `OPENRTS_POP_MAX=100` | Plafond global de population (200 par défaut) |
| `OPENRTS_SCREENSHOT=capture.png` | Après 14 s : affiche les FPS moyens des 6 dernières secondes, enregistre une capture du rendu et quitte (synchronisation verticale coupée pour mesurer) |

Exemples : `OPENRTS_WINDOWED=1 OPENRTS_STRESS=1 OPENRTS_SCREENSHOT=/tmp/charge.png cargo run --release`,
`OPENRTS_WINDOWED=1 OPENRTS_DEMO=buildings OPENRTS_ZOOM=36 OPENRTS_SCREENSHOT=/tmp/batiments.png cargo run --release`,
`OPENRTS_FACTION=sylves OPENRTS_WINDOWED=1 OPENRTS_DEMO=buildings OPENRTS_ZOOM=40 OPENRTS_FOCUS=0,-5 OPENRTS_SCREENSHOT=/tmp/sylves.png cargo run --release`,
`OPENRTS_WINDOWED=1 OPENRTS_DEMO=economy OPENRTS_ZOOM=34 OPENRTS_FOCUS=1,-11 OPENRTS_SCREENSHOT=/tmp/economie.png cargo run --release`.
Pour mesurer les performances, compiler en `--release` : en mode debug, les
vérifications internes de Bevy et de wgpu coûtent beaucoup d'images par seconde.

## Licence

Le code est sous double licence, au choix :

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT ([LICENSE-MIT](LICENSE-MIT))

Sauf mention contraire, toute contribution envoyée pour être incluse dans le projet
est placée sous cette même double licence, sans condition supplémentaire.
