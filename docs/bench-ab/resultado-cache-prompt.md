# A/B — `cache_prompt` no modo local (2026-11-05)

**Pergunta:** o `cache_prompt: true` (reaproveitar o KV cache do prefixo
`system`) reduz a latência da indexação em lote?

**Método:** 5 arquivos reais (`search.rs`, `mcp.rs`, `process.rs`, `Mcp.ts`,
`pricing.rs`), mesmo build, mesmo modelo (Gemma 4 E2B Q4_K_M), GPU, raciocínio
desligado, `provider=local`. Duas configurações:

- **A — `cache_prompt: true`** (padrão);
- **B — `cache_prompt: false`** (interruptor interno `C2_NO_CACHE_PROMPT=1`).

Cada execução: metadata e hash-cache apagados, runtime reiniciado. Duas réplicas,
com ordem alternada (A→B e B→A) para reduzir efeito de aquecimento.

## Resultados

| Réplica | A (cache on) | B (cache off) | Δ (A−B) |
|---|---:|---:|---:|
| 1 | 41.777 ms | 42.943 ms | **−1.166 ms** |
| 2 | 43.232 ms | 42.672 ms | **+560 ms** |
| **Média** | **42.504 ms** | **42.808 ms** | **−304 ms (~0,7%)** |

- chamadas: 5 em todas as execuções (sem cache de hash entre rodadas);
- `completion_tokens`: ~3.400 nas duas condições (diferença é ruído de geração);
- `prompt_tokens`: 12.898 nas duas.

## Leitura

O ganho esperado era modesto porque **o prefixo cacheável é pequeno** perto do
conteúdo: as instruções `system` têm ~150 tokens, contra ~2.500 tokens de
arquivo por chamada (~6% do prompt). O cache economiza o reprocessamento
desses ~150 tokens — algo que fica dentro do ruído de uma máquina compartilhada.

**Conclusão:** `cache_prompt: true` **não degrada** e pode ajudar quando o
prefixo crescer (prompt maior, few-shot, nuvem com cache de prefixo cobrado),
mas **não é um ganho mensurável** no corpus atual. O ganho grande de verdade
continua sendo **não chamar o modelo**: cache de hash (0 chamadas na 2ª
passada) e reindex incremental.

## O que ficou no código

- instruções no `system` + arquivo no `user` (necessário para cache de prefixo e
  correto para provedores OpenAI-compat);
- `cache_prompt` enviado como booleano no endpoint local;
- `C2_NO_CACHE_PROMPT=1` desliga o cache — só para diagnóstico/medição.

## Limitações

- 2 réplicas, máquina compartilhada, sem isolamento de GPU;
- latência inclui geração (não só prefill);
- comparação com números históricos de outras rodadas (ex.: 17,7 s/5 chamadas)
  **não é válida**: build/configuração do ambiente mudaram.
