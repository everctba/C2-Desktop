#!/bin/bash
set -e

ROOT_DIR="${1:-.}"

echo "🧹 Removendo TODOS os .c2.json em: $ROOT_DIR"
echo "----------------------------------------"

find "$ROOT_DIR" -type f -name "*.c2.json" -print -delete

echo "----------------------------------------"
echo "✔ Limpeza completa finalizada"