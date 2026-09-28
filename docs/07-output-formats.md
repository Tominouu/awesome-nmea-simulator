# 07 — Formats de sortie

## 1. Règles communes

### 1.1 Phrase NMEA

```
$<talker><formatter>,<champ 1>,...,<champ n>*<checksum XX>\r\n
```

- Checksum : **XOR** sur les caractères situés entre `$` et `*`, en majuscules
  hexadécimal sur deux caractères.
- Champ vide : chaîne vide, jamais `null`, jamais `"undefined"`, jamais `"NaN"`.
- Talkers configurables par formatter, avec une valeur par défaut par phrase.
- Le `\r\n` appartient au **transport**, pas à la phrase. L'encodeur produit un
  objet `{ talker, formatter, fields }` et laisse le framing au transport.

Cette séparation corrige un défaut structurel du legacy, où la phrase elle-même
portait son terminateur et devait être réécrite selon le transport.

### 1.2 Talking par défaut

| Formatter | Talker | Note |
|---|---|---|
| `RMC, GLL, GGA, GSA, ZDA, GBS, VDR, RLM, RMB, VTG` | `GP` | le legacy utilise `GP` pour `VTG` malgré le help |
| `VHW, HDT, VBW, MTW, RPM, APB` | `II` | |
| `MWD, MWV` | `WI` | |
| `DPT, DBT` | `SD` | |
| `WPL` | `IN` | |
| `AIVDM, AIVDO` | `AI` | |

## 2. Contenu par phrase

### Navigation

| Sentence | Champs significatifs | Note cible |
|---|---|---|
| `RMC` | heure UTC, statut, lat, N/S, lon, E/W, SOG, cap, date, variation, mode | **ajout du mode** |
| `VHW` | cap rel, `T/M`, cap vrai, SOG, STW | |
| `VTG` | cap, `T/M`, SOG, STW, `N/K` | talker `GP` |
| `HDT` | cap vrai | |
| `GLL` | lat, N/S, lon, E/W, heure, statut | |
| `ZDA` | heure UTC, jour, mois, année, zone (heures signées), zone (minutes) | zone du système, `+` implicite, `-` explicite (doc 04 §3.4) ; fuseau configurable dans la cible |
| `VDR` | cap, cap vrai | ajout cible, pour waterlogging |

### GNSS

| Sentence | Champs | Note |
|---|---|---|
| `GGA` | heure, lat, lon, quality, satellites, HDOP, altitude, géo, unités, PDOP, VDOP | 4 champs terminaux vides côté legacy : toléré |
| `GSA` | mode (`A`/`M`), PRN utilisés, PDOP, HDOP, VDOP | |
| `GBS` | horodatage, satellites en erreur, probabilité, exactitude | **ajout cible** |
| `WPL` | lat, N/S, lon, E/W, nom | muet par défaut côté legacy |

### Environnement

| Sentence | Champs | Note |
|---|---|---|
| `MWD` | direction vent, N, vitesse vent, M, N, K | |
| `MWV` | angle, `R`, vitesse, `N/M/S/K` | |
| `MTW` | température, `C` | degrés Celsius, une décimale |
| `DPT` | profondeur, `m` | |
| `DBT` | pieds, `f`, mètres, `M`, brasses, `F` | ordre legacy **conforme**, conservé (doc 04 §3.1, D4) |

### Propulsion

| Sentence | Champs | Note |
|---|---|---|
| `RPM` | source, `E`/`D`, propulseur, régime, `R`, pct charge, statut | charge legacy en dur `10.5` → corrigé |
| `VBW` | vitesse axiale, `L/R`, vitesse transversale, `L/R` | legacy non déterministe → corrigé |

### Route

| Sentence | Champs | Note |
|---|---|---|
| `APB` | LKA, route status, XTE, sens, type, ETA, origine, rel, `T`, destination, rel, `T`, hts, `T`, mode | inerte si pas de route |
| `RMB` | statut, XTE, sens, origine, destination, lat, N/S, lon, E/W, distance, relèvement, vitesse de rapprochement, arrivée | **hémisphères à corriger** (doc 04 §3.2) |

**Règle de cohérence** : `APB` et `RMB` doivent publier exactement les mêmes
grandeurs, extraites du même calcul. Toute divergence entre la distance publiée
et les coordonnées publiées est impossible par construction.

## 3. Préfixe propriétaire 61162-450

Format vérifié en exécution :

```
\c:<unix_seconds>,s:nmeasim*<CS>\<sentence>\r\n
```

- Le préfixe précède la sentence, y compris son `$`.
- Le checksum couvre le contenu entre le début du préfixe et le `*` inclus, sur
  le même modèle que les sentences.
- Il est appliqué à **toutes** les phrases générées, et aux phrases injectées par
  l'utilisateur, lorsque l'option est active.
- L'échappement est un antislash littéral devant `$`.

## 4. Signal K

### 4.1 Hello

Émis à l'ouverture d'une connexion sur les transports orientés connexion
(TCP et WebSocket). Structure observée :

```json
{
  "name": "nmeasimulator",
  "version": "1.6.1",
  "self": "urn:mrn:signalk:uuid:...",
  "roles": ["master", "main"],
  "timestamp": "2026-09-28T08:37:44.985Z"
}
```

**Correction obligatoire** : la version doit venir de la source de version unique
de l'application. Le legacy annonce `17.12.05`, vestige d'une version de classe de
base copiée avant la résolution IPC.

### 4.2 Delta

```json
{
  "context": "vessels.urn:mrn:signalk:uuid:...",
  "updates": [{
    "timestamp": "2026-09-28T08:37:44.985Z",
    "$source": "nmea-simulator",
    "values": {
      "navigation.speedOverGround": 3.24,
      "navigation.headingTrue": 0.3054
    }
  }]
}
```

- Un message par tick, **sans CRLF final**.
- Chemins en kebab-case, valeurs en **unités SI** :
  - vitesses m/s, angles et relèvements en **radians**,
  - températures en **Kelvin**, profondeurs et positions en mètres et degrés.
- `$source` vaut `nmea-simulator` (valeur observée, conservée).

### 4.3 Chemins émis

| Chemin | Unité | Source |
|---|---|---|
| `navigation.courseOverGroundTrue` | rad | cap vrai |
| `navigation.headingTrue` | rad | cap vrai |
| `navigation.speedOverGround` | m/s | SOG |
| `navigation.speedThroughWater` | m/s | STW |
| `navigation.gnss.methodQuality` | enum | type de fix |
| `navigation.gnss.type` | string | constellation |
| `navigation.gnss.satellites` | num | satellites utilisés |
| `navigation.gnss.horizontalDilution` | num | HDOP |
| `navigation.gnss.positionDilution` | num | PDOP |
| `navigation.gnss.verticalDilution` | num | VDOP |
| `navigation.position` | objet | `longitude`, `latitude`, `altitude` |
| `environment.wind.directionTrue` | rad | vent réel |
| `environment.wind.speedTrue` | m/s | vent réel |
| `environment.wind.directionApparent` | rad | vent apparent |
| `environment.wind.speedApparent` | m/s | vent apparent |
| `environment.water.temperature` | K | température d'eau |
| `environment.depth.belowTransducer` | m | profondeur |
| `propulsion.<id>.state` | string | `stopped` / `running` |
| `propulsion.<id>.label` | string | `Port` / `Starboard` |
| `propulsion.<id>.temperature` | K | température moteur |
| `propulsion.<id>.revolutions` | num | régime |

### 4.4 Corrections

| Défaut legacy | Cible |
|---|---|
| `navigation.destination.waypoint .<id>` (espace) | chemin valide |
| version périmée | version applicative |
| aucune donnée de destination | chemin de destination correct, activé seulement si une route existe |

## 5. ViewSync

Protocole CSV sans checksum ni terminateur, envoyé en UDP.

```
counter,latitude,longitude,altitude,heading,tilt,roll,timeStart,timeEnd,planet
```

| Champ | Format legacy observé | Cible |
|---|---|---|
| `counter` | entier, démarre à 1, incrémenté après envoi | identique |
| `latitude` | degrés décimaux | identique |
| `longitude` | degrés décimaux | identique |
| `altitude` | config, remplacé par l'altitude GPS si supérieure | identique, plus explicite |
| `heading` | **degrés** | identique (à ne pas confondre avec les radians de Signal K) |
| `tilt` | constante `85` | identique |
| `roll` | constante `0` | lié à la gîte si activée |
| `timeStart` | `62167219200 + floor(Date.now()/1000)` | identique |
| `timeEnd` | `timeStart + interval/1000` si interval ≥ 1000, sinon `timeStart` | **corrigé** |
| `planet` | vide, le paquet se termine par `,` | identique |

Port et adresse par défaut : `127.0.0.1:7001`.

## 6. AIS

### 6.1 Encodage

- Payload à 6 bits, selon la table de caractères ITU-R M.1371 (`0-9`, `A-Z`, puis
  les caractères de substitution vers `u`, `v`, `w`, `x`, `y`, `z`).
- Champ à 6 bits dont la valeur `0` doit être encodée par substitution pour
  éviter les mots de Beginning/End of Sequence.
- Checksum : CRC 16-CCITT sur la trame entre `!` et `*`.
- Fragmentation : champ numéro de fragment sur 2 bits, `0` pour non fragmenté.

### 6.2 Messages

| Type | Legacy | Cible |
|---|---|---|
| 1 (position) | `AIVDO` + `AIVDM` | identique, PDOP corrigé |
| 5 (destination) | `AIVDO` + `AIVDM`, 2 fragments, séquence 9 | identique |
| 18 (position précise) | absent | **ajout optionnel** |

Valeurs fixes observées côté destination : callsign `SIM1234`, nom `NMEASIM`,
type de navire `37`, destination `SYDNEY`, ETA en heure locale.

## 7. Ordre et regroupement d'un tick

Le legacy émet 24 phrases par bloc, dans un ordre fixe. La cible conserve cet
ordre par défaut pour que les **golden files** restent comparables, mais
l'ordre devient une donnée de configuration (`sentenceOrder`) et non une
conséquence de l'ordre des méthodes dans le code.

## 8. Journal de sortie

Le journal est une sortie de plus : il consomme le **même** bloc de phrases que
les sockets, et l'écrit dans le format `.nmeasim` (voir `10-routes-and-replay.md`).
Il ne doit jamais construire ses propres phrases.
