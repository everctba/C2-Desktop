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
| 1 | Instalar o C2 em Windows ou Linux | ⚠️ | Windows: instalador com escolha de pasta (app + CLI + runtime, 116 MB); artefato de release validado fora do repo: checksums OK, `c2 init/status/reindex` com runtime embarcado e janela abrindo por `--root` | Linux não validado (`c2-e12.2`) |
| 2 | Executar `c2 init` dentro de um projeto | ✅ | cria `.c2/config.json`, detecta linguagens, idempotente (teste de integração) | — |
| 3 | O desktop reconhecer o workspace | ✅ | `C2.exe` abre com projeto (`c2 open --root <dir>` via `initial_root`, ou escolha de pasta) | — |
| 4 | Configurar um modelo local | ✅ | `c2 config set model.path|binary|port` + validação por `--version` | — |
| 5 | Pressionar PLAY | ✅ | botão **Iniciar** na janela sobe engine (thread + eventos) e runtime; **Parar** encerra os dois; **Reindexar** faz passada única | — |
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
| Migração do protótipo bash | ⚠️ | leitura tolerante pronta; `c2 migrate` não existe (`c2-e11`) |

## 3. Benchmark de indexação e busca (500 arquivos)

Corpus sintético: 500 arquivos `.rs` em 20 módulos, com links entre si e
vocabulário de domínio variado. Script: `scripts/bench.ps1`.

| Métrica | Valor medido | Como |
|---|---|---|
| Corpus gerado | 500 arquivos `.rs` em 20 módulos | `scripts/bench.ps1 -Files 500` |
| Arquivos indexados antes da interrupção manual | **188** | `*.c2.json` no corpus |
| Janela de indexação observada | **714 s** (11,9 min) | mtime do 1º ao último metadado |
| Throughput real | **0,26 arquivo/s** | 188 ÷ 714 |
| Projeção para 500 arquivos | **~31,6 min** | 500 ÷ 0,26 |
| Modelo | 188 chamadas, ~3,8 s por arquivo | resumo do `c2 reindex` |
| Segunda passada (projeto real, 37 arquivos) | **0 chamadas ao modelo** | `c2 reindex` repetido |
| `c2 grep` p50 | **49 ms** | 5 consultas, índice de 188 arquivos |
| `c2 grep` p95 | **91 ms** | idem |
| `c2 related` | **25 ms** | 1 arquivo |
| Binário | `c2.exe` release, 3,6 MB, build em 32 s | `cargo build --release` |

**Leitura honesta:** a indexação inicial é o gargalo (0,26 arquivo/s com Gemma 4
E2B Q4 em GPU de 8 GB), não a busca. Para um projeto de 500 arquivos isso é
meia hora de trabalho de máquina, uma vez só — a segunda passada é gratuita.
A busca ficou bem abaixo da meta de 150 ms mesmo com o processo de CLI frio.

Duas ressalvas registradas:

- o run de 500 foi **interrompido manualmente** em 188 arquivos; o número
  completo está em `c2-e12.3` (rodar em background/CI, ~32 min);
- o corpus sintético original não tinha `src/lib.rs`, então `crate::modNN::…`
  não resolvia e o grafo saía vazio (`related` = 0 arestas). O script já foi
  corrigido; no projeto real o grafo tem **49 arestas**.

## 4. Veredito

**GO para "alpha Windows"** · **NO-GO para "MVP fechado"** — restam dois itens:

1. Linux sem validação (`c2-e12.2`).
2. MCP fora do escopo por decisão (`c2-dec2` / `c2-e13`).

O desktop mínimo existe e roda (pacote portátil com instalador), o runtime do
llama.cpp vai embarcado (sem LM Studio) e os critérios 3 e 5 do PRD são
atendidos pela janela.

**GO para "alpha usável Core + CLI"**: init, indexação incremental, grafo,
busca, leitura e segurança funcionam de ponta a ponta, com testes e evidência.

> O critério de saída do MVP continua sendo o do PRD: um agente externo consulta
> o projeto e o usuário opera sem terminal. Nenhum dos dois fecha enquanto o
> desktop e o MCP não existirem.
