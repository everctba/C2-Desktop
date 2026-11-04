# Migração do protótipo bash → implementação Rust: divergências

**Bead:** `c2-e11t4` · **Data:** 2026-11-04
**Evidência:** testes de `crates/c2-core/src/schema.rs` e `migrate.rs`; `c2 migrate`.

## Por que não é preciso rodar o protótipo bash

A lógica de indexação é a implementação Rust — o protótipo é legado e já foi
portado (`c2-e11t2`, `c2-e11t3`). Rodar o bash de novo compararia **textos
gerados pelo mesmo modelo com prompts diferentes**, o que não muda nenhuma
decisão de produto nem de migração. O que importa — e está coberto — é:

1. o formato antigo **é lido** sem erro (leitor tolerante);
2. `c2 migrate` **converte** para o schema v1 com backup reversível;
3. nada de informação semântica se perde na conversão.

Os `.c2.json` que restaram em `Base/` já estão no formato v1 (foram
regenerados pelo Rust), então também não há amostra antiga para comparar.

## Divergências estruturais (bash → v1)

| Campo | Protótipo bash | Schema v1 | Migração |
|---|---|---|---|
| `schema_version` | ausente | `1` | adicionado |
| `what` | **não existia** | texto do "o que é" | usa o `when` antigo quando `what` está vazio |
| `when` | "quando/onde é usado" | — | **conteúdo preservado em `what`** |
| `responsibility` | não existia | fronteira de responsabilidade | vazio até o próximo `c2 reindex` |
| `links[]` | não existia | arestas estáticas | extraído no próximo `c2 reindex` (sem LLM) |
| `size` | não existia | bytes | preenchido (0 se ausente; corrigido no reindex) |
| `hash` | hex puro | `sha256:<hex>` | prefixo normalizado |
| `file` | `./src/a.rs` | `src/a.rs` | caminho relativo sem `./` |
| backup | nenhum | `.c2/backups/<epoch>/` | criado antes de sobrescrever |

Campos semânticos que **não mudam**: `why`, `goal`, `summary` são preservados
exatamente como estavam.

## Divergência encontrada e corrigida nesta tarefa

O leitor aceitava `when` mas **descartava o conteúdo** (`_legacy_when` não era
usado). Como o v1 não tem campo equivalente, migrar perderia informação
semântica. Correção:

```rust
// `when` do protótipo passa a preencher `what` quando `what` está vazio
let what = if value.is_empty() { legacy_when.unwrap_or_default() } else { value };
```

Coberto por dois testes novos:

- `when_do_prototipo_preserva_o_conteudo_em_what` (leitura);
- `when_legado_vira_what_sem_perda` (migração real com backup).

## O que NÃO é divergência (é regeneração)

Qualidade do texto (`why`/`goal`/`summary`, novidade do `what` e dos `links`) é
**saída do modelo**, não do formato. Depois do `c2 reindex` o metadado v1 é
reescrito pelo pipeline novo — os textos antigos servem como fallback até lá.

## Como validar em um projeto legado

```powershell
c2 migrate              # dry-run: conta e mostra o que seria convertido
c2 migrate --yes        # converte com backup em .c2/backups/<epoch>/
c2 reindex              # regenera what/responsibility/links com o pipeline novo
```

Reversão: copiar os arquivos de `.c2/backups/<epoch>/` de volta.
