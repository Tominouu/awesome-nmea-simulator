# 10 — Routage, journal et rejeu

## 1. GPX

### 1.1 Éléments lus

| Élément | Attributs | Usage |
|---|---|---|
| `trkpt` | `lat`, `lon` | position | obligatoire |
| | `ele` | altitude | si présent |
| | `time` | horodatage | si présent |
| | `course` | cap | prioritaire |
| | `speed` | vitesse | prioritaire |
| `rtept` | idem | idem | transformés en `trkpt` |
| `wpt` | `name` | waypoint nommé | devient un WPL |

### 1.2 Suivi, tel que relevé sur le legacy

- **Un point par tick**, sans interpolation temporelle : la trace est rejouée à
  la cadence de sortie, pas à son rythme d'enregistrement.
- Vitesse : `<speed>` si présent, sinon distance divisée par l'écart de temps
  entre points, sinon le seed GPS.
- Cap : `<course>` si présent, sinon relèvement vers le point suivant.
- Altitude et horodatage non réinjectés dans le modèle.
- Fin de trace : arrêt du simulateur et message
  `End of track has been reached.`
- Charger une trace **verrouille la position GPS** : un clic sur la carte ou une
  saisie manuelle est ignoré tant qu'une trace est active.

### 1.3 Cible

- Deux modes : **cadence fixe** (compatible legacy) et **temps réel**
  (interpolation linéaire sur `time`).
- Interpolation en position sur l'arc de cercle, pas en coordonnées décimales,
  pour éviter les sauts de route aux hautes latitudes.
- Le cap est interpolé par le plus court chemin angulaire.
- La vitesse et le cap issus de la trace sont appliqués en `override` le temps du
  suivi, puis relâchés.
- `ele` alimente l'altitude GNSS.
- La trace reste rejouable en boucle (`repeat`), option activée par défaut.
- Export de la trace courante en GPX.

## 2. KML

### 2.1 Structures couvertes par le legacy

Lues par la classe interne `Mk` (module `745.js`) :

`Document`, `Folder`, `Placemark`, `MultiTrack`, `MultiGeometry`, `Track`.

Le help embarqué annonce la balise `<gx:Track>`, c'est-à-dire la Google
extension. Validation dynamique (P2, `tests/fixtures/legacy-p2/`) :

| Cas | Résultat legacy | Statut |
|---|---|---|
| `Placemark/gx:Track` | trace proposée et suivie | OK |
| `Placemark/LineString` | « No Track Data » | FIX |
| `Folder/Placemark` | lu | OK |
| `Folder/Folder/Placemark` | ignoré | FIX |
| suivi d'une trace de N points | N−2 points émis : ni le premier ni le dernier | FIX |
| heure des phrases | prise dans `<when>` | OK |
| coordonnées émises | fraction des minutes tronquée, longueur variable | FIX (doc 04 §3.3) |

### 2.2 Cible

Prise en charge explicite de :

| Chemin KML | Traitement |
|---|---|
| `gx:Track/gx:coord` | trace, `lon,lat[,alt]` |
| `LineString/coordinates` | trace, idem |
| `gx:MultiTrack/gx:Track` | plusieurs traces |
| `Placemark/Point/coordinates` | waypoint |
| `Document` / `Folder` | récursion, nommage des traces |
| `when` (TimeSpan) | horodatage, pour le mode temps réel |
| `gx:label` | nom du waypoint |
| `LineStyle`, `gx:color` | ignorés |

Les séparateurs de coordonnées gèrent les espaces, les virgules et les
retours ligne. Les modes `altitudeMode` (`absolute`, `relativeToGround`,
`clampToGround`) sont gérés ou signalés.

## 3. Journal `.nmeasim`

### 3.1 Format, tel que vérifié en exécution

```
nmeasim,1.6.1\r\n~
$GPRMC,083744.985,A,3459.625014,S,13830.069004,E,6.3,17.9,280926,,,*0A\r\n
$IIVHW,17.9,T,17.9,M,6.3,N,11.6,K*66\r\n
...
$IIAPB,A,A,...*7D\r\n~
$GPRMC,...*0A\r\n
...
~
```

| Élément | Valeur |
|---|---|
| En-tête | `<appId>,<version>` puis CRLF |
| Séparateur de bloc | `~` |
| Séparateur de phrases dans un bloc | `\r\n` |
| Fréquence | un bloc par tick de sortie |
| Extension ajoutée | `.nmeasim` si absente |

La lecture legacy fait `split("~")`, vérifie `slice(0,7) === "nmeasim"`, puis
`shift()` pour retirer l'en-tête et `pop()` pour retirer le bloc vide final.

### 3.2 Limites du format relevées

- Aucune information d'horodatage autre que le contenu des phrases : un journal
  rejoué à une autre cadence produit des positions incohérentes.
- Pas de version de format interne : un journal de la 1.6.1 et un journal d'une
  version ultérieure seraient indiscernables.
- Le rejeu legacy envoie un **bloc entier en un seul `send()`**, CRLF final
  compris, là où le mode direct fait un write par phrase (vérifié :
  `legacy-p2/cap-replay.log`).
- Le bloc est décodé avec `split("\n")`, ce qui laisse un `\r` résiduel.

### 3.3 Extension `meta:`, rétrocompatible (décision D7)

```
nmeasim,2.0.0\r\n
meta:{"format":1,"recordedAt":...,"intervalMs":1000,"stepMs":20,"seed":1234,"output":"nmea"}\r\n
~
$GPRMC,...\r\n$IIVHW,...\r\n~
...
~
```

Règles, détaillées dans `docs/decisions/007-journal.md` :

- la ligne `meta:` est dans le **segment d'en-tête**, avant le premier `~`. Le
  legacy jette ce segment après avoir contrôlé ses 7 premiers caractères : il
  ne l'affiche pas et ne l'émet pas. Vérifié : `legacy-p2/meta-test.nmeasim`
  chargé « 1 of 5 », aucun octet `meta:` sur TCP ;
- un bloc `meta:` **séparé**, proposé dans une version antérieure de ce
  document, est proscrit : le rejeu legacy envoie chaque segment brut sur le
  réseau ;
- le fichier se termine toujours par `~`, sinon le `pop()` du chargeur legacy
  retire le dernier bloc réel ;
- un lecteur qui ne reconnaît pas `meta:` l'ignore ; un journal sans `meta:`
  reste un journal legacy valide.

### 3.4 Rotation

- Rotation par taille et par durée, compression `gzip` de l'archive close.
- Écriture asynchrone en file, jamais sur le thread de simulation.
- Si le disque est plein ou en lecture seule, l'enregistrement **désactive
  l'écriture et émet un avertissement** au lieu de bloquer ou de faire
  échouer la simulation.

## 4. Rejeu

### 4.1 Comportement legacy

| Aspect | Observé |
|---|---|
| Transport | l'instance de communications déjà configurée, après `stop()` du direct |
| Granularité | `step()` avance ou recule d'un **bloc** |
| Lecture | `go()` envoie le bloc courant puis incrémente |
| Cadence | `config.interval` |
| Fin | arrêt à la fin du journal, **sans boucle** |
| Pas après pause | `step(+1)` saute le bloc affiché non envoyé (**FIX**, vérifié) |
| Ouverture / fermeture | démarre puis arrête le transport configuré |
| Affichage | `Sentence group: N of M` |
| Barre d'outils | précédent, lecture, pause, suivant, retour au début |

### 4.2 Cible

- Le rejeu est un **mode de la simulation**, pas un cas particulier : le même
  encodeur produit les messages, le même transport les envoie.
- Un bloc du journal est un **instantané complet** : le rejeu réinjecte
  l'instantané dans le modèle plutôt que de rejouer des chaînes, ce qui permet
  d'afficher l'état et de changer de format de sortie pendant la lecture.
- Un journal legacy, sans instantané, est rejoué en mode **chaîne** : les
  phrases sont réémises telles quelles, ce qui préserve le comportement.
- Commandes : `play`, `pause`, `step ±1`, `seek(i)`, `repeat`, `speed ×k`.
- Reprise à une cadence différente de l'enregistrement sans dérive de
  position : le journal porte la ligne `meta:` de son en-tête (D7).
- Export du rejeu vers un autre format à la volée.

## 5. Interactions à clarifier

| Question | Décision à acter |
|---|---|
| Le rejeu et la simulation live peuvent-ils coexister ? | Non. Un seul producteur de messages. |
| Un rejeu peut-il écrire dans un journal ? | Oui, avec un suffixe de source, sinon boucle de réenregistrement. |
| Charger une trace GPX pendant un rejeu ? | Non, sans arrêt explicite du rejeu. |
| Le rejeu lit-il un journal compressé ? | Oui, `gzip` transparemment. |
| La fin d'un journal `repeat` émet-elle un événement ? | Oui : `replay:ended`, `replay:looped`. |

## 6. Séquencement

```
track-player  ──▶ VesselState (override position/vitesse/cap)
journal-writer ◀── MessageBus
replay-reader ──▶ MessageBus (mode chaîne)  |  ──▶ VesselState (mode instantané)
```

Ces trois sources ne peuvent pas produire de messages simultanément : un
**arbitre à jeton unique** garantit qu'un seul producteur est actif. Sans cet
arbitre, la simulation réelle, le rejeu et l'écriture de journal peuvent produire
des phrases concurrentes dans le même flux, ce qui est le bug le plus coûteux à
diagnostiquer sur un banc d'essai.
