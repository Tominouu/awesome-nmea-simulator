# 02 — Analyse du legacy (NMEASimulator 1.6.1)

Toute l'analyse dérive de l'extraction du binaire, jamais du dépôt public qui ne
contient qu'un `README.md`. Les chemins référencés sont relatifs à
`/home/tom/nmeasim-rs/analysis/`.

## 1. Environnement d'origine

| Élément | Valeur |
|---|---|
| Paquet | `nmeasimulator_1.6.1_amd64.deb` |
| Version applicative | `1.6.1` |
| Runtime | Electron `24.3.0`, Chromium `112.0.5615.165`, Node 18.x |
| Empaquetage | ASAR unique (`resources/app.asar`, 207 fichiers) |
| UI | Angular (Material CDK, OpenLayers) |
| Persistance | `localStorage` du profil Electron |
| Journal | Fichier texte `.nmeasim` via dialogue natif |

Sources d'analyse :

- binaire : `deb/opt/NMEASimulator/nmeasimulator`
- ASAR extrait : `asar/`
- modules beautifiés : `mods_b/`

Cartographie des modules critiques :

| Module | Rôle |
|---|---|
| `1326.js` | Objet `config` : défauts et migrations |
| `3352.js` | Store `localStorage` |
| `2178.js` | `navsim` : orchestration, réseau, journal, replay |
| `7264.js` | `sim` : objets de simulation et encodeurs |
| `8138.js` | Énumérations partagées (types serveur, formats) |
| `933.js` | `comm` : serveurs et clients |
| `2750.js` | `vsync` : ViewSync |
| `7078.js` | NMEA : checksum, sentence, AIS |
| `745.js` | Géodésie : coordonnées, distance, bearing, GPX |
| `3309.js` | Composants : UI panels, import, replay |
| `5213.js` | Composant principal : contrôles, clavier, engines, carte |
| `5606.js` | Conversions d'unités |

## 2. Configuration

### 2.1 Stockage

Clé unique `nmeasim_config` dans le `localStorage` du profil. Le store lit et
écrit l'objet complet.

### 2.2 Chargement et migrations

`loadConfig()` **remplace intégralement** l'objet courant par celui lu sur disque,
puis exécute les migrations de `1326.js`. Conséquence importante : une
configuration écrite à la main et incomplète fait disparaître les clés absentes
jusqu'à ce qu'une migration les restaure.

Migrations observées :

- ajout de `panelOrder` si absent ;
- ajout de `server.interfaceId` si absent, en prenant la première interface
  renvoyée par `nwInterfaceList` (sinon `lo`) ;
- `server.ip` par défaut `127.0.0.1:3100` ;
- `server.type` par défaut `udp`.

### 2.3 Valeurs par défaut (source `1326.js`)

```jsonc
{
  "version": "",
  "interval": 1000,          // ms entre deux ticks
  "outputFormat": "NMEA",    // "NMEA" | "SIGNALK"
  "autoStart": false,
  "sendNmeaPrefix": false,   // préfixe propriétaire 61162-450
  "viewsync": { "address": "127.0.0.1", "port": 7001,
                "altitude": 30, "tilt": 85 },
  "vesselId": "urn:mrn:signalk:uuid:b7590868-1d62-47d9-989c-32321b349fb9",
  "context": "vessels",
  "server": {
    "type": 4,               // udp
    "ip": { "address": "127.0.0.1", "port": 3100, "secure": false,
            "interfaceId": "lo",
            "multicastAddress": "239.255.0.0", "multicastPort": 0 },
    "serial": null
  },
  "talkers": [],
  "panelOrder": ["vessel","gps","wind","water","propulsion",
                 "destination","terminal"]
}
```

### 2.4 Seeds par défaut

| Objet | Champ | Valeur | Min | Max | Dérive |
|---|---|---|---|---|---|
| `gps` | `speed` (kn) | 5 | 0 | 70 | ±0.01 |
| `gps` | `heading` (°) | 15 | 0 | 359 | ±0.05 |
| `gps` | `rudderAngle` (°) | 0 | −40 | 40 | — |
| `gps` | `hdop`/`pdop`/`vdop` | 1.5 | 0.5 | 5 | ±0.07 |
| `gps` | `position` | 35° S, 138.5° E | — | — | — |
| `gps` | `altitude` (m) | 2 | 0 | 3000 | — |
| `wind` | `direction` (°) | 285 | 0 | 359 | ±0.05 |
| `wind` | `speed` (kn) | 11 | 2 | 40 | ±0.2 |
| `water` | `depth` (m) | 8 | 1 | 100 | — |
| `water` | `temperature` (°C) | 7.5 | 0 | 25 | — |
| `p0`/`p1` | `rpm` | 600 | 600 | 6000 | ±0.02 |
| `p0`/`p1` | `temperature` (°C) | 85 | 85 | 115 | ±0.01 |

À noter : les **valeurs initiales des classes** diffèrent des seeds de config
(par exemple `hdop: 1.9`, `pdop: 2.3`, `vdop: 3.1` dans la classe GPS). La config
appliquée écrase ces valeurs, c'est donc la config qui fait foi.

## 3. Modèle de simulation

### 3.1 Objets

Huit objets enregistrés dans `simObjects` :

| Clé | Objet | Par défaut |
|---|---|---|
| `gps` | GPS | actif |
| `wind` | Vent | actif |
| `water` | Eau | actif |
| `wpt` | Waypoint | **muet** |
| `destination` | Destination AIS | actif |
| `p0` | Moteur bâbord | actif, arrêté |
| `p1` | Moteur tribord | actif, arrêté |
| `ap` | Pilote automatique / route | actif, sans destination |

Il n'existe **pas** d'objet `vessel` : le « bateau » n'est que l'agrégat
`gps` + `wind` + `water` + `p0`/`p1` + `destination`. Le panneau UI `vessel`
n'est qu'un en-tête.

### 3.2 Boucle de temps

`navsim.start()` arme un `setInterval(interval)`. En dessous de 500 ms,
`tickScaling = floor(500 / interval)` est appliqué pour l'orchestration des
mises à jour, ce qui normalise la cadence effective à ~500 ms.

### 3.3 Dérive aléatoire (`applySeed`)

Pour chaque champ masqué et non verrouillé :

```js
const up = Math.floor(10 * Math.random()) % 2 === 0;
value = (value === 0)
  ? (up ? value + plus : value - minus)
  : (up ? value * (1 + plus) : value / (1 + minus));
value = clamp(value, min, max);
```

Deux propriétés importantes :

1. **Les deux branches ne sont pas symétriques** : `+plus` contre `−(minus/(1+minus))`.
   L'espérance du multiplicateur vaut `((1+plus) + 1/(1+minus)) / 2 > 1`.
   Il existe donc une **dérive ascendante systémique** et **aucun rappel à la
   moyenne**.
2. **Au voisinage du minimum**, la branche descendante est immédiatement
   re-clampée sur `min`, ce qui produit une oscillation à deux états
   (observé : 600 ↔ 612 rpm).

### 3.4 Navigation

```js
// 7264.js — updateHeading()
if (rudderAngle === 0) return;                       // coupure franche
turnRateDegPerMin =
    rudder >= 20        ? speed_kn / (150 / 1852) :
    rudder >= 10        ? speed_kn / (200 / 1852) :
    rudder >= 5         ? speed_kn / (400 / 1852) :
                           speed_kn / (600 / 1852);
heading += sign(rudder) * turnRateDegPerMin / 60 * elapsedSeconds;
```

`speed` est ici en **noudets** et `1852` m par mille marin. Vérification
expérimentale : rudder = 2, speed = 6.3 kn → `6.3 * 1852 / 600 = 19.45 °/min`,
soit `0.324 °/tick` à 1 s. La capture mesure `+0.30 °/s` (17.9 → 18.2 → 18.5 →
…). **Conforme.**

Quatre points de rupture :

- le rudder est un entier ;
- le rudder exactement nul **stoppe instantanément** le virage : pas d'inertie
  de barre, la coupe est franche ;
- un virage déjà engagé s'arrête dès que le rudder repasse à 0 ;
- il n'y a **aucun retour automatique** au centre : la barre reste là où on
  l'a laissée, et un virage en cours reste engagé.

Déplacement de la position :

```js
deltaKm = knotsToMSec(speed) * elapsedSeconds / 1000;
position = destinationPoint(position, heading, deltaKm);   // rayon 6371 km
```

Vérifié : à 10 kn et 1 s, déplacement mesuré 5.136 m contre 5.144 m attendus.
L'unité `we` est donc bien en kilomètres malgré le nom trompeur.

### 3.5 Vent

Le vent apparent est recalculé à chaque tick à partir du vent réel et de la
vitesse du navire (composition vectorielle, `updateApparentWind`).

## 4. Réseau

### 4.1 Types de serveur

`8138.js` définit des **constantes numériques**, et `933.js` fait un `switch` sur
ces nombres :

| Constante | Valeur |
|---|---|
| `websocket` | 0 |
| `tcpserver` | 1 |
| `tcpclient` | 2 |
| `serial` | 3 |
| `udp` | 4 |
| `udpclient` | 5 |
| `udpmulticast` | 6 |

Le sélecteur de l'UI utilise `mat-select` avec `[value]`, ce qui renvoie la
**valeur numérique** ; l'objet stocké contient donc un nombre. Écrire `"udp"`
dans la configuration ne produit aucun serveur (`switch` sans cas correspondant)
sans message d'erreur.

### 4.2 Comportements vérifiés en exécution

| Transport | Vérification | Résultat observé |
|---|---|---|
| TCP server | `listen(port)` sans hôte | écoute sur toutes les interfaces (`*:10110` en test) |
| TCP server (NMEA) | 24 `send()` par tick | un write par phrase, `\r\n` inclus ; TCP peut coalescer |
| TCP server (Signal K) | 1 `send()` par tick | un write JSON, sans CRLF ; hello à l'ouverture de connexion |
| WebSocket server | port configurable | NMEA fonctionne, aucun hello |
| UDP (`type:4`) | socket créé au `start()` | broadcast sur l'interface sélectionnée, un datagramme par phrase, port source éphémère |
| ViewSync | UDP séparé, port 7001 | un datagramme par update |

Écoute TCP confirmée par capture sur `0.0.0.0:10110`.
TCP client, UDP client, UDP multicast et série ont été vérifiés en P1/P2 :
voir `04-compatibility-matrix.md` §4.

UDP broadcast confirmé sur `wlp0s20f3` (`192.168.11.130/22`) vers
`192.168.15.255:3100`.

### 4.3 Défauts de cycle de vie

- `startTCPServer()` résout sa promesse **avant** le callback `listen`, donc
  l'UI peut afficher « démarré » alors que le bind échouera.
- `stop()` ne remet pas `this.server` à `null` de façon synchrone ; `changeType()`
  ne change de type que si `this.server` est falsy, d'où une fenêtre de course
  lors d'un changement de type juste après l'arrêt.

## 5. Journal et rejeu

### 5.1 Format `.nmeasim`

```
nmeasim,1.6.1\r\n~
$GPRMC,...\r\n$IIVHW,...\r\n...\r\n~
$GPRMC,...\r\n...\r\n~
```

- Première ligne : `<appId>,<version>` — ici `nmeasim,1.6.1`.
- Séparateur de bloc : `~`.
- Un bloc par tick, phrases séparées par `\r\n`.
- Écriture directe avec `fs.createWriteStream` ; le nom reçoit l'extension
  `.nmeasim` si elle est absente.

`loadLogFile()` découpe sur `~`, vérifie `data.slice(0,7) === "nmeasim"`, puis
retire le premier bloc (header) et le dernier (vide) via `shift()` et `pop()`.

### 5.2 Rejeu

- Utilise **l'instance de communications déjà configurée** (singleton injecté à la
  racine) après `stop()`.
- `step()` envoie **un bloc entier en un seul `send()`** : à la différence du
  mode direct, toutes les phrases d'un tick, CRLF final compris, sont
  dans un même write TCP (vérifié : `legacy-p2/cap-replay.log`, 981 à 990
  octets par write, le bloc se termine par `\r\n`).
- Après une pause, `step(+1)` incrémente l'index **avant** d'envoyer : le bloc
  affiché mais non encore envoyé est sauté (vérifié : bloc 10 jamais émis).
- Le dialogue de rejeu démarre le transport à l'ouverture et l'arrête à la
  fermeture.
- `go()` boucle sur `config.interval`, s'arrête **à la fin** du journal, sans
  bouclage.
- L'affichage décode `group.split("\n")`, ce qui laisse le `\r` en fin de ligne.

## 6. Import GPX / KML

- GPX : tracks et routes lus ; `ele`, `datetime`, `course`, `speed` utilisés
  quand présents.
- Suivi : **un point par tick**, sans interpolation temporelle.
- Vitesse : `<speed>`, sinon distance/Δt, sinon le seed GPS.
- Cap : `<course>`, sinon relèvement vers le point suivant.
- Fin de trace : arrêt du simulateur et message
  `End of track has been reached.`
- KML : conversion via la classe interne `Mk` → structures `Document`, `Folder`,
  `Placemark`, `MultiTrack`, `MultiGeometry`, `Track` supportées. Le help annonce
  `<gx:Track>`. Validation dynamique (P2, `legacy-p2/`) : `gx:Track` importé et
  suivi ; `LineString` rejeté (« No Track Data ») ; seules les `Placemark` du
  premier niveau de `Folder` sont lues. Le suivi n'émet ni le premier ni le
  dernier point de la trace, et l'heure des phrases vient des `<when>`.
- L'entrée de fichier est un vrai `<input type=file>` (`ap-file-input`) de la
  barre d'outils, visible simulateur arrêté.

## 7. Interface

- Panneau `vessel` (helm) : mode direction, cap, barre, bouton *Mark Position*.
- Le mode direction **force `mask.heading = true` et remet le rudder à 0** à
  chaque bascule.
- *Mark Position* écrit le waypoint `wpt` (et l'active), il ne crée pas de route.
- **Le clic droit sur la carte fixe la destination** (le clic gauche sert au
  suivi de trace ; le tooltip l'indique explicitement).
- `setDestination()` renseigne `ap.value.destination` et fige
  `ap.value.origin` sur la position courante.
- La position du clic droit provient de `this.cursor`, alimenté par le dernier
  `pointermove` : un clic droit sans mouvement de souris utilise la dernière
  position connue.
- Clavier : `↑`/`↓` agissent sur la vitesse **uniquement si `mask.speed`** ;
  `←`/`→` agissent sur le rudder en mode direction, sinon sur le cap si
  `mask.heading`. Pas de 1, valeurs clampées.
- Les boutons Start/Stop des moteurs et les overrides RPM Positionnent
  `value.running` ; le RPM n'est publié que si le moteur tourne.

## 8. Ce que le legacy ne fait pas

- Pas de cinématique : inertie, dérive, courants, houle, résistance.
- Pas de modèle d'équipage ni d'autopilote Rosenthal.
- Pas de trafic AIS ni de cibles tierces : seuls le navire et sa destination
  sont émis.
- Pas d'API de contrôle distante (seulement IPC local).
- Pas de version Signal K correcte dans le hello.
