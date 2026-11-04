#!/bin/bash
set -e

#############################################
# CONFIG
#############################################

LLM_URL="http://localhost:1234/v1/chat/completions"
MODEL="local-model"

WATCH_DIR="./"

#############################################
# LLM CALL
#############################################
llm() {
  local prompt="$1"

  payload=$(mktemp)

  jq -n \
    --arg model "$MODEL" \
    --arg content "$prompt" \
    '{
      model: $model,
      messages: [
        {
          role: "user",
          content: $content
        }
      ],
      temperature: 0.3
    }' > "$payload"

  response=$(curl -s "$LLM_URL" \
    -H "Content-Type: application/json" \
    --data-binary @"$payload")

  rm -f "$payload"

  echo "$response" | jq -r '.choices[0].message.content // empty'
}
#############################################
# PROCESS FILE
#############################################

process_file() {
  file="$1"

  # ignora metadata gerada
  if [[ "$file" == *.c2.json ]]; then
    return
  fi

  json_file="${file}.c2.json"

  # hash do arquivo (MUDANÇA REAL)
  new_hash=$(sha256sum "$file" 2>/dev/null | awk '{print $1}')
  old_hash=$(jq -r '.hash // empty' "$json_file" 2>/dev/null || true)

  # se não existe ou mudou conteúdo
  if [ ! -f "$json_file" ] || [ "$new_hash" != "$old_hash" ]; then

    echo "Processando: $file"

    content=$(cat "$file")

    prompt="
Você é um system architect.

Analise o arquivo e retorne APENAS JSON válido.

REGRAS OBRIGATÓRIAS:
- sem markdown
- sem \`\`\`
- sem texto extra
- saída deve ser JSON puro

Formato:
{
  \"why\": \"...\",
  \"when\": \"...\",
  \"goal\": \"...\",
  \"summary\": \"...\"
}

Arquivo:
$content
"

    response=$(llm "$prompt")

    # valida JSON antes de salvar
    if echo "$response" | jq '.' >/dev/null 2>&1; then

      hash=$(sha256sum "$file" | awk '{print $1}')
      mtime=$(stat -c %Y "$file" 2>/dev/null || stat -f %m "$file")

      echo "$response" | jq \
        --arg file "$file" \
        --arg hash "$hash" \
        --arg mtime "$mtime" \
        '{
          file: $file,
          hash: $hash,
          mtime: ($mtime|tonumber),
          semantic: .
        }' > "$json_file"

      echo "OK -> $json_file"

    else
      echo "❌ JSON inválido do LLM: $file"
    fi
  fi
}

#############################################
# SCAN DIRECTORY
#############################################

scan() {
  find "$WATCH_DIR" \
    \( -path "*/node_modules" -o \
       -path "*/.git" -o \
       -path "*/dist" -o \
       -path "*/build" -o \
       -path "*/__pycache__" -o \
       -path "*/.beans" \) -prune \
    -o -type f ! -name "*.c2.json" -print |
  while IFS= read -r file; do

    basename=$(basename "$file")

    # EXCEÇÕES IMPORTANTES (config files)
    if [[ "$basename" == .*env* ]] || \
       [[ "$basename" == "config"* ]] || \
       [[ "$basename" == "*.config.*" ]] || \
       [[ "$basename" == ".gitignore" ]] || \
       [[ "$basename" == ".editorconfig" ]]; then

      process_file "$file"
      continue
    fi

    # ignora outros arquivos ocultos
    if [[ "$basename" == .* ]]; then
      continue
    fi

    process_file "$file"

  done
}

#############################################
# LIMPEZA DE ARQUIVOS DELETADOS
#############################################

cleanup_orphans() {
  find "$WATCH_DIR" -name "*.c2.json" | while read meta; do

    original="${meta%.c2.json}"

    if [ ! -f "$original" ]; then
      echo "🧹 Removendo órfão: $meta"
      rm -f "$meta"
    fi

  done
}

#############################################
# LE ARQUIVOS EXISTENTE PRA SABER SE ATUALIZOU
#############################################

read_existing_meta() {
  local json_file="$1"

  if [ -f "$json_file" ]; then
    cat "$json_file"
  else
    echo "{}"
  fi
}
#############################################
# LOOP INFINITO
#############################################

echo "Semantic indexer iniciado..."

while true; do
  scan
  cleanup_orphans
  sleep 5
done