# Configuration

## Fichier

`~/.config/nmeasim-rs/config.json` (Linux), `%APPDATA%\nmeasim-rs\config.json`
(Windows), `~/Library/Application Support/nmeasim-rs/config.json` (macOS), ou
`--config <fichier>`. Écrit par l'interface (Appliquer, fermeture) de façon
atomique.

Règles :

- **schéma versionné** (`"schema": 2`) ;
- une **configuration NMEASimulator 1.6.1** (`nmeasim_config`, export
  `localStorage`) est détectée et **migrée** : `server.type` 0 à 6 → transport
  nommé, seeds → état initial et bruit, talkers, ViewSync, préfixe, `vesselId` ;
  `nmeasim --import-legacy <fichier>` l'écrit au nouveau format ;
- **aucun champ perdu** : une clé inconnue est conservée telle quelle et
  réécrite, avec un avertissement ; les clés legacy non migrées sont gardées
  sous `legacy` ;
- **erreur explicite** : une valeur invalide est refusée avec le champ fautif
  (`output.intervalMs : doit être dans 1..=10000`) ; un `server.type` legacy
  en chaîne (`"udp"`) est une erreur, là où 1.6.1 ne démarrait rien en silence.

## Structure

```jsonc
{
  "schema": 2,
  "autoStart": false,
  "simulation": {
    "seed": 1234,                  // null = graine tirée puis journalisée
    "stepMs": 20,                  // pas d'intégration
    "startTimeMs": null,           // null = heure murale
    "compatibility": { "profile": "modern", "overrides": { "rudder": "legacy" } },
    "vessel": { "name": "NMEASIM", "lengthM": 20, "maxSpeedAheadKn": 12, "maxSpeedAsternKn": 4,
                "accelTimeS": 12, "decelTimeS": 25, "rudderRateDegS": 8, "maxRudderDeg": 40,
                "yawHalfLifeMs": 1500, "idleRpm": 600, "maxRpm": 3600, "rpmRate": 800,
                "leewayCoeff": 0.03, "engineLabels": ["Port Engine", "Starboard Engine"] },
    "initial": { "position": { "lat": -35.0, "lon": 138.5 }, "altitudeM": 2, "headingDeg": 15,
                 "speedKn": 5, "enginesRunning": false, "throttle": 0 },
    "environment": { "windDirectionDeg": 285, "windSpeedKn": 11, "depthM": 8, "waterTemperatureC": 7.5,
                     "currentSetDeg": 0, "currentDriftKn": 0, "magneticVariationDeg": 0 },
    "gnss": { "fix": true, "hdop": 1.5, "vdop": 1.5, "pdop": 1.5, "satellites": 10 },
    "drift": { "windSpeed": { "enabled": true, "min": 2, "max": 40, "amplitude": 0.2,
                              "halfLifeMs": 60000, "plus": 0.2, "minus": 0.2 } },
    "arrivalRadiusM": 100
  },
  "output": {
    "intervalMs": 1000,
    "nmea": { "talkers": { "VTG": "GP" }, "sentences": null, "prefix": false,
              "prefixSource": "nmeasim", "zdaOffsetMinutes": null,
              "ais": { "mmsi": 503999999, "callsign": "SIM1234", "name": "NMEASIM",
                       "shipType": 37, "destination": "SYDNEY", "sequentialId": 9 } },
    "signalk": { "vesselId": "urn:mrn:signalk:uuid:…", "context": "vessels", "source": "nmea-simulator" }
  },
  "transports": [ { "id": "nmea-tcp", "type": "tcp-server", "port": 10110, "format": "nmea" } ],
  "viewsync": { "enabled": false, "host": "127.0.0.1", "port": 7001, "altitude": 30, "tilt": 85 },
  "gamepad": { "enabled": true, "active": "default", "profiles": [ … ] },
  "ui": { "theme": "dark", "bottomTab": "nmea", "monitorLines": 2000,
          "map": { "follow": true, "zoom": 13, "tiles": false, "trailPoints": 2000 } },
  "logging": { "meta": true },
  "api": { "enabled": false, "bind": "127.0.0.1", "port": 8375, "token": null },
  "track": { "kmlLegacy": false, "playMode": "fixed-rate", "repeat": false }
}
```

Tous les champs sont facultatifs : une valeur absente prend sa valeur par
défaut.

## Profil de compatibilité

`simulation.compatibility.profile` : `modern` (défaut) ou `legacy`.
`overrides` règle un comportement à la fois, sans dupliquer le moteur :

| Clé | Legacy | Modern |
|---|---|---|
| `rudder` | barre instantanée, entière | vitesse de barre |
| `zeroRudder` | virage coupé net à 0 | taux de giration qui décroît |
| `noise` | `applySeed` relatif | bruit borné avec rappel |
| `vbw` | `speed − random()` | vitesses du modèle |
| `rpm` | pas d'hélice `10.5` | pas du modèle |
| `propulsion` | vitesse commandée | dynamique avec inertie |
| `sentences` | 24 phrases legacy | jeu moderne (+HDM, ROT, GSV) |

Les défauts corrigés de 1.6.1 (DBT déjà conforme, RMB hémisphères, formatage
des coordonnées, RMC mode, hello Signal K…) sont corrigés dans les deux profils.

## Bruit

Chaque grandeur bruitée a un `DriftSpec` : bornes `min`/`max`, `amplitude`
absolue par √s et `halfLifeMs` de rappel (modern), `plus`/`minus` relatifs
(legacy). `enabled: false` fige la grandeur. Avec une graine fixe, deux
exécutions avec les mêmes commandes produisent exactement le même état.
