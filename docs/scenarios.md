# Scénarios

Un scénario est un fichier JSON qui fige une situation de test : position,
cap, vitesse, navire, environnement, graine, route, pilote, format NMEA et,
facultativement, transports et ViewSync.

## Onglet SCENARIO

| Bouton | Effet |
|---|---|
| Nouveau | configuration de simulation par défaut, route effacée |
| Ouvrir… | applique le scénario, pose la route et engage le pilote |
| Enregistrer / Enregistrer sous… | **capture l'état courant** : position, cap, vitesse, propulsion, vent, courant, profondeur, graine, route |
| Lancer / Pause / Arrêter | cycle de vie de la simulation |

## Ligne de commande

```bash
nmeasim --scenario examples/scenarios/port-approach.json
nmeasim --headless --scenario examples/scenarios/port-approach.json --duration 600 --print
```

## Format

```json
{
  "schema": 1,
  "name": "Approche de port",
  "description": "Route de 3 points, pilote en suivi de route, courant de travers",
  "simulation": {
    "seed": 42,
    "initial": { "position": { "lat": -35.02, "lon": 138.45 }, "headingDeg": 90, "speedKn": 6 },
    "environment": { "windDirectionDeg": 200, "windSpeedKn": 15, "currentSetDeg": 0, "currentDriftKn": 1.0 }
  },
  "intervalMs": 1000,
  "route": [
    { "name": "WP1", "position": { "lat": -35.02, "lon": 138.47 } },
    { "name": "WP2", "position": { "lat": -35.00, "lon": 138.49 } },
    { "name": "PORT", "position": { "lat": -34.98, "lon": 138.50 } }
  ],
  "autopilot": "route",
  "autoStart": true,
  "transports": null
}
```

- `transports: null` conserve les transports de la configuration ; une liste
  les remplace (banc d'essai avec ses propres ports).
- `autopilot` : `off`, `heading`, `route`.
- `autoStart: true` démarre la simulation au chargement.
- La graine rend le scénario reproductible : même scénario, mêmes commandes,
  même trajectoire et mêmes phrases.

Des exemples sont fournis dans `examples/scenarios/`.
