# D2 — Virage à barre nulle

| | |
|---|---|
| Statut | Acceptée |
| Date | 2026-09-28 |
| Auteur | session P2/P3 (agent), sur arbitrage de l'utilisateur |
| Exigences | SIM-02, SIM-07 |
| Documents | `06-simulation-model.md` §3.1-3.2, `04-compatibility-matrix.md` §3.15, D1 |

## Contexte

Quand la barre revient à zéro, un navire réel ne cesse pas de tourner
instantanément : le taux de giration décroît. Avec D1, l'angle de barre passe
par des valeurs intermédiaires ; il faut définir ce qui se passe quand il
atteint zéro, et ce que produit une barre maintenue à zéro.

## Comportement legacy

`updateHeading()` commence par `if (rudderAngle === 0) return;`
(`mods_b/7264.js`, cité dans `02-legacy-analysis.md` §3.4) :

- barre à zéro : **aucune** variation de cap due à la barre ;
- un virage engagé s'arrête **net** au pas où la barre repasse à 0 ;
- le cap peut encore varier par la dérive aléatoire de `applySeed` si
  `mask.heading` est actif, indépendamment de la barre.

## Options

1. **Arrêt net** (legacy) : taux de virage nul dès que la barre est à 0.
2. **Dérive nulle avec décroissance** : le taux de virage suit l'angle réel de
   barre (D1) et, à angle nul, le taux résiduel décroît exponentiellement vers
   zéro ; aucune dérive de cap n'est produite par la barre à zéro établie.
3. **Modèle hydrodynamique** (Nomoto) : hors périmètre, `01-overview.md` §2.2.

## Décision

Option 2 par défaut ; option 1 derrière le drapeau `compatibility`
(`overrides.zeroRudder: "legacy"`, ou profil `legacy`).

- Taux cible `r* = f(angle, SOG)` avec la loi de paliers legacy, signe de
  l'angle.
- Taux effectif : `r ← r + (r* − r)·(1 − 2^(−dt / yawHalfLifeMs))`,
  `yawHalfLifeMs` = 1500 ms par défaut.
- À barre nulle **établie**, `r* = 0` et `r` tend vers 0 : la dérive de cap
  imputable à la barre est **nulle** ; aucun biais, aucune dérive lente.
- Mode legacy : `r = r*` à chaque pas, donc arrêt net.

## Justification

- Le « taux qui s'éteint » est le comportement maritime attendu et évite une
  discontinuité de cap que les filtres des pilotes automatiques interprètent
  comme un défaut capteur.
- La garantie « dérive nulle à barre zéro » est le point contractuel : un banc
  qui fixe la barre à zéro doit obtenir un cap stable, aux seuls bruits de D3
  près, et ces bruits sont à moyenne nulle.
- L'arrêt net reste utile pour comparer aux captures legacy.

## Compatibilité

- Profil `legacy` : identique au legacy (combiné avec D1 legacy).
- Profil `modern` : après un retour de barre à zéro, le cap continue d'évoluer
  pendant quelques secondes, puis se stabilise ; l'écart final vaut
  environ `r₀ · yawHalfLife / ln 2`.
- D1 et D2 sont indépendants : chaque combinaison est valide et testée.

## Conséquences

- Nouvelle grandeur d'état `vessel.rateOfTurn` (rad/s), publiée aussi en `ROT`
  AIS et, plus tard, dans une éventuelle phrase `ROT`.
- Le mode direction du legacy (« remet la barre à 0 ») remet la **consigne** à
  0 ; en profil `modern`, le cap continue brièvement de tourner.

## Tests à prévoir

| Test | Attendu |
|---|---|
| `zero-rudder:no-drift` | barre 0 pendant 10 000 pas, bruit désactivé : cap strictement constant |
| `zero-rudder:decay` | barre 20 → 0 : `r` divisé par 2 toutes les 1,5 s, ± 1 % |
| `zero-rudder:sign` | aucun changement de signe de `r` pendant la décroissance |
| `legacy-mode:zero-rudder` | profil `legacy` : `r = 0` au premier pas à barre nulle |
| `compat:d1-d2-matrix` | les 4 combinaisons D1 × D2 produisent un cap fini et borné |
