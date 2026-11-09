# Runtime de embeddings

Task `c2-kga.3`.

## Processo

O runtime é um `llama-server` **separado** do Gemma:

```text
llama-server --embeddings --pooling cls \
  --model bge-m3-Q8_0.gguf \
  --host 127.0.0.1 --port 8199 \
  --no-webui -c 8192 -ngl 0 -t 8
```

- CPU: `-ngl 0`;
- GPU: `-ngl <gpu_layers>`;
- logs: `.c2/logs/embedding.out.log` e `.c2/logs/embedding.err.log`;
- PID: `.c2/embedding.pid`;
- porta padrão: `8199`.

## Config

```json
"index": {
  "embeddings": {
    "enabled": false,
    "model_path": null,
    "model_id": "lm-kit/bge-m3-gguf",
    "dim": 1024,
    "pooling": "cls",
    "device": "cpu",
    "port": 8199,
    "ctx_size": 8192,
    "threads": 8,
    "gpu_layers": 99,
    "min_score": 0.35
  }
}
```

`device`: `cpu` | `gpu`.

## CLI

```bash
c2 embeddings status
c2 embeddings start
c2 embeddings stop
c2 embeddings embed "onde valida pagamento"
```

`embed` sobe o runtime se necessário e imprime dimensão/norma; com `--json`,
imprime o vetor.

## API do Core

```rust
embedding_runtime::status(&workspace, &config);
embedding_runtime::ensure(&workspace, &config)?;
embedding_runtime::embed(&config, "texto")?;
embedding_runtime::stop(&workspace)?;
```

`ensure` reutiliza o runtime se `health` estiver OK; caso contrário, sobe,
espera até 120 s e valida `/health`.

## Troca CPU/GPU

A troca reinicia só o runtime de embeddings. Vetores não mudam e não precisam
de reindexação; o cache é por `model_id` + `dim` + hash do metadata.
