# 04 — Matrice de compatibilité et défauts legacy

Légende des statuts :

- **OK** — comportement vérifié, à reproduire.
- **FIX** — comportement vérifié défectueux, corrigé **sans drapeau**.
- **FLAG** — corrigé par défaut, comportement legacy reproductible via le mode
  `legacy`.
- **UNKNOWN** — non vérifié ; chaque `UNKNOWN` restant porte sa justification.

Sources des preuves :

- `legacy/<fichier>` : golden files P1, `tests/fixtures/legacy/` (manifeste
  `README.md`) ;
- `p2/<fichier>` : captures P2, `tests/fixtures/legacy-p2/` (manifeste
  `README.md`) ;
- `mods_b/<module>:<ligne>` : bundle beautifié, `analysis/mods_b/`.

Les décisions de compatibilité sont détaillées dans `docs/decisions/001` à
`007`.

## 1. Configuration

| Élément | Legacy | Cible | Statut | Preuve |
|---|---|---|---|---|
| Clé de stockage | `nmeasim_config` | migration acceptée, forme canonique nouvelle | FLAG | `mods_b/3352.js` |
| `server.type` numérique | `0..6` | nom canonique + migration des valeurs numériques | FLAG | `mods_b/933.js:32` |
| `server.type` en chaîne | ignoré silencieusement | erreur explicite | FIX | `mods_b/933.js:53-62` |
| `server.ip` défaut | `127.0.0.1:3100` | identique | OK | `mods_b/1326.js` |
| Type par défaut | `4` (udp) | identique | OK | `mods_b/1326.js` |
| Chargement | remplacement intégral puis migrations | fusion sans perte | FIX | `mods_b/1326.js:154` |
| `panelOrder` | 7 panneaux | identique | OK | `mods_b/1326.js` |
| Version | `version: ""` puis écrite | version applicative unique | FIX | `mods_b/1326.js` |
| Page Settings sans port série détecté | bascule `server.type` série → WebSocket si `manualSerialPort` est vrai | aucune réécriture implicite du type | FIX (analyse statique seule) | `mods_b/7238.js:395` |

La dernière ligne n'a pas pu être reproduite : l'hôte expose 32 `/dev/ttyS*`,
la liste de ports n'est donc jamais vide. Le code est sans ambiguïté :
`refreshSerialPorts()` exécute `server.type = websocket` quand aucun port
n'est listé, que le port manuel est renseigné et `manualSerialPort` vrai.

## 2. Inventaire NMEA par tick

24 phrases à l'intervalle par défaut, dans cet ordre (`legacy/cap-tcp.log`) :
18 phrases `$` et 6 phrases `!`.

| # | Sentence | Talker défaut | Contenu | Statut | Décision |
|---|---|---|---|---|---|
| 1 | `RMC` | GP | position, SOG, cap, date, heure | FIX (mode absent) | — |
| 2 | `VHW` | II | cap relatif, cap vrai, SOG, STW | OK | — |
| 3 | `VTG` | **GP** | cap, SOG, STW | OK (≠ help) | — |
| 4 | `HDT` | II | cap vrai | OK | — |
| 5 | `GLL` | GP | position, heure, statut | OK, formatage FIX | §3.3 |
| 6 | `GGA` | GP | fix, satellites, DOP, altitude | OK, formatage FIX | §3.3 |
| 7 | `GSA` | GP | mode, satellites visibles, DOP | OK | — |
| 8 | `ZDA` | GP | heure et date UTC, zone du système | OK (forme), voir §3.4 | — |
| 9 | `VBW` | II | vitesses d'eau doubles | FLAG | D5 |
| 10 | `AIVDO` type 1 | AI | position propre | OK | D6 |
| 11 | `AIVDM` type 1 | AI | position propre | OK | D6 |
| 12 | `MWD` | WI | direction et vitesse du vent | OK | — |
| 13 | `MWV` | WI | vent relatif/app | OK | — |
| 14 | `MTW` | II | température d'eau | OK | — |
| 15 | `DPT` | SD | profondeur | OK | — |
| 16 | `DBT` | SD | profondeur pieds / mètres / brasses | **OK** (conforme) | D4 |
| 17 | `AIVDO` type 5 frag 1 | AI | destination | OK | D6 |
| 18 | `AIVDO` type 5 frag 2 | AI | destination | OK | D6 |
| 19 | `AIVDM` type 5 frag 1 | AI | destination | OK | D6 |
| 20 | `AIVDM` type 5 frag 2 | AI | destination | OK | D6 |
| 21 | `RPM` p0 | II | régime ligne 1 | FLAG (charge) | D5 |
| 22 | `RPM` p1 | II | régime ligne 2 | FLAG (charge) | D5 |
| 23 | `APB` | II | route APB | OK | — |
| 24 | `RMB` | GP | route RMB | FIX (hémisphères) | §3.2 |

### 2.1 Phrases absentes par défaut

| Sentence | Talker | Raison | Statut |
|---|---|---|---|
| `WPL` | IN | objet `wpt` muet par défaut | OK |
| `GBS` | GP | non implémentée | FLAG |
| `VDR` | GP | non implémentée | FLAG |
| `RLM` | GP | non implémentée | FLAG |

## 3. Défauts et comportements legacy

### 3.1 DBT — **conforme**, l'inversion documentée auparavant était fausse

Observation (`legacy/cap-tcp.log`, `legacy/cap-fixed.log`) :

```
$SDDBT,26.2,f,8.0,M,4.4,F*38      profondeur 8.0 m
$SDDBT,33.5,f,10.2,M,5.6,F*03     profondeur 10.2 m
```

Code (`mods_b/7264.js:467`) : champ 1 = `3.28084 × depth` suivi de `f`,
champ 3 = `depth` suivi de `M`, champ 5 = `0.546807 × depth` suivi de `F`.
C'est exactement l'ordre NMEA 0183 `pieds,f,mètres,M,brasses,F`, et les
valeurs sont justes (8,0 m = 26,2 ft = 4,4 fm).

Les versions précédentes de ce document, du plan et de la passation
affirmaient une inversion de champs, avec un exemple
`$SDDBT,0008.0,f,0013.1,F,0008.0,M` qui ne figure dans **aucune** capture.
Cette affirmation est retirée.
**Statut : OK. Décision D4 : conserver l'ordre et les valeurs, verrouiller par
un test de non-régression. Aucun drapeau.**

### 3.2 RMB — hémisphères toujours `N` et `E` : cause racine isolée

Observation (`legacy/cap-dest.log`) :

```
$GPRMB,A,-0.001684,L,origin,dest,3631.460336,N,13621.430572,E,139.574,228.1,-1.7,,A
```

Les champs RMB sont : 6-7 latitude, 8-9 longitude, **10 distance (NM)**,
**11 relèvement**, 12 vitesse de rapprochement. Les versions précédentes de ce
document lisaient le champ 12 comme la distance.

Cause (`mods_b/7264.js:609`) : les champs 7 et 9 sont calculés par
`destination.latitude < 0 ? "S" : "N"` et `destination.longitude < 0 ? "W" : "E"`.
Or `destination.latitude` est une instance de la classe de coordonnée
(`mods_b/745.js:7-30`), qui n'a pas de `valueOf` : la comparaison vaut
toujours `false`. L'hémisphère émis est donc toujours `N` et `E`, alors que la
classe expose un accesseur `hemisphere` correct.

Recoupement numérique, navire en `34.99026 S, 138.50815 E` :

| Hémisphère de la destination | Distance | Relèvement |
|---|---|---|
| `N` (émis) | 4 295,5 NM | 358,2° |
| `S` (réel) | **139,5 NM** | **228,1°** |
| champs 10 et 11 émis | **139,574 NM** | **228,1°** |

La distance et le relèvement émis sont justes ; seules les lettres
d'hémisphère sont fausses. Toute destination au sud de l'équateur ou à l'ouest
de Greenwich est donc émise dans le mauvais quadrant.
**Statut : FIX, sans drapeau. Cause racine : isolée.**

### 3.3 Formatage des coordonnées — fraction des minutes tronquée, longueur variable

Code (`mods_b/745.js:50`) :

```js
let Zt = O.minutes_decimal.toString().split("."),
    ce = Zt.length > 1 ? Zt[1].slice(0, 6) : "0";
return deg + ("00" + O.minutes).slice(-2) + "." + ("00" + ce).slice(2) + "," + hemi
```

Conséquences, toutes observées sauf la dernière :

| Cas | Émis | Attendu | Preuve |
|---|---|---|---|
| minutes entières, dont coordonnée zéro | `00000.0,E` | `00000.000000,E` | `legacy/cap-fixed.log` |
| latitude `35°00'` exacte | `3500.0,S` | `3500.000000,S` | `p2/cap-kml-gxtrack.log` |
| `138.503°` | `13830.179999,E` | `13830.180000,E` | `p2/cap-kml-gxtrack.log` |
| fraction < 1e-6 | `toString()` exponentiel, ex. `5e-7` dans le champ | 6 décimales | analyse statique seule |

La fraction est **tronquée**, jamais arrondie, et sa longueur varie avec la
valeur, puisque les zéros de fin disparaissent avec `toString()`. Le défaut
touche `RMC`, `GLL`, `GGA`, `RMB` et `WPL`.
**Statut : FIX, sans drapeau.** Largeur fixe de 6 décimales, arrondi au plus
proche. Les golden files concernés sont comparés après normalisation de ce
champ : c'est une différence assumée, pas une régression.

### 3.4 ZDA — zone locale du système

Code (`mods_b/7264.js:312, 330-334`) : heure et date en UTC ; champs 5 et 6
découpés dans `Date.toString()` après `GMT` ; un `+` en tête est retiré, un `-`
est conservé.

Exécution, un relancement par fuseau (`p2/zda-tz.log`, `p2/zda-run.log`) :

| `TZ` du processus | Décalage réel | Heure `ZDA` | = heure `RMC` | Champ 5 | Champ 6 |
|---|---|---|---|---|---|
| `UTC` | +00:00 | `100348.119` | oui | `00` | `00` |
| `Asia/Tokyo` | +09:00 | `100439.365` | oui | `09` | `00` |
| `America/St_Johns` | −02:30 (été) | `100530.628` | oui | `-02` | `30` |
| `Asia/Kolkata` | +05:30 | `100621.918` | oui | `05` | `30` |
| `Pacific/Chatham` | +13:45 (été) | `100713.169` | oui | `13` | `45` |
| `Europe/Paris` | +02:00 (été) | `100804.427` | oui | `02` | `00` |

Lecture :

- l'heure et la date `ZDA` sont en UTC, identiques à `RMC` : **OK** ;
- la zone décrite est celle du **système**, pas celle de la position du navire ;
- le signe `+` est implicite, le signe `-` explicite, les minutes non signées.
  La référence gpsd de NMEA 0183 définit le champ 5 comme « 00 to ±13 hours »
  et le champ 6 comme « 00 to 59, apply same sign as local hours » : `-02,30`
  est donc **conforme dans sa forme**. La passation qualifiait cette sortie de
  non conforme au motif d'une plage « 0 à 23 » : cette affirmation est retirée ;
- le **sens** du signe (décalage `local − UTC`, comme le legacy, ou « heures à
  ajouter à l'heure locale pour obtenir UTC ») n'est pas défini par la source
  consultée ; la norme IEC 61162-1 n'est pas disponible dans ce dépôt ;
- un décalage de +14 h (`Pacific/Kiritimati`) sortirait de la plage ±13 ;
- dériver un champ de `Date.toString()` dépend du moteur JavaScript.

**Statut : OK pour la forme et la convention observées**, que la cible
reproduit par défaut. Le calcul devient numérique, et le fuseau devient
configurable (`system`, fixe, ou dérivé de la longitude).
**Sens du signe : UNKNOWN justifié**, faute d'accès au texte normatif ; à
trancher avec la norme ou un récepteur réel lors du protocole d'acceptation
(`11-testing-strategy.md` §7). La convention legacy reste le défaut.

### 3.5 RMB/APB — inertes tant qu'aucune route n'est posée

Sans clic droit, les deux phrases sortent en statut `V` avec des champs vides
(`legacy/cap-tcp.log`). C'est le comportement attendu. `ap.value.origin` est
**figé** au moment du clic et ne suit plus le navire. **OK.**

### 3.6 `RMC` sans mode de navigation

Pas de champ `Mode` (A/D/E/M/S) : `RMC` se termine par `,,,` avant le
checksum. **FIX** : `Mode: A` est sans risque et attendu.

### 3.7 `VTG` sous talker `GP` alors que le help annonce `II`

Le help embarqué liste `VTG` dans la catégorie `II`, le code utilise `GP`.
**OK** : conserver `GP`, corriger le help.

### 3.8 `VBW` non déterministe par construction

```js
speedWaterForward  = speed - Math.random();
speedWaterBackward = speed - Math.random();
```

Observé : `$IIVBW,9.5,9.1,A,9.5,9.6,A,...` à 10 kn (`legacy/cap-fixed.log`),
soit une vitesse **transversale** de 9,1 kn. **FLAG (D5).**

### 3.9 `RPM` charge moteur constante

Champ de charge `10.5` en dur, y compris moteur arrêté :
`$IIRPM,E,1,0,10.5,A` (`legacy/cap-tcp.log`). **FLAG (D5).**

### 3.10 `GGA` — champs terminaux vides

**OK**, sortie valide et tolérée.

### 3.11 ViewSync — `timeEnd` invalide sous cadence élevée

Quand `tickScaling > 0`, `timeEnd` vaut `timeStart`. **FIX.**

### 3.12 Signal K — version périmée et chemin malformé

- hello `version: "17.12.05"`, lue avant la réponse IPC : **FIX** ;
- `navigation.destination.waypoint .<id>` avec une espace : **FIX**.

### 3.13 Cycle de vie réseau

- `startTCPServer()` résout avant le callback `listen` : **FIX** ;
- `stop()` ne vide pas `this.server` de façon synchrone et `changeType()` ne
  bascule que si `this.server` est falsy : course au changement de type. **FIX.**

### 3.14 `applySeed` — dérive ascendante sans rappel

`E[multiplicateur] = ((1+plus) + 1/(1+minus))/2 > 1`, aucun rappel, oscillation
à deux états au minimum (`600 → 612 → 600` rpm). **FLAG (D3)** : bruit borné
absolu avec rappel par défaut, dérive relative legacy derrière drapeau.

### 3.15 Barre instantanée et arrêt net du virage

Voir `06-simulation-model.md` §3. **FLAG (D1, D2).**

### 3.16 Barre négative : palier de 600 m quelle que soit la barre (implémentation)

`updateHeading()` (`mods_b/7264.js`) teste `rudderAngle >= 20`, `>= 10`,
`>= 5` : pour une barre négative, aucun test n'est vrai et le rayon de 600 m
s'applique toujours. À −25°, le navire tourne 4 fois moins vite qu'à +25°.
**FIX, sans drapeau** : paliers sur la valeur absolue (test
`legacy_negative_rudder_uses_same_law`).

### 3.17 APB/RMB : sens de correction inversé et XTE signé (implémentation)

Le legacy émet `xte` signé (`-0.001684`) et choisit `L` quand `xte < 0`. Or
`crossTrackDistanceTo` est négatif **à gauche** de la route : il faut alors
revenir à **droite**. Le champ XTE doit être une grandeur positive, le sens
étant porté par le champ suivant. **FIX** : valeur absolue et sens corrigé ;
champs « arrivée » et « perpendiculaire franchie » renseignés.

### 3.18 AIS type 5 : dernier groupe de bits mal complété (implémentation)

`_encodeNMEAPayload` lit le dernier groupe incomplet (4 bits) aligné à droite
au lieu de le compléter par des zéros : le dernier caractère vaut `2` au lieu
de `8` (`…@00000000000002` contre `…@00000000000008`). **FIX en profil
modern** ; le profil legacy reproduit la charge utile octet pour octet
(test `message5_matches_golden_except_legacy_padding`).

### 3.19 AIS type 5 : ETA = heure UTC courante (implémentation)

`_encodeVoyageData` réécrit mois, jour, heure et minute avec l'heure UTC
courante : l'ETA de l'objet `destination` est ignorée. La version antérieure
de D6 affirmait « ETA en heure locale » : corrigé. **Cible** : ETA
configurable (UTC), heure courante par défaut comme le legacy.

### 3.20 Signal K : nombres entiers

`JSON.stringify` écrit `2`, `0`, `359` ; `serde_json` écrirait `2.0`. **OK** :
la cible émet les flottants entiers sans décimale (égalité octet pour octet des
deltas, test `signalk_golden_legacy_vs_new`).

## 4. Compatibilité transport

| Transport | Legacy observé | Cible | Statut | Preuve |
|---|---|---|---|---|
| TCP server | `listen(port)`, toutes interfaces | identique | OK | `legacy/cap-tcp.log` |
| TCP NMEA | 1 write par phrase, CRLF | identique | OK | `legacy/cap-tcp.log` |
| TCP Signal K | 1 write JSON, sans CRLF | identique | OK | `legacy/cap-sk.log` |
| TCP coalescing | observé | non contractuel | OK | `legacy/cap-tcp.log` |
| WebSocket | port configurable, pas de hello en NMEA | identique | OK | `legacy/cap-ws.log` |
| UDP broadcast (type 4) | 1 datagramme par phrase ; destination = adresse de diffusion dérivée du CIDR de l'interface, `ip.address` ignoré | identique | OK | `legacy/cap-udp2.log`, `mods_b/933.js` |
| ViewSync | UDP dédié, 1 datagramme par update | identique | OK | `legacy/cap-udp.log` |
| Signal K hello | connexion uniquement | identique | OK | `legacy/cap-sk.log` |
| TCP client (type 2) | 1 connexion, 408 phrases, checksums valides ; segments non alignés sur les phrases | flux d'octets délimité par CRLF | OK | `legacy/cap-tcpclient.log` |
| TCP client — reconnexion | aucune : une seule connexion sur la fenêtre après fermeture serveur | reconnexion avec backoff et jitter | FIX | `legacy/cap-tcpclient-recon.log` |
| TCP client — cible absente | `started` émis avant la connexion ; l'UI affiche « Stop Simulator » sur `ECONNREFUSED` ; seul un `console.warning` | état `failed` visible, reprise | FIX | `legacy/console-notarget.log` |
| UDP client (type 5) | 1 datagramme par phrase, 24 par cycle, vers `ip.address:ip.port`, port source éphémère, pas de `setBroadcast` | identique | OK | `legacy/cap-udpclient.log` |
| UDP multicast (type 6) | 2 récepteurs servis à l'identique ; le départ de l'un n'affecte pas l'autre | identique | OK | `legacy/cap-mcast.log`, `legacy/console-cap-mcast.log` |
| UDP multicast — émetteur | rejoint lui-même le groupe sur son port éphémère, sans interface | pas d'adhésion côté émetteur ; interface et TTL configurables | FIX | `legacy/console-cap-mcast.log`, `mods_b/933.js` |
| Série (type 3) — sortie | 408 phrases en 17 cycles, CRLF, checksums valides, un write par phrase | identique | OK | `p2/cap-serial.log` |
| Série — chemin manuel | port absent de la liste accepté si `manualSerialPort: true` | identique, sans bascule implicite (§1) | OK | `p2/cfg-serial.json`, `p2/console-serial.log` |
| Série — entrée | ligne reçue par `ReadlineParser`, passée à `handleServerMsg` qui ne fait que `console.info` | entrée analysée comme source de commandes, optionnelle | FIX (ajout) | `p2/console-serial.log`, `mods_b/2178.js:36` |
| Série — Signal K | JSON suivi de CRLF, seul transport à recevoir un terminateur en Signal K | identique | OK (analyse statique) | `mods_b/2178.js:58` |

## 5. Sources de trace et rejeu

| Élément | Legacy observé | Cible | Statut | Preuve |
|---|---|---|---|---|
| Entrée de fichier | deux `ap-file-input` réels dans la barre d'outils, visibles simulateur arrêté : `video_library` (journal) et `streetview` (GPX/KML) | commandes API équivalentes | OK | `p2/README.md` |
| KML `gx:Track` | importé, trace proposée, suivi démarré | identique | OK | `p2/console-kml-gxtrack.log` |
| KML `LineString` | rejeté : « No Track Data » | importé | FIX | `p2/console-kml-linestring.log`, `mods_b/3309.js:786` |
| KML `Folder` | seules les `Placemark` du premier niveau de `Folder` sont lues ; `Folder` imbriqué ignoré | récursion complète | FIX | `p2/console-kml-folder.log`, `mods_b/3309.js:768` |
| KML `Placemark` à la racine de `kml` sans `Document` | ignoré | lu | FIX (analyse statique seule) | `mods_b/3309.js:768` |
| Suivi de trace — bornes | 3 points émis sur 5 : ni le premier ni le dernier | tous les points émis | FIX | `p2/cap-kml-gxtrack.log`, `mods_b/3309.js:1221-1241` |
| Suivi de trace — horodatage | heure `RMC` prise dans `when` (`100010.000`, `100020.000`…) | identique en mode legacy, option heure murale | OK | `p2/cap-kml-gxtrack.log` |
| Suivi de trace — vitesse | distance / Δt des `when` : 17,7 kn pour 91 m en 10 s | identique | OK | `p2/cap-kml-gxtrack.log` |
| Suivi de trace — fin | arrêt, « End of track has been reached. » | identique, plus événement `track:ended` | OK | `p2/README.md` |
| Rejeu — transport | le dialogue démarre le transport configuré à l'ouverture, l'arrête à la fermeture | identique | OK | `p2/console-replay.log` |
| Rejeu — framing | un `write` par bloc : 24 phrases, 981 à 990 octets, **CRLF final présent** | identique en mode chaîne | OK | `p2/cap-replay.log` |
| Rejeu — cadence | un bloc par `config.interval` ; premier envoi un intervalle après `play` | identique | OK | `p2/cap-replay.log` |
| Rejeu — pas après pause | `skip_next` après `pause` saute le bloc affiché (bloc 10 jamais émis) | le pas suivant émet le bloc affiché | FIX | `p2/cap-replay.log`, `mods_b/3309.js:936-941` |
| Rejeu — fin | arrêt en fin de journal, sans boucle | identique par défaut, `repeat` optionnel | OK | `mods_b/3309.js:940` |
| Barre d'outils `play_arrow` | `startSim()`, pas le rejeu | — | OK | `mods_b/3309.js:1060` |
| Terminal `content_copy` | copie le flux affiché dans le presse-papiers | identique | OK | `mods_b/5213.js:693` |

## 6. Golden files

Golden files P1 : `tests/fixtures/legacy/`, 19 fichiers, manifeste et
`SHA256SUMS`. Captures P2 : `tests/fixtures/legacy-p2/`, manifeste et
`SHA256SUMS`. Les deux jeux sont en lecture seule.

## 7. Vérification « legacy VS nouveau » (implémentation)

`crates/encode/tests/golden.rs` reconstruit l'état de chaque bloc des captures
NMEA (`cap-tcp`, `cap-fixed`, `cap-prefix`, `cap-ws`, `cap-udp2`, `cap-turn`,
`cap-steer`, `cap-ui2`, `cap-udpclient`, `cap-tcpclient`, `cap-mcast`), le
réencode en profil legacy et compare champ par champ :

| Classe | Phrases |
|---|---|
| Identiques octet pour octet | 2 746 |
| Écart dans la précision de reconstruction (valeur déduite d'un champ arrondi) | 410 |
| **Bogue corrigé volontairement** : mode RMC | 144 |
| **Bogue corrigé volontairement** : format des coordonnées | 21 |
| **Amélioration volontaire / aléa legacy** : VBW | 144 |
| **Divergence involontaire** | **0** |

Sur 3 456 phrases comparées. Également vérifiés : Signal K (deltas identiques,
hello corrigé), ViewSync (paquets identiques), séquence des 24 phrases du
profil legacy sur TCP (`legacy_profile_emits_legacy_sentence_sequence_over_tcp`),
hémisphère RMB (`route_golden_hemisphere_fix`), GPX/KML/rejeu sur les
captures P2 (`crates/source/tests/sources.rs`).

## 8. `UNKNOWN` restants

| Sujet | Pourquoi il reste ouvert | Comment le fermer |
|---|---|---|
| Sens du signe de la zone `ZDA` | texte IEC 61162-1 non disponible ; la référence gpsd donne la plage et le signe, pas le sens | lire la norme, ou brancher un récepteur réel (protocole §7 de `11`) |
