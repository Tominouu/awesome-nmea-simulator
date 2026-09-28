# 06 — Modèle de simulation

## 1. Principes

1. **Unités SI dans le noyau.** Vitesses en m/s, angles en radians, temps en ms
   epoch, distance en mètres. Les unités nautiques n'apparaissent qu'au bord.
2. **Pas de temps fixe.** Le noyau avance par pas fixes ; l'intervalle de
   publication est un client de l'horloge et ne modifie pas la trajectoire.
3. **Pas de dérives ascendantes.** Chaque grandeur stochastique est un bruit
   borné à moyenne nulle, avec retour vers la valeur nominale.
4. **Déterminisme possible.** Le générateur aléatoire est injecté et
   initialisable : à graine égale et à séquence d'événements égale, l'état est
   identique. C'est ce qui rend les tests reproductibles.

## 2. Le navire

Le legacy n'a pas d'objet `vessel` : la position vient de `gps`, la barre de
`gps.rudderAngle`, la propulsion de `p0`/`p1`. La cible introduit un objet
`vessel` explicite qui **agrège** ces grandeurs sans les dupliquer.

```ts
interface VesselState {
  t: number;
  position: LatLon;
  headingTrue: number;         // rad, [0, 2π)
  headingMagnetic: number;     // rad
  speedOverGround: number;     // m/s
  speedThroughWater: number;   // m/s
  rudder: number;              // deg entier, [-40, 40]
  heel: number;                // rad
  pitch: number;               // rad
}
```

### 2.1 Le bateau comme unique source de vérité

Toute écriture passe par le chemin `vessel.*`. Le GPS devient une **projection**
de l'état du navire (position, vitesse, cap) et non un stockage parallèle. Cela
supprime la classe de défaut où l'UI affiche un cap et où la phrase NMEA en sort
un autre, tous deux Initialisés à des valeurs par défaut divergentes.

## 3. Barre et virage

### 3.1 Dynamique de barre

Le legacy fait sauter la barre d'un degré par frappe et coupe le virage à zéro
instantanément. La cible ajoute un taux de barre.

```ts
const RUDDER_RATE_DEG_PER_S = 8;     // configurable

function stepRudder(current: number, target: number, dt: number): number {
  const maxDelta = RUDDER_RATE_DEG_PER_S * (dt / 1000);
  const delta = clamp(target - current, -maxDelta, maxDelta);
  return clamp(current + delta, RUDDER_MIN, RUDDER_MAX);
}
```

Trois points à documenter dans l'UI :

- la barre ne bouge plus par pas entiers instantanés ;
- le virage continue **pendant** que la barre revient au centre, ce qui est le
  comportement maritime attendu ;
- le mode `legacy` (barre instantanée, arrêt du virage à zéro) est disponible
  derrière une option, pour les scénarios qui en dépendent.

### 3.2 Taux de virage

On conserve exactement la loi du legacy, car elle est vérifiée et partagée par
beaucoup d'installations :

```
turnRate [°/min] = speed_kn * 1852 / turnRadius_m
```

| Barre | Rayon |
|---|---|
| ≥ 20° | 150 m |
| 10–19° | 200 m |
| 5–9° | 400 m |
| 1–4° | 600 m |
| 0° | pas de virage (`legacy`) / dérive nulle (`défaut`) |

Vérification de référence conservée en test : barre 2, vitesse 6.3 kn →
`6.3 * 1852 / 600 = 19.45 °/min` → `0.324 °/s`.

### 3.3 Gîte et tangage

Ajoutés, avec une dépendance faible mais plausible :

- la gîte suit l'accélération latérale : `heel ≈ atan(v² / (g·R))`, bornée à 30°,
  lissée sur une constante de temps de 2 s ;
- le tangage suit la houle, pas la barre.

Ces deux grandeurs n'existent pas dans le legacy. Elles sont **désactivées par
défaut** afin que la sortie par défaut reste comparable, et activables
explicitement.

## 4. Intégration de la position

Le legacy applique par tick :

```
deltaKm = knotsToMSec(speed) * elapsedSeconds / 1000
position = destinationPoint(position, heading, deltaKm)
```

La cible conserve la **géodésie** (destination point sur sphère, rayon moyen
6371 km) mais généralise :

```ts
function integrate(p: LatLon, headingRad: number, sog: number, dt: number): LatLon {
  const distanceM = sog * (dt / 1000);
  return destinationPoint(p, headingRad, distanceM, EARTH_RADIUS_M);
}
```

Points de vigilance documentés par l'analyse :

- l'unité `we` du legacy est en **kilomètres** malgré son nom ; la conversion
  est vérifiée à 10 kn / 1 s : 5.136 m mesurés contre 5.144 m attendus ;
- `RMC`, `GLL` et `GGA` doivent partager la **même** position, prise dans le
  snapshot, jamais recalculée par phrase ;
- la vitesse de surface et la vitesse fond doivent pouvoir diverger (courant),
  ce que le legacy ne sait pas faire.

## 5. Bruit borné avec retour

Remplacement de `applySeed` :

```ts
class BoundedDrift {
  constructor(
    readonly nominal: number,
    readonly min: number,
    readonly max: number,
    readonly amplitude: number,
    readonly halfLifeMs: number,
    readonly rng: Rng,
  ) {}

  step(current: number, dt: number): number {
    const pull = 1 - Math.pow(0.5, dt / this.halfLifeMs);
    const pulled = current + (this.nominal - current) * pull;
    const noisy = pulled + (this.rng.next() * 2 - 1) * this.amplitude;
    return clamp(noisy, this.min, this.max);
  }
}
```

Comparaison avec le legacy :

| Propriété | Legacy | Cible |
|---|---|---|
| Amplitude | relative (`×(1+p)` ou `÷(1+m)`) | absolue, en unités du champ |
| Espérance | `> 1`, dérive ascendante | nulle |
| Rappel vers le nominal | aucun | exponentiel, `halfLife` configurable |
| Comportement au minimum | oscillation 2 états | borné et lissé |
| Reproductibilité | non (PRNG global) | oui (`Rng` injecté) |

Le choix d'une amplitude **absolue** est important : `±0.05 %` sur un cap de
15° vaut 0.0075°, invisible, alors que `±0.2 kn` sur le vent est signifiant. Le
nouveau modèle déclare des amplitudes dans les unités de la grandeur.

## 6. Vent

```
apparentVector = trueVector - vesselVelocity
apparentSpeed  = |apparentVector|
apparentDir    = bearing(apparentVector)
```

Le vent apparent est une **composition vectorielle** en repère monde, pas une
somme scalaire de vitesses. C'est ce qui produit un vent apparent qui pivote
correctement lorsque le navire change de cap.

S'ajoutent, désactivés par défaut :

- **courant** : vitesse et direction propres, qui font diverger SOG et STW ;
- **rafales** : bruit basse fréquence sur la vitesse du vent ;
- **décalage de direction** : écart entre le vent apparent et le sillage, qui
  n'affecte pas la compatibilité de la sortie par défaut.

## 7. Mer et sondeur

| Grandeur | Unité noyau | Plage par défaut | Source |
|---|---|---|---|
| Température d'eau | K | 273.15 – 298.15 | seed |
| Profondeur sous quille | m | 1 – 100 | seed, ou sondeur configuré |
| Hauteur de houle | m | 0 – 4 | seed, désactivé |
| Période de houle | s | 3 – 12 | seed, désactivé |

La conversion vers Celsius dans le noyau est à éviter : la cible stocke en
Kelvin et convertit à l'encodage, ce qui évite les arrondis successifs observés
sur les températures moteur (seed 85 °C → 358.15 K → reconverti).

## 8. GNSS

- `satsInView` liste de PRN, `satsUsed` sous-ensemble, `satsVisible` nombre.
- `fix` enum : `NO_FIX, GPS, DGPS, RTK_FLOAT, RTK_FIXED, DEAD_RECKONING,
  SIMULATED`. Le legacy ne publie pas le mode dans `RMC` : la cible publie `A`
  par défaut, `D` en `DEAD_RECKONING`, `N` sans fix.
- DOP corrélé : HDOP ≥ PDOP ≥ VDOP n'est pas une contrainte physique, mais
  **VDOP > HDOP** ne l'est pas plus. La cible émet des valeurs cohérentes et
  bornées, au lieu de trois tirages indépendants comme le legacy.
- Les satellites sont **persistants** dans le temps : une liste qui change
  entièrement chaque seconde n'est pas exploitable par un récepteur.

## 9. Propulsion

```ts
interface EngineState {
  running: boolean;
  rpm: number;                 // tr/min
  temperature: number;         // K
  load: number;                // 0..1
  fuelRate?: number;           // L/h, optionnel
}
```

Comportement vérifié sur le legacy (capture `cap-ui2.log`) :

| Événement | Effet observé |
|---|---|
| Moteur arrêté | `RPM,E,1,0,10.5,A` |
| Clic Start | le régime est publié et dérive ensuite par `applySeed` |
| Pas de ramp-up | le régime n'a pas de temps de montée propre |

La cible introduit un **temps de montée** et un **temps de chute**, plus une
charge dérivée de la demande de vitesse. `10.5` en dur devient la valeur par
défaut d'un mode `legacy` (`load` constant), pas la règle.

La ligne `RPM` ne publie le régime que si le moteur tourne, comme le legacy.

## 10. Route et navigation

```ts
interface Route {
  origin: LatLon | null;       // figé à la pose de la route
  destination: LatLon | null;
  waypoints: NamedWaypoint[];
  activeWaypoint: number;
}
```

Grandeurs dérivées, toutes recalculées à chaque pas :

| Grandeur | Symbole NMEA | Unité noyau |
|---|---|---|
| Relèvement départ → destination | `brgFromOrigin` | rad |
| Relèvement courant → destination | `brgFromCurrent` | rad |
| Distance → destination | `dtg` | m |
| Écart de route (cross track) | `xte` | m |
| Vitesse de fermeture | `closingSpeed` | m/s |
| Vitesse pouregarçon destination | `hts` | m/s |

Toutes sont dérivées d'une **source unique** (`RouteGeometry`). Le legacy émet
des coordonnées de destination dans le mauvais hémisphère, incohérentes avec la
distance émise dans la même phrase : voir `04-compatibility-matrix.md` §3.2 (hémisphères toujours `N`/`E`). Dans la
cible, les coordonnées et la distance sont garanties cohérentes par construction,
parce qu'elles sortent du même calcul.

## 11. AIS

| Message | Type | Contenu | Statut |
|---|---|---|---|
| Position propre | 1 | MMSI, lat/lon, cap, SOG, COG, HDOP, ROT | legacy |
| Position propre | 18 | idem en résolution 1/1000 min | ajout cible |
| Destination | 5 | callsign `SIM1234`, nom `NMEASIM`, type 37, ETA | legacy |
| Destination | 5 | 2 fragments, numéro de séquence 9 | legacy |

Phénomènes conservés du legacy, car observables par les récepteurs :

- le navire propre est émis **deux fois** : en `AIVDO` (own vessel) et en
  `AIVDM` (broadcast) ;
- la destination est émise **quatre fois** : `AIVDO` et `AIVDM`, chacune en
  2 fragments ;
- le PDOP émis est une constante `1`, indépendante du HDOP réellement publié.

Décision : conserver la duplication (certains récepteurs ne traitent que
`AIVDM`, d'autres que `AIVDO`), corriger le PDOP, et rendre le type 18
optionnel.

## 12. Bruit et reproductibilité

| Propriété | Valeur |
|---|---|
| Source | PRNG mulberry32 ou xoshiro, **injecté** |
| Graine par défaut | aléatoire, affichée dans l'UI et le journal |
| Graine fixe | tout est reproductible bit à bit |
| Corrélation entre champs | configurable (vent/direction corrélés, DOP independants) |
| Fréquence de mise à jour | alignée sur le pas d'intégration, pas sur la publication |
