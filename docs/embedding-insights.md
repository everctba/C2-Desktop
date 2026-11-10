# Inteligência semântica

Tasks `c2-kga.6`..`c2-kga.9`.

## Similares + domínios

```bash
c2 embeddings similar crates/c2-core/src/search.rs --limit 10
c2 embeddings similar crates/c2-core/src/search.rs --domains
```

Saída real:

```text
0.805  crates/c2-core/src/cleaner.rs
0.773  crates/c2-core/src/discover.rs
0.756  crates/c2-core/src/indexer.rs
[c2-core] cleaner.rs, discover.rs, indexer.rs...
```

Domínio: primeiro diretório estrutural (`src/<dominio>`, `crates/<dominio>`).

## Duplicados/genéricos

```bash
c2 embeddings duplicates --threshold 0.90
```

Retorna:

- pares com cosine acima do threshold;
- flag `linked` se o grafo já tem link;
- metadados genéricos: resumo com menos de 32 caracteres ou `what` com
  menos de 8.

> Threshold 0.90 gera falsos positivos de estrutura (Cargo.toml, CSS). Para
> duplicados reais, usar 0.97+ e confirmar no código.

## Links sugeridos

```bash
c2 embeddings links --threshold 0.90 --limit 20
```

Só sugere pares fortes que **ainda não estão ligados** no grafo.

## Co-mudança

```bash
c2 embeddings cochange crates/c2-core/src/search.rs --limit 10
```

Usa `git log --name-only --since=1.year`; conta commits em que os dois arquivos
aparecem juntos. Exemplo real:

```text
3 commits  .beads/issues.jsonl
2 commits  crates/c2-cli/src/main.rs
2 commits  crates/c2-core/src/config.rs
```

## Limitações

- similares/duplicados dependem do cache de embeddings já gerado;
- co-mudança depende do histórico git e de caminhos relativos;
- ainda não há UI dedicada (task `c2-kga.10`) nem comandos MCP.
