# C2 — contexto semântico de projetos para agentes de IA

O C2 indexa um projeto de software e mantém, ao lado de cada arquivo, um
metadado (`<arquivo>.c2.json`) com o significado do arquivo: o que é, por que
existe, que objetivo garante, qual sua responsabilidade e de quais arquivos
depende. Um agente consulta esse índice **antes** de ler código, gastando menos
tokens e errando menos.

- **Um núcleo, múltiplas interfaces:** `c2-core` (biblioteca Rust) + `c2` (CLI);
  desktop e MCP estão no roadmap.
- **Incremental por SHA-256:** arquivo inalterado não chama o modelo.
- **Local por padrão:** modelo GGUF rodando via `llama-server` na sua máquina.
- **Sem banco de dados, sem embeddings:** busca híbrida (lexical + metadados).

## Requisitos

| Item | Versão |
|---|---|
| Rust | 1.75+ (`cargo`) |
| llama-server | build do llama.cpp (o C2 localiza e supervisiona) |
| Modelo | um `.gguf` de sua escolha (testado com Gemma 4 E2B Q4_K_M) |
| SO | Windows 10/11 ou Linux |

## Compilar

```bash
cargo build --release
# binário em target/release/c2 (Windows: c2.exe)
```

## Pacote portátil (Windows)

```bash
pwsh -File scripts/build-portable.ps1
```

Gera `dist/desktop/`:

| Arquivo | O que é |
|---|---|
| `C2.exe` | app desktop (janela), frontend embutido, ~12 MB |
| `cli/c2.exe` | CLI do Core (init, status, grep, related, read) |
| `runtime/` | **llama-server oficial** (Vulkan + CPU, ~101 MB) — sem LM Studio |
| `instalar.ps1` | instalador com escolha de pasta, atalho e desinstalador |
| `LEIA-ME.txt` | requisitos e primeiros passos |
| `CHECKSUMS.txt` | SHA-256 de C2.exe, cli/c2.exe e do runtime |

```powershell
pwsh -File dist/desktop/instalar.ps1 -Dest "D:\Apps\C2"
```

A CLI fica numa subpasta porque no Windows `c2.exe` e `C2.exe` são o **mesmo**
arquivo. O pacote não embarca o llama.cpp: aponte `model.binary` (ou use um
`llama-server` no PATH).

## Primeiro uso

```bash
cd meu-projeto
c2 init                      # cria .c2/config.json
c2 config set model.path /caminho/modelo.gguf
c2 config set model.binary /caminho/llama-server   # opcional: busca automática
c2 start                     # indexa e fica observando (Ctrl+C encerra)
```

Uma passada só (útil em CI ou para conferir):

```bash
c2 reindex                   # sobe o runtime, indexa, para o runtime
c2 reindex --limit 10        # só os primeiros 10
c2 reindex --full            # ignora o hash e reindexa tudo
c2 reindex --dry-run         # mostra o plano sem chamar o modelo
```

## Consultar

```bash
c2 grep "onde o sistema valida permissão"    # busca por significado
c2 grep "autenticação" --files-only          # só caminhos
c2 grep "sessão" --explain                   # mostra a composição do score
c2 related src/auth/session.rs               # usa / usado por
c2 read src/auth/session.rs --lines 40:95    # trecho com número de linha
c2 status                                    # estado do índice e do runtime
```

## Índice e metadados

- Metadado ao lado do arquivo: `src/main.rs` → `src/main.rs.c2.json`.
- Conteúdo: `what`, `why`, `goal`, `summary`, `responsibility` e `links[]`
  (imports/mod/require…). Formato completo em [docs/schema.md](docs/schema.md).
- Escrita atômica; metadado sem semântica é recusado.
- Re-hash depois da análise: se o arquivo mudou durante a inferência, o
  resultado é descartado e o arquivo entra no próximo ciclo.

## Segurança

- `.env`, chaves privadas (`*.pem`, `id_rsa`…) e credenciais são **pulados
  antes de serem abertos**.
- Conteúdo com segredo é bloqueado (`index.secret_policy=skip`, padrão) ou
  mascarado (`mask`); fixtures podem ser liberadas com `// c2:allow-secret`
  ou por `index.secret_allow`.
- Nada sai da máquina: o modelo é local.

## Configuração útil

```bash
c2 config list
c2 config set model.path ./modelo.gguf
c2 config set model.port 8099
c2 config set model.ctx_size 8192
c2 config set model.disable_thinking true      # modelos de raciocínio: ~7x mais rápido
c2 config set index.max_file_bytes 262144
c2 config set index.secret_policy mask
c2 config set engine.poll_interval_secs 5
```

## Comandos

| Comando | Função |
|---|---|
| `c2 init` | cria `.c2/config.json` (idempotente) |
| `c2 start` | indexa continuamente e supervisiona o runtime |
| `c2 stop` | encerra engine e `llama-server` |
| `c2 status` | workspace, índice, histórico e runtime |
| `c2 reindex` | uma passada (`--full`, `--path`, `--limit`, `--dry-run`) |
| `c2 grep` | busca semântica (`--limit`, `--files-only`, `--explain`, `--json`) |
| `c2 related` | dependências de um arquivo (usa / usado por) |
| `c2 read` | lê trecho com número de linha |
| `c2 cleaner` | remove metadados (lista por padrão; `--yes` remove) |
| `c2 config` | `get` / `set` / `list` |

## Limitações desta versão

- Desktop Tauri e servidor MCP ainda não existem (beads `c2-e14`, `c2-e13`).
- O instalador ainda não embarca o runtime: por ora o C2 usa o `llama-server`
  que você apontar (ou o do LM Studio, como fallback de desenvolvimento).
- O desktop exige o Visual Studio Build Tools (MSVC) no Windows.

## Desenvolvimento

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

No Windows sem MSVC, o projeto usa o toolchain GNU com `rust-lld` — ver
[AGENTS.md](AGENTS.md). O backend de inferência para desenvolvimento pode ser
subido com `pwsh -File scripts/dev-llama.ps1`.

Benchmark de indexação e busca: `pwsh -File scripts/bench.ps1 -Files 500`.
