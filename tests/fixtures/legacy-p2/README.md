# Captures P2 — instruction des `UNKNOWN`

Captures produites le **2026-09-28 entre 12 h 03 et 12 h 15 CEST** sur le legacy
NMEASimulator **1.6.1** (`nmeasimulator_1.6.1_amd64.deb`, non modifié), pour
instruire les `UNKNOWN` de `docs/04-compatibility-matrix.md`.

Ce répertoire est **séparé** de `tests/fixtures/legacy/` pour ne toucher ni aux
19 golden files P1, ni à leur manifeste, ni à leurs `SHA256SUMS`. Les copies
ont été faites avec `cp -p` ; les empreintes sont dans `SHA256SUMS`
(`sha256sum -c SHA256SUMS`).

## Environnement commun

| Élément | Valeur |
|---|---|
| Hôte | Ubuntu 26.04, Xvfb `:99` (1600x1000), fenêtre de l'app 1200x725 |
| Profil | `/tmp/nmeasim-test/profile`, CDP sur `127.0.0.1:9222` |
| Pilotage | scripts de `tools/legacy-harness/` (copie de `/tmp/nmeasim-test/`) |
| Démarrage | bouton d'icône « Start Simulator » par `Input.dispatchMouseEvent`, un seul clic |
| Configuration | `autoStart: false`, injection `localStorage`, attente 9 s, relance |
| Graine | aucune : le legacy utilise `Math.random()` global, non adressable |
| Intervalle | 1000 ms |

## Fichiers

### ZDA multi-fuseaux

| Fichier | Contenu |
|---|---|
| `cfg-zda.json` | TCP serveur (type 1), port 10110, `autoStart: false` |
| `zda-run.log` | sortie de `t-zda.sh` : un relancement par fuseau, démarrage confirmé |
| `zda-tz.log` | par fuseau : premier `RMC` (heure UTC) et premier `ZDA` complet |

Commande : `bash tools/legacy-harness/t-zda.sh` (lance
`TZZ=<fuseau> caprun.sh cfg-zda.json "node cap-tcp-wait.js 10110 zda-one.log 18 40" zda-one.log 16`
pour `UTC`, `Asia/Tokyo`, `America/St_Johns`, `Asia/Kolkata`,
`Pacific/Chatham`, `Europe/Paris`). Les captures unitaires `zda-one.log` sont
effacées par le script après extraction ; `zda-tz.log` en conserve la première
phrase `RMC` et `ZDA`.

Prouve : heure et date en UTC ; champs 5 et 6 = décalage du fuseau système ;
signe `+` supprimé, signe `-` conservé (`-02,30`).

### Rejeu

| Fichier | Contenu |
|---|---|
| `cap-replay.log` | client TCP sur 10110 pendant le rejeu de `legacy/sim.nmeasim` |
| `console-replay.log` | console du renderer : démarrage et arrêt du serveur par le dialogue |

Commande : `bash tools/legacy-harness/t-replay.sh`. Le journal est une copie
`cp -p` de `tests/fixtures/legacy/sim.nmeasim`, injectée par
`DOM.setFileInputFiles` sur le premier `ap-file-input` (icône `video_library`).
Scénario : `play_arrow` à t0, `pause` à t0+9 s, `skip_next` ×2,
`skip_previous`, `close`.

Prouve : un `write` TCP par bloc (24 phrases, 981 à 990 octets, CRLF final
présent) ; cadence `config.interval` ; premier envoi un intervalle après le
clic ; après une pause, `skip_next` saute le bloc affiché (le bloc 10 n'est
jamais émis) ; le dialogue démarre le serveur à l'ouverture et l'arrête à la
fermeture.

### KML

| Fichier | Contenu |
|---|---|
| `kml/gxtrack.kml` | `Document/Placemark/gx:Track`, 5 points, `when` toutes les 10 s |
| `kml/linestring.kml` | `Document/Placemark/LineString`, 5 points |
| `kml/folder.kml` | `Folder/Placemark/gx:Track`, `Folder/Folder/Placemark/gx:Track`, `Folder/Placemark/LineString` |
| `console-kml-*.log` | console du renderer pendant chaque import |
| `cap-kml-gxtrack.log` | client TCP pendant le suivi de `gxtrack.kml` |

Commande : `bash tools/legacy-harness/t-kml.sh`. Injection par
`DOM.setFileInputFiles` sur le second `ap-file-input` (icône `streetview`),
sélection de la première trace proposée, puis démarrage pour `gx:Track`.

Prouve : `gx:Track` importé et suivi ; `LineString` rejeté (« No Track Data ») ;
dans `folder.kml`, seule la trace du premier niveau de `Folder` est proposée ;
suivi : 3 points émis sur 5 (ni le premier ni le dernier), heure `RMC` prise
dans les `when`, puis « End of track has been reached. » ; coordonnées
`3500.0,S` et `13830.179999,E` (défaut de formatage).

### Série

| Fichier | Contenu |
|---|---|
| `cfg-serial.json` | type 3, `serial.port=/dev/pts/2`, 4800 bauds, `manualSerialPort: true` |
| `cap-serial.log` | lectures côté maître du PTY, horodatage relatif ; ligne `INJECT` |
| `console-serial.log` | console du renderer : ouverture, fermeture, réception |

Commande : `bash tools/legacy-harness/t-serial.sh`. La paire de PTY est créée
par `os.openpty()` (`pty-serial.py`), `socat` étant absent. Une phrase
`$GPTXT` est écrite vers l'application 10 s après la première lecture.

Prouve : 408 phrases en 17 cycles, 16 743 octets, 408 checksums valides, CRLF,
aucune LF nue ; l'entrée série est reçue (`console.info Object` 10 ms après
l'injection) et n'a aucun autre effet.

### Journal avec ligne `meta:` (vérification de D7)

| Fichier | Contenu |
|---|---|
| `meta-test.nmeasim` | en-tête `nmeasim,2.0.0` + ligne `meta:{...}` avant le premier `~`, puis les 5 premiers blocs de `legacy/sim.nmeasim` |
| `cap-replay-meta.log` | client TCP pendant son rejeu par le legacy |
| `console-replay-meta.log` | console du renderer |

Commande : `bash tools/legacy-harness/t-replay-meta.sh` (même scénario que
`t-replay.sh`). Prouve : le legacy accepte le fichier (« Sentence group: 1 of
5 »), et aucun octet de la ligne `meta:` n'est émis sur le réseau.

## Artefact d'outillage corrigé pendant P2

`cap-tcp-wait.js` programmait une reconnexion sur `error` **et** sur `close` ;
après un `ECONNREFUSED`, deux connexions parallèles écrivaient dans la même
sortie et chaque bloc apparaissait deux fois. Corrigé (reprise unique sur
`close`) avant la capture `cap-replay.log` conservée ici. Les golden files P1
ont été vérifiés : aucun doublon consécutif.
