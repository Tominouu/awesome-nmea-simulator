# D7 — Extension `meta:` du journal `.nmeasim`

| | |
|---|---|
| Statut | Acceptée |
| Date | 2026-09-28 |
| Auteur | session P2/P3 (agent), sur arbitrage de l'utilisateur |
| Exigences | RTE-05, RTE-06, SIM-12, CFG-04 |
| Documents | `10-routes-and-replay.md` §3-4, `02-legacy-analysis.md` §5 |

## Contexte

Le journal legacy ne contient que des phrases : ni cadence, ni graine, ni
version de format, ni format de sortie. Un rejeu à une autre cadence ou une
reproduction de session est donc impossible. Il faut ajouter des métadonnées
sans rendre le fichier illisible par le legacy 1.6.1, ni lui faire émettre des
octets parasites.

## Comportement legacy

Écriture (`mods_b/2178.js:58`) : en-tête `nmeasim,1.6.1\r\n`, puis pour chaque
tick le bloc des phrases, chacune suivie de `\r\n`, puis `~`.
`legacy/sim.nmeasim` : 1 803 segments après `split("~")` — l'en-tête, 1 801
blocs de 24 phrases terminés par `\r\n`, un segment final vide.

Lecture (`mods_b/3309.js:1251`, `:930-941`) :

1. `data.split("~")` ;
2. rejet si `segments[0].slice(0, 7) !== "nmeasim"` ;
3. dans le dialogue de rejeu, `pop()` retire le **dernier** segment et
   `shift()` le **premier** ;
4. chaque segment restant est envoyé **brut** par `comms.send()`, sans filtre
   ni analyse (vérifié : `legacy-p2/cap-replay.log`, blocs identiques octet
   pour octet au fichier).

Conséquences pour toute extension :

- tout ce qui précède le premier `~` est ignoré par le legacy ;
- tout segment après le premier `~` est **émis sur le réseau** par un rejeu
  legacy ;
- un fichier qui ne se termine pas par `~` perd son dernier bloc réel au
  `pop()`.

## Options

1. Pas d'extension.
2. **Bloc `meta:` séparé** après l'en-tête (proposition antérieure de
   `10-routes-and-replay.md` §3.3) : un rejeu legacy l'enverrait tel quel aux
   récepteurs, octets non NMEA compris. Rejeté.
3. **Ligne `meta:` à l'intérieur du segment d'en-tête**, avant le premier `~`.
4. Fichier annexe `.meta.json` : se perd à la copie, rejeté.

## Décision

Option 3.

```
nmeasim,2.0.0\r\n
meta:{"format":1,"recordedAt":"2026-09-28T10:00:00.000Z","intervalMs":1000,"stepMs":20,"seed":1234,"output":"nmea","compatibility":"modern","app":"nmeasim-rs 0.1.0"}\r\n
~
$GPRMC,...\r\n$IIVHW,...\r\n...\r\n~
...\r\n~
```

Règles :

- la première ligne reste `nmeasim,<version>` ; la version est celle de
  l'application ;
- au plus une ligne `meta:` suivie d'un objet JSON sur **une** ligne, dans le
  segment d'en-tête uniquement ;
- le JSON ne contient ni `~` ni saut de ligne : `~` est échappé en `~` ;
- clé `format` obligatoire (entier, 1 pour cette version) ; clés inconnues
  ignorées par le lecteur ;
- chaque bloc reste terminé par `\r\n` puis `~`, et le fichier se termine par
  `~` ;
- aucune métadonnée par bloc : l'horodatage d'un bloc se déduit de
  `recordedAt + index × intervalMs`.

Lecteur de la cible :

- journal sans `meta:` → journal legacy, rejeu en mode chaîne
  (`10-routes-and-replay.md` §4.2) ;
- `meta.format` supérieur à la version connue → avertissement, rejeu en mode
  chaîne ;
- `meta:` illisible → erreur explicite nommant la ligne, jamais d'arrêt
  silencieux.

## Justification

- Le legacy jette le segment d'en-tête entier après avoir vérifié les 7
  premiers caractères : la ligne `meta:` y est invisible pour lui, dans le
  terminal comme sur le réseau.
- Le bloc séparé de la proposition antérieure aurait été émis sur le fil par
  un rejeu legacy ; cette propriété n'était pas connue avant la capture P2.
- Une seule ligne JSON reste lisible par un humain et extensible.

## Compatibilité

| Sens | Résultat |
|---|---|
| journal cible → legacy 1.6.1 | chargé, même nombre de blocs, aucun octet `meta:` émis |
| journal legacy → cible | lu, rejoué en mode chaîne, comportement legacy |
| journal cible sans `meta:` (option désactivée) | identique au format legacy |

Le texte « le legacy l'affichera dans son terminal sans planter, et ne
l'émettra pas sur le réseau s'il est filtré par les formatters connus » de
`10-routes-and-replay.md` §3.3 était faux et est remplacé.

## Conséquences

- `10-routes-and-replay.md` §3.3 est réécrit selon cette décision.
- L'écriture du journal consomme le bloc produit pour les transports
  (`07-output-formats.md` §8) ; la ligne `meta:` est écrite une fois à
  l'ouverture.
- La graine de D3 et le profil de compatibilité de D1 sont enregistrés, ce qui
  rend une session reproductible à partir de son journal.

## Tests à prévoir

| Test | Attendu |
|---|---|
| `journal:legacy-read` | `legacy/sim.nmeasim` : 1 801 blocs de 24 phrases |
| `journal:roundtrip` | écrire puis relire : blocs identiques octet pour octet, `meta` identique |
| `journal:legacy-loader` | réimplémentation fidèle du chargeur legacy (`split`, contrôle 7 caractères, `pop`, `shift`) sur un journal cible : même nombre de blocs, aucun bloc ne contient `meta:` |
| `journal:trailing-tilde` | le fichier se termine par `~` ; sans lui, le chargeur legacy perdrait un bloc |
| `journal:tilde-escape` | une valeur contenant `~` est écrite `~` et relue à l'identique |
| `journal:unknown-format` | `format: 99` → avertissement et mode chaîne |
| `journal:bad-meta` | JSON invalide → erreur nommant la ligne 2 |
| `journal:legacy-exec` | L5 : un journal cible rejoué par le legacy 1.6.1 sous le harnais ne produit aucun octet `meta:` sur TCP — **déjà vérifié une fois** : `legacy-p2/meta-test.nmeasim`, « 1 of 5 », 0 octet `meta:` dans `legacy-p2/cap-replay-meta.log` |
