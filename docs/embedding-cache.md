# Cache de embeddings

Task `c2-kga.2`.

## Layout

```text
<projeto>/.c2/embeddings/<modelo>-<dim>d/<arquivo-fonte>.json
```

Exemplo:

```text
.c2/embeddings/lm-kit-bge-m3-gguf-1024d/src/pedido/servico.rs.json
```

## Registro

```json
{
  "model_id": "lm-kit/bge-m3-gguf",
  "dim": 1024,
  "source_hash": "sha256:...",
  "vector": [0.0123, -0.0456, "..."]
}
```

- `source_hash` é o hash do `.c2.json` usado para gerar o vetor;
- `model_id` + `dim` evitam misturar modelos/dimensões;
- o vetor é normalizado antes de persistir.

## Invalidação

Na leitura, o cache só é aceito se:

1. `model_id` bate;
2. `dim` bate;
3. `source_hash` bate;
4. `vector.len() == dim`.

Caso contrário, retorna `None` e o arquivo será re-embedado.

## Escrita

- JSON simples (`serde_json`);
- arquivo temporário `.json.tmp`;
- `rename` atômico;
- vetor vazio, não finito ou com dimensão errada é rejeitado.

## Limpeza

`EmbeddingCache::prune(&existentes)` remove vetores de arquivos que não existem
mais. O reindex incremental só re-embedda arquivos com hash diferente.

## Texto embutido

`embedding::text_for(&MetaFile)` monta:

```text
resumo: ...
responsabilidade: ...
o que e: ...
por que existe: ...
objetivo: ...
relacionados: ...
```

Campos vazios são omitidos. O metadado continua sendo a fonte; o vetor é apenas
um índice derivado.

## Não persistir

- conteúdo bruto do código;
- segredos;
- embeddings de modelo/dimensão diferente.
