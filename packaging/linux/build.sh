#!/usr/bin/env bash
# Construit nmeasim pour Linux : binaire release et paquet .deb.
#
# usage : packaging/linux/build.sh [--no-deb] [--install-deps]
#   --no-deb        binaire seul (target/release/nmeasim)
#   --install-deps  installe les dépendances système (apt) et cargo-deb
set -euo pipefail

cd "$(dirname "$0")/../.."

DEB=1
DEPS=0
for a in "$@"; do
  case "$a" in
    --no-deb) DEB=0 ;;
    --install-deps) DEPS=1 ;;
    -h|--help) sed -n '2,7p' "$0"; exit 0 ;;
    *) echo "option inconnue : $a" >&2; exit 2 ;;
  esac
done

command -v cargo >/dev/null || { echo "cargo introuvable : installer Rust (https://rustup.rs)" >&2; exit 1; }

if [ "$DEPS" = 1 ]; then
  sudo apt-get update
  sudo apt-get install -y pkg-config libudev-dev libgtk-3-dev libxkbcommon-dev
fi

# Vérification des bibliothèques de développement nécessaires.
missing=()
for p in libudev gtk+-3.0 xkbcommon; do
  pkg-config --exists "$p" 2>/dev/null || missing+=("$p")
done
if [ ${#missing[@]} -gt 0 ]; then
  echo "bibliothèques manquantes : ${missing[*]}" >&2
  echo "relancer avec --install-deps, ou : sudo apt install pkg-config libudev-dev libgtk-3-dev libxkbcommon-dev" >&2
  exit 1
fi

echo "==> compilation release"
cargo build --release -p nmeasim
echo "binaire : target/release/nmeasim ($(target/release/nmeasim --version))"

if [ "$DEB" = 1 ]; then
  if ! command -v cargo-deb >/dev/null; then
    if [ "$DEPS" = 1 ]; then
      cargo install cargo-deb --locked
    else
      echo "cargo-deb introuvable : relancer avec --install-deps, ou : cargo install cargo-deb" >&2
      exit 1
    fi
  fi
  echo "==> paquet .deb"
  DEB_PATH=$(cargo deb -p nmeasim | tail -1)
  echo "paquet : $DEB_PATH"
  echo "installation : sudo apt install ./${DEB_PATH#"$PWD"/}"
fi
