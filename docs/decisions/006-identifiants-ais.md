# D6 — Identifiants AIS

| | |
|---|---|
| Statut | Acceptée — **prémisse corrigée** par l'observation |
| Date | 2026-09-28 |
| Auteur | session P2/P3 (agent), sur arbitrage de l'utilisateur |
| Exigences | OUT-02, NFR-08 |
| Documents | `06-simulation-model.md` §11, `07-output-formats.md` §6 |

## Contexte

Le plan présentait l'alternative « MMSI aléatoire (legacy) / fixe
configurable ». Un récepteur AIS, un ECDIS ou un agrégateur identifie un navire
par son MMSI : un identifiant instable fait apparaître une nouvelle cible à
chaque session, un identifiant non configurable empêche de simuler plusieurs
navires ou de coller à la configuration « own ship » d'un ECDIS.

## Comportement legacy

Observation sur **toutes** les phrases AIS des 17 captures qui en contiennent
(`tests/fixtures/legacy*/cap-*.log`, décodage des bits 8 à 37) :

| Message | MMSI |
|---|---|
| `AIVDO` / `AIVDM` type 1 | `503999999` |
| `AIVDO` / `AIVDM` type 5 | `503999999` |

Code : `mmsi: 503999999` en dur dans `init()` du codeur AIS
(`mods_b/7078.js:655`). Le MMSI legacy est donc **fixe et non configurable**,
pas aléatoire ; la prémisse du plan est corrigée.

Autres valeurs en dur (`mods_b/7264.js:518`, `07-output-formats.md` §6) :
indicatif `SIM1234`, nom `NMEASIM`, type de navire `37`, destination `SYDNEY`
(nom de l'objet `destination`), identifiant de séquence multi-fragments `9`.
L'objet `destination` prépare une ETA en heure locale (`getMonth()`,
`getHours()`…), mais l'encodeur `_encodeVoyageData` (`mods_b/7078.js`) écrase
mois, jour, heure et minute avec l'**heure UTC courante** : l'ETA émise est
l'instant d'émission, en UTC (constat de l'implémentation, `docs/04` §3.19 ;
la version initiale de cette décision affirmait à tort une ETA locale).

`503` est le MID de l'Australie : `503999999` a la forme d'un MMSI de navire
réel.

## Options

1. Garder les valeurs en dur.
2. MMSI aléatoire par session.
3. **Identité AIS fixe par défaut, égale au legacy, entièrement configurable.**

## Décision

Option 3.

```jsonc
"ais": {
  "mmsi": 503999999,        // défaut = legacy
  "callsign": "SIM1234",
  "name": "NMEASIM",
  "shipType": 37,
  "imo": 0,
  "destination": "SYDNEY",
  "sequentialId": 9,
  "dimensions": { "bow": 0, "stern": 0, "port": 0, "starboard": 0 }
}
```

- Validation : MMSI entier de 9 chiffres ; indicatif ≤ 7 caractères, nom ≤ 20,
  destination ≤ 20, jeu de caractères AIS à 6 bits ; `sequentialId` dans 0..9.
  Une valeur invalide est une **erreur de configuration explicite**, jamais un
  tronquage silencieux.
- L'identité n'est jamais tirée au hasard, en aucun profil.
- ETA du type 5 : **UTC**, configurable (`ais.etaMs`) ; par défaut l'heure
  courante, comme le legacy.
- Duplication `AIVDO` + `AIVDM` conservée (`06-simulation-model.md` §11).

## Justification

- Garder `503999999` par défaut préserve les installations existantes, où
  l'ECDIS est souvent configuré pour reconnaître ce MMSI comme navire propre.
- La configurabilité est nécessaire pour les bancs à plusieurs simulateurs et
  pour éviter qu'un flux de test sorti du laboratoire se confonde avec un vrai
  navire : l'UI signale que la valeur par défaut a la forme d'un MMSI réel.
- Rien ne justifie un identifiant aléatoire : il casserait le suivi de cible.

## Compatibilité

- Par défaut, charges utiles AIS identiques au legacy pour les champs
  d'identité.
- Par défaut, l'ETA est identique au legacy (heure UTC courante).

## Conséquences

- Section `ais` du schéma de configuration et migration depuis la config
  legacy (qui n'a pas de telle section : valeurs par défaut).
- L'encodeur AIS reçoit l'identité comme donnée ; il ne contient plus aucune
  constante d'identité.
- Chemins API : `ais.mmsi`, `ais.callsign`, `ais.name`, `ais.destination`
  modifiables simulateur arrêté ; changement en marche refusé avec une erreur
  explicite, pour ne pas faire changer d'identité une cible suivie.

## Tests à prévoir

| Test | Attendu |
|---|---|
| `ais:default-identity` | type 1 et type 5 : MMSI décodé = `503999999`, indicatif `SIM1234`, nom `NMEASIM` |
| `ais:configured-mmsi` | `mmsi: 227006760` → décodé à l'identique dans les 4 phrases d'un tick |
| `ais:stable` | 1 000 ticks, 2 sessions : MMSI constant |
| `ais:invalid` | `mmsi: 12345` ou nom de 21 caractères → erreur de validation nommant le champ |
| `ais:eta-utc` | ETA configurée 10:00 UTC → champs heure = 10, quel que soit le fuseau |
| `ais:golden-identity` | charges utiles d'identité de `legacy/cap-tcp.log` reproduites octet pour octet |
