#!/usr/bin/env bash
# C2 Context MCP — schema tolerant graph reader
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  echo '{"error":"Usage: c2_context.sh <symbol> [depth]"}' >&2
  exit 1
}

error_json() {
  local message="$1"
  jq -n --arg msg "$message" '{"error": $msg}'
  exit 1
}

[[ $# -ge 1 ]] || usage

SYMBOL="$1"
DEPTH="${2:-1}"

case "$DEPTH" in
  *[!0-9]*|'') error_json "Depth must be a non-negative integer" ;;
esac

command -v jq >/dev/null 2>&1 || error_json "jq is required but not installed"

SYMBOL_INDEX="$(mktemp)"
VISITED_NODES="$(mktemp)"
ENQUEUED="$(mktemp)"
NODES_FILE="$(mktemp)"
LINKS_FILE="$(mktemp)"

cleanup() {
  rm -f "$SYMBOL_INDEX" "$VISITED_NODES" "$ENQUEUED" "$NODES_FILE" "$LINKS_FILE"
}
trap cleanup EXIT

#############################################
# INDEX
#############################################
build_symbol_index() {
  while IFS= read -r json_file; do
    sym="$(basename "$json_file" .c2.json)"

    if [[ -n "$sym" ]]; then
      printf '%s\t%s\n' "$sym" "$json_file" >> "$SYMBOL_INDEX"
    fi
  done < <(find "$PROJECT_ROOT" -name "*.c2.json" -type f 2>/dev/null)
}

lookup_symbol_file() {
  awk -F '\t' -v sym="$1" '$1 == sym { print $2; exit }' "$SYMBOL_INDEX"
}

#############################################
# STATE
#############################################
node_seen() { grep -Fxq "$1" "$VISITED_NODES" 2>/dev/null; }
mark_node_seen() { printf '%s\n' "$1" >> "$VISITED_NODES"; }

queue_seen() { grep -Fxq "$1" "$ENQUEUED" 2>/dev/null; }
mark_queue_seen() { printf '%s\n' "$1" >> "$ENQUEUED"; }

#############################################
# SAFE NODE PARSER (NÚCLEO DA EVOLUÇÃO)
#############################################
add_node_from_file() {
  local sym="$1"
  local json_file="$2"

  jq -c \
    --arg id "$sym" \
    '
    {
      id: $id,

     file: $id, 

      semantic: (
        .semantic //
        {
          what: .what // null,
          why: .why // .reasoning // null,
          goal: .goal // null,
          summary: .summary // null
        }
      ),

      links: (.links // []),

      functions: (.functions // [])
    }
    ' "$json_file" >> "$NODES_FILE"
}

add_warning_node() {
  jq -nc \
    --arg id "$1" \
    '{
      id: $id,
      file: null,
      status: "warning",
      message: "No .c2.json found for this symbol",
      semantic: { what: null, why: null, goal: null, summary: null },
      links: [],
      functions: []
    }' >> "$NODES_FILE"
}

#############################################
# GRAPH LOGIC
#############################################
ensure_node() {
  local sym="$1"

  if node_seen "$sym"; then
    return 0
  fi
  mark_node_seen "$sym"

  json_file="$(lookup_symbol_file "$sym")"

  if [[ -n "$json_file" && -f "$json_file" ]]; then
    add_node_from_file "$sym" "$json_file"
  else
    add_warning_node "$sym"
  fi
}

add_link() {
  jq -nc \
    --arg from "$1" \
    --arg to "$2" \
    '{ from: $from, to: $to, type: "uses" }' >> "$LINKS_FILE"
}

#############################################
# START
#############################################
build_symbol_index

ROOT_FILE="$(lookup_symbol_file "$SYMBOL")"
if [[ -z "$ROOT_FILE" || ! -f "$ROOT_FILE" ]]; then
  error_json "Symbol not found: $SYMBOL"
fi

QUEUE="${SYMBOL}|0"
ensure_node "$SYMBOL"
mark_queue_seen "${SYMBOL}|0"

#############################################
# BFS LOOP
#############################################
while [[ -n "$QUEUE" ]]; do
  current="${QUEUE%% *}"
  QUEUE="${QUEUE#* }"
  QUEUE="${QUEUE#"${QUEUE%%[![:space:]]*}"}"

  current_sym="${current%%|*}"
  current_level="${current##*|}"

  [[ "$current_level" -ge "$DEPTH" ]] && continue

  json_file="$(lookup_symbol_file "$current_sym")"
  [[ -z "$json_file" || ! -f "$json_file" ]] && continue

  # LINKS (tolerante)
  while IFS= read -r linked; do
    [[ -n "$linked" ]] || continue

    add_link "$current_sym" "$linked"
    ensure_node "$linked"

    queue_key="${linked}|$((current_level + 1))"
    if ! queue_seen "$queue_key"; then
      mark_queue_seen "$queue_key"
      QUEUE="$QUEUE $queue_key"
    fi
  done < <(
    jq -r '
      (.links // [])[],
      (.dependencies // [])[],
      (.uses // [])[]
    ' "$json_file" 2>/dev/null || true
  )

done

#############################################
# OUTPUT
#############################################
nodes_array="[]"
links_array="[]"

[[ -s "$NODES_FILE" ]] && nodes_array="$(jq -s '.' "$NODES_FILE")"
[[ -s "$LINKS_FILE" ]] && links_array="$(jq -s '.' "$LINKS_FILE")"

jq -n \
  --arg root "$SYMBOL" \
  --argjson depth "$DEPTH" \
  --argjson nodes "$nodes_array" \
  --argjson links "$links_array" \
  '{
    root: $root,
    depth: $depth,
    nodes: $nodes,
    links: $links
  }'