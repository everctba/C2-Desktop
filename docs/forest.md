# Floresta teleológica

Tasks `c2-jgf.1` a `c2-jgf.6`.

## Artefatos

```text
.c2/folders/<dominio>.json   resumo de dominio/pasta
.c2/forest.json              master do projeto
.c2/embeddings/.../forest/   vetores por secao
```

## Folder

```json
{
  "schema_version": 1,
  "path": "src/pagamento",
  "teleology": {
    "what": "domínio de cobrança",
    "why": "isolar pagamento do pedido",
    "goal": "confirmar antes de avançar"
  },
  "boundaries": ["não define status do pedido"],
  "key_files": ["src/pagamento/servico.rs"],
  "depends_on": ["src/pedido"],
  "sources": ["src/pagamento/servico.rs.c2.json"],
  "subtree_hash": "sha256:...",
  "generated_at": "...",
  "confidence": "medium"
}
```

O `subtree_hash` cobre os hashes dos metadados da subárvore. Se não mudou, o
folder é reaproveitado.

## Forest

`.c2/forest.json` contém:

- `teleology`: what/why/goal/who/success/non_goals;
- `architecture`: summary/layers/entrypoints;
- `domains`: resumo por domínio;
- `decisions`: ADRs + `agent_notes` (teleology/invariant);
- `risks`: `agent_notes` (gotcha/correction/pending);
- `divergences`: reservado para docs × código;
- `sources`: hash/mtime de README/PRD/ADRs;
- `section_hashes`: hashes independentes de visão, arquitetura, domínios,
  decisões e riscos.

## CLI

```bash
c2 forest build
c2 forest build --embed
c2 forest overview
c2 forest domain src/pagamento
c2 forest brief --domain src/pagamento
c2 forest brief --task "corrigir retry"
```

## MCP

- `c2_overview`
- `c2_domain`
- `c2_brief`
- `c2_decisions`

## Atualização incremental

```text
hash dos metadados
  → subtree_hash da pasta
  → forest
  → section_hash
  → embeddings só das seções alteradas
```

Hoje a geração é determinística a partir dos metadados/docs, sem chamada de
modelo. Uma síntese mais rica por LLM fica como evolução.
