# 11 — Stratégie de test

## 1. Pourquoi des golden files

La compatibilité ne se démontre pas en comparant des screenshots. Elle se
démontre en comparant **octet par octet** ce que le legacy a produit sur le
câble, et ce que le nouveau produit dans les mêmes conditions.

Le harnais d'analyse a déjà produit des captures réelles sur le legacy :

| Capture | Contenu | Utilisée pour |
|---|---|---|
| `cap-tcp.log` | NMEA TCP 1 Hz | golden file NMEA de base |
| `cap-prefix.log` | NMEA avec préfixe 61162-450 | test du préfixe |
| `cap-ws.log` | NMEA WebSocket | test WebSocket |
| `cap-udp.log` | ViewSync UDP | golden file ViewSync |
| `cap-udp2.log` | NMEA UDP broadcast | test UDP |
| `cap-sk.log` | Signal K, hello + deltas | golden file Signal K |
| `cap-turn.log` | virage barre = 2 à 6.3 kn | test de la loi de virage |
| `cap-dest.log` | APB/RMB après clic droit | test route (valeurs corrigées) |
| `cap-geo.log` | deux destinations successives | test de cohérence de route |
| `sim.nmeasim` | journal natif | golden file journal |

Ces fichiers sont figés dans `tests/fixtures/legacy/` (P1, 19 fichiers,
manifeste et `SHA256SUMS`) ; les captures P2 (ZDA, série, KML, rejeu) sont dans
`tests/fixtures/legacy-p2/`. Les deux répertoires sont en lecture seule.

## 2. Niveaux de test

| Niveau | Portée | Outil | Cadence |
|---|---|---|---|
| L1 | fonctions pures : checksum, formatage, géodésie, conversions | runner léger | à chaque commit |
| L2 | composants : un encodeur sur un état fixé | runner léger | à chaque commit |
| L3 | intégration : noyau + encodeurs + transport en mémoire | runner + sockets locaux | à chaque commit |
| L4 | contrat : comparaison aux golden files legacy | runner + golden files | à chaque commit |
| L5 | bout en bout : application réelle, sockets, fichiers | pilote d'intégration | pré-version |
| L6 | manuel : scenarios d'acceptation avec un client réel | protocole | par version |

## 3. Tests déterministes

Le noyau reçoit un `Rng` injecté. Avec `randomSeed` épinglé, deux exécutions
produisent le même journal :

```
sim.run({ seed: 1234, ticks: 100 }) === sim.run({ seed: 1234, ticks: 100 })
```

C'est la propriété qui rend les golden files utilisables, et elle est absente du
legacy, dont le PRNG global n'est pas adressable.

## 4. Contrats à couvrir explicitement

Chaque ligne est un test nommé, réutilisé par les niveaux L3 et L4.

### 4.1 NMEA

| Test | Attendu |
|---|---|
| checksum de toutes les sentences | XOR valide sur l'octet |
| `RMC` | champs complets, **mode présent** (correction) |
| `DBT` | `pieds,f,mètres,M,brasses,F`, identique au legacy (non-régression, D4) |
| coordonnées | 6 décimales fixes, arrondi, y compris minutes entières et zéro |
| `VTG` | talker `GP` |
| `ZDA` | heure UTC ; zone signée `-hh,mm` / `hh,mm` comme le legacy ; fuseau injecté |
| `RPM` | régime `0` si moteur arrêté, charge variable si en marche |
| `VBW` | valeurs bornées par le seed, et non `speed - random()` |
| `APB`/`RMB` | **hémisphères `S`/`W` corrects**, coordonnées cohérentes avec distance et relèvement (correction) |
| `APB`/`RMB` | statut `V` sans route, `A` avec route |
| préfixe | appliqué à toutes les phrases générées et aux phrases injectées |
| ordre d'émission | 24 phrases dans l'ordre legacy |
| talkers | surcharge par formatter prise en compte |

### 4.2 Signal K

| Test | Attendu |
|---|---|
| `version` du hello | version applicative, **jamais** `17.12.05` |
| chemin waypoint | `navigation.destination.waypoint.<id>`, sans espace |
| unités | m/s, radians, Kelvin |
| absence de CRLF | dernier octet `}` |
| delta par tick | exactement un message |
| `context` | `vessels.<vesselId>` |
| `$source` | `nmea-simulator` |

### 4.3 ViewSync

| Test | Attendu |
|---|---|
| nombre de champs | 10 |
| `counter` | démarre à 1, incrémente |
| `heading` | en degrés |
| `timeStart` | `62167219200 + floor(epochSeconds)` |
| `timeEnd` | `timeStart + Δ` y compris en cadence élevée (correction) |
| absence de terminateur | pas de `\n` |

### 4.4 Transport

| Test | Attendu |
|---|---|
| TCP server | écoute sur toutes les interfaces |
| NMEA TCP | un write par phrase, CRLF inclus |
| Signal K TCP | un write JSON, sans CRLF |
| hello | à l'ouverture de connexion, une seule fois |
| UDP | un datagramme par phrase |
| broadcast | interface sélectionnée, port source éphémère |
| cycle de vie | `start` idempotent, état `running` seulement après bind |
| changement de type | sérialisé par `id`, pas de course |
| file bornée | un transport lent ne bloque pas le noyau |
| panne isolée | un transport en échec n'affecte pas les autres |

### 4.5 Modèle

| Test | Attendu |
|---|---|
| intégration position | 10 kn / 1 s → 5.144 m ± 0.5 % |
| loi de virage | barre 2, 6.3 kn → 19.45 °/min |
| barre à zéro | virage nul |
| retour au centre | le virage continue pendant le rappel |
| dérive | bornée, moyenne nulle, pas de dérive ascendante |
| graine | reproductibilité bit à bit |
| vent apparent | composition vectorielle, cohérente en virage |
| RP | monte et redescend, borné |

### 4.6 Journal et rejeu

| Test | Attendu |
|---|---|
| écriture | en-tête `nmeasim,<version>` + CRLF, blocs `~` |
| relecture legacy | l'en-tête et le dernier bloc vide sont retirés |
| rejeu | un bloc par tick, arrêt à la fin, sans boucle |
| rejeu en cadence x2 | positions cohérentes grâce à `meta:` |
| extension `meta:` | ignorée par un lecteur non informé |
| rotation | par taille et par durée, gzip, sans blocage du noyau |

### 4.7 Routage

| Test | Attendu |
|---|---|
| GPX track | un point par tick, cadence fixe |
| GPX temps réel | interpolation selon `time` |
| cap | `<course>` prioritaire, sinon relèvement |
| KML `gx:Track` | import sans erreur |
| KML `LineString` | import sans erreur |
| KML `Placemark/Point` | waypoint nommé |
| fin de trace | événement `track:ended`, état correspondant |

## 5. Harnais d'intégration

Le harnais existant (copié de `/tmp/nmeasim-test/` vers `tools/legacy-harness/`) est conservé et rejoué contre la
nouvelle implémentation :

| Outil | Rôle | Réutilisable |
|---|---|---|
| `cdp.js`, `cdp2.js` | évaluateur d'expression dans le renderer Electron | oui, via l'API IPC de la cible |
| `launch.sh` | relance isolée de l'application | oui, ajusté au runtime cible |
| `mkcfg.js` | génération d'une config complète | oui, contre le nouveau schéma |
| `seed.js` | injection de la configuration | oui |
| `cap-tcp.js` | capture TCP | oui |
| `cap-ws.js` | capture WebSocket | oui |
| `cap-udp.js`, `cap-udp2.js` | captures UDP | oui |
| `test-ui.js` | scénario CDP asynchrone | oui |
| `input.js` | séquence d'événements d'entrée CDP | oui |
| `caprun.sh`, `start.sh` | pilote de capture : relance, config, démarrage par clic réel unique | oui |
| `consolecap.js` | console du renderer (messages de transport, absents de `app.log`) | oui |
| `cap-tcp-wait.js` | client TCP qui attend l'ouverture du port (reprise unique corrigée en P2) | oui |
| `setfile.js` | `DOM.setFileInputFiles` sur les vrais `ap-file-input` (journal, GPX/KML) | legacy seulement |
| `clickicon.sh` | clic réel sur un bouton désigné par son `mat-icon` | legacy seulement |
| `pty-serial.py` | paire de PTY par `os.openpty()`, lecture maître, injection | oui |
| `t-zda.sh`, `t-serial.sh`, `t-kml.sh`, `t-replay.sh`, `t-replay-meta.sh` | scénarios P2 | legacy seulement |

Points d'attention repris de l'expérience terrain :

- écrire la configuration puis attendre le flush du stockage avant de relancer ;
- sur un ASAR, un rechargement de page casse l'app : relancer le processus ;
- les événements clavier **réels** (CDP `Input.dispatchKeyEvent`) sont
  nécessaires ; des `dispatchEvent` synthétiques ne suffisent pas, ce qui est
  lui-même un constat à documenter sur le legacy ;
- un clic droit doit être précédé d'un `mouseMoved`, sinon la position utilisée
  est périmée ;
- les avertissements GPU sous Xvfb sont sans effet sur le comportement.
- le démarrage passe par le bouton d'icône « Start Simulator » de la barre
  d'outils, par un vrai `Input.dispatchMouseEvent`, **un seul** clic ; toute
  config de capture a `autoStart: false` ;
- `pkill -f` avec le chemin du binaire legacy tue le shell appelant si le motif
  figure dans sa ligne de commande : le motif ne vit que dans `launch.sh`.

## 6. Transports instruits en P1/P2

| Transport | Moyen employé | Capture |
|---|---|---|
| TCP client | serveur de test `srv-tcp.js`, fermeture forcée | `legacy/cap-tcpclient*.log` |
| UDP client | socket de réception sur port connu | `legacy/cap-udpclient.log` |
| UDP multicast | groupe `239.255.42.99` sur `lo`, deux récepteurs | `legacy/cap-mcast.log` |
| Série | paire de PTY par `os.openpty()` (`socat` absent) | `legacy-p2/cap-serial.log` |

Le harnais est versionné dans `tools/legacy-harness/`. La même paire de PTY
sert au test d'intégration du transport série de la cible.

## 7. Tests d'acceptation avec un client réel

Protocole minimal, à exécuter sur chaque version candidate :

1. Brancher un HELM ou un ECDIS réel sur TCP `10110`.
2. Démarrer, laisser tourner 10 min, vérifier : position qui dérive
   conformément, cap et vitesse cohérents entre `RMC`, `VHW`, `VTG`, `HDT` et
   `GPGSA`, `APB`/`RMB` inactifs sans route.
3. Poser une destination, vérifier la cohérence de `RMB` et `APB`.
4. Basculer en Signal K, vérifier le delta et le hello.
5. Rejouer un journal, vérifier l'absence de décalage.
6. Charger un GPX, vérifier la fin de trace.

## 8. Non-régression des défauts

Chaque correction du `04-compatibility-matrix.md` a un test nommé
`regression:<défaut>`, et un test `legacy-mode:<défaut>` qui vérifie que le mode
de compatibilité reproduit bien l'ancien comportement. Sans ce second test, le
mode de compatibilité ne prouve rien.
