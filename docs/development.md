# Développement

## Prérequis

- Rust stable ≥ 1.85 (édition 2024) ;
- Linux : `libudev-dev`, `libgtk-3-dev`, `libxkbcommon-dev`, `libgl1-mesa-dev`,
  `pkg-config` ;
- facultatif : `cargo-llvm-cov` (couverture), `cargo-deb` (paquet Debian).

```bash
cargo build --release          # target/release/nmeasim
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo cov                      # couverture de nmeasim-core (seuil 100 % lignes)
cargo deb -p nmeasim           # target/debian/nmeasim-rs_<v>_amd64.deb
```

## Architecture

```
            Clavier    Manette (gilrs)    GPX / KML / rejeu    API HTTP    Interface
               └───────────┴──────────────────┴──────────────────┴────────────┘
                                         │  Command { kind, source }
                                         ▼
                           nmeasim-app::Runtime (thread moteur)
                                         │  Simulator::submit / step
                                         ▼
                          nmeasim-sim::Simulator  (seul écrivain de l'état)
                                         │  SimState (instantané)
                  ┌──────────────────────┼───────────────────────┐
                  ▼                      ▼                       ▼
          encode::nmea (+AIS)     encode::signalk          encode::viewsync
                  └──────────────────────┼───────────────────────┘
                                         ▼  Frame (phrase / JSON / brut)
                  transport : tcp-server, tcp-client, udp-*, websocket, serial
```

| Crate | Rôle | Dépend de |
|---|---|---|
| `nmeasim-core` | unités SI, PRNG graineable, géodésie | `std` (+`serde` optionnel) |
| `nmeasim-sim` | état, commandes, profils, bruit, physique, pilote, route | core |
| `nmeasim-encode` | NMEA 0183, AIS, Signal K, ViewSync, analyseur | core, sim |
| `nmeasim-transport` | TCP, UDP, multicast, WebSocket, série | — |
| `nmeasim-source` | GPX, KML, lecteur de trace, journal, rejeu | core |
| `nmeasim-input` | manette et clavier → commandes (pur) | sim |
| `nmeasim-app` | configuration, migration, moteur, API HTTP, scénarios, pilote gilrs | tous |
| `nmeasim` | binaire : interface egui et mode `--headless` | app |

Règles : le noyau est headless ; l'interface ne lit que l'état publié et
n'écrit que des commandes ; un encodeur ne connaît pas les transports ; un
transport ne connaît pas la structure des phrases ; un seul producteur de
messages (live, trace ou rejeu).

## Tests

| Suite | Emplacement |
|---|---|
| unités, PRNG, géodésie | `crates/core` (couverture 100 % des lignes) |
| simulation (déterminisme, inertie, barre, pilote, route, courant, vent…) | `crates/sim/tests/simulation.rs` |
| NMEA, AIS, Signal K, ViewSync | `crates/encode/src` |
| **compatibilité legacy vs nouveau** sur les golden files | `crates/encode/tests/golden.rs` |
| transports sur sockets réels, PTY série, fuites de descripteurs | `crates/transport/tests/` |
| GPX, KML, journal, rejeu sur les fixtures P1/P2 | `crates/source/tests/sources.rs` |
| manette et clavier | `crates/input/src` |
| bout en bout (TCP, WebSocket, journal, rejeu, trace, API, migration, scénarios) | `crates/app/tests/end_to_end.rs` |

Les golden files `tests/fixtures/legacy*/` sont en lecture seule et vérifiés
par SHA-256 en CI.

## Harnais legacy

`tools/legacy-harness/` pilote NMEASimulator 1.6.1 (Electron) par le protocole
DevTools pour produire des captures de référence ; voir
`tests/fixtures/legacy*/README.md` et `docs/11-testing-strategy.md` §5.

## Manette sans matériel

```bash
python3 tools/gamepad/virtual_pad.py 3 &
cargo run -p nmeasim-app --example gamepad_probe -- 14
```

## Paquets

| Cible | Commande | Sortie |
|---|---|---|
| Linux .deb | `cargo deb -p nmeasim` | `target/debian/nmeasim-rs_<v>_amd64.deb` |
| Linux binaire | `cargo build --release` | `target/release/nmeasim` |
| Windows | `packaging/windows/make-zip.ps1` | archive portable `.zip` |
| macOS | `packaging/macos/make-app.sh` | `NMEA Simulator.app` et `.dmg` |

La CI (`.github/workflows/ci.yml`) construit et teste sur Linux, Windows et
macOS, et publie le `.deb` et les archives en artefacts.

## Tester l'interface sans écran

```bash
Xvfb :98 -screen 0 1600x1000x24 &
env -u WAYLAND_DISPLAY DISPLAY=:98 ./target/debug/nmeasim --config /tmp/test.json --start &
PID=$!
sleep 8 && DISPLAY=:98 import -window root capture.png
kill $PID
```

Dans une session Wayland, `WAYLAND_DISPLAY` doit être retiré, sinon la fenêtre
s'ouvre sur le bureau réel au lieu de Xvfb. Arrêter l'application par son PID :
un `pkill -f` dont le motif figure dans la ligne de commande courante tue aussi
le shell appelant.
