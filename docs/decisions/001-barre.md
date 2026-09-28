# D1 — Dynamique de la barre

| | |
|---|---|
| Statut | Acceptée |
| Date | 2026-09-28 |
| Auteur | session P2/P3 (agent), sur arbitrage de l'utilisateur |
| Exigences | SIM-07, UI-05, UI-06 |
| Documents | `06-simulation-model.md` §3, `04-compatibility-matrix.md` §3.15 |

## Contexte

La barre est la commande de navigation la plus sollicitée : clavier, manette,
UI, scripts et API externe la pilotent. Le noyau cible avance à pas fixe
(`05-architecture.md` §3.1) ; la barre doit donc avoir une dynamique définie
par pas de temps, et non par événement d'entrée.

Cette décision fixe aussi le **mécanisme commun des modes de compatibilité**,
repris par D2, D3 et D5.

## Comportement legacy

- `gps.rudderAngle` est un entier dans `[-40, 40]` (`mods_b/7264.js`, seed
  `rudderAngle`).
- Chaque frappe `←`/`→` en mode direction applique ±1 **instantanément**
  (`mods_b/5213.js`, vérifié par `legacy/cap-steer.log`).
- La barre reste où on l'a laissée : aucun retour au centre.
- Le taux de virage dépend de la barre par paliers de rayon (150, 200, 400,
  600 m), vérifié : barre 2 à 6,3 kn → 19,45 °/min (`legacy/cap-turn.log`).
- La bascule du mode direction remet la barre à 0.

## Options

1. **Legacy seul** : barre instantanée, entière.
2. **Dynamique seule** : angle réel qui rejoint une consigne à taux borné.
3. **Dynamique par défaut, legacy derrière un drapeau**, même moteur.

## Décision

Option 3.

- L'état distingue `rudder.command` (consigne, degrés, entier par défaut) et
  `rudder.angle` (angle réel, degrés, réel).
- À chaque pas `dt`, `angle` rejoint `command` à `rudder.rateDegPerS`
  (8 °/s par défaut, configurable), borné à `[-40, 40]`.
- Le taux de virage est calculé sur `angle`, avec la loi de paliers du legacy
  inchangée.
- Mode `legacy` : `angle := command` au pas suivant (taux infini).

Mécanisme commun des modes, valable pour D1, D2, D3 et D5 :

```jsonc
"compatibility": {
  "profile": "modern",              // "modern" | "legacy"
  "overrides": { "rudder": "legacy" } // facultatif, par comportement
}
```

Le profil est résolu **une fois** au démarrage en une structure
`CompatProfile` immuable, dont chaque champ est une petite énumération
(`RudderDynamics::{Rate, Instant}`…). Le moteur est unique : chaque
comportement concerné appelle une stratégie choisie par cette énumération, sans
branche dupliquée du moteur ni second noyau. Les corrections de bogues
(doc 04, statut `FIX`) ne sont **jamais** dans `CompatProfile`.

## Justification

- Une barre qui saute de 40° en un pas est physiquement impossible et produit
  des variations de cap irréalistes pour un pilote automatique en test.
- Le comportement legacy reste utile pour rejouer des scénarios existants et
  comparer aux golden files `cap-turn.log` et `cap-steer.log` : il est donc
  conservé, mais derrière un drapeau.
- Un profil résolu une fois évite la dispersion de `if legacy` dans le code et
  garantit qu'un seul moteur est testé.

## Compatibilité

- Profil `legacy` : trajectoire identique au legacy à précision de l'intégrateur
  près ; les golden files de virage restent comparables.
- Profil `modern` : la sortie diffère transitoirement pendant le mouvement de
  barre (au plus `40 / 8 = 5 s`), puis rejoint la même loi de virage.
- Les commandes de l'API écrivent la **consigne** ; aucune source d'entrée
  n'écrit l'angle réel.

## Conséquences

- Nouveau chemin d'état `vessel.rudder.command` et `vessel.rudder.angle`.
- La manette en mode « axe position » écrit une consigne continue ; en mode
  « bouton » elle incrémente la consigne (`09-inputs-and-ui.md` §3).
- Le paramètre `rudder.rateDegPerS` apparaît dans le schéma déclaratif et la
  config.
- `CompatProfile` devient une dépendance du crate `model`, sans dépendance
  inverse.

## Tests à prévoir

| Test | Attendu |
|---|---|
| `rudder:rate` | consigne 0 → 20 à 8 °/s : angle 8 puis 16 puis 20, pas `dt` = 1 s |
| `rudder:clamp` | consigne 55 → angle borné à 40 |
| `rudder:turn-law` | angle 2 à 6,3 kn → 19,45 °/min ± 0,5 % |
| `legacy-mode:rudder` | profil `legacy` : angle = consigne dès le pas suivant |
| `legacy-mode:rudder-golden` | profil `legacy` : taux de virage moyen de `legacy/cap-turn.log` (≈ 0,30 °/s) reproduit à 5 % près ; la dérive aléatoire du legacy interdit une comparaison pas à pas |
| `compat:resolve` | `profile: modern` + `overrides.rudder: legacy` → seule la barre en legacy |
| `compat:no-bugfix-flag` | test statique : aucun champ de `CompatProfile` ne désigne un défaut `FIX` |
