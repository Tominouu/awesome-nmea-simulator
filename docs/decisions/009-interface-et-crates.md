# D9 — Interface egui et découpage en sept crates

| | |
|---|---|
| Statut | Acceptée |
| Date | 2026-09-28 |
| Auteur | session d'implémentation (agent) |
| Exigences | UI-01 à UI-10, NFR-01, NFR-04, NFR-05, NFR-07 |
| Documents | `12-new-architecture.md`, D8 |

## Contexte

D8 retenait Rust et laissait l'interface « web légère dans Tauri, à confirmer
en J8 ». `12-new-architecture.md` prévoyait onze crates. La consigne de la
phase d'implémentation privilégie une application complète et utilisable à une
architecture parfaite.

## Comportement legacy

Electron + Angular + OpenLayers ; noyau, UI et sockets dans le même processus.

## Options

1. Tauri + front web (Svelte/Preact) : deux chaînes d'outils (Rust + npm),
   WebKitGTK à l'exécution, pont IPC à écrire.
2. **egui / eframe** : Rust pur, un seul binaire, rendu immédiat, test headless
   par l'état publié.
3. Iced ou Slint : moins de composants prêts (tableaux, glissières, zones de
   texte) pour l'effort attendu.

Découpage : onze crates (plan) ou sept.

## Décision

- Interface **egui/eframe 0.31** (rendu OpenGL via glow), dialogues de fichiers
  `rfd`, fond de carte OpenStreetMap facultatif (tuiles, `ureq`, `image`).
- **Sept crates** + le binaire : `core`, `sim` (modèle de domaine **et**
  simulation), `encode` (NMEA, AIS, Signal K, ViewSync), `transport`, `source`,
  `input`, `app` ; binaire `nmeasim` (crate `gui`).

## Justification

- Un seul binaire autonome (18 Mo), sans runtime web : NFR-01 et NFR-04 tenus.
- L'interface lit l'état publié et soumet des commandes ; elle n'a pas besoin
  d'un pont IPC.
- Réunir modèle et simulation évite une frontière artificielle entre deux
  crates qui se modifient toujours ensemble ; la règle « un seul écrivain de
  l'état » reste portée par le type `Simulator`.
- Les encodeurs NMEA, Signal K et ViewSync partagent le formatage numérique
  JavaScript (`toFixed`, nombres entiers) : un seul crate évite la duplication.

## Compatibilité

Aucun effet sur le câble.

## Conséquences

- `12-new-architecture.md` §2 est mis à jour.
- Sous Linux, l'exécution demande GTK 3 (dialogues) et udev (manette), déclarés
  en dépendances du `.deb` ; `fonts-dejavu-core` fournit les symboles absents
  des polices d'egui.

## Tests à prévoir

| Test | Attendu |
|---|---|
| `gui:cli` | analyse des options de ligne de commande |
| `gui:projection` | aller-retour Web Mercator |
| e2e interface | capture d'écran sous Xvfb (manuel, `docs/development.md`) |
