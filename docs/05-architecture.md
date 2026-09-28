# 05 — Architecture cible

## 1. Vue d'ensemble

```
                        ┌──────────────────────────────┐
   UI desktop  ────────▶│          Facade API          │
   (clavier, manette)   │  (surface de contrôle unique)│
                        └──────────────┬───────────────┘
                                       │
   ┌────────────────┐   ┌───────────────▼───────────────┐   ┌──────────────┐
   │ Route sources  │──▶│          Simulation Core      │──▶│  State Store │
   │ replay / GPX   │   │  pas fixe, sans dépendance   │   │ versionné    │
   │ live override  │   │  au rendu ni aux sockets      │   │ sérialisable │
   └────────────────┘   └───────────────┬───────────────┘   └──────────────┘
                                       │  Snapshot (immuable)
                        ┌──────────────▼───────────────┐
                        │        Encodeur / Fan-out     │
                        │ NMEA │ Signal K │ ViewSync     │
                        └──────┬───────┬────────┬───────┘
                               │       │        │
                     ┌─────────▼─┐ ┌───▼────┐ ┌─▼──────────┐
                     │ Transports│ │  N     │ │ Observabilité│
                     │ TCP/UDP/  │ │transports│ │ métriques  │
                     │ WS/Série │ │ actifs  │ │            │
                     └───────────┘ └────────┘ └────────────┘
```

## 2. Couches

| Couche | Rôle | Dépendances |
|---|---|---|
| `core` | État, horloge, pas de simulation | aucune |
| `model` | Physique, environnement, route, propulsion | `core` |
| `encode` | NMEA, Signal K, ViewSync, AIS | `core` |
| `transport` | Sockets, ports série, cycle de vie | `core` |
| `source` | GPX, KML, journal, rejeu, entrées live | `model` |
| `api` | Façade unique, commandes, événements, seul écrivain de l'état | `model`, `transport` |
| `input` | Clavier, manette, profils : fonctions pures `RawInputEvent → Command` | `api` (types de commande seulement) |
| `ui` | Rendu, cartes, dialogue ; lit les instantanés, émet des commandes | `api`, `input` |
| `app` | Assemblage, configuration, migrations | tous |

Règle de dépendance : les flèches ne remontent jamais. `encode` ne connaît ni
`transport` ni `ui`. `model` ne connaît aucun format de sortie.

## 3. Le noyau de simulation

### 3.1 Horloge

```ts
interface Clock {
  now(): number;                 // ms epoch
  monotonic(): number;           // ms, insensible aux changements d'horloge
}
```

Le noyau avance par un **accumulateur** à pas fixe `dt` (1 ms par défaut,
configurable). L'horloge d'émission est un simple client de l'horloge : si
l'intervalle de sortie est de 1 s et que le tick d'intégration est de 20 ms,
le noyau intègre 50 pas et publie un snapshot toutes les 50. Un changement
d'intervalle de sortie ne change donc pas la trajectoire.

### 3.2 État

```ts
interface VesselState {
  t: number;                     // ms epoch de l'instant
  position: LatLon;              // degrés décimaux
  speedOverGround: number;       // m/s
  speedThroughWater: number;     // m/s
  headingTrue: number;           // radians, [0, 2π)
  headingMagnetic: number;       // radians
  heel: number;                  // radians
  pitch: number;                 // radians
  rudder: number;                // degrés, entier, [-40, 40]
}
```

Toutes les grandeurs sont **en unités SI dans le noyau**. La conversion vers
nœuds, mètres, degrés ou Kelvin n'a lieu que dans les encodeurs. Cela évite la
duplication d'unités constatée dans le legacy, où `updateHeading()` raisonne en
nœuds alors que le déplacement raisonne en m/s.

### 3.3 Instantané

Les encodeurs ne reçoivent pas l'objet d'état vivant mais un `Snapshot`
immuable produit une fois par pas de publication. Un encodeur ne peut donc pas
modifier le modèle, et deux encodeurs reçoivent le même état.

## 4. Découpage en modules

### 4.1 `core`

| Module | Contenu |
|---|---|
| `clock` | horloge murale + horloge monotone |
| `scheduler` | accumulateur à pas fixe, publication périodique |
| `state` | types, `Snapshot`, validation d'état |
| `events` | bus d'événements typé |
| `units` | conversions SI ⇄ unités nauticales, mise en cache |
| `random` | PRNG **injecté et graineable** (déterministe pour les tests) |
| `logging` | logs structurés, niveaux, sortie fichier optionnelle |

### 4.2 `model`

| Module | Contenu |
|---|---|
| `vessel` | intégration position/cap/vitesse, gîte, tangage |
| `rudder` | dynamique de barre, retour centre, taux de virage |
| `wind` | vent réel, vent apparent, rafales, dérive de courant |
| `sea` | houle, roulis, température, profondeur, sonar |
| `gnss` | satellites, DOP, fix, qualité, altitude |
| `propulsion` | deux lignes, régime, charge, température, marche |
| `route` | waypoints, destination, bearing, distance, XTE, Vitesse de fermeture |
| `drift` | bruit borné avec retour à la nominale, corrélé ou non par champ |

### 4.3 `encode`

| Module | Contenu |
|---|---|
| `nmea/sentence` | checksum, talker, champs, formatage numérique |
| `nmea/navigation` | `RMC, VHW, VTG, HDT, GLL, ZDA` |
| `nmea/gnss` | `GGA, GSA, GBS, WPL` |
| `nmea/environment` | `MWD, MWV, MTW, DPT, DBT` |
| `nmea/propulsion` | `RPM` |
| `nmea/autopilot` | `APB, RMB` |
| `nmea/vdr` | `VDR, RLM` |
| `signalk/delta` | construction du delta, chemins, unités |
| `signalk/hello` | hello, contextes, rôles |
| `viewsync` | paquet CSV binaire, compteurs, epoch 1900 |
| `ais` | encodage type 1 et type 5, fragmentation |
| `prefix` | préfixe propriétaire 61162-450 |

### 4.4 `transport`

| Module | Contenu |
|---|---|
| `factory` | résolution du type par nom ou valeur numérique |
| `base` | cycle de vie, métriques, remontée d'erreur |
| `tcp` | serveur et client |
| `udp` | broadcast, client, multicast |
| `websocket` | serveur, avec et sans TLS |
| `serial` | port série, reconnexion, décodage ligne |
| `multiplex` | N transports actifs, chacun son adaptateur de framing |

### 4.5 `source`

| Module | Contenu |
|---|---|
| `gpx` | lecture tracks/routes |
| `kml` | lecture `gx:Track`, `LineString`, `Folder` |
| `track-player` | lecture à cadence fixe ou temps réel, interpolation |
| `journal` | écriture `.nmeasim`, lecture, index des blocs |
| `replay` | transport dédié, pas-à-pas, play/pause, fin de trace |
| `live` | overrides, injections, souscriptions externes |

### 4.6 `api`

Une seule surface de contrôle, `ControlApi`, utilisée par l'UI, le clavier, la
manette, la carte, les scripts, l'entrée série et l'API distante. Sa forme
normative (commandes, chemins stables, événements typés, surface HTTP/WS) est
dans `09-inputs-and-ui.md` §7 ; la manette est décrite au §3 du même document.

Principes :

- `execute(Command)` est l'unique primitive d'écriture ; chaque commande porte
  sa `source` et est appliquée au début du pas suivant ;
- `snapshot()` rend un instantané immuable ; `subscribe()` un flux d'`Event`
  typés ;
- `path` est un chemin pointé stable (`vessel.rudder.command`,
  `env.water.temperature`, `propulsion.p0.throttle`), identique pour l'UI,
  l'API, la manette et la config ;
- les bornes et unités viennent du schéma déclaratif (§4.7) ; une commande hors
  bornes est refusée, jamais tronquée.

Le runtime est Rust (`docs/decisions/008-runtime-rust.md`) ; les extraits
TypeScript de ce document sont du pseudo-code de spécification.

### 4.7 `ui`

Panneaux rendus à partir d'un **schéma déclaratif** : chaque champ déclare son
chemin, son unité, ses bornes, son pas, son pas de dérive et son verrou. Le
schéma pilote à la fois le rendu, la validation, la config et l'API, ce qui
supprime le décalage entre l'affichage et l'état réel observé dans le legacy
(la classe GPS avait `pdop: 2.3` alors que la config forçait `1.5`).

## 5. Concurrence

- Un seul **thread de simulation**. Il n'est jamais preempté par le rendu ni
  par les sockets.
- Les sockets lisent et écrivent sur une **file de messages** ; l'écriture
  réseau ne bloque jamais le noyau. Si la file dépasse un seuil, le transport
  concerné est signalé `degraded` puis déconnecté.
- L'UI lit l'état via un snapshot publié, jamais via des accès croisés.

## 6. Configuration

```jsonc
{
  "schema": 2,
  "simulation": { "stepMs": 20, "intervalMs": 1000, "randomSeed": null },
  "vessel": { "position": [-35.0, 138.5], "speed": 5, "heading": 15 },
  "outputs": { "nmea": true, "signalk": false, "viewsync": true, "prefix": false },
  "transports": [
    { "id": "main", "type": "udp", "address": "127.0.0.1", "port": 3100 }
  ],
  "talkers": { "VTG": "GP" }
}
```

La migration lit les anciennes clés (`server.type` numérique, `interval`,
`seed`, `panelOrder`) et écrit la forme nouvelle. Un nom de transport inconnu
produit une erreur de validation explicite, jamais un silence.

## 7. Stratégie de persistance

- Fichier de configuration unique, versionné, avec migrations testées
  unitairement (une migration = une fonction pure `old -> new`).
- Journal `.nmeasim` : en-tête, blocs séparés par `~`, compatible lecture et
  écriture avec le legacy.
- Sauvegarde d'état optionnelle (JSON) pour reprendre une session.

## 8. Sécurité

- Aucune écoute réseau entrante non explicitement demandée.
- Le serveur TCP et le serveur WebSocket ne doivent accepter des connexions
  distantes que si `allowRemote` est actif.
- L'API distante éventuelle est unicast uniquement, sans découverte, avec un
  jeton optionnel.
- Aucune dépendance réseau au démarrage.
