# Golden files du legacy NMEASimulator 1.6.1

Ce répertoire est **figé**. Il contient les sorties réelles du binaire legacy
`nmeasimulator_1.6.1_amd64.deb`, telles qu'observées sur la machine de
référence. Ces fichiers servent de référence de compatibilité pour la
réimplémentation : le noyau cible doit reproduire ce comportement, à
l'exception des corrections listées dans `docs/04-compatibility-matrix.md`.

## Règles

1. **Aucun octet ne doit être modifié**, pas même un retour à la ligne.
2. **Aucun timestamp ne doit être réécrit.** Les `mtime` sont ceux de la capture
   et font partie de la traçabilité.
3. **Aucun fichier ne doit être supprimé ni renommé.** Une observation fausse
   se corrige par un fichier supplémentaire et une note, jamais par une
   réécriture.
4. Les sommes SHA-256 sont dans `SHA256SUMS`. La CI doit les vérifier avant tout
   test de golden file.
5. Toute nouvelle capture est ajoutée avec son scénario documenté ici, jamais
   en remplacement d'une capture existante.

## Environnement de référence

| Élément | Valeur |
|---|---|
| Système | Ubuntu 26.04.1 LTS |
| Noyau | Linux 7.0.0-34-generic x86_64 |
| Legacy | `nmeasimulator` 1.6.1, amd64, Electron |
| Node dans le renderer | v22.23.2 |
| Outils de test | Node v22.23.2, Python 3.14.4 |
| Fuseau au moment des captures | CEST, `TZ=UTC+02:00` |
| Affichage | Xvfb `:99`, 1600x1000x24 |
| Binaire | `analysis/deb/opt/NMEASimulator/nmeasimulator` |
| Profil isolé | `/tmp/nmeasim-test/profile` |
| CDP | `--remote-debugging-port=9222` |

## Graine de simulation commune

Toutes les captures utilisent la graine de base produite par
`/tmp/nmeasim-test/mkcfg.js`, identique d'une capture à l'autre, sauf
`cap-fixed.log` (voir §3.1) :

| Objet | `value` | `plus` / `minus` | `min` / `max` |
|---|---|---|---|
| `gps.speed` | 5 kn | 0.01 | 0 / 70 |
| `gps.heading` | 15° | 0.05 | 0 / 359 |
| `gps.rudderAngle` | 0° | 0 | -40 / 40 |
| `gps.hdop/pdop/vdop` | 1.5 | 0.07 | 0.5 / 5 |
| `gps.position` | -35.0, 138.5 | — | — |
| `gps.altitude` | 2 m | 0 | 0 / 3000 |
| `water.depth` | 8 m | 0.05 | 1 / 100 |
| `water.temperature` | 7.5 °C | 0.01 | 0 / 25 |
| `wind.direction` | 285° | 0.05 | 0 / 359 |
| `wind.speed` | 11 kn | 0.2 | 2 / 40 |
| `p0` / `p1` rpm | 600 | 0.02 | 600 / 6000 |
| `p0` / `p1` temperature | 85 °C | 0.01 | 85 / 115 |
| `wpt.position` | 0, 0 | — | — |

`interval = 1000 ms`, `outputFormat = NMEA` sauf `cap-sk.log`, `autoStart`
activé au moment de la capture.

## Protocole de capture

Toutes les captures TCP, UDP, multicast, WebSocket et ViewSync suivent la même
séquence, implémentée par `/tmp/nmeasim-test/caprun.sh` :

1. arrêt du legacy, relance isolée (`launch.sh`) ;
2. injection de la configuration par
   `localStorage.setItem('nmeasim_config', <cfg>)` via CDP ;
3. attente 9 s du flush du LevelDB Chromium ;
4. nouvelle relance, l'application relit `nmeasim_config` au démarrage et
   applique le type de transport via `changeType()` ;
5. démarrage du récepteur ou du serveur de test ;
6. clic souris réel sur le bouton de barre d'outils dont l'infobulle vaut
   `Start Simulator` (coordonnées 1176, 24 en 1200x725) ;
7. capture pendant 20 à 22 s ;
8. relevé de l'état de la barre d'outils et du journal applicatif.

Le point 6 est important : les boutons dont le libellé textuel vaut `Start`
appartiennent à d'autres panneaux et **ne démarrent pas le simulateur**. Seul le
bouton d'icône de la barre d'outils, infobulle `Start Simulator`, appelle
`SimulatorService.start()`.

## 3. Inventaire des captures

### 3.1 `cap-fixed.log` — valeurs figées

| | |
|---|---|
| mtime | 2026-09-28 10:29:11 +0200 |
| Transport | TCP serveur, `127.0.0.1:10110` |
| Config | `cfg-fixed.json`, `sendNmeaPrefix = false` |
| Graine | **particulière** : `plus = minus = 0` partout, `speed = 10` kn, `heading = 0`, `position = 0, 0` |
| Scénario | Toutes les grandeurs sont immobiles ; la position de départ est l'intersection des méridiens. |
| Établit | Le rendu des valeurs figées, dont la **coordonnée longitude nulle mal formée** : `$GPRMC,...,0000.030518,N,00000.0,E,...`. Le champ 6 devrait valoir `00000.000000`. |
| Observation | `WIMWD` à `0.0`, `GPGSA` sans PDOP/HDOP/VDOP mobiles, `IIRPM` charge `10.5` en marche. |

La cause est identifiée dans le bundle : le calcul des secondes décimales fait
`minutes_decimal.toString().split(".")`, donc lorsque la partie fractionnaire
des minutes est nulle — cas de la coordonnée exactement zéro — le champ
secondes est réduit à un seul `0` au lieu de six chiffres.

### 3.2 `cap-tcp.log` — TCP serveur, NMEA

| | |
|---|---|
| mtime | 2026-09-28 10:25:22 +0200 |
| Transport | TCP serveur, `127.0.0.1:10110`, type `1` |
| Config | `cfg-tcp.json` |
| Graine | commune |
| Scénario | Moteur démarré, capteur passif, 9 cycles. |
| Établit | 24 trames par cycle, framing `\r\n` en fin de phrase, checksums valides, `SDDBT` avec les champs pieds et brasses inversés. |

### 3.3 `cap-prefix.log` — préfixe NMEA 61162-450

| | |
|---|---|
| mtime | 2026-09-28 10:30:58 +0200 |
| Transport | TCP serveur, `127.0.0.1:10110` |
| Config | `cfg-prefix.json`, `sendNmeaPrefix = true` |
| Scénario | Chaque phrase est préfixée par `\c:1790584254,s:nmeasim*49\`, soit un préfixe tau, un `s:` et un nom source. |
| Établit | Le préfixe est calculé **une seule fois** au démarrage et reste identique sur toute la capture : il n'est pas rafraîchi. |

### 3.4 `cap-ws.log` — WebSocket serveur

| | |
|---|---|
| mtime | 2026-09-28 10:31:34 +0200 |
| Transport | WebSocket serveur, `127.0.0.1:10111`, type `0` |
| Config | `cfg-ws.json` |
| Scénario | Connexion unique, 4 cycles. |
| Établit | Chaque phrase est un message WebSocket distinct ; pas d'agrégation. |

### 3.5 `cap-udp.log` — ViewSync

| | |
|---|---|
| mtime | 2026-09-28 10:26:59 +0200 |
| Transport | UDP, `127.0.0.1:7001` |
| Config | `cfg-sk.json`, `outputFormat = SIGNALK`, ViewSync actif |
| Scénario | Client UDP passif branché sur le port ViewSync. |
| Établit | Charge utile CSV `compteur,lat,lon,altitude,tilt,azimuth,range,cible_x,cible_y` terminée par `,`. Le compteur **débute à 15**, et non à 1, car le ViewSync a été lancé avant l'ouverture du récepteur : le compteur est global au service, pas relatif à la connexion. |

### 3.6 `cap-udp2.log` — UDP diffusion

| | |
|---|---|
| mtime | 2026-09-28 10:32:09 +0200 |
| Transport | UDP diffusion, type `4` |
| Config | `cfg-udp.json` |
| Scénario | L'adresse de diffusion est dérivée du CIDR de l'interface `wlp0s20f3` ; la source est `192.168.11.130`. |
| Établit | En mode `udp`, la destination est l'adresse de diffusion **calculée depuis l'interface**, et non `ip.address` de la configuration. `ip.address` est ignoré. |

### 3.7 `cap-sk.log` — Signal K

| | |
|---|---|
| mtime | 2026-09-28 10:26:59 +0200 |
| Transport | TCP serveur, `127.0.0.1:10110` |
| Config | `cfg-sk.json`, `outputFormat = SIGNALK` |
| Scénario | Moteur démarré, client passif. |
| Établit | Le hello porte `"version":"17.12.05"`, périmée, lue avant que l'IPC ne réponde. Le `self` vaut `vessels.urn:mrn:signalk:uuid:...`, donc le préfixe `context` et l'identifiant de navire sont collés sans séparateur. |

### 3.8 `cap-turn.log` — loi de virage

| | |
|---|---|
| mtime | 2026-09-28 10:38:09 +0200 |
| Transport | TCP serveur, `127.0.0.1:10110` |
| Config | `cfg-ui.json` |
| Scénario | Deux touches `ArrowRight` réelles envoyées par CDP, cadence 1 s. |
| Établit | Deux impulsions donnent une barre à `2`, et le taux de virage vaut `vitesse_nœuds × 1852 / rayon_en_mètres`. |

### 3.9 `cap-steer.log` — mode direction

| | |
|---|---|
| mtime | 2026-09-28 10:36:59 +0200 |
| Transport | TCP serveur, `127.0.0.1:10110` |
| Scénario | Bascule en mode direction : `gps.mask.heading = true`, barre remise à zéro, cap et vitesse masqués. |
| Établit | Le mode direction pilote le cap, pas la barre directement. |

### 3.10 `cap-dest.log` — pose de destination

| | |
|---|---|
| mtime | 2026-09-28 10:42:07 +0200 |
| Transport | TCP serveur, `127.0.0.1:10110` |
| Scénario | Un **clic droit** sur la carte pose la destination. Un clic gauche ne la pose pas. |
| Établit | `setDestination()` fige `ap.value.origin` à l'instant du clic. Le clic droit utilise la dernière position connue du pointeur. |

### 3.11 `cap-geo.log` — géométrie APB / RMB

| | |
|---|---|
| mtime | 2026-09-28 10:43:41 +0200 |
| Transport | TCP serveur, `127.0.0.1:10110` |
| Scénario | Route posée, `APB` et `RMB` passent en mode `A`. |
| Établit | Les coordonnées `RMB` restent **incohérentes** avec le relèvement et la distance annoncés. Cause racine non isolée. |

### 3.12 `cap-ui2.log` — panneau et commanditaire

| | |
|---|---|
| mtime | 2026-09-28 10:35:03 +0200 |
| Transport | TCP serveur, `127.0.0.1:10110` |
| Scénario | Capture taken pendant l'interaction avec les panneaux. |
| Établit | La référence du panneau `vessel` n'a aucun objet `vessel` correspondant dans le modèle. |

### 3.13 `cap-udpclient.log` — UDP client, type `5`

| | |
|---|---|
| mtime | 2026-09-28 11:14:05 +0200 |
| Transport | UDP client, `127.0.0.1:15000`, type `5` |
| Config | `cfg-udpclient.json` |
| Récepteur | `node cap-udpclient.js 15000 …` |
| Scénario | 17 cycles, 22 s. |
| Résultat | **408 datagrammes**, 16 747 octets, 24 datagrammes par cycle. |
| Établit | Un datagramme par phrase, `\r\n` inclus, cadence 1 s. Le socket est lié sur un port **éphémère** et n'appelle pas `setBroadcast`. La destination est bien `ip.address:ip.port`, contrairement au mode `udp`. |
| Contenu | 18 phrases `$` et 6 phrases `!` par cycle. Les phrases AIS utilisent `!AIVDO` / `!AIVDM` en trois morceaux, dupliqués. |

### 3.14 `cap-tcpclient.log` — TCP client, type `2`

| | |
|---|---|
| mtime | 2026-09-28 11:15:17 +0200 |
| Transport | TCP client, `127.0.0.1:15001`, type `2` |
| Config | `cfg-tcpclient.json` |
| Serveur | `node srv-tcp.js 15001 …` |
| Résultat | **1 connexion**, 408 phrases, 16 761 octes, 50 segments. |
| Établit | Le transport TCP client fonctionne et tous les checksums sont valides. Les segments TCP **ne sont pas alignés sur les phrases** : un segment peut contenir quatre phrases ou plus. Le récepteur doit donc traiter un flux d'octets délimité par `\r\n`, jamais un message par phrase. |

### 3.15 `cap-tcpclient-recon.log` — pas de reconnexion

| | |
|---|---|
| mtime | 2026-09-28 11:16:18 +0200 |
| Transport | TCP client, `127.0.0.1:15001` |
| Scénario | Le serveur détruit la connexion à 8 s et **continue d'écouter**. |
| Résultat | **1 seule connexion sur toute la fenêtre.** 27 trames reçues avant fermeture, aucune tentative ensuite. |
| Établit | `startTCPClient()` n'a **aucune logique de reconnexion**. Une fois la socket détruite, plus rien n'est émis. La barre d'outils affiche toujours `Stop Simulator` : l'application se croit en marche. |

### 3.16 `console-notarget.log` — cible TCP absente

| | |
|---|---|
| mtime | 2026-09-28 11:21 |
| Scénario | Démarrage du simulateur alors que rien n'écoute sur `127.0.0.1:15001`. |
| Établit | Le processus **ne plante pas**. L'application affiche `Stop Simulator`, donc elle annonce un démarrage réussi. La seule trace est `console.warning error: Error: connect ECONNREFUSED 127.0.0.1:15001`, invisible sans outils de développement. Aucune nouvelle tentative. |
| Statut | Défaut. `messageSource.next({action:"started"})` est émis **avant** l'ouverture de la connexion, et l'échec n'est ni propagé à l'interface ni retrié. |

### 3.17 `cap-mcast.log` — UDP multicast, type `6`

| | |
|---|---|
| mtime | 2026-09-28 11:22:45 +0200 |
| Transport | UDP multicast, groupe `239.255.42.99`, port `15002`, type `6` |
| Config | `cfg-mcast.json` |
| Récepteurs | `node cap-mcast.js 239.255.42.99 15002 …` — **deux** sockets joints sur `lo` |
| Prérequis | `ip route add 239.0.0.0/8 dev lo` |
| Résultat | R1 : 192 datagrammes, 8 cycles. R2 : 456 datagrammes, 19 cycles. |
| Établit | Les deux récepteurs joints reçoivent **exactement le même** flux, 192 datagrammes de part et d'autre. À 11 s, R1 se retire ; R2 reçoit encore 264 datagrammes, soit 11 cycles supplémentaires. **Le multicast n'a pas de contrôle de flux** : le départ d'un récepteur n'affecte pas les autres. |
| Console | `console-cap-mcast.log` contient `UDP (multicast) (239.255.42.99)`. |
| Quirk | L'émetteur joint lui-même le groupe sur son port **éphémère**, sans interface, ce qui est inutile : seul le routage compte pour l'émission. La source observée est l'interface physique par défaut, pas `lo` ; les copies de bouclage atteignent néanmoins les membres de `lo`. |

### 3.18 `sim.nmeasim` — journal de rejeu

| | |
|---|---|
| mtime | 2026-09-28 11:04:24 +0200 |
| Format | En-tête `nmeasim,1.6.1` puis blocs séparés par `~`, phrases terminées par `\r\n` |
| Contenu | 32 418 phrases, 1,9 Mo, 17 cycles de base réinjectés par les sessions d'interaction. |
| Établit | Le format lu par le sélecteur de rejeu, l'ordre d'émission des phrases et la séparation des blocs. |

## 4. Faits transversaux établis par ces captures

| Fait | Preuve |
|---|---|
| 24 trames par cycle de 1 s | 17 × 24 = 408 sur `cap-udpclient.log` ; 8 × 24 = 192 et 19 × 24 = 456 sur `cap-mcast.log` |
| 18 phrases `$` et 6 phrases `!` par cycle | décompte `!AIVDO` / `!AIVDM` sur `cap-mcast.log` |
| Charge moteur codée en dur à `10.5` | `$IIRPM,E,1,0,10.5,A*7C` dans toutes les captures |
| Champ DBT inversé | `$SDDBT,27.6,f,8.4,M,4.6,F` : 27.6 pieds = 8.4 m, et 4.6 brasses = 27.6 pieds. Le champ 1 porte des pieds et le champ 4 des brasses. |
| Zone locale dans `ZDA` | `$GPZDA,091406.235,28,09,2026,02,00` sous `TZ=UTC+02:00` |
| `RMB` et `APB` inertes sans route | `$GPRMB,V,,R,,,,,,,,,,,N*00` et `$IIAPB,V,V,,R,N,,,,T,,,T,,T,N*79` |
| `WPL` muet | aucun `$GPWPL` dans les captures sans waypoint |

## 5. Limites connues de ce jeu de données

- `sim.nmeasim` a été **enrichi** au fil des sessions d'interaction ; il ne
  correspond pas à un scénario unique et propre. Il sert de format, pas de
  référence de contenu.
- `cap-steer.log`, `cap-ui2.log` et `cap-fixed.log` ne figuraient pas dans la
  liste initiale du plan. Ils sont conservés car ils portent des observations
  que rien d'autre n'établit.
- Aucun fichier n'a été nettoyé, normalisé ni ré-encodé. Les fins de ligne
  `CRLF` et les décimales à six chiffres sont conservées telles quelles, y
  compris quand elles sont mal formées.
- Les heures locales `HH:MM:SS` contenues dans les phrases correspondent à
  l'heure de la machine au moment de la capture, pas à une donnée du scénario.
  Seuls les champs structurés sont comparables entre captures.
