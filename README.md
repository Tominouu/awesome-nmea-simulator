# nmeasim-rs — NMEA Simulator

Simulateur de navire qui émet **NMEA 0183, AIS, Signal K et ViewSync** sur
**TCP, UDP (diffusion, client, multicast), WebSocket et port série**, pilotable
au **clavier**, à la **manette** et par **API HTTP**.

C'est une réimplémentation moderne de NMEASimulator 1.6.1 (Electron), en Rust :
compatible avec ses sorties réseau, ses configurations et ses journaux
`.nmeasim`, avec un vrai modèle de navire (inertie, barre, giration, marche
arrière, vent, courant) et une interface graphique native.

![Interface](docs/img/main.png)

## Installation

### Linux (Debian, Ubuntu)

```bash
sudo apt install ./nmeasim-rs_<version>_amd64.deb
nmeasim            # ou « NMEA Simulator » dans le menu des applications
```

Le paquet installe `/usr/bin/nmeasim`, une entrée de bureau et une icône. Il
dépend des bibliothèques graphiques usuelles (GTK 3, udev) et de
`fonts-dejavu-core`.

### Binaire seul

`target/release/nmeasim` est un exécutable autonome : aucune installation de
Rust, de Node ou d'Electron n'est nécessaire pour le lancer.

### Windows, macOS

Les scripts `packaging/windows/make-zip.ps1` et `packaging/macos/make-app.sh`
construisent une archive portable et une application `.app` / `.dmg`. La CI
produit ces artefacts ; voir [docs/development.md](docs/development.md).

## Lancement

```bash
nmeasim                              # interface graphique
nmeasim --start                      # démarre la simulation au lancement
nmeasim --scenario mon-scenario.json # charge un scénario
nmeasim --legacy                     # profil de compatibilité NMEASimulator 1.6.1
nmeasim --headless --start --print   # sans interface, phrases sur la sortie standard
nmeasim --headless --api             # sans interface, API HTTP sur 127.0.0.1:8375
nmeasim --import-legacy nmeasim_config.json   # migre une config 1.6.1
nmeasim --help
```

Par défaut, un serveur **TCP NMEA sur le port 10110** s'ouvre au démarrage de
la simulation : un traceur, OpenCPN ou un ECDIS s'y connecte directement.

## Configuration

Fichier JSON versionné, `~/.config/nmeasim-rs/config.json`, édité depuis
l'onglet **SETTINGS** ou à la main. Une configuration NMEASimulator 1.6.1 est
migrée automatiquement ; aucune clé n'est perdue et toute valeur invalide
produit une erreur qui nomme le champ. Détails :
[docs/configuration.md](docs/configuration.md).

## Manette

Une manette USB ou Bluetooth est détectée à chaud. Mapping par défaut :

| Entrée | Action |
|---|---|
| Stick gauche Y | propulsion (avant / arrière) |
| Stick droit X | barre |
| LT / RT | marche arrière / avant (gâchettes analogiques) |
| A | moteurs marche / arrêt |
| B | pilote automatique (tenue du cap courant) |
| X | reset |
| Y | mouillage |
| Croix | barre ±1°, propulsion ±10 % |
| LB / RB | cap du pilote ±10° |
| Start | pause |

Zone morte, calibration, inversion, sensibilité, courbe et mapping se règlent
dans l'onglet **GAMEPAD**, qui affiche les valeurs brutes et normalisées de
chaque axe. Voir [docs/gamepad.md](docs/gamepad.md).

## Clavier

`↑`/`↓` propulsion, `←`/`→` barre (Maj : ×5), `Pg↑`/`Pg↓` cap du pilote,
`Début` barre au centre, `0` point mort, `Espace` pause, `A` pilote, `E`
moteurs, `N` mouillage, `M` marquer la position, `W` point suivant. En profil
legacy, `↑`/`↓` règlent la vitesse de ±1 nœud comme dans 1.6.1.

## NMEA 0183

Par tick (1 s par défaut) : `RMC VHW VTG HDT HDM ROT GLL GGA GSA GSV ZDA VBW`,
AIS `!AIVDO`/`!AIVDM` types 1 et 5, `MWD MWV MTW DPT DBT`, `WPL` si une
position est marquée, `RPM` ×2, `APB RMB`. Le profil legacy émet exactement les
24 phrases de 1.6.1, dans le même ordre. Talkers, liste de phrases et préfixe
IEC 61162-450 sont configurables ; l'onglet **NMEA** montre le flux en direct
(recherche, filtres, fréquence, checksum, copie) et permet d'injecter une phrase.

## Signal K

Hello à la connexion (version de l'application) puis un delta par tick, sur TCP
ou WebSocket : navigation, position, vitesses, cap, GNSS, vent, eau, profondeur,
propulsion, destination. Choisir le format **Signal K** pour un transport dans
l'onglet **NETWORK**. Voir [docs/network.md](docs/network.md).

## Réseau

| Type | Usage |
|---|---|
| `tcp-server` | traceurs et ECDIS clients (port 10110 par défaut) |
| `tcp-client` | pousser vers un serveur, reconnexion automatique |
| `udp-broadcast` | diffusion sur l'interface choisie |
| `udp-client` | unicast vers un hôte |
| `udp-multicast` | groupe, interface et TTL configurables |
| `websocket-server` | navigateurs, Signal K |
| `serial` | port série, débit, parité, bits |

Plusieurs transports tournent en même temps, chacun avec son format ;
**ViewSync** (Google Earth) a son propre émetteur UDP.

## GPX / KML

Onglet **GPX / KML / REPLAY** : une trace GPX (`trk`, `rte`) ou KML
(`gx:Track`, `LineString`, dossiers imbriqués) pilote le navire, un point par
tick ou en temps réel. « Charger comme route » transforme les points en route
pour le pilote automatique.

## Rejeu

Ouvrir un journal `.nmeasim` (legacy ou nouveau) : lecture, pause, pas à pas,
vitesse ×0,1 à ×20, boucle. Le rejeu est l'unique producteur pendant la
lecture et positionne le navire sur la carte. L'enregistrement (onglet **LOG**)
écrit des journaux lisibles par NMEASimulator 1.6.1.

## Carte

Clic droit : destination (APB/RMB). Maj + clic droit : point de route.
Ctrl + clic : déplacer le navire. Molette : zoom. Glisser : déplacer la vue.
Double-clic : suivre le navire. Fond OpenStreetMap facultatif (réseau requis).

## Documentation

- [Guide utilisateur](docs/user-guide.md)
- [Manette](docs/gamepad.md)
- [Réseau](docs/network.md)
- [Configuration](docs/configuration.md)
- [Scénarios](docs/scenarios.md)
- [Développement](docs/development.md)
- Conception : `docs/01` à `docs/12`, décisions `docs/decisions/`,
  compatibilité `docs/04-compatibility-matrix.md`.

## Licence

MIT ou Apache-2.0, au choix.
