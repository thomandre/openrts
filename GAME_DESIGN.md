# Game design — RTS 3D tactile

Nov 10, 2026 · @Thomas

## Vision et piliers

Un RTS en 3D dans l’esprit de StarCraft 2, jouable aussi bien au clavier-souris sur ordinateur qu’au doigt sur iPad, avec de grandes batailles épiques. Le moteur est open source pour que la communauté le fasse évoluer.

Les cinq piliers, par ordre de priorité :

1. **Le RTS tactile résolu.** Le jeu est entièrement jouable au doigt seul ; le clavier sur iPad est un accélérateur, pas une obligation.
2. **Des batailles épiques.** 200 à 300 unités par joueur au minimum, fluides, y compris sur iPad.
3. **De la profondeur stratégique.** Factions asymétriques, deux ressources, arbre technologique à embranchements.
4. **Une lisibilité parfaite.** Chaque unité se reconnaît d’un coup d’œil à sa silhouette, même sur un écran de tablette.
5. **Un moteur ouvert et réutilisable.** Le moteur est séparé du contenu, pour que d’autres puissent créer leurs propres jeux dessus.

Référence de départ : MechCom, RTS mobile agréable mais limité (une seule faction, une ressource, sélection pauvre, pas de groupes de contrôle, arbre technologique simple, unités qui se ressemblent, plafond d’environ 50 unités). Chacun de ces défauts est un objectif à dépasser.

## Cadre technique

Le jeu est écrit en Rust avec le moteur Bevy (dernière version stable), développé sur Mac.

| Sujet | Décision |
| --- | --- |
| Langage et moteur | Rust + Bevy, architecture ECS |
| Plateformes | Mac, Linux, Windows, puis iPad |
| Machine de développement | Mac (Xcode requis pour iPad) |
| Builds Windows et Linux | GitHub Actions, à chaque mise à jour |
| Licence du code | Double licence MIT / Apache-2.0 (compatible App Store) |
| Licence des assets | À décider, probablement Creative Commons |
| Structure du dépôt | Workspace Cargo : une crate moteur, une crate jeu |

La crate **moteur** contient tout ce qui est générique : caméra, sélection, ordres, pathfinding, économie, combat, brouillard de guerre, couche d’entrées, réseau. La crate **jeu** contient le contenu : factions, unités, bâtiments, cartes, modèles.

Propriété intellectuelle : aucun nom, unité, visuel ou son repris de StarCraft ou de Blizzard. Le gameplay s’en inspire, le contenu est original. Ne pas reprendre de code de Warzone 2100 (licence GPL), seulement des idées.

## Factions

Deux factions asymétriques au lancement, une troisième une fois les bases solides. Le prototype commence avec une seule faction simplifiée.

Asymétrique signifie des façons de jouer différentes, pas seulement des unités différentes : manière de construire, de récolter, de se développer, forces et faiblesses propres. Les deux factions partagent les mêmes règles du moteur ; seules leurs données changent.

Décisions et questions ouvertes :

Première faction : **la Coalition** (nom provisoire), des humains militaires futuristes. La seconde faction reste à définir ; elle accueillera un grand vaisseau porte-drones qui lance une nuée de petits chasseurs, idée réservée à une race non humaine.

- [ ] Nom définitif de la Coalition
- [ ] Thème et style de jeu de la seconde faction
- [ ] Troisième faction, plus tard

En cours d'exploration visuelle : **les Sylves**, une forêt consciente qui avance racine après racine (`OPENRTS_FACTION=sylves`). Ce n'est pas encore une décision de design : rien ne dit qu'elles seront la seconde faction, et elles n'accueillent pas le vaisseau porte-drones prévu pour celle-ci. Rôles provisoires, pensés en contraste avec la Coalition (lentes, organiques, fortes au contact, aucune infanterie rapide à distance) :

| Unité | Rôle provisoire | Silhouette |
| --- | --- | --- |
| Pousse | Ouvrière | Petit bulbe de mousse, deux feuilles dressées |
| Roncier | Mêlée en nombre, rapide | Bas et trapu sur quatre pattes en racines, carapaces d'écorce |
| Sourcier | Soutien, soigne les alliés | Lanterne de sève sur le dos, collerette de pétales |
| Lanceur de spores | Artillerie de zone, lente | Racines en étoile, énorme cosse piquée de spores |
| Gardien | Géant qui encaisse | Arbre qui marche, grande couronne de feuillage |
| Samare | Éclaireur volant, harcèlement | Graine à deux ailes qui plane |

Bâtiments et économie des Sylves (**propositions**, étape 4, à valider et ajuster en test). Palier 1 aux mêmes rôles que celui de la Coalition, formes tirées de la forêt consciente (racines, souches, champignons, poches de sève) :

| Bâtiment | Rôle | Empreinte | PV | Temps | Coût | Prérequis | Touche (après B) | Silhouette |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Arbre-mère | Cœur de base : dépôt, produit les Pousses, 15 de population | 7 × 7 m | 1 500 | 75 s | 350 P | — | A | Grand arbre de 6,5 m sur sept racines, poche de sève en façade (le dépôt), couronne de feuilles et bourgeons d'équipe |
| Souche nourricière | Population (+8) | 3 × 3 m | 350 | 20 s | 75 P | — | S | Large souche coupée, cerne pâle, trois champignons aux chapeaux d'équipe |
| Calice | Ressource rare : posé sur une source, il boit l'Éther | 4 × 4 m | 450 | 22 s | 50 P | — (sur une source libre) | C | Fleur-coupe sur six racines plongées dans la faille, nectar de spores, sept pétales d'équipe |
| Ronceraie | Infanterie : Ronciers (Sourcier, Lanceur de spores grisés) | 5 × 5 m | 1 000 | 45 s | 125 P | Arbre-mère | R | Trois arches de ronces épineuses en voûte, trois cocons d'équipe dessous |
| Épineux | Défense fixe | 2 × 2 m | 300 | 22 s | 100 P | Ronceraie | E | Bulbe d'écorce, tête qui balaie : couronne d'épines, collerette d'équipe, dard |

Construction des Sylves : **la Pousse devient le bâtiment** (`ConstructionStyle::WorkerConsumed` dans le moteur). À la pose, le coût est payé et l'emplacement promis à la Pousse libre la plus proche ; elle s'y rend, s'enracine et disparaît (sa population est libérée) ; le chantier pousse ensuite seul pendant le temps de construction, sans ouvrier. C'est l'asymétrie voulue face à la Coalition, dont le Technicien reste sur le chantier : les Sylves ne perdent pas de temps d'ouvrier mais perdent l'ouvrier, d'où des prix en Cristal un peu plus bas. Règles proposées :

- Une Pousse ne fait qu'un bâtiment : sans Pousse libre dans la sélection, la pose est refusée (« Aucun ouvrier libre »).
- Un autre ordre donné à la Pousse avant qu'elle arrive annule la promesse, remboursée en entier. Si l'emplacement est pris entre-temps, la promesse est remboursée et la Pousse s'arrête.
- Annuler un chantier qui pousse rend **75 %** de son coût et **la Pousse**, qui ressort de terre (même au-delà de la limite de population : la production attend alors qu'il y ait de la place).
- Économie : les Pousses récoltent le Cristal et l'Éther (au pied d'un Calice terminé) avec les mêmes cadences que les Techniciens (5 Cristal en 2,8 s, 4 Éther en 2 s) et rapportent à l'Arbre-mère. Coûts (P = Cristal, R = Éther), temps et population : Pousse 50 P, 12 s, 1 ; Roncier 50 P, 16 s, 1 ; Sourcier 75 P + 25 R, 22 s, 2 ; Lanceur de spores 125 P + 50 R, 30 s, 3 ; Gardien 250 P + 150 R, 55 s, 6 ; Samare 75 P + 25 R, 20 s, 2. Le Sourcier attend la « Fontaine de sève », le Lanceur la « Champignonnière » (palier 2, à concevoir) ; Gardien et Samare n'ont pas encore de bâtiment.

- [ ] Asymétrie de récolte des Sylves (pour l'instant identique à la Coalition)
- [ ] Palier 2 des Sylves (Fontaine de sève, Champignonnière…)

Également en exploration visuelle : **le Collectif**, une joaillerie mécanique à silhouette vivante (`OPENRTS_FACTION=collectif`). Il porte l'idée du grand vaisseau porte-drones (l'Essaimeur). Ce n'est pas non plus une décision : le choix de la seconde faction, entre le Collectif, les Sylves ou une autre, reste ouvert. Rôles provisoires, pensés en contraste avec les deux autres (beaucoup d'unités volantes, nuées de petites machines, peu d'unités au sol mais chères et solides) :

| Unité | Rôle provisoire | Silhouette |
| --- | --- | --- |
| Collecteur | Ouvrier volant | Scarabée volant : corps d'émail vert, tête d'or, élytres d'émail, ailes translucides qui battent, émeraude sous le ventre |
| Automate | Fantassin à rayon, plus cher et plus solide qu'une Sentinelle | Automate couronné : plaques d'or et d'émail vert, plastron en écu serti d'une émeraude, couronne de trois lames d'or, baguette à filet émeraude |
| Perceur | Assaut anti-blindé, rapide | Scorpion-collier : carapace d'or et cabochon serti sur le dos, queue en rang de perles d'or terminée par une émeraude taille brillant, pinces en anneaux ouverts |
| Essaim | Nuée de harcèlement, volante et rapide | Sept guêpes d'or et d'émail en orbite sur deux anneaux qui tournent en sens contraires |
| Essaimeur | Vaisseau porte-drones, lent | Bracelet ouvert qui vole à plat : jonc d'or serti de sept cabochons, émeraudes aux bouts et en pendants, orbe d'émeraude qui tourne dans l'ouverture, escorte de six anneaux en orbite |
| Titan | Géant de siège, très lent | Colosse serti : buste hexagonal d'or ceint d'émeraude, cœur d'émeraude qui tourne, canon hexagonal, épaules en prismes hexagonaux, couronne de cinq lames d'or |

- [ ] Essaimeur : vraie mécanique de lancement de drones (fabrication, lancement, retour, perte), à concevoir avec le combat (étape 5). Pour l'instant, son escorte et les Essaims sont purement visuels.
- [x] Unités volantes : couche de collision séparée. Elles ignorent la grille de navigation et les obstacles au sol, ne se bousculent qu'entre elles, et passent au-dessus des unités au sol.

## Économie et ressources

Deux ressources, qui créent des choix d’expansion et de timing.

| Ressource | Rôle | Disponibilité |
| --- | --- | --- |
| Ressource principale | Le gros des unités et des bâtiments | Abondante |
| Ressource rare | Technologie avancée, unités d’élite | Limitée, pousse à l’expansion |

Des ouvriers récoltent et rapportent les ressources au bâtiment principal. Les bâtiments produisent des unités via une file de production. Une limite de population (équivalent du ravitaillement) plafonne l’armée et se débloque par des bâtiments.

À étudier : un contrôle de territoire (balises à capturer, comme dans MechCom) pour pousser à l’action tôt, sans faire du rush la seule stratégie viable.

- [ ] Noms et thème des deux ressources. *Noms provisoires (étape 4) : **Cristal** (principale : grappes de cristaux bleu glacier sur socles veinés de lumière) et **Éther** (rare : lueur violette d'une faille, récoltée par un Extracteur posé dessus). À remplacer dans `crates/rts_game/src/gisements.rs`.*

Valeurs proposées à l'étape 4 (à ajuster en test) :

| Sujet | Valeur |
| --- | --- |
| Base de départ | 8 gisements de Cristal 2 × 1 m en arc à 2–4 m derrière le Quartier général (900 près, 1 500 au fond), 2 sources d'Éther 4 × 4 m de 2 250 sur ses flancs |
| Récolte | 5 Cristal par voyage en 2,8 s d'extraction ; 4 Éther en 2 s ; un seul ouvrier à la fois par gisement (saturation : 2 à 3 par gisement) |
| Population fournie | Quartier général 15, Module d'habitat 8 ; plafond global réglable, 200 par défaut |
| Temps de production | Technicien 12 s, Sentinelle 18 s, Longue-Vue 25 s, Lancier 22 s, Bastion 45 s, Faucon 25 s |
| Stock de départ | 50 Cristal |
| Annulation | Chantier et unité en file remboursés en entier (à revoir avec le combat : 75 % comme ailleurs ?) |

Décisions prises à l'étape 4 :

- Le coût d'une unité est payé et sa population réservée dès sa mise en file (5 unités au plus par bâtiment) ; si la limite baisse ensuite, la file s'arrête.
- Les ouvriers rapportent au dépôt le plus proche (bâtiments marqués « dépôt » : le Quartier général).
- Tous les ordres de joueur passent par une file unique, appliquée au pas de simulation suivant, après vérification (propriétaire, coût, prérequis) : fondement du lockstep.
- Prérequis, stocks et population sont comptés par joueur (composant `Owner`), même s'il n'y a qu'un joueur humain pour l'instant.

## Unités, combat et lisibilité

Chaque unité doit être identifiable à sa seule silhouette, à la taille d’un pouce sur iPad. C’est une règle de design posée avant tout modèle 3D.

Règles de lisibilité :

- Taille, forme générale et façon de bouger différentes pour chaque unité, pas seulement des détails.
- Couleur d’équipe bien visible sur chaque unité.
- Cercle de sélection au sol et barre de vie lisibles en dezoom.

Combat :

- Système de forces et faiblesses (corps à corps, distance, blindé, aérien…) pour que la composition d’armée compte.
- Portée, dégâts, armure, ciblage automatique des ennemis à portée.
- Attaque-déplacement par défaut : une unité attaquée riposte toujours.

Visuels : modèles procéduraux assemblés de pièces simples, décrits en données (voir « Direction artistique »), puis éventuellement modèles 3D originaux (Blender, packs libres).

### Unités de la Coalition

Six unités pour la première version. Chiffres de départ inspirés des ratios de SC2, à ajuster en test (P = ressource principale, R = ressource rare).

| Unité | Rôle | Coût | PV | Dégâts | Portée | Pop. | Silhouette |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Technicien | Ouvrier : récolte, construit, répare | 50 P | 40 | Faibles, contact | Contact | 1 | Exosquelette de chantier trapu, outil lumineux |
| Sentinelle | Fantassin polyvalent, rapide | 50 P | 50 | 6 | 5 | 1 | Soldat en armure, fusil, visière lumineuse |
| Longue-Vue | Tireur longue portée, tir lent | 100 P | 40 | 18 | 9 | 2 | Grand et fin, long canon, s’agenouille pour tirer |
| Lancier | Anti-blindé | 100 P + 25 R | 70 | 10 (+25 contre blindés) | 6 | 2 | Lance-missiles à l’épaule, silhouette asymétrique |
| Bastion | Blindé lourd, lent, dégâts de zone | 150 P + 100 R | 300 | 30 en zone | 6 | 4 | Char à chenilles large et bas, deux fois une Sentinelle |
| Faucon | Raid et harcèlement, très rapide | 75 P + 25 R | 45 | 8 | 4 | 2 | Soldat léger penché en avant, gros jetpack, flammes visibles |

Triangle de contres : les Sentinelles en masse débordent les Lanciers, les Lanciers détruisent les Bastions, les Bastions écrasent les groupes de Sentinelles. Le Longue-Vue punit l’infanterie immobile ; le Faucon harcèle les Techniciens.

Capacités du Faucon :

- **Bond** : saute par-dessus falaises et obstacles. Demande un terrain à plusieurs niveaux et un pathfinding qui gère le franchissement, à prévoir dès l’étape 2.
- **Seconde capacité, à trancher en test** : frappe plongeante (option privilégiée, axée étourdissement plutôt que dégâts pour ne pas doubler le Bastion) ou vol stationnaire (hors de portée du corps à corps quelques secondes).

### Direction artistique

Référence : la page de direction artistique 3D des races (Coalition, Collectif, Sylves).

- Rendu : HDR et tone mapping filmique, bloom réservé aux pièces émissives (visières, réacteurs), ombres douces, éclairage d'ambiance par image teinté aux couleurs de la faction, une lumière principale et deux contre-jours colorés, brouillard de distance.
- Unités : assemblages de 20 à 30 pièces simples (plaques biseautées, cylindres, prismes) décrites en données, matériaux PBR métal/rugosité avec normal maps procédurales (panneaux, rivets, hexagones).
- Chaque faction fournit sa palette et son ambiance. Coalition : métal sombre #171C24, acier #56667A, sable #CDBB90, visières #4FD1E0 ; ambiance nocturne bleutée, contre-jours orange et cyan, braises. Bandes de danger à chevrons (couleur d'équipe et noir) sur les jupes et le glacis du Bastion et sur le Technicien ; caisses et tourelles en prismes biseautés, réacteurs en formes de révolution. Animations purement visuelles : la tourelle du Bastion balaie, les flammes additives du Faucon scintillent, le radar du Longue-Vue tourne, réacteurs et gyrophares pulsent (les visières restent fixes, pour les performances).
- Sylves (exploration) : écorce striée, feuillage sombre, poches de sève lumineuse. Écorce sombre #2A1E15, écorce #5A4030, feuillage #1B4221, sève #E0A33A, spores #2FE0C4 ; sous-bois nocturne vert, contre-jours turquoise et ambré, spores qui montent du sol, sol d'humus. Formes organiques (boules bosselées, branches courbes, formes de révolution, touffes de feuilles) et normal map d'écorce ; la couleur d'équipe passe sur les feuilles, bourgeons, carapaces, pétales et ailes, visibles de dessus.
- Collectif (exploration) : des machines-bijoux, « or jaune, émail vert et émeraudes » — des machines et créatures dont les lignes et les ornements évoquent la joaillerie, pas des bijoux littéraux. Or #E2B552, or vieilli #B98A2F, émail vert #0F3D2E, émeraude #10B981, halo #6EE7B7. Surfaces lisses sans texture de panneaux ; plaques d'or et d'émail, prismes hexagonaux, sertis et cabochons, rangs de perles, couronnes de lames, émeraudes taillées (brillant), anneaux ouverts (tore partiel du moteur). La couleur d'équipe est un émail laqué sur les pièces vues de dessus (joug et crête du Titan, plastron des Automates, cabochon et perles serties du Perceur, élytres, abdomens des guêpes, cabochons de l'Essaimeur). Ambiance de vitrine de nuit : contre-jours émeraude et or chaud, panneaux de reflet nets dans l'éclairage d'ambiance, sol sombre en dalles hexagonales. Seules les trois grosses pierres pulsent (cœur du Titan, dard du Perceur, orbe de l'Essaimeur) ; ailes des guêpes réduites pour qu'une masse d'Essaims reste lisible en dézoom. L'essai « tout or rose » reste disponible (`OPENRTS_FACTION=collectif-or-rose`).
- Couleur d'équipe : portée par les épaulières, genouillères, crêtes, bandes de toit, surfaces vues de dessus pour rester lisibles en dézoom. L'orange signal #E8732C est la couleur d'équipe du premier joueur.
- Lisibilité d'abord : silhouettes très différentes (fantassin, tireur grand et fin, lancier asymétrique, faucon penché sur ses réacteurs, ouvrier trapu, char large et bas), anneau de sélection lumineux et épais.
- Deux qualités graphiques (Haute, Basse) pour tenir 300 unités par joueur sur iPad.

## Arbre technologique

Un arbre à vrais embranchements : se spécialiser dans une voie retarde les autres, faute de ressources et de temps. C’est ce qui crée des stratégies variées et limite la domination du rush.

- Bâtiments qui débloquent de nouvelles unités et d’autres bâtiments.
- Améliorations (dégâts, armure, vitesse, capacités spéciales) achetées avec la ressource rare.
- Arbre propre à chaque faction, décrit en données (fichiers de configuration) et non codé en dur, pour être facile à modifier et à équilibrer.

Décision : pas de système de doctrine. Toutes les unités et améliorations sont accessibles, comme dans SC2 ; la stratégie vient de l’ordre des investissements.

### Arbre de la Coalition

1. **Palier 1, les fondations** : Quartier général (Techniciens, dépôt des ressources), Module d’habitat (population), Extracteur (ressource rare), Caserne (Sentinelles), Tourelle (défense fixe, débloquée par la Caserne).
2. **Palier 2, la spécialisation** (demande une Caserne) : Armurerie (Longue-Vue et Lanciers à la Caserne, améliorations infanterie), Centre de propulsion (Faucons à la Caserne, amélioration de leur capacité), Usine (Bastions, demande l’Armurerie ; améliorations véhicules).
3. **Palier 3, la puissance** : amélioration du Quartier général (débloque les améliorations de niveau 2 et 3, plus de PV, détection), puis un bâtiment dédié aux unités aériennes, plus tard.

Construction de la Coalition : **l'ouvrier reste sur le chantier**. Le Technicien le plus proche (parmi la sélection) s'y rend ; le chantier est posé tout de suite (c'est déjà un obstacle, ses PV montent de 10 % à 100 %) et n'avance que tant qu'un Technicien le touche. Les autres factions construisent autrement (les Sylves : l'ouvrier devient le bâtiment, qui pousse seul) : c'est l'asymétrie voulue, prévue dans le moteur (`ConstructionStyle`). Tailles retenues : Quartier général 7 × 7 m, Caserne 5 × 6, Extracteur 4 × 4, Module d'habitat 3 × 3, Tourelle 2 × 2 ; la Caserne demande le Quartier général, la Tourelle la Caserne. L'Extracteur se pose exactement sur une source de ressource rare libre. Production : le Quartier général forme les Techniciens, la Caserne les Sentinelles (Longue-Vue et Lancier attendent l'Armurerie, le Faucon le Centre de propulsion : grisés en attendant).

Améliorations par niveaux (armes 1 à 3, armure 1 à 3, séparées pour l’infanterie et les véhicules) : longues à rechercher et payées en ressource rare, impossibles à mener toutes en parallèle.

- [ ] Unités aériennes de la Coalition

## Contrôles

Toutes les entrées passent par une couche d’abstraction : le jeu ne connaît que des **actions** (sélectionner, déplacer, attaquer, construire…), déclenchées par la souris, le clavier ou le tactile. Sur iPad, le jeu se joue au doigt, avec le clavier en option ; la souris n’est pas une cible sur iPad.

| Action | Ordinateur | iPad (doigt) |
| --- | --- | --- |
| Sélectionner une unité | Clic | Tap |
| Sélection en rectangle | Glisser à la souris | Glisser à un doigt |
| Toutes les unités du même type | Double-clic | Double-tap |
| Toute l’armée | Raccourci clavier | Bouton à l’écran |
| Ordre contextuel (déplacer, attaquer, récolter) | Clic droit | Tap sur le sol, l’ennemi ou la ressource |
| Déplacer la caméra | Bords d’écran, flèches | Glisser à deux doigts |
| Zoom | Molette | Pincer |
| Groupes de contrôle | Touches 1 à 0 | Barre de groupes toujours visible |
| Construction, capacités | Grille de raccourcis | Boutons à l’écran |

Sur iPad, un clavier connecté donne accès aux mêmes raccourcis que sur ordinateur (groupes 1 à 0, grille de raccourcis), pour un jeu rapide.

Pour diriger de grandes armées au doigt, prévoir des outils dédiés : formations, ordres de zone, ajout et retrait d’unités dans la sélection.

Un prototype iPad, même très simple, est construit tôt pour tester ces gestes en conditions réelles.

## Échelle des batailles et performances

Cible : 200 à 300 unités par joueur, fluides sur Mac et sur iPad. Le moteur doit permettre de monter plus haut plus tard.

- **ECS de Bevy** : les unités sont traitées par lots dans des systèmes.
- **Pathfinding** : A\* sur grille pour commencer, puis flow fields quand les groupes grossissent.
- **Rendu instancié** : un appel de dessin par modèle, modèles simplifiés en dezoom.
- **Limite d’unités réglable** selon l’appareil, pour gérer chauffe et batterie sur iPad.
- **Mode test de charge** disponible dès que les unités se déplacent : faire apparaître 300 unités d’un coup et vérifier la fluidité.
- **Logique déterministe** (simulation indépendante de l’affichage, pas d’aléatoire non contrôlé), pour permettre un futur multijoueur en lockstep où seuls les ordres des joueurs transitent.

## Feuille de route

On avance par étapes courtes, chacune jouable et testée avant de passer à la suivante.

1. **Bases** : terrain plat éclairé, caméra inclinée façon SC2 (défilement, zoom), une dizaine d’unités en formes simples, sélection au clic et au rectangle avec cercle au sol, déplacement au clic droit, couche d’entrées, mode test 300 unités.
2. **Pathfinding** : grille de navigation, obstacles, A\*, déplacements de groupe sans chevauchement. *Fait (sauf le Bond du Faucon et le terrain à plusieurs niveaux) :*
   - grille de cases de 1 m (réglable, `NavSettings::cell_size`), cases bloquées par les obstacles fixes, **dégagement** de chaque case (distance au plus proche obstacle) : une unité de rayon r passe par une case de dégagement ≥ r − 0,25 m, donc le Bastion évite les passages où seule l'infanterie passe ;
   - A\* déterministe (coûts entiers, heuristique octile, départage fixe, pas de coupe de coin), chemin lissé en « tirant la ficelle » ; cible dans un obstacle : case libre la plus proche ; cible inatteignable : on s'approche au plus près ;
   - les unités d'un même ordre et de même rayon partagent une seule recherche (un couloir commun qu'elles rejoignent et quittent en ligne droite), et un budget de cases explorées par pas étale le reste : 300 unités peuvent recevoir un ordre en même temps. Quand les groupes grossiront, des **flow fields** (un champ de directions par destination) remplaceront ce partage ;
   - les obstacles fixes repoussent les unités au sol (cercle contre rectangle), y compris celles sur lesquelles on pose un bâtiment.
3. **Prototype iPad** : compilation iOS, gestes tactiles de base, test des 300 unités sur iPad.
4. **Économie** : ouvriers, deux ressources, bâtiment principal, file de production, limite de population. *Fait pour la Coalition :*
   - bâtiments décrits en données (empreinte, PV, temps, coût, prérequis, modèle, dépôt, population fournie, production), pose par la couche d'entrées (B puis une lettre, ou barre de boutons ; fantôme vert ou rouge aligné sur la grille), chantiers et construction ;
   - joueurs propriétaires (`Owner`) et file unique des ordres appliquée au pas suivant (prête pour le lockstep) ;
   - gisements de Cristal et sources d'Éther décrits en données de carte, Extracteur posé obligatoirement sur une source ;
   - récolte déterministe (aller, attente de son tour, extraction, retour au dépôt, gisement voisin quand l'un s'épuise), charge visible ;
   - coûts débités à la pose, chantiers annulables et remboursés ; files de production de 5 décrites en données, annulation remboursée, point de ralliement (sur un gisement : récolte directe) ; population par joueur avec plafond global réglable ;
   - interface : compteurs en haut à droite, panneau de production (boutons et raccourcis par la même `GameAction`, file et progression), panneau de chantier, messages courts.
   - *Fait pour les Sylves (propositions) :* style de construction « la Pousse devient le bâtiment » (générique dans le moteur), cinq bâtiments du palier 1, économie de base, partie de départ (`OPENRTS_FACTION=sylves`) et démonstration (`OPENRTS_DEMO=buildings`).
   *Reste : économie du Collectif, récolte propre aux Sylves, contrôle de territoire à étudier.*
5. **Combat** : portée, dégâts, armure, ciblage automatique, attaque-déplacement, mort des unités.
6. **Groupes de contrôle et interface** : barre de groupes, boutons d’ordres, raccourcis, minicarte.
7. **Brouillard de guerre et IA adverse** basique qui construit et attaque par vagues.
8. **Première faction complète** : arbre technologique, unités distinctes, vrais modèles 3D.
9. **Seconde faction asymétrique** et équilibrage.
10. **Multijoueur** en lockstep déterministe.

## Règles de travail pour Claude Code

Ce document est la référence du projet : à placer à la racine du dépôt sous le nom `GAME_DESIGN.md` et à relire avant chaque étape.

- Je débute en Rust : expliquer brièvement chaque concept nouveau (ownership, borrow checker, traits, ECS…).
- Travailler une étape de la feuille de route à la fois ; compiler et lancer le jeu pour vérifier avant de déclarer une étape terminée.
- Respecter la séparation moteur / jeu et la couche d’entrées par actions.
- Décrire unités, bâtiments et arbres technologiques en données, pas en dur dans le code.
- Garder la simulation déterministe et séparée de l’affichage.
- Code lisible et commenté pour des contributeurs open source ; README à jour (compilation, captures).
- Ne jamais reprendre de nom, visuel ou son de StarCraft, ni de code sous licence GPL.
- Mettre à jour ce document quand une décision de design change.
