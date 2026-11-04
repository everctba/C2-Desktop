c2grep() {
  local query="$1"

  echo "🔎 Query original: $query"

  expanded=$(expand_query "$query")

  echo "🧠 Expansão semântica:"
  echo "$expanded" | jq .

  terms=$(echo "$expanded" | jq -r '.expanded[]')

  find . -name "*.c2.json" | while read file; do

    content=$(cat "$file")

    score=0

    for term in $terms; do
      if echo "$content" | grep -iq "$term"; then
        score=$((score + 1))
      fi
    done

    if [ "$score" -gt 0 ]; then
      echo "MATCH ($score) -> $file"
    fi

  done
}