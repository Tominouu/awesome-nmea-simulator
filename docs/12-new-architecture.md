# 12 — Architecture nouvelle : découpage implémentable

Ce document fixe la forme du dépôt cible et le découpage en unités de travail.
Il complète `05-architecture.md`, qui décrit les couches.

## 1. Choix de runtime

Tranché par `docs/decisions/008-runtime-rust.md` : **Rust stable**, workspace
Cargo. La proposition antérieure (Node LTS) est abandonnée.

| Critère | Legacy | Cible | Raison |
|---|---|---|---|
| Runtime | Electron 24 (Chromium 112, obsolète) | binaire Rust natif | sécurité, démarrage, empreinte |
| UI | Angular + Material | web légère dans Tauri (à confirmer en J8) | voir ci-dessous |
| Noyau | TypeScript compilé en bundle, non accessible | crates `nmeasim-*` sans dépendance UI | testabilité, headless |
| Persistance | `localStorage` | fichier + fichier par défaut | robustesse, partage entre comptes |
| Dépendances natives | Electron, OpenLayers | aucune hors sockets et série | portabilité, NFR-05 |

Le noyau ne dépend **jamais** d'un crate d'UI, de rendu, de manette ou de
réseau. La contrainte est portée par le graphe de dépendances Cargo et vérifiée
en CI (`cargo tree` sur `nmeasim-core`).

### 1.1 UI

Choix recommandé : **web stack légère** (Lit, Preact ou Svelte) dans Tauri,
avec la carte en WebGL. Le choix du framework est tranché avant J8.

Critères de choix :

- pas de framework dont le cycle de vie précède celui du noyau ;
- rendu possible en headless pour les tests de contrat ;
- bundle de distribution de moins de 40 Mo ;
- empaquetage Linux, Windows et macOS sans compilateur natif côté utilisateur.

## 2. Arborescence cible

Ordre de construction imposé : `core` → modèle de domaine → simulation →
sorties → transports → UI.

```
nmeasim-rs/
  Cargo.toml             workspace (profil release : LTO « thin », binaire allégé)
  crates/
    core/                units, random, geo (sans dépendance)
    sim/                 domaine + simulation : état, commandes, profils (D1), bruit (D3), physique (D10), pilote, route
    encode/              NMEA 0183, AIS, préfixe, Signal K, ViewSync, analyseur NMEA
    transport/           tcp-server, tcp-client, udp-broadcast/client/multicast, websocket-server, serial
    source/              GPX, KML (modes legacy/modern), lecteur de trace, journal (D7), rejeu
    input/               manette et clavier → commandes (fonctions pures)
    app/                 configuration + migration legacy, moteur d'exécution, API HTTP, scénarios, pilote gilrs
    gui/                 binaire `nmeasim` : interface egui, carte, mode --headless (D9)
  examples/              scénarios et trace d'exemple
  packaging/             .desktop, icône, scripts Windows et macOS
  tests/
    fixtures/legacy/     golden files P1 (lecture seule)
    fixtures/legacy-p2/  captures P2 (lecture seule)
  docs/
  tools/legacy-harness/  harnais de capture du legacy (Node, CDP)
  tools/gamepad/         manette virtuelle uinput
```

Le découpage réel regroupe « modèle de domaine » et « simulation » dans
`sim`, et les trois encodeurs dans `encode` (décision D9).



## 3. Unités de travail

Chaque unité est indépendamment fusionnable et testable.

### U1 — `core`

`clock`, `scheduler` à pas fixe, `state` et `Snapshot`, `events`, `units`,
`random` (PRNG injecté), `logging`.

Sortie : un noyau qui avance un état déterministe, testé sans socket.

### U2 — `model`

`vessel` (intégration), `rudder` (dynamique et taux de virage), `wind`,
`sea`, `gnss`, `propulsion`, `route`, `drift` (bruit borné).

Dépend de `core` uniquement. Tests : les quatre lignes de la section 4.5 de
`11-testing-strategy.md`.

### U3 — `encode-nmea`

`sentence` (checksum, talker, champs), les formatters, `ais`, `prefix`.
L'ordre d'émission est une donnée, pas un effet de bord.

Dépend de `core` pour l'état, et **rien** d'autre.

### U4 — `encode-signalk` et `encode-viewsync`

Deltas, hello, chemins, unités, paquet CSV. Aucun accès réseau.

### U5 — `transport`

`factory` (nom ou valeur numérique), `base` (cycle de vie correct), `tcp`,
`udp`, `websocket`, `serial`, `multiplex` avec file bornée et métriques.

Tests : contrat de framing, cycle de vie, isolation des pannes.

### U6 — `api`

`ControlApi`, chemins pointés, commandes, surcharge `override` / `release`,
événements typés (`09-inputs-and-ui.md` §7). Le crate `input` (manette,
clavier, profils) s'y branche sans toucher l'état.
La surface HTTP s'y greffe, sans modifier le noyau.

### U7 — `source`

`gpx`, `kml`, `track-player` (cadence fixe et temps réel), `journal`
(écriture, rotation, `meta:`), `replay` (chaîne et instantané), `arbitre` à
jeton unique.

### U8 — `config` et migrations

Schéma versionné, validation, migrations pures `old -> new` testées
individuellement, export et import.

### U9 — `ui`

Panneaux pilotés par le schéma, carte, terminal, barre d'état des transports,
réglages, aide. Raccourcis et manette branchés sur l'API, jamais sur l'état.

### U10 — `app` et packaging

Assemblage, IPC, dialogue natif, menus, installation, raccourcis d'application.

## 4. Séquencement

```
U1 ──▶ U2 ──▶ U3 ──▶ U5 ──▶ U6 ──▶ U9
           └──▶ U4          ▲       │
U8 ─────────────────────────┘       │
U7 ────────────────────────────────▶│
U10 ───────────────────────────────▶
```

U1, U2, U3, U4, U8 ne dépendent d'aucune IHM et peuvent être fusionnés et
testés en premier. U5 peut être fusionné dès que U3 produit des messages. U9 et
U10 arrivent en dernier, quand la surface est stable.

## 5. Découpage des « grands » composants

Trois éléments méritent un découpage interne explicite.

### 5.1 Un chemin pointé, un seul écrivain

`set(path, value)` est l'unique primitive d'écriture de l'état. Clavier,
manette, sliders, API, import GPX et rejeu passent tous par elle. Un audit statique
peut vérifier qu'aucun module n'écrit directement dans l'objet d'état.

### 5.2 Une horloge, deux usages

`Clock` a deux méthodes parce que deux besoins coexistent :

- `now()` pour l'horodatage des messages, qui doit refléter l'heure murale ;
- `monotonic()` pour l'intégration, qui ne doit pas sauter si l'horloge est
  ajustée par NTP.

Le legacy utilise partout `Date.now()`, y compris pour l'intervalle de tick.

### 5.3 Un seul producteur de messages

`MessageBus` a exactement un producteur actif à la fois : simulation live,
suivi de trace, ou rejeu. L'arbitre est un jeton unique, libéré seulement par le
producteur qui le détient.

## 6. Risques d'implémentation

| Risque | Impact | Parade |
|---|---|---|
| Reproduire les défauts au lieu de les corriger | compatibilité fausse | revue de la matrice avant chaque encodeur |
| Dérive temporelle entre noyau et publication | trajectoires non reproductibles | accumulateur à pas fixe, test de reproductibilité |
| Boucle de réenregistrement en rejeu | flood réseau | jeton unique + suffixe de source |
| Fuite de descripteurs au changement de transport | crash après plusieurs basculements | test de cycle de vie sur 1000 basculements |
| UI qui écrit hors de l'API | divergence affichage / sortie | audit statique des imports et des écritures |
| Format subtilement différent sur un seul champ | récepteurs réjects | golden files champ par champ |
| Trop de compatibilité | complexité ingérable | chaque drapeau doit avoir un test et une raison |
| Dépendance au runtime d'origine | blocage de portage | noyau sans dépendance UI, vérifié par test |

## 7. Définition de « terminé »

Une unité est terminée quand :

1. ses tests unitaires passent ;
2. son test de contrat passe contre les golden files ;
3. elle s'exécute en headless sans navigateur ni affichage ;
4. elle n'introduit aucune dépendance vers une couche supérieure ;
5. les types publics sont documentés ;
6. le journal de décisions contient les arbitrages pris.
