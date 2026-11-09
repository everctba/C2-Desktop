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

## Idioma da instalação

O instalador portátil abre um **box de idioma** antes de escolher a pasta:
**English (padrão)** ou **Português (Brasil)**. A escolha fica em
`%APPDATA%/dev.c2.desktop/language.json` e será usada pela interface do app.
Para automação: `pwsh -File instalar.ps1 -Lang en|pt-BR`. Dentro do app, troque a
qualquer momento em **Config → Idioma** (English/Português, com bandeiras); a
escolha é gravada no mesmo `language.json`.

Os **comandos da CLI e as tools do MCP permanecem em inglês**
(`c2 reindex`, `c2_annotate`, etc.) — é a API pública do produto.

## Instalador Windows (Inno Setup)

Para a experiência de duplo clique, gere o instalador com Inno Setup 6:

```powershell
pwsh -File scripts/build-installer.ps1
```

O `.exe` resultante instala desktop + CLI + MCP + runtime, permite escolher a
pasta, cria atalhos e adiciona `cli` ao **PATH do sistema** (requer admin;
opção desmarcável). O código do instalador fica em `scripts/c2.iss`.

## Primeiro uso

```bash
cd meu-projeto
c2 init                      # cria .c2/config.json, mostra a intro ASCII e abre o desktop
c2 init --no-open            # so cria o workspace (sem abrir a janela)
c2 init --json               # saida JSON limpa (nao abre janela)
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

### Configuracao automatica (recomendado)

```powershell
c2 init --mcp      # gera .mcp.json (Claude Code) e .cursor/mcp.json (Cursor)
c2 init --agent    # AGENTS.md + os dois arquivos MCP
```

Os arquivos sao project-scoped: o comando e `c2`, sem `--root` — o cliente
inicia o servidor com o cwd do projeto. O C2 nao sobrescreve uma entrada `c2`
existente nem apaga outros servidores do arquivo; para clientes globais
(Claude Desktop) use `--root` na config global.

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

### Reindexar: passada normal x “Reindexar tudo”

- **Reindexar tudo** (botão na Engine e na toolbar) usa `--full`: ignora o hash
  e chama o modelo para **todos** os arquivos, mesmo os que não mudaram. Use
  quando trocou o modelo/prompt de indexação, quer regenerar todos os metadados
  ou suspeita que algum está ruim. É caro na nuvem (cobra tokens por arquivo).
- Durante o reindex a Engine mostra `reindexando… X% (N de TOTAL)` com o botão
  **Abortar reindexação** ao lado; o abort interrompe entre um arquivo e outro.
- A CLI mantém `c2 reindex --limit N` para passadas parciais em automação.
- O engine **Iniciar** já indexa continuamente em background; o botão é uma
  passada única.
- **Não roda o `cleaner` e não apaga os `.c2.json` antes.** Cada arquivo é
  reanalisado e o metadado é **sobrescrito atomicamente** ao final. Metadados
  órfãos (arquivo-fonte removido) e metadados de rename são tratados pelo plano
  normal; para apagar metadados em massa use `c2 cleaner` (explícito).
- Se a chamada ao modelo falhar, o metadado antigo daquele arquivo permanece —
  o C2 não grava semântica vazia.

## Segurança

- `.env`, chaves privadas (`*.pem`, `id_rsa`…) e credenciais são **pulados
  antes de serem abertos**.
- O **prompt Original acompanha o idioma da interface**: com a UI em inglês os
  metadados novos saem em inglês; com a UI em português, em português. Projetos
  antigos com prompt de fábrica são sincronizados ao abrir/trocar o idioma.
- Em Config → Prompt de indexação, **Original** usa o texto de fábrica do idioma
  atual e **Personalizar** libera a edição. O prompt custom não muda quando o
  idioma da interface muda.
- As instruções enviadas ao modelo ficam em `index.prompt` (editável na tela
  Config ou `c2 config set index.prompt "..."`); a linha que trata o arquivo
  como DADO é fixa e não pode ser removida. As instruções vão como mensagem
  `system` (prefixo cacheável, com `cache_prompt: true` no modo local) e o
  conteúdo do arquivo como `user` — o prefixo fixo é reaproveitado no lote.
- Conteúdo com segredo é bloqueado (`index.secret_policy=skip`, padrão) ou
  mascarado (`mask`); fixtures podem ser liberadas com `// c2:allow-secret`
  ou por `index.secret_allow`.
- O C2 não envia seu código: o modelo e o índice são locais. O agente de código
  que você usa pode ser na nuvem (ele recebe só o que decidir ler).

### Anotações de agentes

O agente de IA pode anexar descobertas não óbvias ao metadado com a tool MCP
`c2_annotate` (ou `c2 note add <arquivo> "<texto>" --kind invariant`). Tipos:
`teleology` (por que existe), `invariant` (regra que nunca quebra), `gotcha`
(armadilha), `correction` (pensou A, era B), `pending` (decisao do humano
pendente) e `note`.

| kind | pergunta que responde | exemplo |
|---|---|---|
| `teleology` | por que existe? | “Existe para evitar N+1 no checkout.” |
| `invariant` | o que nunca pode quebrar? | “O score nunca é negativo; empates ordenam por caminho.” |
| `gotcha` | o que parece uma coisa e é outra? | “Parece cache opcional, mas remover quebra o 1º request.” |
| `correction` | onde eu pensei A e era B? | “Achei que o total vinha do banco; é recalculado.” |
| `pending` | o que ainda depende do humano? | “Confirmar com o cliente se desconto vale no frete.” |
| `note` | genérico | observação que não se encaixa acima |

 O critério é tirar do agente a **descrição do óbvio** e
pedir a **teleologia**: por que existe, o que quebra se remover, o que o código
não revela — 1–2 anotações por tarefa, não uma por arquivo. As
anotações ficam em `agent_notes`, no máximo 10 por arquivo (2.000 caracteres
cada), são **preservadas no reindex** e marcadas como "revisar" quando o hash do
arquivo muda. `c2 note list` mostra tudo; `c2 note remove` apaga.

## Novidades (What's New)

O app mostra um modal de novidades no maximo uma vez por post, depois de 5
aberturas e de 24 h desde a ultima checagem. O conteudo vem de
`site/news.json` (publicado junto com a landing), sem release e sem backend.

- URL padrao: `https://everctba.github.io/C2-Desktop/news.json`;
- teste local: `C2_NEWS_URL=http://127.0.0.1:8000/news.json` (servidor HTTP na
  pasta `site/`);
- estado: `%APPDATA%/dev.c2.desktop/news-state.json` (`launches`,
  `lastSeenDate`, `lastCheckAt`);
- regras: um post por novidade, `date` ISO com timezone, link so http/https;
  offline/JSON invalido falha em silencio.

## Modelo global (vale para todos os projetos)

O `.gguf` e uma preferencia do **app**, nao do projeto. Ao selecionar um modelo no
desktop, o caminho (e nome/ctx/camadas/threads) fica salvo em
`%APPDATA%/dev.c2.desktop/global-model.json`. Projetos novos ou sem modelo herdam
esse caminho automaticamente ao abrir; projetos que ja tem `model.path` proprio
nao sao alterados. Pela CLI, a config continua por workspace.

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

# nuvem opcional (OpenAI-compatible) — a chave vai para o cofre, nao para o config
c2 config set model.base_url https://api.openai.com/v1
c2 config set model.name gpt-4o-mini
c2 config set model.provider openai_compat
c2 config set model.cost_input_per_mtok 0.15   # preço por 1M tokens de entrada
c2 config set model.cost_output_per_mtok 0.60  # preço por 1M tokens de saída
c2 config set model.max_cost_usd_per_run 0.50  # teto por execução (0 = sem teto)
```

### Provedor de IA (nuvem opcional)

O padrão é `local` (llama-server embarcado). Para usar um provedor
OpenAI-compatible, defina `model.base_url`, `model.name` e `model.provider =
openai_compat`. A API key **nunca** é gravada no `.c2/config.json`: ela fica no
cofre do sistema (Credential Manager no Windows, Keychain no macOS, Secret
Service no Linux; tela Config → Provedor de IA → Salvar chave) ou na
variável de ambiente `C2_API_KEY` (útil em CI).

Na tela Config, **Esquecer chave** apaga a credencial do cofre local (não
revoga no provedor — para isso, use o painel do provedor). A chave nunca é
gravada no `.c2/config.json` nem aparece em logs/dumps: o `Debug` do cliente
redige o bearer.

Falha no provedor **não** cai automaticamente para o modelo local: o C2 mostra
um popup de erro e mantém o provedor escolhido. O local precisa da GPU (que pode
estar em uso por outra coisa), então a troca é sempre uma decisão do usuário.

O mesmo caminho serve para **BYO endpoint** (servidor que você já roda):
LM Studio (`http://127.0.0.1:1234/v1`), Ollama (`http://127.0.0.1:11434/v1`),
vLLM (`http://127.0.0.1:8000/v1`) ou llama.cpp manual. Nesse modo o C2 não
sobe nem reinicia o servidor; o contexto precisa caber no servidor (≥ 8k) e
endpoint local dispensa API key.

Ao ativar a nuvem, o conteúdo dos arquivos é enviado ao provedor — a tela mostra
esse aviso e você pode voltar para `local` quando quiser. O runtime local não é
iniciado nesse modo; `model.base_url` apontando para `127.0.0.1` também serve
para Ollama/vLLM/LM Studio.

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

### Porta 8080 em uso ao dar play

Se outro processo já escuta a porta do modelo (padrão `8080`), o C2 escolhe
**automaticamente uma porta livre**, grava em `model.port` e sobe o runtime nela.
Não precisa fechar o outro programa nem rodar `c2 config set model.port`.
Se nenhuma porta livre for encontrada, a mensagem antiga ainda orienta a trocar
manualmente.

## Hot reload (desenvolvimento)

```powershell
pwsh -File scripts/dev.ps1          # Tauri + Vite (HMR)
pwsh -File scripts/dev.ps1 -UiOnly  # so navegador, mocks, sem Rust
```

- `.ts/.css/.html` → Vite HMR/reload imediato, sem recompilar Rust;
- `src-tauri/*.rs` → recompila e reinicia a janela (incremental);
- release/instalador só quando necessário (`scripts/dev.ps1 -Build`).

### Limpar metadados (acao destrutiva)

Na CLI:

```powershell
c2 cleaner          # dry-run: lista e conta
c2 cleaner --yes    # remove todos os .c2.json
```

No desktop: Config -> modo avancado -> **Manutenção** -> **Remove all c2.json files**
(ou o mesmo botão à direita na barra de ações da Engine).
O app mostra a contagem, exige digitar `DELETE` em maiúsculo e só então remove.
Código e `.c2/config.json` não são alterados; para recriar o contexto, rode
Reindexar tudo depois. Anotações de agentes são perdidas.

### `c2 mcp` não é um comando interativo

`c2 mcp` é um servidor **STDIO JSON-RPC**: quem conversa com ele é o cliente MCP
(Claude Desktop, Cursor, etc.). Se você abrir num terminal e digitar `c2 start`,
essa linha vira JSON inválido e o servidor responde `-32700`. O banner
`c2-mcp: pronto` vai para **stderr**; o stdout fica reservado ao protocolo.
Para iniciar o engine, use outro terminal/desktop.

## Diagnóstico de cold start

O desktop grava marcas de boot em `%APPDATA%/dev.c2.desktop/perf.log` (JSONL):
`js_boot_start`, `init_lang_ok`, `status_ok` (com ms), `render_ok`, `first_raf`,
`workspace_status` e `tauri_setup`.

```powershell
pwsh -File scripts/perf-report.ps1
```

Para registrar tambem cliques/latencia de UI (opcional), ligue em
Config -> avancado -> Manutencao -> **Registrar cliques e latencia (diagnostico)**
ou `localStorage.setItem("c2.perf","1")` no DevTools.

Otimizacoes aplicadas:
- `runtime::status` nao executa mais `llama-server --version` (isso carregava
  ~100 MB de DLLs no primeiro start); a versao fica em cache no workspace e a
  validacao roda so ao iniciar o engine;
- antes do HTTP `/health`, um connect TCP com timeout de 150 ms evita esperar
  ~2 s por uma porta fechada (gargalo medido: runtime 2037 ms -> ~160 ms);
- `workspace_status` roda em `spawn_blocking` e nao prende a thread da UI.
Com isso o status caiu de ~2,1 s para ~217 ms e o boot completo para ~1 s.

### Carregamento do modelo na GPU

O dialog de carregamento mostra uma barra que cresce com a VRAM ocupada pelo
runtime (poll de `gpu_info` a cada 500 ms) e uma interpolacao pelo tempo para
nao ficar parada enquanto o llama-server le os tensores. Sem GPU detectada,
usa a barra indeterminada. O rotulo mostra `Carregando modelo… N% (usado/total GB)`.

## Model on GPU (banner)

Com runtime local saudavel, o topo da janela mostra no centro:

```text
Model on GPU · VRAM 4.3 GB
```

- engine rodando: banner destacado (verde/accents);
- engine parado mas modelo carregado: `Model on GPU loaded`, cinza/apagado.
  O Stop do desktop mantem o runtime/modelo na VRAM para religar rapido;
  para liberar a GPU, feche o C2 (ou pare o runtime manualmente).
- a VRAM e atualizada a cada 5 s via `gpu_info`.

## Onde ficam os dados e o que o desinstalador limpa

- **Modelo entre projetos:** `%APPDATA%\dev.c2.desktop\global-model.json` guarda
  o caminho do `.gguf` e as preferencias de runtime; projetos sem modelo herdam
  esse caminho. Trocar de projeto nao apaga essa preferencia.
- **Config do projeto:** `<projeto>\.c2\config.json` (nao e removido pelo
  desinstalador; e dado do projeto).
- **Cache/janela:** `%LOCALAPPDATA%\dev.c2.desktop` (WebView2, `c2.onboarded`,
  `c2.root`, recentes).
- **Desinstalar:** o uninstaller pergunta *"Remover tambem configuracoes do C2,
  caminho do GGUF, idioma e cache deste PC?"*. Em **Yes** apaga
  `%APPDATA%\dev.c2.desktop` e `%LOCALAPPDATA%\dev.c2.desktop`. Em upgrade
  silencioso essas pastas sao preservadas. Os `.c2.json`, `.c2/` dos projetos
  e os arquivos `.gguf` **nao** sao tocados.

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
| `c2 init` | cria `.c2/config.json` (idempotente), imprime a intro e abre o desktop |
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
