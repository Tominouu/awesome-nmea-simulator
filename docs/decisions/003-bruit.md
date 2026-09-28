# D3 — Modèle de bruit des grandeurs dérivantes

| | |
|---|---|
| Statut | Acceptée |
| Date | 2026-09-28 |
| Auteur | session P2/P3 (agent), sur arbitrage de l'utilisateur |
| Exigences | SIM-05, SIM-06, NFR-08 |
| Documents | `06-simulation-model.md` §5 et §12, `04-compatibility-matrix.md` §3.14 |

## Contexte

Les grandeurs « masquées » (vitesse, cap, DOP, vent, profondeur, température,
régime, température moteur) varient d'un tick à l'autre pour que la sortie ait
l'air vivante. Ce bruit doit être reproductible (graine), borné et sans biais,
sinon les tests à long terme deviennent impossibles.

## Comportement legacy

`applySeed` (`02-legacy-analysis.md` §3.3) :

```js
const up = Math.floor(10 * Math.random()) % 2 === 0;
value = value === 0 ? (up ? value + plus : value - minus)
                    : (up ? value * (1 + plus) : value / (1 + minus));
value = clamp(value, min, max);
```

- amplitude **relative** : `±0,05` sur un cap de 15° vaut ±0,75° mais ±15° sur
  un cap de 300° ;
- espérance du multiplicateur `((1+p) + 1/(1+m))/2 > 1` : **dérive ascendante**
  sans rappel ;
- au minimum, oscillation à deux états, observée `600 ↔ 612` rpm
  (`legacy/cap-ui2.log`) ;
- `Math.random()` global : non reproductible.

## Options

1. **Relatif legacy** conservé tel quel.
2. **Absolu borné avec rappel** : bruit additif dans l'unité de la grandeur,
   rappel exponentiel vers la valeur nominale, bornes dures.
3. **Processus d'Ornstein-Uhlenbeck exact** : équivalent à 2 en continu, plus
   coûteux à expliquer et à paramétrer.

## Décision

Option 2 par défaut ; option 1 derrière le drapeau (`overrides.noise:
"legacy"` ou profil `legacy`).

```
pull  = 1 − 2^(−dt / halfLifeMs)
x    ← x + (nominal − x)·pull + U(−1, 1)·amplitude·√(dt / 1000)
x    ← clamp(x, min, max)
```

- `amplitude` est exprimée dans l'unité de la grandeur (kn, °, m…), par seconde
  de racine.
- L'aléa vient du `Rng` injecté de `core/random` ; une graine fixe rend la
  séquence identique bit à bit.
- Le cap est traité sur le cercle : l'écart au nominal est l'écart angulaire
  signé le plus court, puis ramené dans `[0, 360)`.
- Mode legacy : la formule `applySeed` exacte, **mais** alimentée par le même
  `Rng` injecté, ce qui la rend reproductible sans changer sa loi.

## Justification

- Un bruit sans biais et borné est la condition des tests de longue durée
  (NFR-03, 24 h) : avec le legacy, les grandeurs finissent collées à leur borne
  haute.
- L'amplitude absolue a un sens physique stable quel que soit le point de
  fonctionnement.
- Le facteur `√dt` rend le bruit indépendant du pas d'intégration choisi
  (SIM-04).
- Garder la loi legacy derrière un drapeau permet de comparer statistiquement
  aux captures existantes ; la brancher sur le `Rng` injecté ne coûte rien.

## Compatibilité

- Les phrases restent au même format ; seules les valeurs diffèrent.
- La config legacy `seed.<obj>.<champ>.{plus, minus}` est migrée : en profil
  `legacy`, elle est lue telle quelle ; en profil `modern`, `amplitude` vaut par
  défaut `plus × nominal` (conversion relative → absolue au point nominal) et
  `halfLifeMs` vaut 60 000 ms.
- Aucune golden file ne peut être comparée champ à champ sur une grandeur
  bruitée : les tests de contrat désactivent le bruit ou comparent des
  statistiques.

## Conséquences

- Module `model/drift` unique ; `core/random` doit offrir `next_f64()` sur
  `[0, 1)` et `range(a, b)`, livrés en J0.
- Le schéma déclaratif de champ (`09-inputs-and-ui.md` §5.1) porte
  `drift.amplitude` et `drift.halfLifeMs`.
- La graine utilisée est écrite dans l'en-tête `meta:` du journal (D7).

## Tests à prévoir

| Test | Attendu |
|---|---|
| `noise:bounded` | 10⁶ pas : toujours dans `[min, max]` |
| `noise:zero-mean` | 10⁶ pas, nominal loin des bornes : moyenne = nominal à ± 1 % de `max − min` |
| `noise:no-upward-drift` | départ au nominal, 10⁶ pas : pas de tendance (pente de régression ≈ 0) |
| `noise:reproducible` | même graine → même séquence bit à bit |
| `noise:dt-invariance` | écart-type stationnaire identique à ± 5 % pour `dt` = 10 ms et 100 ms |
| `noise:heading-wrap` | nominal 359°, aucun saut de 360° dans l'écart |
| `legacy-mode:noise` | profil `legacy` : formule `applySeed` exacte sur une séquence `Rng` imposée |
| `legacy-mode:noise-floor` | profil `legacy` : oscillation à deux états au minimum (`600 ↔ 612`) reproduite |
