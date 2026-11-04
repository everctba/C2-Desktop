# Gate do MVP — checklist com evidência

**Data:** 2026-11-04 · **Versão:** 0.1.0 · **Bead:** `c2-e12t5`
**Máquina:** Windows 11, Ryzen/RTX 4060 Ti 8 GB · **Modelo:** Gemma 4 E2B Q4_K_M
**Toolchain:** Rust 1.99.1, alvo `x86_64-pc-windows-gnu` com `rust-lld`

Este documento responde a uma pergunta só: **o MVP está pronto?** Cada critério
abaixo tem status e evidência verificável. Critério sem evidência não conta.

Status: ✅ atendido · ⚠️ atendido parcialmente · ❌ não atendido · ⏸️ deferido por decisão

---

## 1. Critérios do PRD §18

| # | Critério | Status | Evidência | Lacuna |
|---|---|---|---|---|
| 1 | Instalar o C2 em Windows | ✅ | Instalador com escolha de pasta (app + CLI + runtime, 116 MB), atalhos e desinstalador; artefato de release validado fora do repo (checksums OK, `init/status/reindex` com runtime embarcado, janela por `--root`) | — (Linux adiado: `c2-uf5`) |
| 2 | Executar `c2 init` dentro de um projeto | ✅ | cria `.c2/config.json`, detecta linguagens, idempotente (teste de integração) | — |
| 3 | O desktop reconhecer o workspace | ✅ | `C2.exe` abre com projeto (`c2 open --root <dir>` via `initial_root`, ou escolha de pasta) | — |
| 4 | Configurar um modelo local | ✅ | `c2 config set model.path|binary|port` + validação por `--version` | — |
| 5 | Pressionar PLAY | ✅ | botão **Iniciar** na janela sobe engine (thread + eventos) e runtime; **Parar** encerra os dois; **Reindexar** faz passada única — janela validada pelo usuário no portátil | — |
| 6 | Runtime iniciar e indexar os arquivos | ✅ | `c2 reindex` em projeto real: 37/37 arquivos, 0 erros | — |
| 7 | Alterações atualizarem os metadados | ✅ | hash diferente → reindexa só o arquivo mudado (teste + execução real) | — |
| 8 | Arquivos inalterados não chamarem o LLM | ✅ | 2ª passada: 0 chamadas (teste de integração e execução real) | — |
| 9 | Arquivos excluídos terem metadados órfãos removidos | ✅ | remoção de 5 arquivos removidos detectada e metadados apagados | — |
| 10 | Pesquisas pelo Semantic Grep | ✅ | `c2 grep` com motivo, trecho e `--json`; 78+ testes | — |
| 11 | Um agente MCP consultar o projeto | ⏸️ | decisão `c2-dec2`: MCP fora do MVP | `c2-e13` (deferido) |
| 12 | Funcionar sem LM Studio ou Git Bash | ✅ | sem Git Bash; `runtime/` com llama.cpp oficial (Vulkan+CPU) embarcado no pacote — `Origem: runtime embarcado`; LM Studio desligado por padrão (`allow_lmstudio=false`) | — |

## 2. Critérios do MVP definido em `c2-dec2` (Core + CLI + desktop mínimo)

| Item | Status | Evidência |
|---|---|---|
| Core descoberta/filtros/hash | ✅ | `.gitignore`/`.c2ignore`, binários, tamanho, allowlist — com testes |
| Metadados incrementais e atômicos | ✅ | escrita tmp+rename, re-hash pós-análise, validação de semântica |
| Runtime LLM supervisionado | ✅ | localiza, valida, sobe, health, reinicia com backoff, encerra |
| Indexação ponta a ponta | ✅ | 37 arquivos reais; segunda passada sem chamadas |
| Grafo de links | ✅ | 49 arestas no índice real (Rust/JS/Python) |
| Semantic Grep | ✅ | busca híbrida com evidência e trecho |
| `read` e `related` | ✅ | faixa de linhas; usa/usado por |
| Segurança de segredos | ✅ | `.env`/chaves pulados; conteúdo bloqueado ou mascarado |
| Desktop mínimo | ✅ | janela Tauri com Projetos/Engine/Busca, seletor de `.gguf` com validação, PLAY/STOP/REINDEX e `c2 open` |
| Migração do protótipo bash | ✅ | `c2 migrate` converte para o schema v1 com backup reversível (`c2-e11t2`) |

## 3. Benchmark de indexação e busca (500 arquivos)

Corpus sintético: 501 arquivos `.rs` em 20 módulos (500 + `src/lib.rs`), com
`use crate::modNN::arquivo_XXX` ligando os arquivos. Script: `scripts/bench.ps1`.
Evidência: [`docs/bench-500.json`](bench-500.json).

| Métrica | Valor medido | Como |
|---|---|---|
| Primeira indexação | **2203 s (36,7 min)** | `pwsh -File scripts/bench.ps1 -Files 500` |
| Throughput | **0,23 arquivo/s (~4,4 s/arquivo)** | 501 ÷ 2203 |
| Modelo | **501 chamadas, 359.688 tokens** | resumo do `c2 reindex` |
| Segunda passada | **0,4 s · 0 chamadas · 501 hashes do cache** | `c2 reindex` repetido |
| `c2 grep` p50 / p95 | **60 ms / 82 ms** | 5 consultas no índice de 501 |
| `c2 related` | **49 ms** | 1 arquivo (1 `usa`, 1 `usado por`) |
| Grafo | 501 metadados com links | relink estático |
| Pacote portátil | 116,6 MB (app + CLI + runtime) | `scripts/build-portable.ps1` |

**Leitura honesta:** a indexação inicial é o gargalo (~4,4 s/arquivo com Gemma 4
E2B Q4 numa GPU de 8 GB), não a busca; para 500 arquivos é meia hora de máquina,
uma única vez. A segunda passada é gratuita e quase instantânea (0,4 s para 501
arquivos) graças ao cache de hash. A busca fica bem abaixo da meta de 150 ms
mesmo com o processo de CLI frio.

O benchmark de **100 arquivos** continua como verificação rápida
(`-Files 100`): 343,6 s na primeira passada, 0,1 s na segunda, grep p50 24 ms.

Achado corrigido na rodada anterior: `crate_root_of` verificava `src/lib.rs`
relativo ao diretório do processo, então `use crate::modNN::…` nunca resolvia e
o grafo saía vazio. Corrigido para resolver contra a raiz do workspace, com
teste de regressão; o corpus agora tem o grafo completo e `related` mostra
`uses`/`used_by` reais.

## 4. Veredito

**GO para "alpha Windows"** — escopo do MVP fechado em Windows por decisão (`c2-uf5`).

Fora do escopo desta fase, por decisão registrada:

1. **Linux** — adiado (`c2-uf5`); o código compila e passa clippy para
   `x86_64-unknown-linux-gnu` e o roteiro/CI ficam preparados para quando
   o escopo reabrir (`c2-e12.2`).
2. **MCP** — deferido (`c2-dec2` / `c2-e13`).

O desktop mínimo existe e roda (pacote portátil com instalador), o runtime do
llama.cpp vai embarcado (sem LM Studio) e os critérios 3 e 5 do PRD são
atendidos pela janela.

**GO para "alpha usável Core + CLI"**: init, indexação incremental, grafo,
busca, leitura e segurança funcionam de ponta a ponta, com testes e evidência.

> O critério de saída do MVP continua sendo o do PRD: um agente externo consulta
> o projeto e o usuário opera sem terminal. Nenhum dos dois fecha enquanto o
> desktop e o MCP não existirem.
