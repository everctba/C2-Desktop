#!/usr/bin/env bash
# Build completo do C2 no Linux (testado em Ubuntu/Mint) e pacote dist/linux.
#
# Uso:
#   bash scripts/build-linux.sh                  # CLI + desktop + runtime (Vulkan)
#   bash scripts/build-linux.sh --install-deps   # instala dependencias via apt (sudo)
#   bash scripts/build-linux.sh --no-desktop     # so CLI + runtime (sem Tauri)
#   FLAVOR=cpu bash scripts/build-linux.sh       # runtime apenas CPU
#
# Saida: dist/linux/{C2, cli/c2, runtime/, LEIA-ME.txt, CHECKSUMS.txt}
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

WITH_DESKTOP=1
INSTALL_DEPS=0
FLAVOR="${FLAVOR:-vulkan}"

for arg in "$@"; do
    case "$arg" in
        --no-desktop) WITH_DESKTOP=0 ;;
        --install-deps) INSTALL_DEPS=1 ;;
        --flavor=*) FLAVOR="${arg#*=}" ;;
        -h|--help)
            sed -n '2,16p' "$0"
            exit 0
            ;;
        *)
            echo "argumento desconhecido: $arg" >&2
            exit 2
            ;;
    esac
done

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "faltando: $1${2:+ ($2)}" >&2
        return 1
    fi
}

DEPS_APT=(
    build-essential pkg-config curl wget file tar unzip
    libssl-dev
)

if [ "$WITH_DESKTOP" = "1" ]; then
    DEPS_APT+=(
        libwebkit2gtk-4.1-dev
        libgtk-3-dev
        libayatana-appindicator3-dev
        librsvg2-dev
        patchelf
    )
fi

if [ "$INSTALL_DEPS" = "1" ]; then
    echo "instalando dependencias de sistema (sudo)..."
    sudo apt-get update
    sudo apt-get install -y "${DEPS_APT[@]}"
fi

echo "== 1/5 checando ferramentas =="
need cargo "instale o Rust: https://rustup.rs" || exit 1
need cc "build-essential" || exit 1
if [ "$WITH_DESKTOP" = "1" ]; then
    need node "instale Node 20+ (nvm ou https://nodejs.org)" || exit 1
    need pnpm "corepack enable && corepack prepare pnpm@10 --activate" || exit 1
    if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
        echo "aviso: webkit2gtk-4.1 nao encontrado." >&2
        echo "       Mint 21/Ubuntu 22.04 pode precisar do Mint 22/Ubuntu 24.04" >&2
        echo "       (o CLI continua funcionando: use --no-desktop)" >&2
    fi
fi
echo "cargo: $(cargo --version)"

echo "== 2/5 compilando Core + CLI (release) =="
cargo build --workspace --release

echo "== 3/5 baixando runtime do llama.cpp ($FLAVOR) =="
FLAVOR="$FLAVOR" bash scripts/fetch-runtime.sh

echo "== 4/5 montando dist/linux =="
OUT="$ROOT/dist/linux"
rm -rf "$OUT"
mkdir -p "$OUT/cli"
cp "$ROOT/target/release/c2" "$OUT/cli/c2"
cp -a "$ROOT/build/runtime-linux" "$OUT/runtime"
chmod +x "$OUT/cli/c2"

if [ "$WITH_DESKTOP" = "1" ]; then
    echo "== 4b/5 compilando o desktop (Tauri) =="
    cd "$ROOT/apps/desktop"
    pnpm install --frozen-lockfile
    pnpm tauri build --no-bundle
    APP=""
    for candidato in \
        "$ROOT/apps/desktop/src-tauri/target/release/c2-desktop" \
        "$ROOT/apps/desktop/src-tauri/target/release/C2"; do
        if [ -f "$candidato" ]; then APP="$candidato"; break; fi
    done
    if [ -z "$APP" ]; then
        echo "aviso: binario do desktop nao encontrado; pacote sai so com o CLI" >&2
    else
        cp "$APP" "$OUT/C2"
        chmod +x "$OUT/C2"
    fi
    cd "$ROOT"
fi

echo "== 5/5 LEIA-ME, checksums e smoke test =="
cat > "$OUT/LEIA-ME.txt" <<'TXT'
C2 - pacote Linux
=================

O que tem aqui
--------------
C2              app desktop (janela)
cli/c2          CLI do Core (init, status, grep, related, read)
runtime/        llama-server oficial do llama.cpp (Vulkan + CPU)

Requisitos
----------
- Um modelo .gguf no seu disco (o C2 NAO copia o modelo).
- Desktop: webkit2gtk 4.1 (Mint 22 / Ubuntu 24.04) e um servidor grafico.
- Vulkan: libvulkan1 + driver da GPU (a CPU e usada como fallback).

Primeiro uso
------------
./cli/c2 --root /caminho/do/projeto init
./cli/c2 --root /caminho/do/projeto config set model.path /caminho/modelo.gguf
./cli/c2 --root /caminho/do/projeto config set model.port 8099
./cli/c2 --root /caminho/do/projeto reindex   # indexa e para o runtime
./cli/c2 --root /caminho/do/projeto status

Abra a janela:
./C2 --root /caminho/do/projeto

O runtime vai junto: nao precisa de LM Studio nem de CUDA.
TXT

(
    cd "$OUT"
    sha256sum C2 cli/c2 runtime/llama-server 2>/dev/null > CHECKSUMS.txt || true
)

chmod +x "$OUT/cli/c2" 2>/dev/null || true
"$OUT/cli/c2" --version

TMP_PROJ="$(mktemp -d)"
trap 'rm -rf "$TMP_PROJ"' EXIT
printf 'fn main() {}\n' > "$TMP_PROJ/main.rs"
"$OUT/cli/c2" --root "$TMP_PROJ" init >/dev/null
"$OUT/cli/c2" --root "$TMP_PROJ" status >/dev/null
"$OUT/cli/c2" --root "$TMP_PROJ" reindex --dry-run >/dev/null
"$OUT/cli/c2" --root "$TMP_PROJ" cleaner >/dev/null
echo "smoke test: ok (init/status/reindex --dry-run/cleaner)"

echo
echo "pronto: $OUT ($(du -sh "$OUT" | cut -f1))"
ls -1 "$OUT"
