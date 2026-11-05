# Guia completo do C2

> Referência detalhada (instalação, configuração, comandos e segurança).
> Para a visão geral, veja o [README](../README.md).

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

## Plataformas

- **Windows 10/11 (x64)** — suportado e validado nesta fase.
- **Linux** — planejado para uma fase seguinte (decisão `c2-uf5`); o Core já
  compila para `x86_64-unknown-linux-gnu` e o build está pronto:
  `bash scripts/build-linux.sh` (roteiro em `docs/linux-validation.md`).
- **macOS** — fora do escopo.

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

## MCP (agentes de IA)

O C2 expõe o Core por **MCP** sobre STDIO, somente leitura:

```powershell
c2 --root . mcp          # JSON-RPC: initialize, tools/list, tools/call
```

Ferramentas: `c2_grep`, `c2_read`, `c2_related`, `c2_map`. Elas reutilizam a
mesma lógica do CLI (sem duplicar regra) e limitam a resposta.

Configure no seu cliente MCP (Claude Desktop, Cursor, …):

```json
{
  "mcpServers": {
    "c2": {
      "command": "<pasta instalada>\cli\c2.exe",
      "args": ["--root", "<pasta do projeto>", "mcp"]
    }
  }
}
```

A janela tem a aba **MCP** com o estado real do servidor (handshake ao entrar),
a lista de ferramentas e essa configuração pronta para copiar.

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
  (imports/mod/require…). Formato completo em [docs/schema.md](schema.md).
- Escrita atômica; metadado sem semântica é recusado.
- **Cache de hash** (`.c2/hash-cache.json`): se `(tamanho, mtime)` não mudam, o
  SHA-256 anterior é reaproveitado — 300 arquivos inalterados caem de ~1,2 s
  para ~0,14 s na segunda passada. `--full` ignora o cache.
- **Rename por hash**: arquivo movido com o mesmo conteúdo herda o metadado
  existente, sem chamar o modelo, e os `links[]` que apontavam para o caminho
  antigo são atualizados.
- Re-hash depois da análise: se o arquivo mudou durante a inferência, o
  resultado é descartado e o arquivo entra no próximo ciclo.

## Segurança

- `.env`, chaves privadas (`*.pem`, `id_rsa`…) e credenciais são **pulados
  antes de serem abertos**.
- As instruções enviadas ao modelo ficam em `index.prompt` (editável na tela
  Config ou `c2 config set index.prompt "..."`); a linha que trata o arquivo
  como DADO é fixa e não pode ser removida. As instruções vão como mensagem
  `system` (prefixo cacheável, com `cache_prompt: true` no modo local) e o
  conteúdo do arquivo como `user` — o prefixo fixo é reaproveitado no lote.
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

### Links de linguagens sem suporte nativo

Para linguagens fora do extrator (Rust, JS/TS, Python), dá para declarar uma
regex de import em `index.link_fallback`:

```json
"index": {
  "link_fallback": [
    { "extension": "lua", "regex": "require\\(`"([^`"]+)`"\\)" }
  ]
}
```

O grupo 1 é o alvo; o C2 tenta `<dir do arquivo>/<alvo>`, `<alvo>.<ext>` e
`<alvo>` na raiz. Regex inválida é ignorada com aviso (não derruba a indexação).

## Tipografia

- **UI:** sans-serif do sistema (`Segoe UI`/`system-ui`) — sem fonte externa.
- **Código/terminal:** **IBM Plex Mono** (woff2 embutido no app, licença SIL
  OFL em `apps/desktop/src/styles/fonts/OFL.txt`). Funciona offline.

## Troubleshooting

### O build do desktop quebra com `missing field identifier`

O Tauri lê **todo `.json`** dentro de `apps/desktop/src-tauri/capabilities/` como
especificação de permissão. O metadado `default.json.c2.json` que o C2 gera ao
lado do arquivo cai nessa regra e quebra o build.

Solução: o `.c2ignore` da raiz já exclui essa pasta. Se você criar outro
diretório que uma ferramenta varre por extensão, adicione-o ao `.c2ignore`:

```gitignore
apps/desktop/src-tauri/capabilities/
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
| `c2 logs` | últimas linhas de `.c2/logs/c2.log` (`--lines`) |
| `c2 migrate` | converte metadados do protótipo bash para o schema v1 (`--yes`, backup) |
| `c2 open` | abre a janela do C2 Desktop neste projeto |

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
[AGENTS.md](../AGENTS.md). O backend de inferência para desenvolvimento pode ser
subido com `pwsh -File scripts/dev-llama.ps1`.

Benchmark de indexação e busca:

```powershell
pwsh -File scripts/bench.ps1            # 100 arquivos (verificação rápida)
pwsh -File scripts/bench.ps1 -Files 500 # corpus completo
```

Medido em 501 arquivos `.rs` (Windows, Gemma 4 E2B Q4, runtime embarcado):

| Métrica | Valor |
|---|---|
| Primeira indexação | 36,7 min (501 chamadas, ~4,4 s/arquivo) |
| Segunda passada | **0,4 s, 0 chamadas** (501 hashes do cache) |
| `c2 grep` p50 / p95 | **60 ms / 82 ms** |
| `c2 related` | 49 ms |

Evidência: [`docs/bench-500.json`](bench-500.json).
