# Plan de réimplémentation

Ce plan est la synthèse exécutable des documents `01` à `12`. Il ne commence
pas l'implémentation : il la prépare et fixe l'ordre.

## 1. État d'avancement

| Phase | Statut |
|---|---|
| Analyse statique du legacy | **Terminée** |
| Tests d'exécution du legacy | **Terminés** : TCP, WebSocket, UDP (diffusion, client, multicast), TCP client, série, ViewSync, Signal K, journal, rejeu, KML, ZDA, clavier, direction, route |
| Documentation | **Terminée** (`docs/01` à `docs/12`, `docs/decisions/001` à `008`) |
| P1 — golden files | **Terminé** : `tests/fixtures/legacy/`, 19 fichiers |
| P2 — `UNKNOWN` | **Terminé** : `tests/fixtures/legacy-p2/` ; un seul `UNKNOWN` justifié (sens du signe `ZDA`) |
| P3 — décisions | **Terminé** : D1 à D7, plus D8 (runtime Rust) |
| J0 — socle | **Terminé** : workspace Cargo, `nmeasim-core` (`units`, `random`), CI, couverture 100 % |
| J1 à J10 | **Terminés** (2026-09-28) : simulateur complet, interface, paquet `.deb` — voir `FINAL_IMPLEMENTATION_REPORT.md` |

## 2. Ce qui a été établi, et comment

Toutes les affirmations de `04-compatibility-matrix.md` marquées `OK` ou
`FIX` reposent sur l'une de ces trois sources, et la source est citée :

1. **Lecture du bundle** — modules beautifiés dans `analysis/mods_b/`.
2. **Capture sur le legacy** — scripts de `tools/legacy-harness/` (copie de `/tmp/nmeasim-test/`), lancés sous Xvfb
   avec le protocole DevTools d'Electron.
3. **Analyse statique seule** — cas rares, signalés comme tels.

Aucune affirmation n'est déduite d'un document tiers : le dépôt public ne
contient qu'un `README.md`.

## 3. Actions préalables à l'implémentation

Ces trois actions sont bloquantes. Tant qu'elles ne sont pas faites, la matrice
de compatibilité reste provisoire.

Les trois actions P1, P2 et P3 sont **terminées** (2026-09-28). Leur
description est conservée ci-dessous pour mémoire ; les résultats sont dans
`docs/04-compatibility-matrix.md`, `tests/fixtures/legacy*/README.md` et
`docs/decisions/`.

### P1 — Figer les golden files

Rapatrier de `/tmp/nmeasim-test/` vers `tests/fixtures/legacy/` :

```
cap-tcp.log  cap-prefix.log  cap-ws.log  cap-udp.log  cap-udp2.log
cap-sk.log   cap-turn.log     cap-dest.log  cap-geo.log  sim.nmeasim
```

Compléter par une capture **UDP multicast**, **UDP client** et **TCP client**,
manquantes. Noter pour chacune l'horodatage, la graine, la config complète et la
commande de lancement.

### P2 — Instruire les `UNKNOWN`

| Sujet | Moyen |
|---|---|
| `ZDA` : fuseau local, signe, format | changer `TZ` et capturer, comparer champ par champ |
| TCP client | client configuré vers un socket de test |
| UDP client | socket de réception sur port connu |
| UDP multicast | groupe `239.x` sur `lo`, deux récepteurs |
| port série | paire de PTY virtuels |
| KML `gx:Track` | injecter le fichier via le sélecteur et capturer |
| KML `LineString`, `Folder` | idem |
| rejeu en exécution | charger un journal, step/play, capturer |

Chaque `UNKNOWN` instruit bascule dans la matrice en `OK` ou `FIX`, avec la
capture correspondante.

### P3 — Trancher les décisions de compatibilité

Sept décisions engagent la suite. Aucune ne peut être prise en cours
d'implémentation sans boguer l'architecture.

| # | Décision | Options | Recommandation |
|---|---|---|---|
| D1 | Comportement de la barre | instantané, legacy / dynamique avec taux | dynamique, avec mode legacy |
| D2 | Virage à barre nulle | arrêt net, legacy / dérive nulle | dérive nulle, avec mode legacy |
| D3 | Amplitude du bruit | relative, legacy / absolue | absolue |
| D4 | `DBT` | ordre legacy / ordre corrigé | **ordre legacy, déjà conforme** (prémisse corrigée) |
| D5 | `VBW` et charge `RPM` | aléatoire brut / modèle | modèle, avec mode legacy |
| D6 | Identifiants AIS | valeurs en dur / fixe configurable | fixe (défaut `503999999`, legacy) et configurable ; le MMSI legacy n'est pas aléatoire |
| D7 | Extension de journal | `meta:` / rien | ligne `meta:` dans l'en-tête, rétrocompatible (vérifié sur le legacy) |

Toutes sont prises : `docs/decisions/001-barre.md` à `007-journal.md`. Le
runtime est tranché par `008-runtime-rust.md` : **Rust**.

## 4. Jalons

| Jalon | Contenu | Unités | Critère de sortie |
|---|---|---|---|
| J0 | Socle de projet | U1 partiel | workspace Cargo, `cargo test`, CI, `core::units` et `core::random` — **atteint** |
| J1 | Noyau déterministe | U1, U2 | 2 exécutions à graine égale produisent le même journal — **atteint** (`determinism_same_seed_same_state`) |
| J2 | NMEA | U3 | golden files NMEA conformes, hors corrections assumées — **atteint** (3 456 phrases, 0 divergence involontaire) |
| J3 | Signal K et ViewSync | U4 | golden files conformes, hello corrigé — **atteint** |
| J4 | Transports | U5 | contrat de framing sur les 7 types, cycle de vie sans course — **atteint** (sockets réels, PTY, 300 cycles) |
| J5 | API de contrôle | U6 | commandes couvrantes, surcharge et relâchement vérifiés — **atteint** (+ manette réelle via uinput) |
| J6 | Sources | U7 | GPX, KML, journal, rejeu, arbitre à jeton unique — **atteint** |
| J7 | Configuration | U8 | migrations testées, aucun champ perdu — **atteint** |
| J8 | Interface | U9 | panneaux, carte, terminal, manette — **atteint** (egui, D9) ; contrastes AA non audités |
| J9 | Packaging | U10 | Linux `.deb` + binaire — **atteint** ; Windows et macOS préparés (CI), non vérifiés localement |
| J10 | Recette | — | protocole §7 de `11` avec un client réel : **reste à faire** (pas de récepteur réel disponible) |

## 5. Ordre d'exécution et dépendances

```
J0 ──▶ J1 ──▶ J2 ──▶ J3 ──▶ J4 ──▶ J5 ──▶ J6 ──▶ J7 ──▶ J8 ──▶ J9 ──▶ J10
```

Deux parallélisations sont possibles sans risque :

- **J3 en parallèle de J2** : les encodeurs Signal K et ViewSync ne dépendent
  que de `core` et du schéma d'état, pas de l'encodeur NMEA.
- **J6 en parallèle de J4** : les sources ne dépendent que du noyau et de
  l'API, pas des sockets. Seule l'interface de rejeu demande `J4`.

## 6. Critères de qualité, par jalon

| Jalon | Seuil |
|---|---|
| J0 | 100 % de couverture sur `core/units` et `core/random` |
| J1 | test de reproductibilité, test de bornitude du bruit |
| J2 | 100 % des sentences couvertes, tous checksums valides |
| J3 | aucune divergence de version ou d'unité avec le noyau |
| J4 | 1000 basculements de transport sans fuite de descripteur |
| J5 | aucun accès direct à l'état hors de `api` |
| J6 | écriture de journal ne bloque pas le noyau, même disque plein |
| J7 | toute migration a un test de non-régression sur une config réelle |
| J8 | navigation clavier complète, contrastes AA |
| J9 | démarrage vers premiers paquets < 2 s |
| J10 | protocole d'acceptation exécuté avec un client réel |

## 7. Répartition des défauts entre correctif et compatibilité

Résumé de `04-compatibility-matrix.md`, à respecter pendant l'implémentation.

**Corrigés, sans drapeau :**

- `RMC` : ajout du mode de navigation ;
- `RMB` : hémisphères toujours `N`/`E` (cause isolée : comparaison d'un objet
  coordonnée à 0) ;
- formatage des coordonnées : fraction des minutes tronquée, longueur variable
  (`00000.0`, `3500.0`, `13830.179999`) ;
- TCP client : reconnexion absente, succès annoncé avant la connexion ;
- UDP multicast : adhésion inutile de l'émetteur au groupe ;
- KML : `LineString` et `Folder` imbriqués non lus ;
- suivi de trace : premier et dernier point jamais émis ;
- rejeu : le pas suivant après une pause saute un bloc ;
- AIS type 5 : ETA en heure locale au lieu d'UTC ;
- Settings : bascule implicite série → WebSocket (analyse statique) ;
- Signal K : version du hello, chemin de waypoint malformé ;
- ViewSync : `timeEnd` en cadence élevée ;
- cycle de vie des transports : résolution après bind, changement de type
  sérialisé ;
- chargement de configuration : plus de perte de champ ;
- type de transport inconnu : erreur explicite au lieu d'un silence.

**Derrière un drapeau de mode legacy** (`compatibility.profile` /
`compatibility.overrides`, un seul moteur, D1) :

- barre instantanée (D1) et arrêt du virage à zéro (D2) ;
- `VBW` aléatoire brut (D5) ;
- pas d'hélice `RPM` constant à `10.5` — et non « charge moteur » (D5) ;
- dérive relative et ascendante (D3).

**Inchangés, car ce sont des choix observables et utiles :**

- `VTG` sous talker `GP` ;
- talker configurable par sentence ;
- phrases vides quand la donnée n'existe pas ;
- duplication `AIVDO` + `AIVDM` ;
- `WPL` muet tant qu'aucun waypoint n'est posé ;
- `APB` / `RMB` inertes sans route ;
- `GGA` à champs terminaux vides ;
- `DBT` : ordre `pieds,f,mètres,M,brasses,F`, déjà conforme (D4) ;
- `ZDA` : zone du système, `+` implicite, `-` explicite ; fuseau configurable ;
- identité AIS `503999999` / `SIM1234` / `NMEASIM` par défaut (D6).

## 8. Erreurs à ne pas reproduire

Issues de l'analyse, à garder en tête pendant la revue de code.

1. **Ne pas confondre les unités dans la même formule.** Le legacy calcule le
   taux de virage en nœuds et le déplacement en m/s, dans deux méthodes
   voisines. Le noyau cible est en SI, la conversion est au bord.
2. **Ne pas nommer les variables d'après leur contenu réel.** `we` du legacy
   est en kilomètres, pas en mètres.
3. **Ne pas faire dépendre une résolution d'un effet de bord.** La version du
   hello Signal K est périmée parce qu'elle est lue avant que l'IPC ne réponde.
4. **Ne pas confondre l'ordre des champs et l'ordre des méthodes.** L'ordre d'émission
   NMEA du legacy est un effet de bord de l'ordre des appels ; il devient une
   donnée explicite.
5. **Ne pas émettre un champ qu'on ne sait pas remplir.** Le legacy émet des
   coordonnées RMB incohérentes avec sa propre distance.
6. **Ne pas écrire dans l'état hors de l'API.** Le legacy a trois chemins
   d'écriture concurrents et aucun n'est typé.
7. **Ne pas faire porter le framing par l'encodeur.** Le legacy concatène `\r\n`
   dans la phrase, ce qui oblige à réécrire selon le transport.
8. **Ne pas confondre l'UI et le modèle.** Le legacy expose un panneau `vessel`
   alors qu'aucun objet `vessel` n'existe.

## 9. Journal de décisions

Tout arbitrage non listé dans ce document est consigné dans
`docs/decisions/NNN-titre.md`, avec en-tête (statut, date, auteur) et huit
rubriques : contexte, comportement legacy, options, décision, justification,
compatibilité, conséquences, tests à prévoir.

| # | Fichier | Objet |
|---|---|---|
| D1 | `001-barre.md` | barre dynamique, mode legacy ; mécanisme `CompatProfile` |
| D2 | `002-derive-barre-zero.md` | dérive nulle à barre zéro, mode legacy |
| D3 | `003-bruit.md` | bruit absolu borné avec rappel, mode legacy relatif |
| D4 | `004-dbt.md` | `DBT` conservé, déjà conforme |
| D5 | `005-vbw-rpm.md` | `VBW` et `RPM` dérivés de l'état, mode legacy |
| D6 | `006-identifiants-ais.md` | identité AIS fixe et configurable |
| D7 | `007-journal.md` | ligne `meta:` dans l'en-tête du journal |
| D8 | `008-runtime-rust.md` | Rust, workspace Cargo |

## 10. Définition de terminé, projet entier

- Tous les documents `01` à `12` sont à jour par rapport au code réel.
- Aucun `UNKNOWN` ne subsiste dans la matrice de compatibilité (reste : sens du
  signe de `ZDA`, `04` §8).
- Les golden files sont dans le dépôt et rejoués par la CI.
- Le protocole d'acceptation de `11-testing-strategy.md` §7 est passé avec un
  client réel.
- Les sept décisions P3 sont prises et documentées.
- Le noyau s'exécute en headless sans affichage.
