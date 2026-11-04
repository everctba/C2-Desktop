#!/usr/bin/env bash
# Baixa o runtime oficial do llama.cpp para Linux em build/runtime-linux.
#
# Uso:
#   bash scripts/fetch-runtime.sh              # Vulkan x64 (GPU, com fallback CPU)
#   FLAVOR=cpu bash scripts/fetch-runtime.sh   # apenas CPU
#   VERSION=b12110 bash scripts/fetch-runtime.sh
#
# Os assets de Linux sao .tar.gz (no Windows sao .zip).
set -euo pipefail

FLAVOR="${FLAVOR:-vulkan}"
VERSION="${VERSION:-}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/build/runtime-linux"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [ -z "$VERSION" ]; then
    echo "consultando o ultimo build do llama.cpp..."
    VERSION="$(curl -fsSL 'https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10' \
        | grep -o '"tag_name": *"b[0-9]*"' | head -1 | grep -o 'b[0-9]*')"
fi
if [ -z "$VERSION" ]; then
    echo "erro: nao encontrei um build (tag b####) do llama.cpp" >&2
    exit 1
fi

ASSET="llama-${VERSION}-bin-ubuntu-${FLAVOR}-x64.tar.gz"
URL="https://github.com/ggml-org/llama.cpp/releases/download/${VERSION}/${ASSET}"

echo "baixando $ASSET ..."
curl -fL --progress-bar -o "$TMP/runtime.tar.gz" "$URL"
tar -xzf "$TMP/runtime.tar.gz" -C "$TMP"

SERVER="$(find "$TMP" -maxdepth 3 -name llama-server -type f | head -1)"
if [ -z "$SERVER" ]; then
    echo "erro: llama-server nao encontrado no pacote baixado" >&2
    exit 1
fi

rm -rf "$DEST"
mkdir -p "$DEST"
# achata a pasta interna (llama-bXXXX/) para a raiz de runtime-linux
cp -a "$(dirname "$SERVER")/." "$DEST/"
chmod +x "$DEST"/llama-* 2>/dev/null || true
echo "$FLAVOR $VERSION" > "$DEST/.version"

echo "runtime pronto em $DEST ($(du -sh "$DEST" | cut -f1))"
"$DEST/llama-server" --version | head -2 || true
echo "dica: o C2 procura o runtime em <pasta do C2>/runtime (ou cli/../runtime)"
