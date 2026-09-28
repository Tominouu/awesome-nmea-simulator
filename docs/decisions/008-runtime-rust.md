# D8 — Langage et runtime : Rust

| | |
|---|---|
| Statut | Acceptée (à ratifier par l'utilisateur) |
| Date | 2026-09-28 |
| Auteur | session P2/P3 (agent) |
| Exigences | NFR-01 à NFR-07, NFR-10 |
| Documents | `12-new-architecture.md` §1-2, `05-architecture.md` |

Décision hors de la liste P3, consignée conformément à
`REIMPLEMENTATION_PLAN.md` §9. Elle devait être tranchée avant J0.

## Contexte

`12-new-architecture.md` §1 proposait « Node LTS courant ». La demande de
l'utilisateur évoque Rust, le dépôt s'appelle `nmeasim-rs`, et la machine de
développement dispose de Rust 1.96 et de `cargo-tauri`.

## Comportement legacy

Electron 24 (Chromium 112, Node 18), TypeScript compilé en un bundle Angular ;
noyau, UI et sockets dans le même processus renderer ; `Math.random()` et
`Date.now()` globaux.

## Options

1. **Node LTS + TypeScript** : proche du legacy, écosystème `serialport`
   natif (module compilé), runtime à embarquer.
2. **Rust**, cœur en bibliothèques, UI web légère sur Tauri.
3. Cœur Rust, UI et transports en Node : deux chaînes d'outils, rejeté.

## Décision

Option 2, **Rust stable** (édition 2024, MSRV 1.85), en workspace Cargo.

Ordre de construction imposé : `core` → modèle de domaine → simulation →
sorties (encodeurs) → transports → UI. J0 ne contient que `core` (`units`,
`random`) et l'outillage.

## Justification

- Conforme à la demande de l'utilisateur et au nom du dépôt.
- Binaire unique sans runtime JavaScript : NFR-01 (< 2 s) et NFR-04
  (< 300 Mo) tenus sans effort ; NFR-05 : aucune dépendance native hors
  sockets et série.
- Le noyau headless (NFR-07) est une bibliothèque sans aucune dépendance d'UI,
  vérifiée par le graphe de crates plutôt que par un audit d'imports.
- Déterminisme : PRNG et horloge injectés par le système de types ; aucune
  source globale d'aléa n'est accessible au noyau.
- L'exigence d'un seul écrivain de l'état (`12-new-architecture.md` §5.1) est
  exprimable par la propriété Rust : seul le module `api` détient `&mut State`.
- L'UI web légère recommandée par `12` §1.1 reste possible via Tauri.

## Compatibilité

Aucun effet sur le câble : la compatibilité se prouve par les golden files,
indépendamment du langage. Le harnais de capture (Node, `tools/legacy-harness/`)
reste en Node, car il pilote l'Electron legacy par CDP.

## Conséquences

- `12-new-architecture.md` §1-2 est réécrit : crates au lieu de paquets npm.
- Les exemples TypeScript de `05`, `06`, `08` et `09` restent des
  **pseudo-code** de spécification ; la forme normative de l'API est celle de
  `09-inputs-and-ui.md` §7.
- Couverture mesurée par `cargo llvm-cov` ; CI GitHub Actions avec
  `fmt`, `clippy -D warnings`, tests, couverture.
- Crates candidats, à valider au jalon qui les utilise : `serialport` (J4),
  `tokio` (J4), `gilrs` pour la manette (J8), `tauri` (J8). Aucun en J0.

## Tests à prévoir

| Test | Attendu |
|---|---|
| `ci:build` | `cargo build --workspace` sur Linux, Windows, macOS |
| `ci:lint` | `cargo fmt --check`, `cargo clippy -- -D warnings` |
| `arch:core-deps` | `nmeasim-core` n'a aucune dépendance hors `std` |
| `coverage:j0` | 100 % des lignes de `core::units` et `core::random` |
