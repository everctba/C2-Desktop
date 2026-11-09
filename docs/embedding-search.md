# Busca híbrida (lexical + cosine)

Task `c2-kga.5`.

## Como funciona

```text
query
  ├─ lexical atual (path, símbolo, what/why/goal/summary/responsibility)
  └─ embedding da query
        → cosine com vetores do cache
        → se cosine >= min_score, entra como candidato

score = path + symbol + semantic + 2.0 × embedding
```

- o grep lexical continua sendo a base; embeddings são um sinal adicional;
- se o runtime de embeddings estiver desligado/indisponível, a busca cai no
  lexical sem erro;
- `embedding_score` só aparece em resultados híbridos.

## CLI

```bash
c2 grep "onde o pedido e cancelado" --limit 5 --hybrid
c2 grep "onde o pedido e cancelado" --limit 5 --semantic   # alias
c2 grep "..." --hybrid --json
```

## Tauri

`search_grep` aceita `hybrid` opcional. Na aba **Busca**, o toggle
**Híbrida** liga o cosine; cada resultado mostra `sem X.XX` e o pill
`semântico`.

## Configuração

```json
"index": {
  "embeddings": {
    "enabled": true,
    "min_score": 0.35
  }
}
```

`min_score` é calibrável; 0.35 é o ponto de partida.

## Teste real

Workspace do C2, consulta `"onde o pedido e cancelado"`:

```text
lexical: 4 hits
híbrido: 5 hits
```

Acima do top-3 lexical, os hits híbridos ganharam `embedding_score` entre
0.39 e 0.44 e subiram no ranking.

## Pendências da task

- flag `hybrid` no MCP;
- benchmark recall@5 com threshold calibrado;
- peso do embedding configurável (hoje constante 2.0).
