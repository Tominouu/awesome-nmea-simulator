# 03 — Exigences

## 1. Exigences fonctionnelles

### 1.1 Modèle de simulation (SIM)

| ID | Exigence | Priorité |
|---|---|---|
| SIM-01 | Le modèle doit exposer un état horodaté et versionné, indépendant des encodeurs. | Must |
| SIM-02 | Position, cap, vitesse SOG et STW doivent être intégrées à pas de temps fixe, indépendant de la fréquence d'émission. | Must |
| SIM-03 | La cadence d'émission doit être configurable de 100 ms à 10 s. | Must |
| SIM-04 | Le pas de simulation doit rester stable si l'horloge de sortie est modifiée. | Must |
| SIM-05 | Chaque grandeur doit pouvoir être verrouillée (`override`) ou laissée dériver (`drift`). | Must |
| SIM-06 | La dérive doit être **bornée** et réversible (retour vers la valeur nominale), afin d'éviter la dérive ascendante du legacy. | Must |
| SIM-07 | La barre doit avoir une position entière, un sens, et un comportement au centre explicite. | Must |
| SIM-08 | Le modèle doit pouvoir suivre une route (waypoints) et produire relèvements, distances, XTE et Vitesse de fermeture. | Must |
| SIM-09 | Le modèle doit gérer le vent réel et le vent apparent par composition vectorielle. | Must |
| SIM-10 | Le modèle doit exposer température d'eau, profondeur, DOP et nombre de satellites. | Must |
| SIM-11 | Le modèle doit gérer deux lignes de propulsion indépendantes (régime, température, marche/arrêt). | Must |
| SIM-12 | L'état doit être sérialisable et restaurable à l'identique. | Should |

### 1.2 Formats de sortie (OUT)

| ID | Exigence | Priorité |
|---|---|---|
| OUT-01 | Émettre NMEA 0183 avec checksum XOR valide, talker configurable par phrase. | Must |
| OUT-02 | Implémenter au minimum `RMC, VHW, VTG, HDT, GLL, GGA, GSA, ZDA, MTW, MWV, MWD, DPT, DBT, VBW, RPM, WPL, GBS, APB, RMB, VDR, RLM, AIVDM, AIVDO`. | Must |
| OUT-03 | Talkers par défaut identiques au legacy (voir `04-compatibility-matrix.md`). | Must |
| OUT-04 | Respecter l'ordre d'émission et le regroupement d'un tick legacy (24 phrases, un bloc). | Should |
| OUT-05 | Proposer le préfixe propriétaire 61162-450, activable, appliqué à toute phrase générée. | Should |
| OUT-06 | Émettre un delta Signal K par tick, chemin par chemin, avec unités SI et radians. | Must |
| OUT-07 | Émettre un hello Signal K à la connexion pour les transports orientés connexion. | Must |
| OUT-08 | Le `version` du hello doit provenir de la version applicative unique. | Must |
| OUT-09 | Émettre un paquet ViewSync par update, avec compteurs et temps epoch 1900. | Must |
| OUT-10 | Chaque champ binaire doit être optionnel ; un simulateur ne doit jamais émettre un champ corrompu. | Must |
| OUT-11 | Permettre l'injection de phrases utilisateur en sortie, avec ou sans préfixe. | Should |

### 1.3 Transports (NET)

| ID | Exigence | Priorité |
|---|---|---|
| NET-01 | Supporter TCP server, TCP client, UDP broadcast, UDP client, UDP multicast, WebSocket server, port série. | Must |
| NET-02 | N transports actifs simultanément à partir d'un même état. | Must |
| NET-03 | Le type de transport doit être **exprimé par un nom** dans la config, avec compatibilité des valeurs numériques legacy. | Must |
| NET-04 | Le cycle de vie doit être idempotent : `start`/`stop` sûrs à répéter, statut reflétant l'état réel du bind. | Must |
| NET-05 | Un transport en erreur ne doit pas interrompre la simulation ni les autres transports. | Must |
| NET-06 | Le framing (CRLF, un message par datagramme) doit être une propriété du transport. | Must |
| NET-07 | Le port série doit être configurable en débit, parité, bits, arrêt, avec reconnexion. | Should |
| NET-08 | Le multicast doit permettre l'adhésion multiple avec interface et TTL configurables. | Should |
| NET-09 | Les métriques (octets, messages, erreurs, clients connectés) doivent être exposées. | Should |

### 1.4 Routage et rejeu (RTE)

| ID | Exigence | Priorité |
|---|---|---|
| RTE-01 | Importer GPX tracks et routes (trkpt, rtept, ele, time, course, speed). | Must |
| RTE-02 | Importer KML (`Document`, `Folder`, `Placemark`, `Track`, `gx:Track`, `LineString`, `MultiGeometry`). | Must |
| RTE-03 | Pouvoir suivre une trace à cadence fixe ou en temps réel, avec option d'interpolation. | Must |
| RTE-04 | Signaler la fin de trace et l'état correspondant. | Must |
| RTE-05 | Écrire un journal horodaté de la session au format legacy `.nmeasim`. | Must |
| RTE-06 | Relire un journal, avancer/reculer tick par tick, jouer/reprendre, et s'arrêter proprement en fin de trace. | Must |
| RTE-07 | Le rejeu doit réutiliser la configuration de transport courante. | Should |

### 1.5 Interface et contrôle (UI)

| ID | Exigence | Priorité |
|---|---|---|
| UI-01 | Panneaux réordonnables par glisser-déposer, avec ordre persisté. | Should |
| UI-02 | Édition directe de chaque champ, avec affichage de l'unité et de la borne. | Must |
| UI-03 | Cartes 2D interactives : marqueur navire, marqueur destination, waypoints, trace. | Should |
| UI-04 | Clic droit sur la carte = destination ; clic gauche = suivi de trace ; le comportement doit être documenté dans l'UI. | Must |
| UI-05 | Clavier : flèches ±1, conditionnées aux verrous, avec repli seguro hors conditions. | Must |
| UI-06 | Manette : axes et boutons mappés sur le **modèle** (barre, régime, cap, route), jamais sur une phrase. | Must |
| UI-07 | Terminal utilisateur affichant les phrases envoyées et acceptant l'injection. | Must |
| UI-08 | API de contrôle distante (HTTP ou WebSocket) pour l'automatisation de bancs d'essai. | Should |
| UI-09 | Démarrage automatique, arrêt propre, et reprise d'une session sauvegardée. | Must |
| UI-10 | Accessibilité : navigation clavier, contrastes, libellés. | Should |

### 1.6 Persistance (CFG)

| ID | Exigence | Priorité |
|---|---|---|
| CFG-01 | Configuration versionnée avec migrations testées. | Must |
| CFG-02 | Un champ absent ne doit jamais être perdu au chargement. | Must |
| CFG-03 | La config doit être exportable/importable (fichier) pour les bancs d'essai. | Should |
| CFG-04 | Format de stockage documenté et stable. | Must |

## 2. Exigences non fonctionnelles

| ID | Exigence | Cible |
|---|---|---|
| NFR-01 | Startup vers premiers paquets | < 2 s |
| NFR-02 | Overhead CPU à 1 Hz, 7 transports actifs | < 5 % d'un cœur |
| NFR-03 | Perte de tick sous charge | 0 sur 24 h à 10 Hz |
| NFR-04 | Encombrement mémoire | < 300 Mo |
| NFR-05 | Dépendances natives | **aucune** (pas de build natif hors gestionnaire de fichiers et sockets) |
| NFR-06 | Plateformes | Linux, Windows, macOS |
| NFR-07 | Le noyau doit tourner headless, sans navigateur ni rendu. | obligatoire |
| NFR-08 | Tous les encodeurs doivent être couverts par des tests d'égalité sur captures réelles. | obligatoire |
| NFR-09 | Les défauts legacy documentés doivent être couvrables par un test. | obligatoire |
| NFR-10 | Aucune dépendance à l'accès au réseau au démarrage. | obligatoire |

## 3. Contraintes de compatibilité

- Les valeurs numériques de `server.type` de la config legacy doivent être
  acceptées (migration) mais un nom doit être la forme canonique.
- Le format de journal `.nmeasim` doit rester lisible par le legacy et
  réécrivable par le nouveau.
- Les phrases de référence capturées sur le legacy servent de **golden files**
  pour les tests de non-régression.
- Un client réel doit pouvoir se connecter sans configuration spécifique au
  simulateur.

## 4. Hors exigence, par choix

- Recalculer la dérive ascendante du legacy n'est pas un défaut à reproduire :
  c'est une restriction du modèle, traitée en SIM-06.
- Le hello Signal K `17.12.05` est un défaut, pas une contrainte : traité en
  OUT-08.
- L'encodage corrompu de RMB n'est pas reproductible : traité en OUT-10.
