# Memória de decisões (`agent_notes`)

Task `c2-kf0`.

## Objetivo

Responder perguntas como:

```text
por que o retry do pagamento é assim?
qual decisão tomamos sobre cache?
onde já documentamos esse gotcha?
```

usando as anotações de agentes como fonte separada do resumo do arquivo.

## Cache

```text
.c2/embeddings/<modelo>-<dim>d/notes/<arquivo>-<hash-da-nota>.json
```

- hash da nota = arquivo + at + kind + texto;
- mudar a nota gera só o vetor dela;
- o vetor do `.c2.json` do arquivo não é alterado.

## CLI

```bash
c2 decisions "por que o retry é assim"
c2 decisions "cache" --index
c2 decisions "cache" --json
```

`--index` gera/atualiza os vetores das notas. Sem runtime de embeddings, a busca
cai no lexical.

## MCP

```text
c2_decisions { "query": "por que o retry é assim?", "limit": 10 }
```

Retorno: arquivo, kind, texto, score lexical/cosine.

## Tipos

- `teleology`: propósito;
- `invariant`: regra que não pode quebrar;
- `gotcha`: armadilha;
- `correction`: interpretação corrigida;
- `pending`: dúvida pendente;
- `note`: genérica.
