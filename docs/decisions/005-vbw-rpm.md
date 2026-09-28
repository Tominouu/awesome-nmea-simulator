# D5 — `VBW` et `RPM` modernisés

| | |
|---|---|
| Statut | Acceptée |
| Date | 2026-09-28 |
| Auteur | session P2/P3 (agent), sur arbitrage de l'utilisateur |
| Exigences | OUT-02, SIM-11, SIM-06 |
| Documents | `04-compatibility-matrix.md` §3.8-3.9, `06-simulation-model.md` §9, `07-output-formats.md` §2 |

## Contexte

`VBW` (vitesses eau et fond, longitudinales et transversales) et `RPM`
(régime et pas d'hélice) sont émis à chaque tick. Dans le legacy, ce sont des
valeurs de décor, sans lien avec l'état du navire. Les récepteurs de test
(instruments, VDR, pilotes) les exploitent pourtant.

## Comportement legacy

`VBW` (`mods_b/7264.js:335`) : les **six** valeurs numériques valent
`(speed − Math.random()).toFixed(1)`, statuts tous `A` :

```
$IIVBW,9.5,9.1,A,9.5,9.6,A,9.4,A,9.7,A      à 10 kn (legacy/cap-fixed.log)
```

Les vitesses **transversales** (champs 2, 5, 7, 9) valent donc presque la
vitesse d'avance : 9,1 kn de vitesse latérale pour un navire en route droite.

`RPM` (`mods_b/7264.js:558`) :

```
$IIRPM,E,1,0,10.5,A          moteur arrêté (legacy/cap-tcp.log)
```

- champ 3 : `rpm.toFixed(1)` si `running`, sinon l'entier `0` (format
  différent : `0` au lieu de `0.0`) ;
- champ 4 : `"10.5"` en dur. Le dictionnaire de champs du legacy lui-même
  (`mods_b/7078.js:418`) le décrit comme « Propeller pitch, % of maximum, '-'
  means astern ». Les documents antérieurs l'appelaient « charge moteur » :
  c'est le **pas d'hélice** ;
- champ 5 : `A` en toutes circonstances.

## Options

1. Legacy seul.
2. Valeurs dérivées de l'état, sans mode legacy.
3. Valeurs dérivées de l'état par défaut, legacy derrière le drapeau.

## Décision

Option 3 (`overrides.vbw: "legacy"`, `overrides.rpm: "legacy"`, ou profil
`legacy`).

`VBW`, profil `modern` :

| Champ | Valeur |
|---|---|
| 1 longitudinale eau | `STW · cos(dérive)` |
| 2 transversale eau | `STW · sin(dérive)` ; dérive = 0 sans modèle de dérive, donc `0.0` |
| 4, 5 fond | projection de la vitesse fond (SOG, COG) sur l'axe et le travers du cap |
| 7 transversale arrière eau | champ 2 + `r · L/2` (taux de giration D2, `L` longueur configurable, 20 m) |
| 9 transversale arrière fond | champ 5 + `r · L/2` |
| 3, 6, 8, 10 | `A` si la source est valide, `V` sinon |

Chaque valeur reçoit le bruit borné de D3 (amplitude 0,05 kn par défaut).
Signe : positif vers tribord pour les transversales, négatif en marche arrière
pour les longitudinales, conformément à NMEA 0183.

`RPM`, profil `modern` :

| Champ | Valeur |
|---|---|
| 3 régime | `rpm` au format `%.1f`, `0.0` moteur arrêté |
| 4 pas d'hélice | `propulsion.<id>.pitchPercent` : hélice à pas fixe → `100.0` en avant, `-100.0` en arrière, `0.0` au point mort ; pas variable → valeur commandée |
| 5 statut | `A` si le capteur est valide, `V` en panne simulée |

La **charge moteur** existe comme grandeur d'état (`propulsion.<id>.load`,
0..1, `06-simulation-model.md` §9) et est publiée en Signal K
(`propulsion.<id>.engineLoad`) ; elle n'est pas émise dans `RPM`, qui n'a pas
de champ pour elle.

Profil `legacy` : formules legacy exactes, avec `Math.random()` remplacé par le
`Rng` injecté (D3), donc reproductibles.

## Justification

- Une vitesse transversale de 9 kn en route droite rend `VBW` inutilisable pour
  tester un instrument ou un VDR.
- Nommer correctement le champ 4 évite de construire un faux « modèle de
  charge » sur un champ de pas d'hélice.
- Le mode legacy reste utile aux bancs qui ont été calibrés sur ces valeurs.

## Compatibilité

- Formats et ordre des champs inchangés ; seules les valeurs changent en profil
  `modern`.
- Moteur arrêté : `0` devient `0.0` en profil `modern` ; `0` est conservé en
  profil `legacy`.
- Les golden files `VBW` et `RPM` ne sont comparables qu'en profil `legacy`
  avec une séquence `Rng` imposée, ou après masquage des valeurs.

## Conséquences

- Nouvelles grandeurs : `vessel.leeway` (rad, 0 par défaut),
  `vessel.length` (m), `propulsion.<id>.pitchPercent`,
  `propulsion.<id>.propellerType`.
- La commande de propulsion de l'API (throttle, marche avant/arrière) pilote
  `pitchPercent` et `rpm` ; aucune entrée n'écrit ces champs directement.

## Tests à prévoir

| Test | Attendu |
|---|---|
| `nmea:vbw-straight` | route droite, 10 kn, bruit désactivé : `10.0,0.0,A,10.0,0.0,A,0.0,A,0.0,A` |
| `nmea:vbw-turn` | `r` ≠ 0 : champs 7 et 9 = `r · L/2` avec le signe du virage |
| `nmea:vbw-bounded` | 10⁵ pas avec bruit : \|champ 2\| ≤ 3 × amplitude |
| `nmea:rpm-stopped` | moteur arrêté : `$IIRPM,E,1,0.0,0.0,A` |
| `nmea:rpm-ahead-astern` | pas fixe : `100.0` en avant, `-100.0` en arrière |
| `legacy-mode:vbw` | profil `legacy`, `Rng` imposé : six valeurs `speed − u` |
| `legacy-mode:rpm` | profil `legacy` : champ 4 = `10.5`, arrêté = `0` |
