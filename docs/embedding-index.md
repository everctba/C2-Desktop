# Indexação de embeddings

Task `c2-kga.4`.

## Fluxo

```text
.c2.json → MetaFile::read
  → hash + model_id + dim
  → cache hit? reaproveita
  → senão: text_for(meta) → embedding runtime → cache.store
```

## Comando

```bash
c2 embeddings index          # incremental
c2 embeddings index --full   # ignora cache e re-embedda tudo
c2 embeddings index --json   # relatório máquina
```

Relatório:

```json
{
  "files": 173,
  "embedded": 172,
  "cached": 0,
  "skipped": 0,
  "errors": 1,
  "last_error": "...",
  "model_id": "lm-kit/bge-m3-gguf",
  "dim": 1024
}
```

## Regras

- arquivo sem campos semânticos ou sem hash é pulado;
- cada erro tem até 2 retries com backoff;
- dimensão errada não é gravada no cache;
- prune remove vetores de arquivos que não existem mais;
- `--full` não apaga o cache antes; apenas ignora hits e reescreve.

## Teste real

Rodado no workspace do C2 com 173 metadados:

```text
embeddings: 173 arquivos, 172 gerados, 0 em cache, 0 pulados, 1 erro
```

O cache ficou em `.c2/embeddings/lm-kit-bge-m3-gguf-1024d/`.
O erro isolado foi transitório do runtime; o retry foi adicionado depois.

## Integração

- CLI: `c2 embeddings index`;
- Tauri: comando `embedding_index` e botão **Indexar embeddings** em Config;
- reindex manual/engine: ainda falta chamar automaticamente após um ciclo
  quando `index.embeddings.enabled=true`.
