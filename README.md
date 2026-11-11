# openRTS

A 3D real-time strategy (RTS) game, playable with a mouse or by touch,
written in Rust with the [Bevy](https://bevyengine.org) 0.20 engine.

The game design is described in [GAME_DESIGN.md](GAME_DESIGN.md) (in French).
The in-game interface is in French for now: unit, building and resource names
below are given as they appear in the game.

## Repository layout

| Folder | Role |
| --- | --- |
| `crates/rts_engine` | The generic engine: input, camera, selection, control groups (`control_groups`), minimap (`minimap`), players (`players`), order queue (`orders`), simulation, collisions, navigation (grid, A\*), buildings (placement, construction sites), economy (`economy`: resource nodes, harvesting, production, supply, UI), combat (`combat`: weapons, targeting, orders, damage, order bar, effects), enemy AI (`ai`), fog of war (`vision`), end of game (`outcome`), rendering (`graphics`). No game content. |
| `crates/rts_game` | The game: units, buildings, factions, maps, resource nodes (`garde.rs`, `sylves.rs`, `collectif.rs`, `map.rs`, `base.rs`, `base_sylves.rs`, `base_collectif.rs`, `gisements.rs`, `combat.rs`: combat stats, `battle.rs`: battle demo, `menu.rs`: start menu, `side.rs`: player sides). Uses `rts_engine`. |

## Building and running

You need stable Rust (installed with [rustup](https://rustup.rs)), then:

```sh
cargo run --release
```

The first Bevy build takes several minutes; later ones are fast.
On Linux you also need Bevy's system dependencies
(see the [Bevy Linux guide](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)).

The game opens on a **menu**: pick your faction and the opponent's (played by
the AI), or « Aléatoire » (random, drawn when the game starts), then
« Lancer la partie » (or Enter). The development variables `OPENRTS_FACTION`,
`OPENRTS_ENEMY`, `OPENRTS_DEMO` and `OPENRTS_SCREENSHOT` skip the menu;
`OPENRTS_MENU=1` forces it, `OPENRTS_MENU=start` confirms it on its own after 2 s.

## Controls (desktop)

| Action | Input |
| --- | --- |
| Select a unit | Left click |
| Box selection | Left-click drag |
| Add to / remove from selection | Shift + click or Shift + box |
| Attack-move (default) | Right click on the ground: armed units go there fighting whatever they meet; workers and unarmed units simply move |
| Attack an enemy | Right click on an enemy unit or building |
| Attack-move (workers included) | A then left click (on an enemy: attack it), or the « Attaquer » button of the order bar |
| Plain move (ignores enemies, to retreat) | M then left click, or the « Déplacer » button |
| Stop / Hold position | S / H, or the order bar buttons (bottom left, shown when units are selected) |
| Leave targeting mode | Right click or Escape (Shift + click: give the order and stay in targeting mode) |
| Inspect an enemy unit | Left click on it (red ring; no orders possible) |
| Harvest (Techniciens, Pousses or Collecteurs selected) | Right click on a Cristal node, or on a finished extractor (Extracteur; Calice for the Sylves, Creuset for the Collectif) |
| Rally point (producing building selected) | Right click on the ground, or on a resource node (produced workers harvest it directly) |
| Train (producing building selected) | The unit's key or a button of the bottom panel, e.g. for the Garde: T (Technicien, Quartier général); S (Sentinelle), V (Longue-Vue), L (Lancier), F (Faucon) at the Caserne, greyed out until the Armurerie (V, L) or the Centre de propulsion (F) exists; C (Bastion, Usine) |
| Cancel a queued unit | Click its slot in the queue (refunded) |
| Cancel a selected construction site | X or the « Annuler » button (refunded) |
| Store the selection in a group | Ctrl + 1 … 9 or 0 (also Cmd on Mac) |
| Add the selection to a group | Shift + 1 … 9 or 0 |
| Recall a group | 1 … 9 or 0, or click its slot in the group bar (top left); twice quickly: the camera centers on it |
| Move the camera with the minimap (bottom right) | Left click or drag on the minimap |
| Order towards a minimap point | Right click on the minimap (as on the ground: attack-move for armies); in targeting mode (A or M), left click |
| Scroll the camera | Arrow keys or mouse at the screen edge |
| Zoom | Mouse wheel or trackpad |
| Stress test (300 units) | F3 |
| Build (a Technicien, Pousse or Collecteur selected) | B then the building's key, or a button of the bar at the bottom of the screen |
| Place the building | Left click (Shift + click: place another one) |
| Cancel placement | Right click or Escape |
| Release the mouse | Escape (a click in the game captures it again; while placing, the first Escape cancels the placement) |
| Quit | Cmd + Q (macOS), Alt + F4 (Windows, Linux) |

Group keys are the top-row keys, on AZERTY as on QWERTY (without Shift). A
group only keeps the player's own units and buildings; dead ones are removed.
The group bar shows, for each non-empty group, its number, its size and its
main type; the slot of the group matching the selection is highlighted. The
minimap shows the ground, resource nodes, buildings (rectangles) and units
(dots) in team colors, and the trapezoid seen by the camera.

The game starts fullscreen. So that screen edges can scroll without triggering
macOS hot corners, the menu bar or the Dock, the system cursor is locked and
the game draws its own, which cannot leave the screen.

Units do not overlap: they jostle, push idle units out of their way and stop
in a tight group on arrival. They path around buildings (1 m navigation grid
and A\*); large units avoid passages too narrow for them. Flying units pass
over everything (at least 3 m above the ground) and only collide with other
flyers.

### Garde buildings

| Key (after B) | Building | Footprint | Cost | Requires | Role |
| --- | --- | --- | --- | --- | --- |
| Q | Quartier général | 7 × 7 m | 400 Cristal | — | Drop-off, Techniciens (T) |
| H | Module d'habitat | 3 × 3 m | 100 Cristal | — | Supply (+8) |
| E | Extracteur | 4 × 4 m | 75 Cristal | — (right on a free Éther source) | Éther |
| C | Caserne | 5 × 6 m | 150 Cristal | Quartier général | Sentinelles (S), Longue-Vue (V), Lanciers (L), Faucons (F) |
| T | Tourelle | 2 × 2 m | 100 Cristal | Caserne | Static defense |
| A | Armurerie | 4 × 4 m | 150 Cristal + 50 Éther | Caserne | Unlocks Longue-Vue and Lanciers at the Caserne |
| P | Centre de propulsion | 5 × 5 m | 150 Cristal + 100 Éther | Caserne | Unlocks Faucons at the Caserne |
| U | Usine | 6 × 7 m | 200 Cristal + 100 Éther | Armurerie | Bastions (C) |

Hotkeys are typed letters: they follow what is printed on the keys, on AZERTY
as on QWERTY. In placement mode a ghost follows the pointer, snapped to the
grid: green if the spot is free, red otherwise. The nearest Technicien walks
to the site and stays there: the building rises from the ground while it
works. An abandoned site (Technicien sent elsewhere or killed) stops:
**right click on it** with Techniciens selected to resume it. The cost is
paid on placement (refused with « Ressources insuffisantes » otherwise) and
refunded if the site is cancelled. The Extracteur snaps onto the nearest Éther
source; anywhere else its ghost is red (« Doit être posé sur une source »).

### Sylves buildings

| Key (after B) | Building | Footprint | Requires | Role |
| --- | --- | --- | --- | --- |
| A | Arbre-mère | 7 × 7 m | — | Base core, drop-off, trains Pousses (P), 15 supply |
| S | Souche nourricière | 3 × 3 m | — | 8 supply |
| C | Calice | 4 × 4 m | — (right on a free Éther source) | Éther harvesting |
| R | Ronceraie | 5 × 5 m | Arbre-mère | Trains Ronciers (R), Sourciers (S, with a Fontaine de sève) and Lanceurs de spores (L, with a Champignonnière) |
| E | Épineux | 2 × 2 m | Ronceraie | Static defense |
| F | Fontaine de sève | 4 × 4 m | Ronceraie | Tier 2: unlocks the Sourcier (Ronceraie) and the Gardien (Érablière) |
| H | Champignonnière | 4 × 4 m | Ronceraie | Tier 2: unlocks the Lanceur de spores (Ronceraie) |
| L | Érablière | 6 × 6 m | Ronceraie | Tier 2: trains Samares (S) and Gardiens (G, with a Fontaine de sève) |

The Sylves build differently: **the Pousse becomes the building**. With Pousses
selected, B then a letter (or the button bar) opens placement, as for the
Garde. The cost is paid on placement and the spot is promised (green frame on
the ground) to the nearest free Pousse; it walks there, takes root and
disappears (its supply is freed). The site then grows on its own, without
scaffolding: the building grows out of the ground. Each Pousse makes only one
building (« Aucun ouvrier libre » otherwise). Giving the Pousse another order
before it arrives cancels the promise, fully refunded; cancelling a growing
site (X) returns 75 % of the cost and the Pousse comes back out of the ground.

### Collectif buildings

| Key (after B) | Building | Footprint | Requires | Role |
| --- | --- | --- | --- | --- |
| E | Écrin | 7 × 7 m | — | Base core, drop-off, trains Collecteurs (C), 15 supply, 8 m resonance field |
| R | Résonateur | 2 × 2 m | — | Relay: 8 supply, 6.5 m resonance field |
| C | Creuset | 4 × 4 m | — (right on a free Éther source) | Éther harvesting |
| O | Orfèvrerie | 5 × 5 m | Écrin, inside a field | Trains Automates (A); Perceur (P) with a Lapidaire |
| A | Aiguillon | 2 × 2 m | Orfèvrerie, inside a field | Static defense |
| L | Lapidaire | 4 × 4 m | Orfèvrerie, inside a field | Gem cutting: unlocks the Perceur, the Essaimeur and the Fonderie |
| V | Volière | 5 × 5 m | Orfèvrerie, inside a field | Air workshop: trains Essaims (E); Essaimeur (M) with a Lapidaire |
| F | Fonderie | 6 × 6 m | Lapidaire, inside a field | Trains Titans (T) |

The Collectif builds by **remote setting**. With Collecteurs selected, B then a
letter opens placement; the **resonance fields** of the Écrin and of finished
Résonateurs then show on the ground (pale green circles). Every building but
the Écrin, the Résonateur and the Creuset can only be placed inside a field:
elsewhere its ghost is red and the click shows « Hors champ ». The cost is paid
on placement and the spot promised to the nearest free Collecteur; it flies
there, sets an anchor gem and **immediately goes back** to harvesting (or to
the nearest Cristal node). The building then assembles itself under its
spinning gem. Another order given to the Collecteur before it arrives cancels
the promise, fully refunded; cancelling a site (X) returns 75 % of its cost.

### Combat

Every unit and building has hit points, armor and categories (infantry,
armored, air, melee, ranged, worker, massive, structure); armed units and
static defenses (Tourelle, Épineux, Aiguillon) have a weapon: damage, range,
rate of fire, ground and/or air, bonus against some categories, splash damage.
A hit deals the weapon's damage, plus its bonus, minus the target's armor
(at least 0.5).

- An idle unit spots nearby enemies on its own and shoots them: attackers
  first, then the nearest. When attacked it always fights back; it chases
  without straying more than a few meters, then returns to its spot. A worker
  that is harvesting or building does not fight.
- Health bars above selected, damaged or hovered units; shot tracers in team
  colors, sparks, collapsing wrecks.
- A killed unit frees its supply; a destroyed building frees its footprint
  (units can pass again) and the supply it provided.
- The Faucon flies over units and buildings but counts as a ground unit in
  combat: every weapon can hit it.

### Fog of war

Every unit and building sees around itself (6 to 12 m depending on the type,
measured from its edge, never less than its acquisition range). The ground is
darkened where you cannot see, and darker still where you have never been.
Enemy units out of sight are hidden: they cannot be selected or targeted, and
units only shoot at what their player sees. An enemy building you have seen
stays displayed as it was last seen, even if destroyed since, until you see
its spot again. The AI only knows what its player sees and the buildings it
has spotted; it assumes the enemy base is opposite its own (the map is
symmetric) and scouts if it finds nothing there. `OPENRTS_FOG=0` turns the fog off.

### Economy

Two resources, with **provisional** names: **Cristal** (primary, ice-blue
clusters in a line behind the main building) and **Éther** (rare, purple rifts
on the sides, harvested through an extractor). The counters at the top right
show stocks and supply (used / available).

- Workers go to a node, extract (2.8 s for 5 Cristal, 2 s for 4 Éther), carry
  their (visible) load to the nearest drop-off, and start again. Only one
  worker at a time per node: others wait their turn or move to a free
  neighbor; two to three per node saturate it. An emptied node disappears and
  its workers move to the next one.
- Production: a selected producing building shows its buttons and its queue
  (up to 5, paid when queued, progress of the first unit). Produced units head
  to the rally point.
- Supply: each unit uses some (1 to 8), the main building provides 15 and each
  supply building 8, up to a global cap (200, adjustable with
  `OPENRTS_POP_MAX`). Beyond that: « Population maximale ».

A game starts with 50 Cristal, a main building, two or three supply
buildings, resource nodes and four workers already harvesting, plus a small
army. You start in the north; the AI opponent starts in the mirrored base in
the south.

All orders (move, harvest, place, train, rally, cancel) go through a queue
applied at the next simulation step: this is the basis for future lockstep
multiplayer (see `crates/rts_engine/src/orders.rs`).

### AI opponent

A generic AI in the engine (`ai.rs`) plays any faction from its data alone:
it harvests (three workers per extractor), builds up to 16 workers, adds
supply before being blocked, production buildings and what unlocks its units,
its extractor, trains a varied army, defends its base and attacks in waves
from 2 min 30 s on (10 units, then 4 more each wave). It acts only through the
order queue, like a player, in a fixed order (deterministic).

### End of game

A player is defeated when they have no building left (construction sites
included). When only one player is left, the game is over: « Victoire » or
« Défaite » screen with the game duration. **Enter** (or « Continuer à
regarder ») closes the screen, « Quitter » quits the game. A scene without an
enemy base (demos) never ends.

## Rendering and graphics settings

Movement animations (display only, for every unit, without extra data):
a bounce on each stride and a forward lean for ground units, vibration and
pitch for heavy vehicles, recoil on each shot, smooth turns. Flyers (Faucon
jetpacks, Samares, Essaims…) fly at least 3 m above the ground, over ground
units; they climb a little while moving, dip their nose when moving forward
and bank into turns.

The engine handles rendering (`graphics` module): HDR, filmic tone mapping
(ACES), bloom on glowing parts, cascaded shadows fitted to the visible area,
zoom-dependent distance fog, image-based lighting (IBL) computed at startup,
key light and colored rim lights, ambient particles. Colors (lights, sky, fog)
come from an `Ambiance` provided by the game; unit appearance is described as
data, part by part (`ModelBuilder`), with a material `Palette` per faction.

Two quality levels (`GraphicsQuality`):

| Quality | Content | Target |
| --- | --- | --- |
| High (default) | HDR + bloom, MSAA ×4, 3 soft-filtered shadow cascades | Desktop |
| Low | No HDR or bloom, FXAA, 1 shadow cascade, 3 times fewer particles | iPad, modest machines |

## Development variables

| Variable | Effect |
| --- | --- |
| `OPENRTS_WINDOWED=1` | Window instead of fullscreen, free mouse |
| `OPENRTS_FACTION=sylves` | Player faction: `garde` (default; `coalition` accepted), `sylves` or `collectif` (ambiance, ground, units, stress test); skips the menu |
| `OPENRTS_ENEMY=garde` | Opponent faction (blue, mirrored base in the south): by default the next one, Garde → Sylves → Collectif → Garde; skips the menu |
| `OPENRTS_MENU=1` | Shows the start menu even when another variable would skip it (`start`: confirms it after 2 s) |
| `OPENRTS_QUALITY=bas` | Low graphics quality (`haut` by default) |
| `OPENRTS_STRESS=1` | Starts the stress test (300 units) at launch |
| `OPENRTS_ZOOM=12` | Starting camera distance (10 to 70 m) |
| `OPENRTS_DEMO=buildings` | Garde: building showcase scene (the eight buildings, a site under construction, a green and a red ghost, a group that walks around the Caserne after 11 s) |
| `OPENRTS_FACTION=sylves OPENRTS_DEMO=buildings` | Sylves: the eight buildings (tier 2 in a row at the back of the base), a Calice and the Cristal line harvested by Pousses, a growing Ronceraie, a green and a red ghost, a selected Pousse that leaves to take root after 10.5 s (promised spot in green) |
| `OPENRTS_FACTION=collectif OPENRTS_DEMO=buildings` | Collectif: the eight buildings (Lapidaire, Volière and Fonderie to the north, `OPENRTS_FOCUS=-1,8`), resonance fields on the ground, a Creuset and the Cristal line harvested by Collecteurs, an Orfèvrerie assembling under its gem, a green ghost inside a field and a red one outside, a selected Collecteur that leaves to set an Aiguillon after 10 s then goes back to harvesting |
| `OPENRTS_FOCUS=x,z` | The camera looks at this ground point at startup |
| `OPENRTS_DEMO=economy` | Garde: the economy at work (twelve Techniciens on the Cristal line, three at an Extracteur, Quartier général selected with three Techniciens queued and its rally point, counters) |
| `OPENRTS_DEMO=battle` | Two armies (player 1 orange in the south, player 2 blue in the north), each in front of two static defenses, that attack-move after 8 s and meet in the middle. `OPENRTS_FACTION` and `OPENRTS_ENEMY` choose the factions, `OPENRTS_STRESS=1` makes it 300 versus 300 |
| `OPENRTS_POP_MAX=100` | Global supply cap (200 by default) |
| `OPENRTS_AI=0` | The opponent does not play (by default a basic AI controls it in a normal game) |
| `OPENRTS_AI_ATTACK=60` | Delay before the AI's first wave (150 s by default) |
| `OPENRTS_SPEED=8` | Speeds up game time (to watch the AI) |
| `OPENRTS_FOG=0` | No fog of war (everything visible). Demos (`OPENRTS_DEMO`) do without it by default; `OPENRTS_FOG=1` turns it back on |
| `OPENRTS_SCREENSHOT=capture.png` | After 14 s: prints the average FPS of the last 6 seconds, saves a screenshot of the render and quits (vsync off to measure); skips the menu |
| `OPENRTS_SCREENSHOT_DELAY=30` | Time of the automatic screenshot (14 s by default) |

Examples: `OPENRTS_WINDOWED=1 OPENRTS_STRESS=1 OPENRTS_SCREENSHOT=/tmp/stress.png cargo run --release`,
`OPENRTS_WINDOWED=1 OPENRTS_DEMO=buildings OPENRTS_ZOOM=36 OPENRTS_SCREENSHOT=/tmp/buildings.png cargo run --release`,
`OPENRTS_FACTION=sylves OPENRTS_WINDOWED=1 OPENRTS_DEMO=buildings OPENRTS_ZOOM=40 OPENRTS_FOCUS=0,-5 OPENRTS_SCREENSHOT=/tmp/sylves.png cargo run --release`,
`OPENRTS_FACTION=collectif OPENRTS_WINDOWED=1 OPENRTS_DEMO=buildings OPENRTS_ZOOM=40 OPENRTS_FOCUS=0,-6 OPENRTS_SCREENSHOT=/tmp/collectif.png cargo run --release`,
`OPENRTS_WINDOWED=1 OPENRTS_DEMO=economy OPENRTS_ZOOM=34 OPENRTS_FOCUS=1,-11 OPENRTS_SCREENSHOT=/tmp/economy.png cargo run --release`,
`OPENRTS_WINDOWED=1 OPENRTS_DEMO=battle OPENRTS_ZOOM=18 OPENRTS_FOCUS=0,1 OPENRTS_SCREENSHOT=/tmp/battle.png cargo run --release`,
`OPENRTS_WINDOWED=1 OPENRTS_DEMO=battle OPENRTS_STRESS=1 OPENRTS_ZOOM=70 OPENRTS_SCREENSHOT=/tmp/stress.png cargo run --release`.

Cost of one simulation step during a 300 versus 300 battle (headless):
`cargo test --release -p rts_engine -- --ignored --nocapture battle_300`; cost
of the vision computation (600 units):
`cargo test --release -p rts_engine -- --ignored --nocapture vision_cost`.
To measure performance, build with `--release`: in debug mode, Bevy's and
wgpu's internal checks cost a lot of frames per second.

## License

The code is dual-licensed, at your option:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT ([LICENSE-MIT](LICENSE-MIT))

Unless stated otherwise, any contribution intentionally submitted for
inclusion in the project is dual-licensed as above, without any additional
terms or conditions.
