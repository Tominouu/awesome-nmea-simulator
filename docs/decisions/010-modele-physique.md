# D10 — Modèle physique du navire

| | |
|---|---|
| Statut | Acceptée |
| Date | 2026-09-28 |
| Auteur | session d'implémentation (agent) |
| Exigences | SIM-02, SIM-07, SIM-09, SIM-11 |
| Documents | `06-simulation-model.md`, D1, D2, D5 |

## Contexte

La consigne demande un modèle « simple mais crédible » : accélération et
décélération progressives, inertie, gouvernail, giration, marche avant, neutre
et arrière, vent, courant, et une architecture qui permette d'améliorer la
physique plus tard.

## Comportement legacy

Aucune dynamique : la vitesse est une valeur commandée ; le cap tourne selon
`speed_kn / (R / 1852)` °/min avec R par paliers (600, 400, 200, 150 m) ; pas
de vent ni de courant sur la trajectoire. Défaut : pour toute barre
**négative**, le legacy applique toujours le palier de 600 m
(`rudderAngle >= 20` est faux pour −25).

## Options

1. Modèle de Nomoto ou MMG : hors périmètre (`01-overview.md` §2.2).
2. **Modèle cinématique du premier ordre**, paramétré en unités de marin.

## Décision

Option 2 (module `crates/sim/src/sim.rs`, profil `modern`) :

| Élément | Loi |
|---|---|
| Régime | tend vers `ralenti + |commande| × (max − ralenti)` à `rpmRate` tr/min/s ; 0 moteur arrêté |
| Poussée | par moteur en prise : `signe(commande) × max(r, 0,08)`, `r` régime réduit ; moyenne des deux moteurs |
| Vitesse cible | poussée × `maxSpeedAheadKn` (ou `maxSpeedAsternKn` en arrière) ; 0 au mouillage |
| Vitesse surface | premier ordre vers la cible : `accelTimeS` si elle augmente, `decelTimeS` sinon (inertie) |
| Barre | D1 : consigne atteinte à `rudderRateDegS` |
| Giration | cible `u × κ(δ) × signe(δ)`, κ interpolée linéairement par (0°, 0), (2°, 1/600), (5°, 1/400), (10°, 1/200), (20°, 1/150), (40°, 1/100) ; D2 : décroissance de demi-vie `yawHalfLifeMs` |
| Marche arrière | `u < 0` inverse naturellement le sens de giration |
| Vent | dérive transversale `leewayCoeff × composante travers du vent`, premier ordre 10 s ; vent apparent vectoriel sur la vitesse fond |
| Courant | vitesse fond = vitesse surface (axe + dérive) + courant |
| Mouillage | vitesse fond nulle, position fixe |
| Température moteur | premier ordre vers 80 °C + 10 °C × charge (90 s), refroidit vers l'eau (900 s) |

La loi de courbure passe par le palier legacy de 2° : barre 2 à 6,3 kn donne
19,45 °/min, comme `legacy/cap-turn.log`.

Profil `legacy` : vitesse commandée directement, loi `updateHeading` par paliers
**appliquée à la valeur absolue** de la barre (le défaut de symétrie est corrigé
sans drapeau, `docs/04` §3.16).

## Justification

- Chaque paramètre a un sens pour un marin et se règle dans l'interface.
- L'équilibre vitesse = commande × vitesse maximale rend `nav.speed.set`
  exact et prévisible.
- La structure (régime → poussée → vitesse → giration → vitesse fond →
  intégration) accepte un modèle plus fin étape par étape.

## Compatibilité

Profil `legacy` : trajectoires comparables aux captures (test
`legacy_turn_law_golden`). Profil `modern` : la vitesse n'est plus commandée
directement ; `nav.speed.set` règle la propulsion.

## Conséquences

- La vitesse initiale non nulle démarre les moteurs à la commande
  correspondante (modern), pour que la situation par défaut reste stable.
- En profil modern, `VBW`, `RPM` (pas d'hélice), `ROT`, `HDM` publient des
  valeurs issues du modèle.

## Tests à prévoir

Implémentés dans `crates/sim/tests/simulation.rs` : accélération progressive,
équilibre à 6 kn pour 0,5, inertie au point mort, arrière à −4 kn, vitesse de
barre, décroissance du virage, dérive nulle à barre zéro, pilote sans
dépassement > 10°, suivi de route, courant (COG entre 15° et 30° pour 2 kn de
travers), dérive au vent, mouillage, symétrie de la barre legacy.
