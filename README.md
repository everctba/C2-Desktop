# C2

<p align="center"><img src="site/assets/ascii/logo.svg" width="420" alt="C2 — Comments 2.0 for AI"></p>

[![ci](https://github.com/everctba/C2-Desktop/actions/workflows/ci.yml/badge.svg)](https://github.com/everctba/C2-Desktop/actions/workflows/ci.yml)

**Comments 2.0 for AI** — contexto semântico de projeto para agentes de IA.
Local. Incremental. Sem embeddings.

O C2 comenta seu código automaticamente com IA local: `what`, `why`, `goal`,
`summary`, `responsibility`. Um loop detecta o que mudou pelo hash e reescreve
só o comentário afetado. Sua IA paga consome esse comentário atualizado em vez
de reler o projeto inteiro: **menos tokens, menos erro, menos releitura.**

---

## Por que usar

- **Menos tokens.** O agente acha o arquivo certo sem abrir o repositório.
- **Menos erro.** Metadado com `what`, `why`, `goal`, `summary`, `responsibility`.
- **Sempre atualizado.** Incremental por SHA-256: arquivo igual não chama o modelo.
- **Consulta rápida.** `c2 grep` p50 **60 ms**; `c2 related` **49 ms**.
- **Grafo sem LLM.** Imports/`use` viram arestas estáticas.
- **Local.** Modelo e índice na sua máquina. Nada sai.
- **Sem infra.** Sem banco, sem embeddings — só arquivos ao lado do código.
- **Reversível.** `c2 cleaner` remove; `c2 migrate` converte metadado antigo.

## Números medidos

| | |
|---|---|
| 501 arquivos indexados | **36,7 min** (~4,4 s/arquivo, 1ª vez) |
| 2ª passada | **0,4 s · 0 chamadas** · 501 hashes em cache |
| Busca `c2 grep` | **p50 60 ms · p95 82 ms** |
| Consulta semântica | **86 ms** acha o que o `grep` lexical não encontra |
| Pacote Windows | **117 MB** (app + CLI + runtime llama.cpp) |

Detalhe e metodologia: [docs/bench-500.json](docs/bench-500.json).

## Como funciona

1. **Indexar** — o C2 lê o código e pede a um modelo local `what/why/goal/summary`.
2. **Gravar** — o resultado vira `<arquivo>.c2.json`, com hash e links.
3. **Consultar** — agente ou você usam `grep`, `related`, `read` — sem ler tudo.

Mudou o arquivo? Só ele é reindexado.

## Começar (Windows)

```powershell
pwsh -File instalar.ps1                       # escolhe a pasta, cria atalho, põe o CLI no PATH
pwsh -File instalar.ps1 -Dest "D:\Apps\C2"
```

Depois:

```powershell
cd meu-projeto
c2 init --agent   # grava regras para o seu agente em AGENTS.md
c2 config set model.path "H:\modelos\gemma-4-E2B-Q4_K_M.gguf"
c2 reindex
c2 grep "onde valida permissão de usuário"
```

Ou abra **C2.exe**: abrir projeto → selecionar `.gguf` → carregar na GPU → indexar → buscar.

## CLI

```bash
c2 status                           # índice, fila, runtime, log
c2 reindex [--full] [--limit N]     # uma passada
c2 start / c2 stop                  # engine contínuo / parar
c2 grep "por significado" --limit 5 # busca híbrida
c2 related src/auth/session.rs      # usa / usado por
c2 read src/auth/session.rs --lines 40:95
c2 migrate --yes                    # metadado antigo → schema v1 (com backup)
c2 logs --lines 20
```

## Desktop

Janela Tauri com **Projetos**, **Engine** (log ao vivo, contadores),
**Busca** (score, destaque dos termos, relacionados), **MCP** e **Config**.
Seletor de modelo valida cabeçalho e VRAM antes de carregar.

## MCP

```powershell
c2 --root . mcp     # STDIO: initialize, tools/list, tools/call
```

Ferramentas somente leitura: `c2_grep`, `c2_read`, `c2_related`, `c2_map`.
Configuração pronta na aba **MCP** da janela.

## Privacidade

- Modelo e índice rodam **na sua máquina**.
- Segredos são detectados **antes** do modelo (`skip`/`mask`).
- Sem telemetria. Sem upload. Sem conta.

## Plataformas

| | |
|---|---|
| Windows 10/11 (x64) | suportado e validado |
| Linux | build pronto (`scripts/build-linux.sh`), execução em validação |
| macOS | fora do escopo |

## Quando NÃO vale

Projeto pequeno? Abrir os arquivos direto é mais barato.
O C2 compensa a partir de projetos médios, quando o agente precisaria abrir
10–20% do repositório. Medimos isso: [docs/bench-ab/resultado-cego.md](docs/bench-ab/resultado-cego.md).

## Documentação

- [Guia completo](docs/guia.md) — instalação, configuração, comandos, segurança
- [Instruções completas para agentes](docs/agent-instructions.md) — guia autocontido para colar nas regras\n- [Protocolo para agentes](docs/agent-protocol.md) — contexto antes de código
- [Schema do metadado](docs/schema.md)
- [Gate do MVP](docs/mvp-gate.md) — critérios, benchmarks, veredito
- [Landing briefing](docs/landing/briefing.md) — números e posicionamento

## Desenvolvimento

```bash
cargo build --workspace
cargo test --workspace        # 141 testes
cargo clippy --workspace --all-targets -- -D warnings
```

Uso: `cargo run -p c2-cli -- --help`.
