# Rapport final d'implémentation — nmeasim-rs 0.1.0

Session du 2026-09-28. Jalons J1 à J9 enchaînés depuis le socle J0. J10
(recette avec un récepteur réel) reste à faire, faute de matériel.

**Résultat** : un exécutable `nmeasim` (interface graphique ou `--headless`)
et un paquet `target/debian/nmeasim-rs_0.1.0-1_amd64.deb` (5,3 Mo), construits,
lancés et vérifiés sur le réseau.

| Indicateur | Valeur |
|---|---|
| Code Rust | 16 279 lignes, 7 crates + le binaire |
| Tests | **125 passés, 0 échec, 0 ignoré** (22 binaires, Linux) |
| Clippy `-D warnings`, rustfmt | propres |
| Compatibilité legacy (golden files) | 3 456 phrases comparées, **0 divergence involontaire** |
| Couverture `nmeasim-core` | 100 % des lignes (`units`, `random`, `geo`) |
| Lancement → premiers octets NMEA | 0,99 s (seuil J9 : 2 s) |
| 1 000 cycles de transport avec client réel | aucune fuite de descripteur |

## Fonctionnalités implémentées

| Domaine | Contenu | Où |
|---|---|---|
| Noyau | unités SI, PRNG graineable (xoshiro256**), géodésie sphérique | `crates/core` |
| Simulation | état SI, commandes typées, profils `modern` / `legacy`, bruit borné (D3), physique (D10), pilote automatique (cap, route), route et géométrie, GNSS, overrides, déterminisme bit à bit | `crates/sim` |
| NMEA 0183 | RMC, VHW, VTG, HDT, HDM, ROT, GLL, GGA, GSA, GSV, ZDA, VBW, MWD, MWV, MTW, DPT, DBT, WPL, RPM, APB, RMB, préfixe 61162-450, talkers et liste configurables | `crates/encode/src/nmea.rs` |
| AIS | `!AIVDO` / `!AIVDM` types 1 et 5, identité configurable (D6) | `crates/encode/src/ais.rs` |
| Signal K | hello (version corrigée), delta par tick, chemins de navigation, environnement, propulsion, destination | `crates/encode/src/signalk.rs` |
| ViewSync | paquet CSV, `timeEnd` corrigé | `crates/encode/src/viewsync.rs` |
| Transports | TCP serveur (multi-clients), TCP client (reconnexion), UDP diffusion, UDP client, UDP multicast, WebSocket, série | `crates/transport` |
| Sources | GPX (traces, segments, routes, waypoints), KML (`gx:Track`, `LineString`, dossiers ; mode legacy), lecteur de trace (cadence fixe, temps réel), journal `.nmeasim` avec `meta:` (D7), rejeu | `crates/source` |
| API de contrôle | 35 commandes (dont 2 réservées aux sources), refus typés, événements typés, jeton de producteur | `crates/sim/src/command.rs`, `crates/app/src/runtime.rs` |
| API HTTP | `/api/v1/commands`, `state`, `status`, `transports`, `paths`, `monitor`, `replay` ; jeton facultatif | `crates/app/src/http.rs` |
| Configuration | schéma 2 persistant, migration de la config 1.6.1, clés inconnues conservées, erreurs nommées | `crates/app/src/config.rs`, `legacy.rs` |
| Scénarios | capture de l'état courant, ouverture, enregistrement, lancement | `crates/app/src/scenario.rs`, `examples/scenarios/` |
| Manette | gilrs, détection à chaud, multi-manettes, calibration, zone morte, inversion, sensibilité, courbe, modes position/taux, répétition, mapping éditable, profils JSON | `crates/input`, `crates/app/src/gamepad.rs` |
| Clavier | flèches, pages, raccourcis, profil legacy | `crates/input/src/keyboard.rs` |
| Interface | fenêtre egui : carte, instruments, bandeau propulsion/barre, onglets NMEA, NETWORK, GAMEPAD, GPX/KML/REPLAY, LOG, SCENARIO, SETTINGS | `crates/gui` |
| Carte | Web Mercator, carroyage, échelle, sillage, route, destination, trace, cap, vecteur COG, fond OpenStreetMap facultatif | `crates/gui/src/map.rs`, `tiles.rs` |
| Moniteur NMEA | pause, recherche, filtres phrase et talker, compteur, horodatage, checksum, fréquence, copie, terminal d'injection | `crates/gui/src/panels.rs` |
| Journal | écriture non bloquante (file bornée), disque plein sans blocage, lisible par 1.6.1 | `crates/source/src/journal.rs` |
| Packaging | `.deb`, entrée de bureau, icône ; scripts Windows (`.zip`) et macOS (`.app`, `.dmg`) ; CI multi-OS | `packaging/`, `.github/workflows/ci.yml` |

## Compatibilité legacy

Test **« NMEASimulator legacy VS nouveau simulateur »** :
`crates/encode/tests/golden.rs`. Chaque bloc des 11 captures NMEA de
`tests/fixtures/legacy/` est reconstruit, réencodé en profil legacy et comparé
champ par champ.

| Classe | Phrases |
|---|---|
| identiques octet pour octet | 2 746 |
| dans la précision de reconstruction | 410 |
| 1. bogue corrigé volontairement : mode RMC | 144 |
| 1. bogue corrigé volontairement : format des coordonnées | 21 |
| 2. amélioration volontaire ou aléa legacy : VBW | 144 |
| **3. divergence involontaire** | **0** |

Également vérifiés contre le legacy :

- Signal K : deltas identiques valeur pour valeur, y compris le format des
  nombres entiers ;
- ViewSync : paquets identiques ;
- AIS type 5 : charge utile legacy identique octet pour octet ;
- ordre des 24 phrases du profil legacy réellement émis sur TCP ;
- hémisphère RMB, recoupé par la distance et le relèvement ;
- KML legacy, sur les fichiers de P2 ;
- vitesse de suivi `gx:Track` (17,7 nœuds) ;
- rejeu de `sim.nmeasim` (1 801 blocs) ;
- migration des configurations réellement injectées dans le legacy (`legacy-p2/cfg-*.json`) ;
- journal `meta:` relu par un chargeur legacy fidèle.

Les **golden files sont intacts** : 19/19 pour P1 et 18/18 pour P2, par
SHA-256, contrôlés en CI.

Le profil `legacy` (`--legacy` ou SETTINGS) reproduit les comportements
observables nécessaires aux captures :

- vitesse commandée directement ;
- barre instantanée et entière ;
- virage coupé net à barre nulle ;
- bruit `applySeed` sur le PRNG injecté ;
- VBW brut ;
- pas d'hélice `10.5` ;
- 24 phrases dans l'ordre de 1.6.1 ;
- satellites fixes ;
- charge AIS legacy ;
- KML limité.

## Corrections par rapport au legacy

Appliquées sans drapeau, dans les deux profils :

| Défaut 1.6.1 | Correction | Preuve |
|---|---|---|
| RMB : hémisphères toujours `N`/`E` | hémisphère réel | `route_golden_hemisphere_fix` |
| coordonnées : fraction tronquée, longueur variable | 6 décimales, arrondi | `golden.rs`, `sentence::coordinates` |
| RMC sans mode | mode `A` / `N` | `golden.rs` |
| APB/RMB : XTE signé, sens de correction inversé | valeur absolue, sens correct, arrivée et perpendiculaire | `docs/04` §3.17 |
| barre négative : rayon de 600 m quel que soit l'angle | paliers sur la valeur absolue | `legacy_negative_rudder_uses_same_law` |
| hello Signal K `17.12.05` | version de l'application | `signalk_websocket_hello_and_deltas` |
| ViewSync `timeEnd` en cadence élevée | `timeStart + intervalle` | `viewsync.rs` |
| TCP serveur « démarré » avant le bind | `running` seulement après bind | `tcp_server_bind_failure_is_reported` |
| TCP client : pas de reconnexion, succès annoncé sur `ECONNREFUSED` | reconnexion 0,5 à 10 s, état `reconnecting` | `tcp_client_reconnects_and_reports_state` |
| multicast : adhésion inutile de l'émetteur | aucune adhésion, TTL et interface réglables | `udp_multicast_two_receivers` |
| suivi de trace : premier et dernier point jamais émis | tous les points | `fixed_rate_emits_every_point_including_ends`, `track_follow_and_end` |
| KML : `LineString`, dossiers imbriqués ignorés | lus (mode modern) | `kml_modern_mode_reads_everything` |
| rejeu : le pas après une pause saute un bloc | bloc affiché émis | `replay_semantics_and_step_fix` |
| AIS type 5 : dernier groupe de bits mal complété | bourrage correct (modern) | `message5_matches_golden_except_legacy_padding` |
| config : type en chaîne ignoré en silence, remplacement intégral | erreur nommée, fusion sans perte | `legacy_config_migration_and_persistence` |
| Settings : bascule implicite série → WebSocket | aucune réécriture implicite | `docs/04` §1 |

Ces classements sont documentés dans `docs/04-compatibility-matrix.md`
(§3.1 à 3.20 et §7).

## Fonctionnalités nouvelles

- physique : inertie, accélération et décélération, marche arrière, dérive au
  vent, courant, mouillage, régime et température moteur (D10) ;
- pilote automatique en tenue de cap et en suivi de route, avec passage
  automatique au point suivant ;
- phrases HDM, ROT, GSV ; satellites persistants ; DOP cohérents ;
- plusieurs transports simultanés, chacun avec son format NMEA ou Signal K ;
- API HTTP ;
- scénarios reproductibles (graine) ;
- manette complète ;
- fond OpenStreetMap ;
- moniteur avec terminal ;
- mode headless (`--print`, `--duration`, `--api`) ;
- migration `--import-legacy`.

## Gamepad

- Pilote gilrs dans un thread dédié, puis traduction pure `RawEvent` →
  commandes (`InputMapper`), soumises à la même API que le clavier et
  l'interface.
- Mapping par défaut demandé :

  | Entrée | Action |
  |---|---|
  | stick gauche Y | propulsion |
  | stick droit X | barre |
  | LT | marche arrière |
  | RT | marche avant |
  | A | moteurs |
  | B | pilote automatique |
  | X | reset |
  | Y | mouillage |

- **Vérification sans matériel réel** : `tools/gamepad/virtual_pad.py` crée une
  manette Xbox 360 virtuelle au niveau du noyau (uinput). La chaîne complète
  evdev → udev → gilrs → mapping → API a été vérifiée : détection à chaud,
  A → moteurs, RT → propulsion +1,00, stick gauche → propulsion,
  stick droit → barre +40°, B → pilote, Y → mouillage, débranchement.
- Deux défauts trouvés et corrigés pendant cette vérification :
  - gilrs n'émet pas `ButtonChanged` à l'appui d'une gâchette : la valeur
    analogique est maintenant relue dans l'état ;
  - un test instable révélait qu'une trame du client TCP pouvait être jetée
    juste après la connexion.
- L'onglet GAMEPAD a été vérifié à l'écran (capture `docs/img/gamepad.png`) :
  valeurs brutes et normalisées, boutons, mode test, calibration, mapping,
  import et export.
- **Aucune manette physique n'était branchée** : la vérification sur matériel
  réel reste à faire.

## Réseau

Vérifié sur sockets réels (tests) et avec le binaire empaqueté :

| Transport | Vérifié |
|---|---|
| TCP serveur | plusieurs clients, CRLF, hello Signal K, client lent déconnecté, erreur de bind, lignes reçues |
| TCP client | reconnexion après absence de cible et après fermeture du serveur |
| UDP client | un datagramme par phrase |
| UDP diffusion | sur l'interface de boucle locale |
| UDP multicast | deux récepteurs, groupe invalide refusé |
| WebSocket | hello et messages |
| Série | paire de PTY (`openpty`), émission et réception |
| ViewSync | test de paquet |
| API HTTP | jeton, 200 / 400 / 401 / 404 / 422 |

Binaire `.deb` extrait et lancé : 22 types de phrases sur TCP, checksums
valides, commande acceptée par l'API.

## NMEA

- Checksums : vérifiés sur chaque phrase émise dans les tests de bout en bout.
- Format numérique : réimplémentation de `toFixed` de JavaScript, pour que les
  arrondis soient identiques octet pour octet (vérifié contre Node).
- DBT : conservé tel quel, il est conforme (D4).
- ZDA : fuseau configurable, système par défaut.

## Signal K

- Forme du legacy : `values` en tableau `{path, value}`.
- Unités SI, nombres entiers sans décimale.
- Chemins ajoutés en profil modern : `headingMagnetic`, `rateOfTurn`,
  `steering.rudderAngle`, `environment.current`, `navigation.anchor.state`,
  `engineLoad`, `navigation.destination.*`, `courseGreatCircle.*`.

## UI

Fenêtre vérifiée sous Xvfb par captures d'écran (`docs/img/`) :

- carte avec fond OSM ;
- instruments ;
- bandeau propulsion/barre ;
- moniteur NMEA avec checksums et fréquences ;
- onglet réseau ;
- onglet manette avec une manette virtuelle.

L'interface n'écrit jamais dans l'état : toutes ses actions passent par
`Runtime::submit`. Les symboles absents des polices d'egui sont fournis par
DejaVu.

## Tests

| Suite | Nombre |
|---|---|
| core (unités, PRNG, géodésie) | 40 |
| sim (unités + 17 scénarios de simulation) | 25 |
| encode (unités + 4 compatibilité legacy) | 15 |
| transport (2 unités + 10 sockets réels + 1 fuite de descripteurs) | 13 |
| source (unités + 8 sur fixtures réelles) | 14 |
| input (manette 4, clavier 1) | 5 |
| app (bout en bout, 11) | 11 |
| binaire (ligne de commande, projection) | 2 |

Commandes : `cargo test --workspace`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`.

## Packaging

- **Linux** : `target/release/nmeasim` (binaire autonome) et
  `target/debian/nmeasim-rs_0.1.0-1_amd64.deb` (SHA-256
  `286ea184…4cc270`) :
  - dépendances : GTK 3, udev, glib, `fonts-dejavu-core` ;
  - contenu : `/usr/bin/nmeasim`, entrée de bureau, icône, README ;
  - vérification : extrait avec `dpkg -x` et lancé. **Il n'a pas été installé
    sur le système.**
- Construit sur Ubuntu 26.04, le paquet exige glibc ≥ 2.43. Le job CI
  `package-linux` (ubuntu-latest) produit un `.deb` compatible avec des
  distributions plus anciennes.
- **Windows** (`make-zip.ps1`) et **macOS** (`make-app.sh`, `.app` + `.dmg`) :
  scripts et jobs CI écrits, **non exécutés ici** (pas d'environnement Windows
  ni macOS).

## Limitations restantes

- **J10** : protocole d'acceptation avec un HELM ou un ECDIS réel non exécuté.
- **Manette** : vérifiée avec une manette virtuelle uinput, pas avec du
  matériel physique.
- **Windows et macOS** : compilation, tests et paquets non vérifiés
  localement. Les tests multicast et diffusion sur boucle locale sont ignorés
  hors Linux.
- **ZDA** : sens du signe de la zone `UNKNOWN` (norme IEC 61162-1 non
  consultée).
- **Non implémentés** :
  - WebSocket TLS ;
  - option `allowRemote` : le serveur TCP écoute sur `0.0.0.0` par défaut,
    comme le legacy ;
  - flux d'événements WebSocket de l'API (interrogation `status` / `monitor`
    à la place) ;
  - rotation et gzip des journaux ;
  - rejeu en mode « instantané » : il se fait en mode chaîne, et le navire
    suit la phrase RMC ;
  - AIS type 18 ; GBS, VDR, RLM ;
  - gîte et tangage ;
  - export GPX depuis l'interface (la fonction existe dans `source::gpx`) ;
  - interprétation des phrases reçues comme commandes : elles ne sont
    qu'affichées.
- **Accessibilité** : contrastes AA et navigation clavier complète non audités.
- **Réglages** : l'activation de l'API HTTP et de la manette s'applique au
  prochain lancement.
- **Fond OSM** : réseau requis, désactivé par défaut, soumis à la politique
  d'usage des tuiles OpenStreetMap.
- **Environnement** :
  - `cargo-deb` (et `cargo-llvm-cov`) installés globalement dans `~/.cargo` ;
  - le projet n'est toujours pas un dépôt git, rien n'est commité ;
  - lors d'un premier essai de capture d'écran, la fenêtre de l'application
    s'est ouverte quelques minutes sur le bureau Wayland réel avant d'être
    arrêtée (corrigé en forçant X11 pour Xvfb).

## Comment lancer l'application

```bash
# Depuis les sources
cargo build --release
./target/release/nmeasim                 # interface graphique
./target/release/nmeasim --start --legacy

# Paquet Debian
sudo apt install ./target/debian/nmeasim-rs_0.1.0-1_amd64.deb
nmeasim

# Sans interface
nmeasim --headless --start --print --duration 60
nmeasim --headless --scenario examples/scenarios/port-approach.json --api 8375

# Brancher un client : TCP 127.0.0.1:10110 (NMEA 0183)
```

Documentation :

- `README.md` ;
- guides utilisateur : `docs/user-guide.md`, `docs/gamepad.md`,
  `docs/network.md`, `docs/configuration.md`, `docs/scenarios.md` ;
- développement : `docs/development.md` ;
- conception : `docs/01` à `docs/12` et `docs/decisions/001` à `010`.
