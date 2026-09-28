# Passation de session — P1 / P2 / P3

Rédigé le 2026-09-28 vers 12 h 00 CEST, à la demande de l'utilisateur, pour
transmettre le travail à un autre agent. **Rien n'a été perdu** : tous les
fichiers, scripts et résultats sont sur le disque.

---

## 1. Objectif de la session

Traiter les phases **P1**, **P2** et **P3** du plan, puis préparer **J0**, et
**s'arrêter** sur un rapport. Aucune implémentation J1 ou ultérieure.

| Phase | Contenu |
|---|---|
| **P1** | Figer dans le dépôt les golden files du legacy 1.6.1, avec manifeste complet et sommes SHA-256. |
| **P2** | Éliminer les `UNKNOWN` de `docs/04-compatibility-matrix.md` par des tests d'exécution réels, chacun classé `OK`, `FIX` ou `UNKNOWN` justifié. |
| **P3** | Rédiger les sept décisions de compatibilité dans `docs/decisions/001..007`. |
| **J0** | Socle de projet seulement : monorepo, cœur, runner de tests, CI, `core/units`, `core/random`. Ni NMEA, ni physique, ni UI, ni manette complète. |

Contraintes permanent :

- ne jamais modifier le `.deb`, l'ASAR, ni un octet d'un golden file, ni ses
  horodatages ;
- toute découverte suit la chaîne **observation → documentation existante →
  décision → test → implémentation** ; pas de deuxième source de vérité ;
- le choix technologique (Rust ou Node) doit être **tranché explicitement**
  avant J0. `docs/12-new-architecture.md` propose aujourd'hui Node LTS, alors
  que la demande utilisateur évoque Rust. Point non résolu.

---

## 2. État d'avancement

| Poste | Statut |
|---|---|
| Lecture du plan, des 12 documents et de l'analyse du bundle | **Terminé** |
| Correction d'un diagramme abîmé dans le plan (flèches `──▶` supprimées) | **Terminé** |
| P1 — rapatriement des 10 golden files du plan, horodatages préservés | **Terminé** |
| P1 — 3 captures manquantes (multicast, UDP client, TCP client) | **Terminé** |
| P1 — manifeste `tests/fixtures/legacy/README.md` + `SHA256SUMS` | **Terminé** |
| P2 — UDP client, TCP client, absence de reconnexion, cible absente, multicast | **Terminé** |
| P2 — ZDA multi-fuseaux | **En cours, dernière tentative interrompue** |
| P2 — port série, KML, rejeu | **Non commencé** |
| P2 — mise à jour de `04-compatibility-matrix.md` | **Non commencé** |
| P3 — les sept ADR | **Non commencé** |
| Architecture manette / API de contrôle | **Non commencé** |
| J0 | **Non commencé** |
| Rapport final | **Non commencé** |

---

## 3. P1 — terminé

### 3.1 Fichiers

`tests/fixtures/legacy/` contient **19 fichiers** plus un `README.md` et un
`SHA256SUMS`. Tous ont été copiés avec `cp -p` et vérifiés par empreinte
identique à la source.

Originaux du plan : `cap-tcp.log`, `cap-prefix.log`, `cap-ws.log`,
`cap-udp.log`, `cap-udp2.log`, `cap-sk.log`, `cap-turn.log`, `cap-dest.log`,
`cap-geo.log`, `sim.nmeasim`.

Trois captures utiles hors liste initiale, conservées :
`cap-fixed.log`, `cap-steer.log`, `cap-ui2.log`.

Six fichiers produits pendant cette session :

| Fichier | Ce qu'il établit |
|---|---|
| `cap-udpclient.log` | 408 datagrammes, 24 par cycle, un datagramme par phrase |
| `cap-tcpclient.log` | 408 phrases, segments TCP non alignés sur les phrases |
| `cap-tcpclient-recon.log` | aucune reconnexion après fermeture du serveur |
| `console-notarget.log` | `ECONNREFUSED` invisible, aucune reprise |
| `cap-mcast.log` | deux récepteurs servis, l'un partant n'affecte pas l'autre |
| `console-cap-mcast.log` | trace console `UDP (multicast) (239.255.42.99)` |

### 3.2 Résultat

P1 est **clos**. Le manifeste documente pour chaque capture l'origine, la
commande, la configuration, la graine, l'horodatage, l'environnement, le
transport, le scénario et ce qu'elle prouve.

---

## 4. P2 — résultats acquis

### 4.1 UDP client (type 5) — **OK**

408 datagrammes en 17 cycles, 16 747 octets. 24 datagrammes par cycle : 18
phrases `$` et 6 phrases `!`. Le socket est lié sur un port éphémère et
n'appelle pas `setBroadcast`. Contrairement au mode `udp`, la destination est
bien `ip.address:ip.port`.

### 4.2 TCP client (type 2) — **OK avec défaut**

1 connexion, 408 phrases, tous les checksums valides. Les segments TCP ne
respectent pas les limites de phrase : il faut traiter un flux d'octets délimité
par `\r\n`, jamais un message par phrase.

### 4.3 Absence de reconnexion — **FIX**

Serveur détruit la connexion à 8 s et continue d'écouter : **une seule
connexion sur toute la fenêtre**, 27 trames avant fermeture, aucune tentative
ensuite. `startTCPClient()` ne contient aucune logique de reconnexion.

### 4.4 Cible absente — **FIX**

Démarrage avec rien sur le port : le processus ne plante pas, la barre
d'outils affiche `Stop Simulator`, donc l'application annonce un succès. La
seule trace est un `console.warning` invisible sans outils de développement.
`messageSource.next({action:"started"})` est émis **avant** l'ouverture de la
connexion, et l'échec n'est ni propagé à l'interface ni retrié.

### 4.5 Multicast (type 6) — **OK avec quirk**

Groupe `239.255.42.99` port `15002`. Deux récepteurs joints sur `lo` reçoivent
exactement le même flux, 192 datagrammes de part et d'autre. R1 se retire à
11 s ; R2 reçoit encore 264 datagrammes, soit 11 cycles de plus. Le multicast
n'a pas de contrôle de flux.

Quirk : l'émetteur joint lui-même le groupe sur son port éphémère, sans
interface, ce qui est inutile. La source observée est l'interface physique par
défaut, pas `lo` ; les copies de bouclage atteignent néanmoins les membres de
`lo`.

Prérequis à rétablir avant toute nouvelle capture multicast :
`sudo ip route add 239.0.0.0/8 dev lo`. La route est en place sur cette machine.

### 4.6 ZDA — analyse de code établie, test d'exécution à finir

Le mécanisme est identifié dans `analysis/mods_b/7264.js`, lignes 312 et 330 à
334 :

```js
const Zt = gt || new Date, ce = Zt.toString(), we = ce.indexOf("GMT")
et.fields[1] = formatTimeString(Zt)                        // heure UTC
et.fields[2] = ("00" + Zt.getUTCDate()).slice(-2)         // jour UTC
et.fields[3] = ("00" + (Zt.getUTCMonth() + 1)).slice(-2)   // mois UTC
et.fields[4] = Zt.getUTCFullYear()                         // année UTC
const K = ce.substring(we + 3, we + 6)
et.fields[5] = "+" === K[0] ? K.slice(1) : K               // zone locale
et.fields[6] = ce.substring(we + 6, we + 8)               // minutes de zone
```

Conséquences à confirmer par capture :

- l'heure et la date sont en **UTC**, la zone décrite est celle du système ;
- un décalage **positif** perd son signe, un décalage **négatif le conserve** ;
  `America/St_Johns` (`GMT-0230`) devrait donc émettre `-02,30`, ce qui est
  non conforme à NMEA 0183 où la zone locale est un entier 0 à 23 ;
- les décalages à minutes non nulles passent par la découpe et semblent corrects
  (`Asia/Kolkata` `+0530`, `Pacific/Chatham` `+1345`) ;
- près de minuit, la date UTC et la zone locale peuvent se contredire.

Obséré jusqu'ici : `$GPZDA,091406.235,28,09,2026,02,00` sous `TZ=UTC+02:00`,
soit la valeur `02,00` attendue.

### 4.7 Autre fait établi pendant cette session

`cap-fixed.log` révèle un défaut d'encodage de coordonnées **non documenté
auparavant** : lorsque la partie fractionnaire des minutes est nulle — cas de la
coordonnée exactement zéro — le champ secondes est réduit à un seul `0`.
Observation : `$GPRMC,...,0000.030518,N,00000.0,E,...` au lieu de
`00000.000000`. À ajouter à la matrice en `FIX`.

---

## 5. Outillage disponible

Tout est dans `/tmp/nmeasim-test/`. Ce répertoire est **éphémère** : le copier
ailleurs si l'environnement doit être reconstruit.

| Script | Rôle |
|---|---|
| `launch.sh` | relance isolée du legacy sous Xvfb, profil dans `/tmp/nmeasim-test/profile`, CDP sur 9222 |
| `launch-tz.sh <TZ>` | idem avec fuseau imposé |
| `mkcfg.js '<json>' <fichier>` | génère une configuration à partir de la graine de base |
| `seed.js <cfg>` | produit l'expression d'injection `localStorage` |
| `cdp.js '<expr>'` | évalue une expression dans le renderer |
| `input.js <actions.json>` | pilote CDP : `Runtime.evaluate`, `Input.dispatchMouseEvent` |
| `consolecap.js <durée> <sortie>` | capture le console du renderer |
| `start.sh` | démarre le simulateur, vérifie, réessaie |
| `caprun.sh <cfg> "<récepteur>" <sortie> [durée]` | **pilote principal** |
| `cap-tcp.js` / `cap-tcp-wait.js` | client TCP, avec ou sans attente du port |
| `srv-tcp.js` | serveur TCP, avec fermeture forcée à l'instant voulu |
| `cap-udpclient.js` | récepteur UDP unicast |
| `cap-mcast.js` | deux récepteurs multicast, l'un se retire à mi-capture |
| `shot.js` | capture d'écran via CDP |

Configurations : `cfg-tcp.json`, `cfg-prefix.json`, `cfg-ws.json`, `cfg-udp.json`,
`cfg-sk.json`, `cfg-fixed.json`, `cfg-ui.json`, et celles créées ici :
`cfg-udpclient.json` (type 5), `cfg-tcpclient.json` (type 2),
`cfg-mcast.json` (type 6), `cfg-zda.json` (type 1, `autoStart: false`).

### 5.1 Séquence de capture

```bash
cd /tmp/nmeasim-test
TZZ=UTC bash caprun.sh cfg-zda.json "node cap-tcp-wait.js 10110 zda-one.log 18 40" zda-one.log 16
```

`caprun.sh` enchaîne : relance, injection de configuration, attente du flush
LevelDB, seconde relance, démarrage du récepteur, `start.sh`, capture, relevé de
l'état.

---

## 6. Pièges rencontrés — ne pas les refaire

Ces cinq erreurs ont coûté l'essentiel du temps de cette session. Elles sont
toutes corrigées dans les scripts, mais la connaissance doit être transmise.

1. **Le bouton texte `Start` ne démarre pas le simulateur.** Il existe deux
   boutons `Start` dans le DOM, dans des panneaux sans rapport. Seul le bouton
   d'icône de la barre d'outils dont l'infobulle vaut `Start Simulator`
   appelle `SimulatorService.start()`. Rectangle `1152,0 48x48`, centre
   `(1176, 24)` en 1200x725. Un `.click()` programmatique ne suffit pas : il
   faut un vrai `Input.dispatchMouseEvent`.

2. **Un seul clic par démarrage.** Deux clics de suite = démarrage
   puis arrêt. La barre oscille et la capture reste vide. `start.sh` n'envoie
   qu'une séquence.

3. **`autoStart: true` entre en conflit avec le clic.** L'application démarre
   seule au chargement, le clic l'arrête. Toutes les configurations de capture
   doivent avoir `autoStart: false`.

4. **`pkill -f "opt/NMEASimulator/nmeasimulator"` tue le shell appelant** si
   cette chaîne figure dans la commande en ligne, car `pkill -f` compare la
   ligne de commande complète. Le motif vit dans `launch.sh` et
   `launch-tz.sh` ; ne jamais le recopier dans une commande interactive. C'est
   ce qui a fait expirer une commande à 240 s.

5. **Un transport TCP serveur n'ouvre son port qu'au démarrage.** Le récepteur
   doit attendre, sinon il meurt sur `ECONNREFUSED` avant que l'application
   n'écoute. Utiliser `cap-tcp-wait.js`. Et ne pas employer `srv-tcp.js` contre
   `cfg-tcp.json` : les deux veulent le port 10110.

Deux remarques supplémentaires :

- les messages de la classe de transport vivent dans le **renderer**, pas dans
  `app.log` ; `consolecap.js` est obligatoire pour les lire ;
- `location.reload()` sur `file://.../app.asar/` échoue en
  `ERR_FILE_NOT_FOUND`. Pour appliquer une configuration, on injecte puis on
  relance l'application.

---

## 7. Suite immédiate recommandée

1. **Terminer ZDA.** Relancer `bash /tmp/nmeasim-test/t-zda.sh`. Les six fuseaux
   sont déjà paramétrés. Si une capture reste vide, vérifier dans l'ordre :
   config effectivement relue, port 10110 ouvert, `start.sh` confirmé, fichier
   non vide. Reporter les champs 5 et 6 dans un tableau.
2. **Port série.** `socat` est **absent** de la machine. Créer une paire de PTY
   virtuels en Python avec `os.openpty()` et la passer en
   `server.serial.port`. `manualSerialPort` vaut `false` par défaut.
3. **KML.** Fabriquer trois fichiers, `gx:Track`, `LineString` et `Folder`.
   L'injection se fait par `DOM.setFileInputFiles` sur l'entrée de fichier réelle,
   pas par un `setLogFile`.
4. **Rejeu.** Charger `tests/fixtures/legacy/sim.nmeasim` puis inspecter les
   boutons `play_arrow` et `content_copy` de la barre d'outils.
5. **Mettre à jour `docs/04-compatibility-matrix.md`** avec les statuts acquis :
   UDP client `OK`, TCP client `OK`, reconnexion `FIX`, cible absente `FIX`,
   multicast `OK`, ZDA selon résultat, coordonnées à zéro `FIX`, série et KML
   selon résultat.
6. **P3**, puis architecture manette et API de contrôle, puis J0, puis le
   rapport `SESSION_P1_P2_P3_REPORT.md`.

---

## 8. Fichiers du projet utiles

| Chemin | Contenu |
|---|---|
| `REIMPLEMENTATION_PLAN.md` | plan maître, à mettre à jour en fin de session |
| `docs/01` à `docs/12` | documentation existante, source de vérité unique |
| `docs/04-compatibility-matrix.md` | **à mettre à jour en priorité** |
| `tests/fixtures/legacy/README.md` | manifeste P1, écrit et complet |
| `tests/fixtures/legacy/SHA256SUMS` | empreintes de référence |
| `analysis/mods_b/933.js` | implémentations des sept transports |
| `analysis/mods_b/7264.js` | simulation et encodeurs NMEA, dont ZDA |
| `analysis/mods_b/2178.js` | orchestration, démarrage et arrêt |
| `analysis/mods_b/3309.js` | import KML et GPX, rejeu |
| `/home/tom/Téléchargements/nmeasimulator_1.6.1_amd64.deb` | paquet original, ne jamais modifier |

## 9. État de la machine

- Xvfb `:99` tourne, l'application legacy tourne, port `9222` ouvert.
- Route multicast `239.0.0.0/8 dev lo` en place.
- `socat` absent. `node` v22.23.2, `python3` 3.14.4, `sudo` sans mot de passe.
- Le répertoire `/tmp/nmeasim-test/` contient le profil Chromium de test ; le
  vider force une réinstallation des données de session du legacy.
