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

Première faction : **la Garde** (nom décidé ; anciennement « Coalition »), des humains militaires futuristes. La seconde faction reste à définir ; elle accueillera un grand vaisseau porte-drones qui lance une nuée de petits chasseurs, idée réservée à une race non humaine.

- [x] Nom définitif de la première faction : la Garde
- [ ] Thème et style de jeu de la seconde faction
- [ ] Troisième faction, plus tard

En cours d'exploration visuelle : **les Sylves**, une forêt consciente qui avance racine après racine (`OPENRTS_FACTION=sylves`). Ce n'est pas encore une décision de design : rien ne dit qu'elles seront la seconde faction, et elles n'accueillent pas le vaisseau porte-drones prévu pour celle-ci. Rôles provisoires, pensés en contraste avec la Garde (lentes, organiques, fortes au contact, aucune infanterie rapide à distance) :

| Unité | Rôle provisoire | Silhouette |
| --- | --- | --- |
| Pousse | Ouvrière | Petit bulbe de mousse, deux feuilles dressées |
| Roncier | Mêlée en nombre, rapide | Bas et trapu sur quatre pattes en racines, carapaces d'écorce |
| Sourcier | Soutien, soigne les alliés | Lanterne de sève sur le dos, collerette de pétales |
| Lanceur de spores | Artillerie de zone, lente | Racines en étoile, énorme cosse piquée de spores |
| Gardien | Géant qui encaisse | Arbre qui marche, grande couronne de feuillage |
| Samare | Éclaireur volant, harcèlement | Graine à deux ailes qui plane |

Bâtiments et économie des Sylves (**propositions**, étape 4, à valider et ajuster en test). Palier 1 aux mêmes rôles que celui de la Garde, formes tirées de la forêt consciente (racines, souches, champignons, poches de sève) :

| Bâtiment | Rôle | Empreinte | PV | Temps | Coût | Prérequis | Touche (après B) | Silhouette |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Arbre-mère | Cœur de base : dépôt, produit les Pousses, 15 de population | 7 × 7 m | 1 500 | 75 s | 350 P | — | A | Grand arbre de 6,5 m sur sept racines, poche de sève en façade (le dépôt), couronne de feuilles et bourgeons d'équipe |
| Souche nourricière | Population (+8) | 3 × 3 m | 350 | 20 s | 75 P | — | S | Large souche coupée, cerne pâle, trois champignons aux chapeaux d'équipe |
| Calice | Ressource rare : posé sur une source, il boit l'Éther | 4 × 4 m | 450 | 22 s | 50 P | — (sur une source libre) | C | Fleur-coupe sur six racines plongées dans la faille, nectar de spores, sept pétales d'équipe |
| Ronceraie | Infanterie : Ronciers (Sourcier, Lanceur de spores grisés) | 5 × 5 m | 1 000 | 45 s | 125 P | Arbre-mère | R | Trois arches de ronces épineuses en voûte, trois cocons d'équipe dessous |
| Épineux | Défense fixe | 2 × 2 m | 300 | 22 s | 100 P | Ronceraie | E | Bulbe d'écorce, tête qui balaie : couronne d'épines, collerette d'équipe, dard |

Construction des Sylves : **la Pousse devient le bâtiment** (`ConstructionStyle::WorkerConsumed` dans le moteur). À la pose, le coût est payé et l'emplacement promis à la Pousse libre la plus proche ; elle s'y rend, s'enracine et disparaît (sa population est libérée) ; le chantier pousse ensuite seul pendant le temps de construction, sans ouvrier. C'est l'asymétrie voulue face à la Garde, dont le Technicien reste sur le chantier : les Sylves ne perdent pas de temps d'ouvrier mais perdent l'ouvrier, d'où des prix en Cristal un peu plus bas. Règles proposées :

- Une Pousse ne fait qu'un bâtiment : sans Pousse libre dans la sélection, la pose est refusée (« Aucun ouvrier libre »).
- Un autre ordre donné à la Pousse avant qu'elle arrive annule la promesse, remboursée en entier. Si l'emplacement est pris entre-temps, la promesse est remboursée et la Pousse s'arrête.
- Annuler un chantier qui pousse rend **75 %** de son coût et **la Pousse**, qui ressort de terre (même au-delà de la limite de population : la production attend alors qu'il y ait de la place).
- Économie : les Pousses récoltent le Cristal et l'Éther (au pied d'un Calice terminé) avec les mêmes cadences que les Techniciens (5 Cristal en 2,8 s, 4 Éther en 2 s) et rapportent à l'Arbre-mère. Coûts (P = Cristal, R = Éther), temps et population : Pousse 50 P, 12 s, 1 ; Roncier 50 P, 16 s, 1 ; Sourcier 75 P + 25 R, 22 s, 2 ; Lanceur de spores 125 P + 50 R, 30 s, 3 ; Gardien 250 P + 150 R, 55 s, 6 ; Samare 75 P + 25 R, 20 s, 2. Le Sourcier attend la « Fontaine de sève », le Lanceur la « Champignonnière » (palier 2, à concevoir) ; Gardien et Samare n'ont pas encore de bâtiment.

- [ ] Asymétrie de récolte des Sylves (pour l'instant identique à la Garde)
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

Bâtiments et économie du Collectif (**propositions**, étape 4, à valider et ajuster en test). Palier 1 aux mêmes rôles que ceux de la Garde et des Sylves, dans la direction artistique actuelle : des machines aux lignes de joaillerie (socles hexagonaux d'émail vert, griffes de chaton, arches d'or, rangs de perles, émeraudes taillées), pas des bijoux posés au sol ; l'émail d'équipe sur les surfaces vues de dessus.

| Bâtiment | Rôle | Empreinte | PV | Temps | Coût | Prérequis | Touche (après B) | Champ | Silhouette |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Écrin | Cœur de base : dépôt, produit les Collecteurs, 15 de population | 7 × 7 m | 1 500 | 75 s | 400 P | — | E | Émet 8 m | Socle hexagonal d'émail, plateau d'équipe cerclé de perles, six griffes d'or tenant une grosse émeraude qui tourne, arche d'or du dépôt en façade |
| Résonateur | Relais : population (+8), étend le champ | 2 × 2 m | 300 | 25 s | 100 P | — | R | Émet 6,5 m | Colonne d'or sur pied d'équipe, trois griffes tenant une émeraude qui pulse, anneau d'or ouvert qui tourne |
| Creuset | Ressource rare : posé sur une source, il distille l'Éther | 4 × 4 m | 450 | 25 s | 75 P | — (sur une source libre) | C | — | Coupe d'or sur trois jambes plongées dans la faille, nappe émeraude, bord d'équipe, anse d'or et pierre pendue |
| Orfèvrerie | Infanterie : Automates, Perceurs (avec un Lapidaire) | 5 × 5 m | 1 000 | 50 s | 150 P | Écrin ; dans un champ | O | — | Dalle d'émail, deux arches d'or en croix, quatre losanges d'équipe, piliers coiffés d'émeraudes |
| Aiguillon | Défense fixe | 2 × 2 m | 300 | 25 s | 100 P | Orfèvrerie ; dans un champ | A | — | Tour d'or sur pied d'équipe, tête qui balaie : cabochon d'équipe, longue aiguille d'or, émeraude, lames de couronne |

Palier 2 du Collectif (**propositions**), en miroir des rôles de celui de la Garde (Armurerie, Centre de propulsion, Usine) avec une identité propre : tout passe par la **taille des grosses pierres** (le dard du Perceur, l'orbe de l'Essaimeur, le cœur du Titan), et l'air, fort chez le Collectif, a son propre atelier au lieu d'une simple option à l'Orfèvrerie. Le palier 2 coûte de l'Éther : il faut un Creuset avant de s'y lancer. Tous se posent dans un champ de résonance.

| Bâtiment | Rôle | Empreinte | PV | Temps | Coût | Prérequis | Touche (après B) | Champ | Silhouette |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Lapidaire | Spécialisation (miroir de l'Armurerie) : débloque le Perceur à l'Orfèvrerie, l'Essaimeur à la Volière et la Fonderie ; ne produit rien | 4 × 4 m | 750 | 50 s | 150 P + 100 R | Orfèvrerie ; dans un champ | L | — | Socle hexagonal d'équipe, meule d'or rayée d'émeraude qui tourne à plat, bras d'or coudé sur un pilier couronné tenant une émeraude taille brillant pointe en bas, présentoir de trois pierres |
| Volière | Atelier aérien (miroir du Centre de propulsion) : Essaims (E), Essaimeurs (M, avec un Lapidaire) | 5 × 5 m | 1 000 | 55 s | 150 P + 100 R | Orfèvrerie ; dans un champ | V | — | Aire d'envol d'équipe (cœur d'émail) sous un dôme ajouré de six côtes d'or, ceinture d'or vieilli, couronne et flèche d'émeraude ; des guêpes tournent autour sur deux anneaux |
| Fonderie | Géants (miroir de l'Usine) : Titans (T) | 6 × 6 m | 1 250 | 65 s | 200 P + 150 R | Lapidaire ; dans un champ | F | — | Portique en H : deux pylônes hexagonaux d'or couronnés, poutre au dos d'équipe, cloche-moule d'or pendue d'où file un jet émeraude sur un lit de coulée d'or bordé d'équipe |

Les touches de production (bâtiment sélectionné) : C (Collecteur, Écrin) ; A, P (Orfèvrerie) ; E, M (Volière) ; T (Fonderie).

Construction du Collectif : **le sertissage à distance** (`ConstructionStyle::RemoteAnchor` dans le moteur). À la pose, le coût est payé et l'emplacement promis au Collecteur libre le plus proche ; il vole jusque-là, pose une « gemme d'ancrage » (le chantier apparaît, la gemme tourne au-dessus) et repart aussitôt à sa récolte ; le bâtiment s'assemble seul pendant son temps de construction. C'est la troisième asymétrie : la Garde immobilise son ouvrier, les Sylves le perdent, le Collectif ne perd que quelques secondes de vol, mais ne bâtit l'essentiel que dans ses **champs de résonance**. Règles proposées :

- Champ de résonance : un cercle centré sur l'Écrin (8 m) ou sur un Résonateur (6,5 m), actif seulement une fois le bâtiment terminé. L'Orfèvrerie, l'Aiguillon et le palier 2 ne se posent que si le centre de leur empreinte est dans un champ du joueur (fantôme rouge et message « Hors champ » sinon) ; l'Écrin, le Résonateur et le Creuset se posent partout. Les champs sont dessinés au sol pendant la pose.
- Un Collecteur n'a qu'un emplacement promis à la fois (« Aucun ouvrier libre » si toute la sélection est déjà en route). Un autre ordre donné avant l'arrivée annule la promesse, remboursée en entier ; l'emplacement est revérifié à l'arrivée (champ compris).
- Annuler un chantier qui s'assemble rend **75 %** de son coût (le Collecteur, lui, n'a rien perdu).
- Plus tard, avec le combat : un bâtiment qui n'est plus dans aucun champ (Résonateur détruit) sera désactivé. La requête existe déjà dans le moteur (`buildings::field::in_active_field`), elle n'est pas encore appliquée.
- Économie : les Collecteurs récoltent le Cristal et l'Éther (au bord d'un Creuset terminé) avec les mêmes cadences que les autres (5 Cristal en 2,8 s, 4 Éther en 2 s) ; ils volent tout droit, par-dessus les bâtiments, sans chemin au sol. Coûts, temps et population : Collecteur 50 P, 12 s, 1 ; Automate 100 P, 24 s, 2 ; Perceur 125 P + 50 R, 30 s, 3 ; Essaim 75 P + 50 R, 25 s, 2 ; Essaimeur 300 P + 200 R, 60 s, 6 ; Titan 350 P + 250 R, 70 s, 8. Toutes ont leur bâtiment : Collecteur à l'Écrin, Automate et Perceur (avec un Lapidaire) à l'Orfèvrerie, Essaim et Essaimeur (avec un Lapidaire) à la Volière, Titan à la Fonderie.

- [x] Palier 2 du Collectif : Lapidaire, Volière, Fonderie (propositions)
- [ ] Désactivation des bâtiments hors champ, avec le combat
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

**Interprétation de « attaque-déplacement par défaut » (étape 5).** L'ordre par défaut — clic droit, ou tap sur le sol — est une **attaque-déplacement** pour les unités armées : elles vont au point visé en combattant tout ennemi qu'elles croisent, puis s'y mettent au repos. C'est l'option la plus fidèle au texte, et la meilleure au doigt (pilier n° 1) : l'ordre le plus fréquent d'une armée ne demande qu'un tap. Le **déplacement simple**, qui ignore les ennemis, sert à battre en retraite ou à contourner : touche M puis clic, ou bouton « Déplacer » de la barre d'ordres (deux taps, moins fréquent). A puis clic donne explicitement une attaque-déplacement (ouvriers compris), comme dans SC2 ; sur un ennemi, A puis clic ou clic droit l'attaque. Exceptions : les ouvriers et les unités sans arme font un déplacement simple au clic droit (pour qu'un ouvrier envoyé à l'expansion ne parte pas se battre). « Une unité attaquée riposte toujours » vaut pour toute unité au repos, en attaque-déplacement ou qui tient la position (si l'attaquant est à portée) ; elle ne vaut pas pendant un déplacement simple (une retraite) ni pour un ouvrier occupé à récolter ou construire. À revoir en test sur iPad.

Règles retenues à l'étape 5 (moteur, module `combat`) :

- Dégâts d'un coup : dégâts de l'arme + bonus contre une catégorie de la cible − armure de la cible, au moins 0,5 (comme l'armure plate de SC2). Arme de zone : tous les ennemis à portée de zone autour de la cible prennent les mêmes dégâts (moins leur armure), pas de tir ami.
- Catégories : infanterie, blindé, aérien, mêlée, distance, ouvrier, géant, bâtiment (tous les bâtiments sont aussi « blindés »). « Aérien » est une donnée de combat, indépendante de la couche de collision : le Faucon flotte mais reste une cible au sol (fantassin à réacteurs).
- Portée mesurée de bord à bord (cercles au sol, rectangle d'un bâtiment). Portée d'acquisition : plus longue portée + 3 m, au moins 6 m. Laisse : 5 m de plus, mesurés depuis la position de repos ; après le combat, l'unité y revient.
- Priorité de cible : ce qui attaque (a une cible), puis les autres unités, puis les bâtiments qui ne tirent pas ; à égalité le plus proche, puis le plus petit identifiant. Recherche complète un pas sur huit (en décalé), riposte immédiate.
- Tirs instantanés, dégâts appliqués au pas du tir, tous ensemble (deux unités peuvent s'entretuer). Les défenses fixes tirent comme une unité qui tient la position, pas en chantier.
- Mort : l'entité disparaît. Population, grille de navigation, récolte, chantiers, ralliement et cibles revérifient leurs références ; un ouvrier tué en route vers un emplacement promis est remboursé ; les unités en file d'un bâtiment détruit sont perdues.

Valeurs de combat (Garde : PV, dégâts et portées du tableau ci-dessous ; **le reste est proposé**, à ajuster en test). Arme : dégâts / portée en m / secondes entre deux coups / cibles.

| Garde | PV | Armure | Catégories | Arme |
| --- | --- | --- | --- | --- |
| Technicien | 40 | 0 | infanterie, ouvrier, mêlée | 5 / contact / 1,5 / sol |
| Sentinelle | 50 | 0 | infanterie, distance | 6 / 5 / 0,6 / sol et air |
| Longue-Vue | 40 | 0 | infanterie, distance | 18 / 9 / 2,0 / sol |
| Lancier | 70 | 1 | infanterie, distance | 10 (+25 blindés) / 6 / 1,5 / sol et air |
| Bastion | 300 | 2 | blindé, distance | 30 en zone de 1,6 m / 6 / 2,2 / sol |
| Faucon | 45 | 0 | infanterie, distance | 8 / 4 / 0,8 / sol |
| Tourelle | 250 | 1 | bâtiment | 12 / 7 / 0,9 / sol et air |

| Sylves (proposé) | PV | Armure | Catégories | Arme |
| --- | --- | --- | --- | --- |
| Pousse | 40 | 0 | infanterie, ouvrier, mêlée | 5 / contact / 1,5 / sol |
| Roncier | 60 | 1 | infanterie, mêlée | 7 / contact / 0,8 / sol |
| Sourcier | 70 | 0 | infanterie, distance | 5 / 4 / 1,2 / sol et air (le soin viendra plus tard) |
| Lanceur de spores | 110 | 0 | blindé, distance | 20 en zone de 2 m / 8 / 3,0 / sol |
| Gardien | 500 | 3 | blindé, géant, mêlée | 35 (+20 bâtiments) / 1 / 2,0 / sol |
| Samare | 50 | 0 | aérien, distance | 6 (+6 ouvriers) / 3 / 1,0 / sol et air |
| Épineux | 300 | 1 | bâtiment | 15 / 7 / 1,2 / sol et air |

| Collectif (proposé) | PV | Armure | Catégories | Arme |
| --- | --- | --- | --- | --- |
| Collecteur | 45 | 0 | aérien, ouvrier | 5 / 0,5 / 1,5 / sol |
| Automate | 90 | 1 | infanterie, distance | 9 / 6 / 0,9 / sol et air |
| Perceur | 130 | 1 | blindé, mêlée | 12 (+20 blindés) / 1,5 / 1,2 / sol |
| Essaim | 60 | 0 | aérien, mêlée | 4 / 1,5 / 0,4 / sol et air |
| Essaimeur | 350 | 2 | aérien, blindé, géant | 8 / 8 / 0,5 / sol et air (provisoire : représente ses drones) |
| Titan | 600 | 3 | blindé, géant, distance | 40 en zone de 1,8 m (+10 bâtiments) / 9 / 2,8 / sol |

Autres bâtiments : leurs PV de l'étape 4, armure 1, sans arme (le palier 2 du Collectif aussi).

- [x] Une partie à deux factions différentes : catalogues et règles communs à la partie, où chaque faction ajoute ses types à la suite ; chaque joueur a sa faction et ne construit que ses bâtiments. Partie normale : le joueur au nord, l'adversaire (`OPENRTS_ENEMY`) au sud, base en miroir.
- [ ] Projectiles simulés (temps de vol, esquive) si le jeu en a besoin ; tirs instantanés pour l'instant.
- [ ] Soin du Sourcier, drones de l'Essaimeur, capacités du Faucon.

Visuels : modèles procéduraux assemblés de pièces simples, décrits en données (voir « Direction artistique »), puis éventuellement modèles 3D originaux (Blender, packs libres).

### Unités de la Garde

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

Référence : la page de direction artistique 3D des races (Garde, Collectif, Sylves).

- Rendu : HDR et tone mapping filmique, bloom réservé aux pièces émissives (visières, réacteurs), ombres douces, éclairage d'ambiance par image teinté aux couleurs de la faction, une lumière principale et deux contre-jours colorés, brouillard de distance.
- Unités : assemblages de 20 à 30 pièces simples (plaques biseautées, cylindres, prismes) décrites en données, matériaux PBR métal/rugosité avec normal maps procédurales (panneaux, rivets, hexagones).
- Chaque faction fournit sa palette et son ambiance. Garde : métal sombre #171C24, acier #56667A, sable #CDBB90, visières #4FD1E0 ; ambiance nocturne bleutée, contre-jours orange et cyan, braises. Bandes de danger à chevrons (couleur d'équipe et noir) sur les jupes et le glacis du Bastion et sur le Technicien ; caisses et tourelles en prismes biseautés, réacteurs en formes de révolution. Animations purement visuelles : la tourelle du Bastion balaie, les flammes additives du Faucon scintillent, le radar du Longue-Vue tourne, réacteurs et gyrophares pulsent (les visières restent fixes, pour les performances).
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

### Arbre de la Garde

1. **Palier 1, les fondations** : Quartier général (Techniciens, dépôt des ressources), Module d’habitat (population), Extracteur (ressource rare), Caserne (Sentinelles), Tourelle (défense fixe, débloquée par la Caserne).
2. **Palier 2, la spécialisation** (demande une Caserne) : Armurerie (Longue-Vue et Lanciers à la Caserne, améliorations infanterie), Centre de propulsion (Faucons à la Caserne, amélioration de leur capacité), Usine (Bastions, demande l’Armurerie ; améliorations véhicules).
3. **Palier 3, la puissance** : amélioration du Quartier général (débloque les améliorations de niveau 2 et 3, plus de PV, détection), puis un bâtiment dédié aux unités aériennes, plus tard.

Construction de la Garde : **l'ouvrier reste sur le chantier**. Le Technicien le plus proche (parmi la sélection) s'y rend ; le chantier est posé tout de suite (c'est déjà un obstacle, ses PV montent de 10 % à 100 %) et n'avance que tant qu'un Technicien le touche. Les autres factions construisent autrement (les Sylves : l'ouvrier devient le bâtiment, qui pousse seul) : c'est l'asymétrie voulue, prévue dans le moteur (`ConstructionStyle`). Tailles retenues : Quartier général 7 × 7 m, Caserne 5 × 6, Extracteur 4 × 4, Module d'habitat 3 × 3, Tourelle 2 × 2 ; la Caserne demande le Quartier général, la Tourelle la Caserne. L'Extracteur se pose exactement sur une source de ressource rare libre. Production : le Quartier général forme les Techniciens, la Caserne les Sentinelles, puis Longue-Vue et Lanciers une fois l'Armurerie terminée et Faucons une fois le Centre de propulsion terminé (grisés en attendant) ; l'Usine forme les Bastions.

Palier 2 de la Garde, fait (bâtiments et prérequis ; **chiffres proposés**, à ajuster en test ; les améliorations viendront plus tard) :

| Bâtiment | Rôle | Empreinte | PV | Temps | Coût | Prérequis | Touche (après B) | Silhouette |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Armurerie | Débloque Longue-Vue et Lanciers à la Caserne | 4 × 4 m | 750 | 45 s | 150 P + 50 R | Caserne | A | Casemate trapue au glacis blindé en couches, meurtrière cyan ; râtelier de longs canons d'un côté, piles de plaques de blindage de l'autre, palan et casier de missiles sur le toit |
| Centre de propulsion | Débloque les Faucons à la Caserne | 5 × 5 m | 1 000 | 50 s | 150 P + 100 R | Caserne | P | Piste d'envol balisée ; portique d'essai où pendent deux gros réacteurs aux flammes vers un déflecteur à chevrons ; cabine de contrôle |
| Usine | Produit les Bastions (touche C) | 6 × 7 m | 1 250 | 60 s | 200 P + 100 R | Armurerie | U | Grand hangar au toit en dents de scie (verrières cyan), portique roulant devant la porte qui soulève une tourelle de Bastion, deux cheminées rougeoyantes |

Les trois coûtent de l'Éther : se spécialiser demande d'avoir lancé l'Extracteur. La Garde n'a plus aucune option « bâtiment pas encore dans le jeu » : ses six unités sont produisibles.

Améliorations par niveaux (armes 1 à 3, armure 1 à 3, séparées pour l’infanterie et les véhicules) : longues à rechercher et payées en ressource rare, impossibles à mener toutes en parallèle.

- [ ] Unités aériennes de la Garde

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
4. **Économie** : ouvriers, deux ressources, bâtiment principal, file de production, limite de population. *Fait pour la Garde :*
   - bâtiments décrits en données (empreinte, PV, temps, coût, prérequis, modèle, dépôt, population fournie, production), pose par la couche d'entrées (B puis une lettre, ou barre de boutons ; fantôme vert ou rouge aligné sur la grille), chantiers et construction ;
   - joueurs propriétaires (`Owner`) et file unique des ordres appliquée au pas suivant (prête pour le lockstep) ;
   - gisements de Cristal et sources d'Éther décrits en données de carte, Extracteur posé obligatoirement sur une source ;
   - récolte déterministe (aller, attente de son tour, extraction, retour au dépôt, gisement voisin quand l'un s'épuise), charge visible ;
   - coûts débités à la pose, chantiers annulables et remboursés ; files de production de 5 décrites en données, annulation remboursée, point de ralliement (sur un gisement : récolte directe) ; population par joueur avec plafond global réglable ;
   - interface : compteurs en haut à droite, panneau de production (boutons et raccourcis par la même `GameAction`, file et progression), panneau de chantier, messages courts.
   - *Fait pour les Sylves (propositions) :* style de construction « la Pousse devient le bâtiment » (générique dans le moteur), cinq bâtiments du palier 1, économie de base, partie de départ (`OPENRTS_FACTION=sylves`) et démonstration (`OPENRTS_DEMO=buildings`).
   - *Fait pour le Collectif (propositions) :* style de construction « sertissage à distance » et champs de construction (génériques dans le moteur), cinq bâtiments du palier 1 et trois du palier 2 (Lapidaire, Volière, Fonderie), économie de base avec un ouvrier volant, partie de départ (`OPENRTS_FACTION=collectif`) et démonstration (`OPENRTS_DEMO=buildings`).
   *Reste : récolte propre aux Sylves et au Collectif, contrôle de territoire à étudier.*
5. **Combat** : portée, dégâts, armure, ciblage automatique, attaque-déplacement, mort des unités. *Fait (valeurs proposées, voir « Unités, combat et lisibilité ») :*
   - un joueur adverse (sans IA) avec sa couleur d'équipe (bleu froid) : seuls les matériaux de couleur d'équipe sont recréés par joueur ; le rectangle ne sélectionne que ses propres entités, un clic sur un ennemi le montre (anneau rouge) sans pouvoir lui donner d'ordre ;
   - fiches de combat en données (PV, armure, catégories, armes, bonus, zone) pour les trois factions, défenses fixes armées ; ciblage automatique dans une grille spatiale, riposte, laisse ; ordres attaquer, attaque-déplacement, stop, tenir la position par la file des ordres, barre d'ordres (A, M, S, H) et mode visée jouables au doigt ;
   - tirs instantanés déterministes, morts et destructions qui libèrent population, grille, gisements ; tests sans fenêtre (duel, armure et bonus, riposte, attaque-déplacement, priorité, air/sol, déterminisme au bit près, ouvrier tué en récolte, bâtiment détruit) ;
   - lisibilité : barres de vie (sélection, blessés, survol ; agrandies en dézoom), traits de tir aux couleurs d'équipe, éclats, épaves et ruines qui s'effondrent, repères d'ordres rouges ou verts ;
   - démonstration `OPENRTS_DEMO=battle` (300 contre 300 avec `OPENRTS_STRESS=1`) : un pas de simulation de 300 contre 300 coûte environ 0,6 ms en moyenne (2 ms au pire) en mode optimisé.
   *Reste : projectiles simulés, soin et capacités, équilibrage en test.*
6. **Groupes de contrôle et interface** : barre de groupes, boutons d’ordres, raccourcis, minicarte. *Fait :*
   - dix groupes de contrôle (touches 1 à 9 puis 0, par position de touche : AZERTY comme QWERTY) : Ctrl ou Cmd + chiffre mémorise, Maj + chiffre ajoute, chiffre seul rappelle, double appui rapide (0,4 s) centre la caméra. Ce sont des actions de la couche d'entrées (`AssignGroup`, `AddToGroup`, `RecallGroup`) ; les groupes sont un état d'interface du joueur local (pas des ordres), les entités mortes en sortent ;
   - barre de groupes en haut à gauche (chiffre, nombre, type principal ; groupe correspondant à la sélection éclairé), cases cliquables (`ActionButton`, prêtes pour le doigt) ;
   - minicarte en bas à droite : sol, quadrillage, gisements, bâtiments et unités aux couleurs d'équipe, trapèze de la caméra. Image redessinée dix fois par seconde (fond préparé une fois), trapèze suivi à chaque image. Clic ou glissé : la caméra s'y place (`LookAt`) ; clic droit : ordre contextuel comme au sol (`CommandAtPoint`) ; en mode visée, clic gauche : ordre préparé (`OrderAtPoint`) ;
   - point d'accroche du brouillard de guerre : `minimap::LocalVision::sees` (tout est visible pour l'instant).
   *Reste : assigner un groupe au doigt (sans clavier), double-clic / double-tap pour sélectionner tout un type, raccourci « toute l'armée », alertes (attaque) sur la minicarte, minicarte qui suit le brouillard.*
7. **Brouillard de guerre et IA adverse** basique qui construit et attaque par vagues.
   - *Fait (IA, propositions) :* une IA générique dans le moteur (`ai.rs`), qui ne lit que les données (dépôt, population, production, extraction) et joue donc les trois factions avec leur style de construction. Elle passe par la file des ordres comme un joueur et décide dans un ordre fixe (déterministe). Elle récolte (trois ouvriers par extracteur), monte à 16 ouvriers, pose ses bâtiments de population avant d'être bloquée, une puis deux productions (la seconde d'un type qu'elle n'a pas encore, en posant d'abord ce qu'il exige), son extracteur, les bâtiments qui débloquent des unités (prérequis des options de production, comme l'Armurerie et le Centre de propulsion de la Garde), produit une armée variée, défend sa base et attaque par vagues à partir de 2 min 30 (10 unités, puis 4 de plus à chaque vague).
   - *Fait (fin de partie) :* un joueur sans aucun bâtiment est vaincu ; le dernier en lice gagne (règle appliquée par la simulation) ; écran « Victoire » / « Défaite » avec la durée, qu'on peut fermer pour continuer à regarder.
   *Reste : brouillard de guerre, niveaux de difficulté, IA qui s'étend vers d'autres bases et adapte sa composition.*
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
