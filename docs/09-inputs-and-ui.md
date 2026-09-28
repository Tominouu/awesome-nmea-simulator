# 09 — Entrées, interface et API

## 1. Principe directeur

Toute commande, quelle que soit sa source, passe par `ControlApi` (§7) et écrit dans
le **modèle**. Aucun périphérique d'entrée n'écrit dans un encodeur, et aucun
encodeur ne lit un périphérique d'entrée.

```
Clavier ─┐
Manette ─┤
Souris ──┤
Carte ───┼──▶ ControlApi ──▶ VesselState / Route / Config
Série ───┤
Fichier ─┤
IPC ─────┤
HTTP ────┘
```

Le legacy ne respecte pas cette règle : le clavier écrit dans
`navsim.sim.simObjects.get("gps").value[...]` via `setValue()`, tandis que la
manette n'existe pas et que l'API distante non plus. La cible unifie : toutes
les sources émettent les commandes de §7.3, avec leur `source`.

## 2. Clavier

### 2.1 Comportement legacy vérifié

| Touche | Effet | Condition |
|---|---|---|
| `↑` | vitesse + 1 | `gps.mask.speed` |
| `↓` | vitesse − 1 | `gps.mask.speed` |
| `→` | barre + 1 **ou** cap + 1 | mode direction, sinon `gps.mask.heading` |
| `←` | barre − 1 **ou** cap − 1 | idem |

- Pas de 1, valeurs clampées aux bornes du seed.
- Le mode direction **force** `mask.heading = true` et **remet la barre à 0** à
  chaque bascule.
- Le `keydown` est traité sur le conteneur de page, ce qui rend le clavier
  inopérant si le focus est ailleurs. Vérifié en exécution : des événements
  synthétiques ne déclenchent pas le handler, alors que des événements clavier
  réels via CDP le font. Le legacy dépend donc du focus du document entier.

### 2.2 Cible

- Écoute sur `window`, avec retrait propre au démontage.
- Le pas devient **configurable** (1 par défaut, pour rester compatible).
- Le pas s'adapte à l'unité : `1` pour les degrés, `0.1` pour les noeuds si le
  pas fractionnaire est activé.
- Hors condition, la touche ne fait **rien** et ne consomme pas l'événement
  seulement si une autre action lui est affectée.
- Raccourcis globaux additionnels : `Espace` start/stop, `R` enregistrement,
  `L` rejeu, `?` aide.

## 3. Manette

Le legacy n'a aucune prise en charge de manette. C'est une exigence nouvelle, pas
une compatibilité à préserver. La manette est une **source d'entrée de premier
niveau**, au même rang que le clavier, l'UI, les scripts et l'API distante.

### 3.1 Principe

- La manette produit des **commandes** de l'API de contrôle (§7), jamais une
  écriture d'état, jamais une phrase NMEA. Un pilote automatique ou un ECDIS
  branché voit exactement la même chose que si un opérateur agissait à la
  souris.
- La chaîne est découpée en trois étages, chacun testable sans matériel :

```
périphérique ──▶ pilote (gilrs, couche app) ──▶ RawInputEvent
             ──▶ InputMapper (profil : zone morte, calibration, courbe…)
             ──▶ Command { source: Gamepad(id), … } ──▶ ControlApi
```

- Le noyau ne dépend d'aucune bibliothèque de manette : le pilote vit dans la
  couche `app`, le `InputMapper` est une fonction pure dans le crate `input`.

### 3.2 Détection

| Aspect | Règle |
|---|---|
| Énumération | au démarrage et à chaud ; chaque périphérique reçoit un `DeviceId` stable pour la session et une empreinte `vendor:product:nom` |
| Connexion / déconnexion | événements `input.device.connected` / `input.device.disconnected` (§7.4) |
| Profil | choisi par empreinte, sinon profil `default` ; le choix est persisté |
| Déconnexion en cours de manœuvre | les axes en mode position émettent **une** dernière commande neutre définie par le profil (`onDisconnect: hold | center`, défaut `hold`) |
| Plusieurs manettes | autorisées ; chaque commande porte son `DeviceId` |

### 3.3 Traitement d'un axe

Appliqué dans cet ordre, sur une valeur brute `x ∈ [-1, 1]` :

| Étape | Paramètre | Effet |
|---|---|---|
| 1. calibration | `min`, `center`, `max` mesurés | ramène l'amplitude réelle sur `[-1, 1]`, centre à 0 |
| 2. inversion | `invert: bool` | `x ← −x` |
| 3. zone morte | `deadzone ∈ [0, 0.5)`, défaut 0,08 | `|x| < d → 0`, sinon rééchelonné `sign(x)·(|x|−d)/(1−d)` pour rester continu |
| 4. courbe | `curve: linear | expo(k) | power(p) | points[]` | adoucit le centre ; `expo(k) = (1−k)·x + k·x³` |
| 5. sensibilité | `sensitivity ∈ (0, 2]`, défaut 1 | `x ← clamp(x·s, −1, 1)` |
| 6. projection | `target`, `mode` | convertit en commande (§3.5) |

Deux modes par axe :

- **position** : la valeur de l'axe est la consigne (barre = `x · 40°`,
  throttle = `x`). Une commande n'est émise que si la consigne change de plus
  de `epsilon` (0,5 % par défaut) ; à l'entrée dans la zone morte, une seule
  commande « centre » est émise, puis plus rien. Un stick au repos ne combat
  donc pas le clavier ;
- **taux** : la valeur de l'axe est une vitesse de variation de la consigne
  (`rate` en unité/s), intégrée à la cadence de scrutation ; zone morte = rien.

### 3.4 Boutons

- Déclenchement sur **front** (`press`), jamais sur niveau.
- Répétition optionnelle (`repeatMs`) pour les incréments.
- Combinaisons par modificateur (`hold: "LB"`).
- Gâchettes analogiques traitées comme des axes `[0, 1]`, ou comme bouton avec
  seuil (`threshold`, défaut 0,5).

### 3.5 Profil et mapping par défaut

Le profil `default` est utilisable sans configuration :

| Entrée | Mode | Commande (§7.3) |
|---|---|---|
| Stick gauche Y | position | `propulsion.throttle.set { engine: "all", value: y }` |
| Stick droit X | position, expo 0,3 | `helm.rudder.set { deg: x·40 }` |
| LT / RT | gâchettes, propulsion = RT − LT | `propulsion.throttle.set` |
| A | bouton | `propulsion.engine.toggle` |
| B | bouton | `autopilot.toggle` |
| X | bouton | `sim.reset` |
| Y | bouton | `anchor.toggle` |
| D-pad gauche/droite | bouton, répétition 150 ms | `helm.rudder.nudge { deg: ∓1 }` |
| D-pad haut/bas | bouton, répétition 150 ms | `propulsion.throttle.nudge { delta: ±0.1 }` |
| LB / RB | bouton, répétition | `nav.heading.nudge { deg: ∓10 }` |
| Start | bouton | `sim.togglePause` |
| Back | bouton | `helm.rudder.center` |
| LS / RS | bouton | `route.mark` / `route.waypoint.next` |

Ce mapping est celui de la consigne d'implémentation ; il remplace la
proposition initiale. Implémentation : `crates/input/src/gamepad.rs`,
documentation utilisateur : `docs/gamepad.md`.

Format de profil, exportable et importable :

```jsonc
{
  "schema": 1,
  "name": "default",
  "match": { "fingerprint": "*" },
  "onDisconnect": "hold",
  "axes": {
    "leftX":  { "calibration": { "min": -1, "center": 0, "max": 1 },
                "invert": false, "deadzone": 0.08,
                "curve": { "expo": 0.3 }, "sensitivity": 1.0,
                "mode": "position",
                "command": "helm.rudder.set", "scale": { "deg": 40 } }
  },
  "buttons": {
    "south": { "command": "route.waypoint.next" }
  }
}
```

La calibration s'obtient par un assistant de l'UI (centre au repos, puis
excursion complète) ; elle est stockée dans le profil, par empreinte.

### 3.6 Arbitrage entre sources

- Chaque commande porte `source ∈ {keyboard, gamepad(id), ui, map, script,
  remote, serial, replay, track}`.
- Règle : **dernier écrivain gagne**, par chemin de consigne. Comme la manette
  n'émet qu'en cas de changement (§3.3), un clavier et une manette au repos
  coexistent.
- Un suivi de trace ou un rejeu détient le **jeton de producteur**
  (`10-routes-and-replay.md` §6) : pendant ce temps, les commandes de
  navigation sont refusées avec `command.rejected { reason: "producer-locked" }`,
  sauf `sim.stop`, `sim.pause` et `sim.reset`.

### 3.7 Tests à prévoir

| Test | Attendu |
|---|---|
| `input:deadzone-continuity` | `f(d + ε) → 0⁺`, pas de saut à la frontière |
| `input:invert` | `f_inv(x) = −f(x)` |
| `input:curve-expo` | `expo(0.3)` : `f(0.5) = 0.3875`, `f(1) = 1` |
| `input:calibration` | brut `[−0.8, 0.9]`, centre 0,05 → `[−1, 1]`, centre 0 |
| `input:position-no-spam` | stick au repos 1 000 cycles : 0 commande |
| `input:rate-mode` | axe 0,5, rate 10 °/s, 1 s → consigne +5° |
| `input:disconnect-hold` | déconnexion : aucune commande après la dernière |
| `input:same-api` | une commande manette et la même commande clavier produisent le même état |

## 4. Carte 2D

### 4.1 Gestes, tels que vérifiés sur le legacy

| Geste | Effet legacy |
|---|---|
| **Clic droit** | pose la destination : `ap.value.destination` et `ap.value.origin` (figée) |
| Clic gauche | suivi de trace, uniquement si la simulation tourne |
| Clic droit sur un marqueur `waypoint.wpt_marker` | sélection du waypoint |
| Survol | met à jour le curseur de position |

Deux pièges legacy relevés :

1. le clic droit utilise `this.cursor`, alimenté par le dernier `pointermove`.
   Un clic droit sans mouvement de souris utilise une position périmée ;
2. l'origine est figée au moment du clic, donc le cross-track error est
   calculé depuis un point fixe, pas depuis le point de départ réel.

### 4.2 Cible

- Clic droit : pose la destination, avec `origin` mis à jour en continu tant que
  la route est ré-anchorée, ou figé si l'utilisateur choisit le mode heritage.
- Le curseur est recalculé à la position du pointeur au moment du clic, plus
  aucun état intermédiaire.
- Ajout : double-clic pour insérer un waypoint intermédiaire.
- Ajout : export de la route affichée en GPX ou KML.
- Rendu : OpenLayers 8 ou équivalent, projection Web Mercator, couches
  optionnelles (trace, waypoints, route, route prévue, fond raster).

## 5. Panneaux et édition

### 5.1 Schéma déclaratif

Chaque champ éditable est déclaré une fois :

```ts
interface FieldSpec {
  path: string;              // 'vessel.speedOverGround'
  label: string;             // 'Speed over ground'
  unit: 'kn' | 'm' | 'deg' | 'm/s' | 'C' | 'K' | 'm/s2' | 'ratio';
  min: number; max: number; step: number;
  nominal: number;
  drift?: { amplitude: number; halfLifeMs: number };
  masked?: boolean;          // mis à jour automatiquement ?
  overridden?: boolean;      // forcé par l'utilisateur ?
  visibleWhen?: string;      // expression sur d'autres chemins
}
```

Avantages directs :

- un seul endroit définit bornes, unités et pas ;
- la validation, l'affichage, la config et l'API en découlent ;
- impossible d reintroduire le défaut du legacy où la classe GPS porte
  `pdop: 2.3` et la config `1.5`, et où l'UI affiche une valeur que l'encodeur
  n'utilise pas.

### 5.2 Verrous et overrides

| Concept | Effet | Priorité |
|---|---|---|
| `masked` vrai | le champ suit le modèle | normale |
| `masked` faux | le champ est **figé** | épingle la valeur |
| `override` défini | la valeur vient de l'API/UI | prioritaire sur le modèle |
| `override` released | retour au modèle | — |

Le legacy utilise des masques booléens, ce qui rend impossible d'exprimer
« figer à 12 kn » autrement qu'en désactivant le masque et en réinjectant la
valeur à chaque tick. La cible sépare les deux notions.

## 6. Terminal utilisateur

- Liste des phrases entrantes, avec horodatage et checksum calculé.
- L'utilisateur peut saisir une phrase NMEA, qui est :
  - transmise telle quelle aux transports actifs ;
  - préfixée si l'option est active (comportement legacy vérifié) ;
  - analysée et, pour un sous-ensemble de phrases reconnues, **appliquée au
    modèle** (par exemple un `RMC` entrant positionne le navire). Ce dernier
    point est une nouveauté, optionnelle.
- Limite de taille d'historique, avec filtrage par talker et par formatter.

## 7. API de contrôle

### 7.1 Principe

Une seule surface, `ControlApi`, pour toutes les sources : UI, clavier,
manette, carte, scripts, API distante, entrée série. **L'UI n'écrit jamais dans
l'état du navire** : elle émet des commandes et lit des instantanés. Le crate
`api` est le seul à détenir une référence mutable à l'état
(`12-new-architecture.md` §5.1).

```rust
pub trait ControlApi {
    fn execute(&mut self, cmd: Command) -> Result<CommandAck, CommandError>;
    fn snapshot(&self) -> Arc<Snapshot>;
    fn subscribe(&self, filter: EventFilter) -> EventStream;
}

pub struct Command {
    pub id: CommandId,        // fourni par l'appelant ou généré
    pub source: Source,       // keyboard | gamepad(DeviceId) | ui | map | script | remote | serial | replay | track
    pub kind: CommandKind,    // ci-dessous
}
```

Une commande est validée (bornes du schéma, état du simulateur, jeton de
producteur) puis appliquée **au début du pas suivant** du noyau : l'ordre
d'application est l'ordre d'arrivée, et deux exécutions à graine et séquence de
commandes égales produisent le même état (SIM-01).

### 7.2 Chemins stables

Les chemins pointés sont la même clé pour l'API, l'UI, la manette, la config
et les overrides. Ils sont versionnés avec le schéma ; un chemin publié n'est
jamais renommé sans alias de migration.

| Chemin | Unité API | Écriture |
|---|---|---|
| `vessel.position` | `{lat, lon}` degrés | override |
| `vessel.headingTrue` | degrés | override, autopilote |
| `vessel.speedThroughWater` | kn | override |
| `vessel.rudder.command` | degrés, `[-40, 40]` | commande barre |
| `vessel.rudder.angle` | degrés | lecture seule (D1) |
| `vessel.rateOfTurn` | °/min | lecture seule (D2) |
| `propulsion.<p0|p1>.throttle` | `[-1, 1]` | commande propulsion |
| `propulsion.<p0|p1>.running` | booléen | commande moteur |
| `propulsion.<p0|p1>.rpm` | tr/min | lecture seule, override |
| `route.destination` | `{lat, lon}` | commande route |
| `route.waypoints[]` | liste | commande route |
| `autopilot.mode` | `off | heading | track | route` | commande autopilote |
| `autopilot.headingTarget` | degrés | commande autopilote |
| `env.wind.*`, `env.water.*`, `gnss.*` | unités du schéma | override |

Les unités de l'API sont les unités d'usage (degrés, nœuds) ; la conversion
vers le SI du noyau se fait dans `api`, une seule fois.

### 7.3 Commandes

| Commande | Paramètres | Effet |
|---|---|---|
| `sim.start` / `sim.stop` | — | démarre / arrête le noyau et les transports |
| `sim.pause` / `sim.resume` / `sim.togglePause` | — | gèle le temps simulé, les transports restent ouverts |
| `sim.reset` | `{ keepRoute?: bool }` | restaure les valeurs initiales de la config |
| `sim.step` | `{ n?: u32 }` | avance de `n` pas, simulateur en pause |
| `helm.rudder.set` | `{ deg }` | consigne de barre |
| `helm.rudder.nudge` | `{ deg }` | consigne ± delta, bornée |
| `helm.rudder.center` | — | consigne 0 |
| `propulsion.throttle.set` | `{ engine: p0|p1|all, value ∈ [-1,1] }` | commande de propulsion |
| `propulsion.engine.start` / `.stop` | `{ engine }` | marche / arrêt moteur |
| `nav.heading.set` | `{ deg }` | override de cap (mode direct, legacy) |
| `nav.speed.set` / `nav.speed.nudge` | `{ kn }` | override de vitesse |
| `route.destination.set` | `{ lat, lon, name? }` | pose la destination ; `origin` = position courante |
| `route.destination.clear` | — | APB/RMB repassent en `V` |
| `route.waypoint.add` / `.addAtVessel` / `.next` / `.clear` | selon | gestion des waypoints |
| `autopilot.engage` | `{ mode, headingTarget? }` | engage le pilote |
| `autopilot.disengage` / `autopilot.toggle` | `{ mode? }` | désengage / bascule |
| `autopilot.heading.set` / `.nudge` | `{ deg }` | consigne de cap du pilote |
| `override.set` | `{ path, value }` | fige une grandeur, prioritaire sur le modèle |
| `override.release` | `{ path }` | rend la grandeur au modèle |
| `transport.start` / `.stop` / `.apply` | `TransportSpec` | cycle de vie des transports |
| `record.start` / `record.stop` | `{ path }` | journal `.nmeasim` (D7) |
| `replay.load` / `replay.control` | `{ path }` / `{ play | pause | step ±1 | seek i }` | rejeu |

Toute commande hors bornes, sur un chemin inconnu, ou interdite dans l'état
courant est **refusée** avec une raison typée ; jamais tronquée en silence.

### 7.4 Événements typés

```rust
pub enum Event {
    SimStarted, SimStopped, SimPaused, SimResumed, SimReset,
    Snapshot(Arc<Snapshot>),                         // à la cadence de publication
    CommandAccepted { id: CommandId, source: Source },
    CommandRejected { id: CommandId, source: Source, reason: RejectReason },
    OverrideChanged { path: Path, active: bool },
    AutopilotChanged { mode: AutopilotMode },
    RouteChanged,
    TransportState { id: TransportId, state: TransportState, error: Option<String> },
    InputDeviceConnected { device: DeviceId, name: String, profile: String },
    InputDeviceDisconnected { device: DeviceId },
    TrackEnded, ReplayEnded, ReplayLooped,
    Warning { code: WarningCode, message: String },
}

pub enum RejectReason {
    UnknownPath, OutOfRange { min: f64, max: f64 }, ReadOnly,
    ProducerLocked, InvalidState, Invalid(String),
}
```

L'UI s'abonne à `Snapshot` et aux événements ; elle n'a pas d'autre canal.

### 7.5 Surface distante

Pour l'automatisation de bancs d'essai, la même `ControlApi` est exposée :

| Point | Choix |
|---|---|
| Protocole | HTTP + WebSocket, sur la même interface de contrôle |
| Écoute | `127.0.0.1` par défaut, jamais `0.0.0.0` sans opt-in explicite |
| Authentification | jeton optionnel, sinon accès local seulement |
| Versionnage | préfixe `/api/v1` |
| Découverte | aucune (pas de mDNS, pas de SSDP) |

```
POST   /api/v1/commands        { "kind": "helm.rudder.set", "deg": 10 } -> CommandAck | CommandError
GET    /api/v1/state           -> Snapshot
GET    /api/v1/paths           -> schéma des chemins (unités, bornes, écriture)
WS     /api/v1/events          -> Event, filtrable par type
```

La forme JSON d'une commande est `{ "kind": <nom §7.3>, ...paramètres }`. Les
anciennes routes spécialisées (`/overrides`, `/route`, `/transport`, `/record`,
`/replay`) deviennent des alias de `POST /commands`.

### 7.6 Tests à prévoir

| Test | Attendu |
|---|---|
| `api:single-writer` | test statique : aucun crate hors `api` n'obtient `&mut State` |
| `api:determinism` | même graine + même séquence de commandes → mêmes instantanés |
| `api:reject-out-of-range` | `helm.rudder.set { deg: 55 }` → `OutOfRange { -40, 40 }`, état inchangé |
| `api:producer-lock` | pendant un rejeu, `helm.rudder.set` → `ProducerLocked` ; `sim.stop` accepté |
| `api:events-order` | `CommandAccepted` précède le premier `Snapshot` qui en montre l'effet |
| `api:remote-equals-local` | la même commande par HTTP et en local produit le même état |

## 8. Persistance de l'interface

| Élément | Stocké |
|---|---|
| Ordre et visibilité des panneaux | oui |
| Schéma d'édition personnalisé | oui |
| Profils de manette | oui, exportables |
| Coordonnées de carte, zoom, couches | oui |
| Dernier fichier de journal, dernier GPX | oui |
| Graine aléatoire | oui si épinglée |

## 9. Accessibilité

- Navigation clavier complète, y compris le réordonnancement des panneaux.
- Contrastes conformes AA sur tous les fonds, y compris les états désactivés.
- Aucun sens porté par la seule couleur : l'état `running`/`stopped` d'un
  moteur a aussi un glyphe et un texte.
- Taille de police et densité de l'interface réglables.
- `prefers-reduced-motion` respecté pour les transitions.
- Le panneau d'état des transports est annoncé aux technologies d'assistance
  quand un transport change d'état.
