# Rapport de session — P1, P2, P3, J0

Session du 2026-09-28, 12 h 00 à 13 h 00 CEST environ. Reprise de la
passation `HANDOVER.md`. Périmètre : terminer P2, P3, puis J0, et s'arrêter.
**Rien n'a été implémenté au-delà de J0.**

## 1. Synthèse

| Poste | Statut | Où |
|---|---|---|
| P1 — golden files | déjà clos, **revérifié 19/19** à chaque étape, aucun octet ni horodatage touché | `tests/fixtures/legacy/` |
| P2 — ZDA, série, KML, rejeu | **terminé**, 18 fichiers de preuve figés avec `SHA256SUMS` | `tests/fixtures/legacy-p2/` |
| P2 — matrice | **réécrite** ; un seul `UNKNOWN` restant, justifié | `docs/04-compatibility-matrix.md` |
| P3 — D1 à D7 | **rédigées**, 8 rubriques chacune | `docs/decisions/001` à `007` |
| Manette et API de contrôle | **documentées** | `docs/09` §3 et §7, `docs/05` §2 et §4.6, `docs/12` |
| Runtime | **tranché : Rust** | `docs/decisions/008-runtime-rust.md` |
| J0 | **terminé** : workspace, `nmeasim-core`, 35 tests, CI, 100 % de couverture | `Cargo.toml`, `crates/core/`, `.github/workflows/ci.yml` |
| Plan | **mis à jour** | `REIMPLEMENTATION_PLAN.md` |

## 2. À ratifier par l'utilisateur — écarts entre la consigne et l'observation

La chaîne observation → documentation → décision a fait tomber quatre « faits
établis ». Je les ai documentés tels qu'observés plutôt que tels que demandés.

1. **DBT n'est pas inversé.** `$SDDBT,26.2,f,8.0,M,4.4,F` est l'ordre NMEA 0183
   exact (pieds, mètres, brasses), avec des conversions justes ; le code
   (`mods_b/7264.js:467`) le confirme. L'exemple « legacy » des anciens
   documents n'existe dans aucune capture. **D4 conserve l'ordre legacy, sans
   drapeau** : la « correction » demandée aurait créé un défaut. Sans drapeau,
   comme demandé.
2. **ZDA `-02,30` est conforme dans sa forme.** La référence gpsd de NMEA 0183
   définit le champ 5 comme « 00 to ±13 hours » et le champ 6 « apply same
   sign as local hours », et non « 0 à 23 ». Le legacy est classé **OK**. Seul
   le **sens** du signe reste `UNKNOWN`, faute d'accès à la norme IEC 61162-1.
3. **Le MMSI legacy n'est pas aléatoire.** Il vaut `503999999`, codé en dur
   (`mods_b/7078.js:655`), identique dans les 17 captures AIS. D6 garde cette
   valeur par défaut et la rend configurable.
4. **Le champ « charge moteur » de `RPM` est le pas d'hélice.** Le dictionnaire
   de champs du legacy lui-même (`mods_b/7078.js:418`) le dit. D5 en tient
   compte.

Deux autres points à ratifier :

- **Rust** a été retenu (D8), pour la demande initiale, le nom du dépôt et la
  chaîne d'outils présente. `docs/12` proposait Node.
- **Ordre du dépôt :** les preuves P2 sont dans `tests/fixtures/legacy-p2/`,
  répertoire séparé, pour ne toucher ni au manifeste ni aux `SHA256SUMS` de P1.

## 3. P2 — résultats

### 3.1 ZDA, six fuseaux (`legacy-p2/zda-tz.log`)

| `TZ` | Décalage | Heure `ZDA` = heure `RMC` (UTC) | Champ 5 | Champ 6 |
|---|---|---|---|---|
| `UTC` | +00:00 | oui | `00` | `00` |
| `Asia/Tokyo` | +09:00 | oui | `09` | `00` |
| `America/St_Johns` | −02:30 | oui | `-02` | `30` |
| `Asia/Kolkata` | +05:30 | oui | `05` | `30` |
| `Pacific/Chatham` | +13:45 | oui | `13` | `45` |
| `Europe/Paris` | +02:00 | oui | `02` | `00` |

6 captures sur 6, démarrage confirmé à chaque fois. L'analyse de code de la
passation est confirmée : `+` retiré, `-` conservé.

### 3.2 Série (`legacy-p2/cap-serial.log`)

Paire de PTY créée par `os.openpty()` (`socat` absent), `/dev/pts/2`,
4800 bauds, `manualSerialPort: true`. **408 phrases en 17 cycles, 16 743
octets, 408 checksums valides, CRLF, aucune LF nue.** L'entrée série est lue
ligne à ligne puis simplement passée à `console.info` : sans effet.
`refreshSerialPorts()` peut réécrire le type série en WebSocket quand aucun
port n'est détecté (`mods_b/7238.js:395`). Ce point est établi par analyse
statique seulement : l'hôte a 32 `/dev/ttyS*`.

### 3.3 KML (`legacy-p2/console-kml-*.log`, `cap-kml-gxtrack.log`)

Injection par `DOM.setFileInputFiles` sur le vrai `ap-file-input`
« streetview ».

| Fichier | Résultat |
|---|---|
| `gx:Track` | importé et suivi → **OK** |
| `LineString` | « No Track Data » → **FIX** |
| `Folder` | seul le premier niveau est lu, `Folder` imbriqué ignoré → **FIX** |

Suivi de trace : sur 5 points, seuls 3 sont émis (ni le premier ni le
dernier) → **FIX**. L'heure `RMC` est prise dans les `<when>` → **OK**.

### 3.4 Rejeu (`legacy-p2/cap-replay.log`)

`sim.nmeasim` (copie `cp -p`) injecté sur le `ap-file-input` « video_library ».
Un `write` TCP par bloc (24 phrases, 981 à 990 octets, **CRLF final présent** —
les documents disaient l'inverse), un bloc par seconde. Après `pause`,
`skip_next` **saute** le bloc affiché : le bloc 10 n'est jamais émis → **FIX**.
Le `play_arrow` de la barre d'outils est `startSim()`, et `content_copy` copie
le flux du terminal. Aucun des deux n'appartient au rejeu, qui a son propre
dialogue.

### 3.5 Découvertes supplémentaires

- **RMB, cause racine isolée** : `destination.latitude < 0` compare un objet
  sans `valueOf` à 0, ce qui vaut toujours `false`. L'hémisphère émis est donc
  toujours `N`/`E`. Recoupement numérique : avec `S`, on trouve 139,5 NM et
  228,1°, exactement les champs 10 et 11 émis. Les anciens documents lisaient
  aussi le champ 12 comme la distance.
- **Formatage des coordonnées**, généralisation du défaut « coordonnée zéro » :
  la fraction des minutes est tronquée et de longueur variable (`00000.0`,
  `3500.0`, `13830.179999`), avec risque de notation exponentielle. **FIX.**
- **AIS type 5** : ETA en heure locale au lieu d'UTC. **FIX** (D6).
- **D7 vérifiée sur le legacy** : un journal dont la ligne `meta:` est dans
  l'en-tête est chargé (« 1 of 5 ») et **aucun octet `meta:`** n'est émis. Le
  bloc `meta:` séparé proposé dans l'ancien `docs/10` aurait été envoyé sur le
  réseau ; il est proscrit.

### 3.6 Défaut d'outillage corrigé

`cap-tcp-wait.js` programmait une reprise sur `error` **et** sur `close`, ce
qui ouvrait deux connexions parallèles et dupliquait chaque bloc. C'est corrigé.
Les 16 captures P1 ont été contrôlées : aucun doublon.

## 4. P3 — décisions

| # | Décision | Mode legacy |
|---|---|---|
| D1 | barre dynamique, 8 °/s ; fixe le mécanisme commun `compatibility.profile` + `overrides`, résolu une fois en `CompatProfile`, un seul moteur | oui |
| D2 | taux de giration à décroissance exponentielle, dérive nulle à barre zéro établie | oui |
| D3 | bruit absolu borné, rappel exponentiel, `√dt`, `Rng` injecté | oui (formule `applySeed`, rendue reproductible) |
| D4 | `DBT` inchangé, déjà conforme | non |
| D5 | `VBW` et `RPM` dérivés de l'état ; champ 4 de `RPM` = pas d'hélice | oui |
| D6 | identité AIS fixe `503999999` par défaut, configurable, validée ; ETA en UTC | non |
| D7 | ligne `meta:` dans l'en-tête, fichier terminé par `~` | — |

Les bogues documentés (RMC, RMB, Signal K, ViewSync, cycle de vie, perte de
champs de config, et ceux ajoutés en P2) sont corrigés **sans drapeau**. Un
test statique `compat:no-bugfix-flag` est prévu pour le garantir.

## 5. Manette et API de contrôle

- `docs/09` §3 : détection à chaud avec empreinte et profil, chaîne calibration
  → inversion → zone morte continue → courbe → sensibilité, modes
  position et taux, boutons sur front, profils JSON exportables, arbitrage
  « dernier écrivain gagne ». Un stick au repos n'émet rien.
- `docs/09` §7 : `ControlApi::execute(Command)` comme unique primitive
  d'écriture, chemins stables, une vingtaine de commandes (sim, barre,
  propulsion, cap, vitesse, destination, waypoints, pilote automatique,
  overrides, transports, journal, rejeu), `Event` et `RejectReason` typés,
  surface HTTP/WS `/api/v1`.
- La manette, le clavier, l'UI, les scripts et l'API distante émettent tous les
  mêmes commandes. L'UI n'écrit jamais dans l'état.

## 6. J0

| Élément | Contenu |
|---|---|
| Workspace | `Cargo.toml` (édition 2024, MSRV 1.85, `unsafe_code = forbid`), `rust-toolchain.toml`, alias `cargo cov` |
| `nmeasim-core` | **aucune dépendance** ; `units` (conversions SI ⇄ nautiques, normalisation et écart d'angles) ; `random` (trait `Rng`, SplitMix64, xoshiro256**, `fork`, sauvegarde d'état, `below` de Lemire, `entropy_seed`) |
| Tests | 28 unitaires + 7 d'intégration déterministes : vecteurs de référence publiés (xoshiro sur `[1,2,3,4]`, SplitMix64(0)), vecteurs d'une implémentation Python indépendante, reproductibilité, restauration d'état, moyenne, khi-deux |
| Couverture | `cargo llvm-cov` : **100 %** des lignes, régions et fonctions de `units.rs` et `random.rs` |
| Lint | `cargo fmt --check`, `cargo clippy --all-targets -D warnings` : propres |
| CI | `.github/workflows/ci.yml` : SHA-256 des fixtures, fmt + clippy, tests Linux/Windows/macOS, MSRV 1.85, absence de dépendance du cœur, couverture ≥ 100 % |

Rejoué localement : fixtures, fmt, clippy, tests, couverture, contrôle des
dépendances, YAML valide. **Pas rejoué** : jobs Windows, macOS et MSRV 1.85,
faute de ces environnements.

## 7. Modifications de l'environnement

- `rustup component add llvm-tools-preview` et `cargo install cargo-llvm-cov`
  (v0.9.1), installés **globalement** dans `~/.cargo`.
- Le dépôt **n'est pas un dépôt git** : rien n'a été commité, et la CI ne
  s'exécutera qu'une fois le dépôt poussé sur GitHub.
- Le harnais de `/tmp/nmeasim-test/` (éphémère) a été copié dans
  `tools/legacy-harness/` : 55 fichiers, scripts et entrées KML, sans le profil
  ni les captures.
- Le legacy tourne toujours sous Xvfb `:99`, CDP 9222.
- `HANDOVER.md` n'a pas été modifié ; ce rapport le remplace pour l'état.

## 8. Ce qui reste

- `UNKNOWN` unique : le sens du signe de la zone `ZDA` (norme ou récepteur réel).
- Ratifier les points de §2.
- Suite du plan : J1 (horloge, ordonnanceur à pas fixe, état, événements,
  `CompatProfile`), dans l'ordre core → domaine → simulation → sorties →
  transports → UI. **Non commencé**, conformément à la consigne.
