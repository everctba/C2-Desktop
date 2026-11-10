# Embedding model — bge-m3 Q8_0

Decisão da task `c2-kga.1`.

## Modelo

| Campo | Valor |
|---|---|
| Repositório | `lm-kit/bge-m3-gguf` |
| Arquivo | `bge-m3-Q8_0.gguf` |
| URL | `https://huggingface.co/lm-kit/bge-m3-gguf/resolve/main/bge-m3-Q8_0.gguf` |
| Tamanho | 634.553.856 bytes (~605 MB) |
| SHA-256 | `c98c7c907bb3ab8c3be4c8ad827cc934c7c1a7e014d1c999e0c45c8027c158bb` |
| Licença | MIT (model card) |
| Dimensão | 1024 |
| Pooling | `cls` |
| Contexto avaliado | 8192 tokens |

## Runtime

Validado com o `llama-server` embarcado (build 12110):

```bash
llama-server \
  --embeddings \
  --pooling cls \
  --model bge-m3-Q8_0.gguf \
  --host 127.0.0.1 \
  --port 8199 \
  --no-webui \
  -ngl 0 \
  --threads 8
```

- CPU: health em ~4 s na primeira carga;
- `/v1/embeddings` respondeu **1024 dimensões**, norma ~1.0;
- latência observada: **~35–65 ms por embedding** em CPU;
- a variante GPU ainda será medida na task de runtime.

## Cosine

Pares de teste (texto em ASCII para evitar problema de encoding do PowerShell):

```text
0,6836  "onde o pagamento e confirmado" <> "valida se a transacao foi aprovada"
0,4608  "onde o pedido e cancelado"      <> "estorna a compra e devolve o estoque"
0,6165  "configuracao de logging"        <> "formatacao de datas"
```

O bge-m3 tem um piso de similaridade relativamente alto entre frases PT. O
threshold final **não pode ser copiado de outro modelo**: será calibrado na
task `c2-kga.5` com consultas reais do benchmark.

## Observações

- não usar o modelo de geração (Gemma) para embeddings;
- usar uma instância separada de `llama-server` com `--embeddings`;
- CPU por padrão para não competir com a VRAM do Gemma;
- vetores de modelos diferentes são incompatíveis (cache guarda `model_id` e `dim`).
