# 01 — Vue d'ensemble

## 1. Objet du projet

Reconcevoir un **simulateur de bateau** moderne, inspiré de NMEASimulator
(v1.6.1), en conservant un **niveau de compatibilité maximal** avec ses formats
de sortie et ses mécanismes de transport, mais en internant entièrement le
modèle de simulation.

Le produit reste un **outil de test** destiné aux HELM, ECDIS, pilotes
automatiques, récepteurs AIS, instruments et passerelles : il produit des flux
de données réalistes, il ne prétend pas être un simulateur naval de haute
fidélité.

## 2. Périmètre

### 2.1 Dans le périmètre

| Domaine | Contenu |
|---|---|
| Formats de sortie | NMEA 0183, Signal K, ViewSync |
| Transports | TCP server, TCP client, UDP broadcast, UDP client, UDP multicast, WebSocket server, port série |
| Modèle | Bateau, navigation, vent, mer, moteurs, destination, waypoint |
| Environnement | Vent réel/apparent, température, profondeur, DOP, satellites, AIS |
| Routage | Import GPX/KML, suivi de route, marqueurs de carte |
| Persistance | Configuration, journalisation `.nmeasim`, rejeu (replay) |
| Interfaces | UI desktop, clavier, manette, API de contrôle |
| Entrées | Terminal utilisateur NMEA, overrides, injection d'événements |

### 2.2 Hors périmètre

- Rendu 3D (le legacy est une carte 2D OpenLayers).
- Simulation haute fidélité des voiliers : le legacy est un modèle cinématique
  simple (voir `06-simulation-model.md`).
- Protocoles instruments propriétaires (SeaTalk, NMEA 2000, CAN).
- Simulation de trafic AIS tiers (seuls le navire propre et la destination sont
  émis par le legacy ; voir `07-output-formats.md`).

## 3. Principes directeurs

1. **Compatibilité d'abord.** Un client (HELM, ECDIS) doit pouvoir être branché
   sur le nouveau simulateur avec le même câblage et la même configuration que
   sur le legacy. Toute divergence doit être **documentée et optionnelle**.
2. **Le modèle, pas les phrases.** Les sentence builders sont des *vues* pures sur
   l'état du modèle. Aucune logique de simulation ne doit vivre dans un encodeur.
3. **Transport agnostique.** Un même message logique part vers N TCP/UDP/WS/Série.
   Le framing est une propriété du transport, jamais de l'encodeur.
4. **Défauts legacy : décider, ne pas hériter par accident.** Les bugs relevés
   dans `04-compatibility-matrix.md` sont chacun classés : *corrigé*,
   *préservé* ou *préservé derrière un drapeau*.
5. **Testable sans affichage.** Le noyau doit être pilotable en headless afin de
   pouvoir reproduire les scénarios de `11-testing-strategy.md`.

## 4. Public cible et cas d'usage

- Développeurs d'instruments : valider un parseur NMEA ou Signal K.
- Intégrateurs AIS : tester un récepteur AIVDM face à des phrases cohérentes.
- Centres de formation : rejouer une trace de navigation.
- Bancs d'homologation : comparer deux versions du simulateur.

## 5. Documents du projet

| Fichier | Sujet |
|---|---|
| `01-overview.md` | Ce document : périmètre et principes. |
| `02-legacy-analysis.md` | Rétro-ingénierie du binaire 1.6.1. |
| `03-requirements.md` | Exigences fonctionnelles et non fonctionnelles. |
| `04-compatibility-matrix.md` | Matrice de parité et défauts legacy. |
| `05-architecture.md` | Architecture cible. |
| `06-simulation-model.md` | Modèle de simulation. |
| `07-output-formats.md` | Encodeurs NMEA, Signal K, ViewSync. |
| `08-transports.md` | Implémentations réseau et série. |
| `09-inputs-and-ui.md` | UI, clavier, manette, API. |
| `10-routes-and-replay.md` | GPX/KML, journal, rejeu. |
| `11-testing-strategy.md` | Stratégie de test. |
| `12-new-architecture.md` | Découpage en modules implémentables. |
| `REIMPLEMENTATION_PLAN.md` | Ordre d'implémentation et jalons. |

## 6. Terminologie

- **Sentence** : une ligne NMEA checksumée commençant par `$`.
- **Talker** : les deux premiers caractères d'une sentence (`GP`, `II`, `IN`…).
- **Delta Signal K** : message JSON `context` + `updates[].values`.
- **Seed** : valeur, bornes et amplitude de dérive aléatoire d'un champ.
- **Mask** : activation de la mise à jour automatique d'un champ.
- **Override** : valeur forcée par l'utilisateur, prioritaire sur le seed.

## 7. Statut de la documentation

| Domaine | Statut |
|---|---|
| Analyse statique du bundle | **Terminée** |
| TCP, UDP, WebSocket, ViewSync, Signal K (exécution) | **Terminés** |
| Journal `.nmeasim` (format) | **Terminé** |
| Clavier et direction (exécution) | **Terminés** |
| Clic map → APB/RMB (exécution) | **Terminé** |
| Transports série, TCP client, UDP client, multicast | **Terminés** (P1, P2) |
| KML en exécution (`gx:Track`, `LineString`, `Folder`) | **Terminé** (P2) |
| Replay en exécution | **Terminé** (P2) |
| ZDA sous six fuseaux | **Terminé** (P2) |
| GPX en exécution | analyse statique ; même chemin de suivi que KML, vérifié par P2 |
| Charge moteur et inertie de propulsion | charge `10.5` constante observée ; inertie absente par lecture du code |

Les éléments non validés restent explicitement `UNKNOWN` dans
`04-compatibility-matrix.md` §8, chacun avec sa justification.

L'implémentation (J1 à J10) est livrée : voir `FINAL_IMPLEMENTATION_REPORT.md`
et la documentation utilisateur (`README.md`, `user-guide.md`, `gamepad.md`,
`network.md`, `configuration.md`, `scenarios.md`, `development.md`).
