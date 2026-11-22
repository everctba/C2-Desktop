# C2 Harness (`c2h`)

CLI experimental para enviar prompts a uma IA OpenAI-compatible, compilar
contexto por blocos e manter anotações em sidecar.

## Estado

MVP:

- `doctor`
- `blocks <arquivo>`
- `annotate <arquivo> [--dry-run]`
- `validate [arquivo]`
- `context <arquivo> [--max-chars N]`
- `ask "<prompt>" [--system TEXT]`
- `ask --file <arquivo> [--apply] "<tarefa>"`
- `validate-response <response.json>`
- `apply <response.json> [--dry-run]`
- `status`
- `logs [--limit N]`
- `fuzz [--iters N]`
- `config`

Implementado no MVP:

- contrato JSON + validator + reparo (até `--max-repairs`);
- aplicação de patch com backup e rollback em falha de formatter;
- `rustfmt` para `.rs`;
- persistência de W5H2/evidence no sidecar;
- `C2H_MOCK_RESPONSE` para testar sem API.

- navegacao e escrita interativa (`/ls`, `/read`, `/write`, `/new component`,
  `/edit`, `/rm`, `/mv`).

Ainda pendente:

- extractors AST para Rust/Python; TS/HTML/CSS ainda heurísticos;
- atualizacao automatica de imports em `/mv`;
- preview com screenshot.
- streaming;
- behavior cloning/embeddings.

## Provider

O `c2h` carrega automaticamente `config.json` na pasta do executavel:

```json
{
  "base_url": "https://api.deepseek.com/v1",
  "model": "deepseek-chat",
  "api_key": "",
  "contract_version": 1
}
```

Edite:

```text
harness/no_path/config.json
```

Variaveis de ambiente tem prioridade:

```powershell
$env:C2H_BASE_URL="https://api.deepseek.com/v1"
$env:C2H_MODEL="deepseek-chat"
$env:C2H_API_KEY="..."
c2h ask "explique este projeto"
```

Aceita também:

```text
DEEPSEEK_API_KEY
OPENCODEGO_API_KEY
C2H_CONFIG=<caminho de outro config.json>
```

`c2h config` mostra os valores efetivos e o caminho do config, sem a chave.

Nada é escrito no código pelo `ask`; ele apenas imprime a resposta.

## Sidecar

```text
.c2h/blocks/<arquivo>.json
```

Cada bloco tem:

- `id` estável;
- `created_at` / `updated_at`;
- `semantic_hash`;
- `w5h2`;
- `evidence`.

O harness é dono do timestamp e do hash. A IA não escreve timestamp.

## Modo interativo

```bash
c2h
```

Abre um chat no terminal:

```text
Bom dia! Sou o c2h...
projeto: ...
modelo: ...
c2h>
```

Comandos:

```text
/pwd /ls [dir] /cd <dir> /tree [n] /read <f> [a:b] /find <texto>
/imports <arquivo>
/write <arquivo> "instrucao"
/new component <Nome> [--html] [--css] [--js]
/edit <arquivo> "instrucao"
/rm <arquivo>
/mv <origem> <destino>
/preview <arquivo.html>
/file <arquivo> /drop /clear /status /doctor /logs [n] /help /sair
```

## Context compiler automatico

Antes de cada mensagem no chat, o harness monta contexto com:

```text
projeto, cwd, arquivo ativo
blocos/W5H2 relevantes (ate 5 arquivos)
imports do arquivo ativo
git diff --stat
```

Logs de tool sao redigidos: `content`/`new_code` viram `*_chars` + `*_hash`,
nunca o conteudo completo. `tool_result_preview` e limitado a 240 chars.

O annotation pass tem cache por `source_hash` em
`.c2h/cache/annotations/<hash>.json`; arquivo inalterado reaproveita os blocos.

O contexto entra no system message; nao depende mais de `/file`. Ele inclui
`ARQUIVOS NO CWD` reais e ignora sidecars de arquivos removidos.

Politica de injecao:

```text
so bloco com why tecnico
so bloco com semantic_hash igual ao conteudo atual
blocos stale nao entram
ranking: ativo > cwd > risk high > match da tarefa > densidade de why
uma linha por bloco: arquivo:linha — what — why [— how]
comentarios apenas dos 2 arquivos mais bem ranqueados
budget ~5000 chars de blocos
```

O log `context_compiled` traz `stale_skipped` e `empty_why_skipped`. O log registra
`context_compiled` com arquivos, blocos e caracteres.

Se `read_file` receber caminho inexistente, o erro volta como resultado de
ferramenta com a lista de arquivos disponiveis; o agente pode tentar de novo em
vez de abortar.

Ferramentas `read_file`/`list_dir` tambem devolvem anotacoes/status do sidecar:

- `read_file`: cabecalho com `TOTAL`/`MOSTRANDO`, pagina padrao de 120 linhas,
  `start`/`end`, codigo + W5H2 completo;
- `list_dir`: nome + `[sidecar status=... blocks=...]`;
- no maximo 2 leituras por arquivo por turno; leitura identica e cacheada;
- resultado de tool acima de 12000 chars e truncado com aviso.

Toda mudanca no contrato exige `evidence` com `arquivo:linha`; sem isso o
validator retorna `E-EVID-00`.

## Annotation pass e sidecar

Depois de criar um arquivo, o sidecar nasce com:

```json
{"status":"unannotated","blocks":[],"source_hash":"sha256:..."}
```

Se `blocks` estiver vazio **ou** o arquivo tiver mais de 150 linhas com menos
de 2 blocos, o harness roda um **annotation pass**:

```text
arquivo -> IA descobre blocos -> validator -> sidecar annotated
```

Cada bloco guarda `start_line`/`end_line`, hash, W5H2 e timestamps. O `/edit`
funciona com esses blocos e, se o sidecar estiver vazio, dispara a descoberta
antes.

Gatilhos do annotation pass:

```text
blocks vazio
qualquer bloco sem why tecnico
arquivo > 120 linhas com 1 bloco
arquivo > 150 linhas com menos de 2 blocos
bloco cobrindo > 150 linhas
bloco high-risk sem why
```

Structural annotate com bloco sem why fica `status: unannotated`; o annotation
pass e disparado no write/replace/annotate. Arquivo pequeno com 2 blocos tambem
passa a ser anotado.

Qualidade estrutural (diagnóstico, sem bloquear):

- arquivo > 150 linhas gera warning;
- blocos grandes demais marcam o sidecar como `status: weak`;
- a sugestão de split é opt-in: `C2H_SUGGEST_SPLIT=1`;
- não existe `--max-lines` bloqueante por enquanto.

Comandos:

```text
/annotate <arquivo>   descobre blocos com IA
```

Politicas genericas (sem JavaScript, tamanho, imports) e um critico de
requisitos sao aplicados antes de gravar. O annotation pass nao inventa
timestamps: o harness carimba.

## Agent loop com ferramentas

No chat, o `c2h` agora envia ferramentas ao modelo e executa `tool_calls`:

```text
list_dir, read_file, find_text, write_file, edit_file,
new_component, move_file, remove_file, run_test
```

O usuario pode pedir em linguagem natural:

```text
c2h> crie player.html com layout de player mp3, sem JS
```

O modelo escolhe `list_dir`, `write_file` etc. O harness valida, mostra preview,
pede confirmacao e grava. `/tools` lista as ferramentas; `/auto` desliga a
confirmacao de escrita (use com cuidado).

`/write` continua disponivel como atalho explicito. `/new component` cria um
scaffold HTML/CSS/JS. `/edit` reutiliza o contrato de blocos e o test gate.

`/doctor` mostra config/diagnostico e `/logs [n]` mostra os ultimos eventos da
timeline sem sair do chat.

## Assinatura de arquivos gerados

O harness grava no topo dos arquivos que ele cria/substitui:

```text
<!-- generated by C2 Harness -->   HTML/MD
// generated by C2 Harness        JS/TS/Rust/Go/C
/* generated by C2 Harness */     CSS
# generated by C2 Harness         Python/shell/YAML/TOML
```

Para desativar:

```powershell
$env:C2H_SIGN="0"
```

O sidecar tambem registra `generated_by` (`c2h`, `c2h-ai`, etc.) e
`origin` por bloco.

## Goal e business_logic opcionais

O W5H2 aceita dois campos opcionais:

```json
{
  "goal": "resultado observavel, ex: doc_load(package.json) retorna null",
  "business_logic": "ordem/invariante/excecao, ex: listen antes de upgrade"
}
```

Nao sao obrigatorios e so entram no contexto/read_file quando preenchidos.
`goal` deve ser mensuravel; `business_logic` deve ser regra, ordem ou excecao,
nao descricao generica.

## Risco e revisao do why no edit

Blocos de I/O, formato, parsing, persistencia e validacao recebem
`risk: high` (heuristica + IA). Se um bloco high-risk estiver sem why tecnico,
o sidecar fica `weak` e o annotation pass e disparado.

`edit_file` agora exige `why_review`:

```json
{"path":"doc.js","block_id":"doc.js::fn::formatOf","new_code":"...","why_review":"o why atual diz que formatOf deve retornar null para extensao desconhecida; a edicao preserva isso"}
```

O harness compara o review com o why atual. Sem confronto, retorna
`E-WHY-REVIEW` e mostra o why atual.

## Substituicao atomica

Para arquivo existente, use `replace_file`:

```json
{"name":"replace_file","args":{"path":"src/index.js","content":"...","summary":"..."}}
```

Fluxo: valida path, backup, tmp, rename atomico, sidecar e gates.
`write_file` e somente para arquivo novo; nunca use `remove_file` para editar.

## Politica de escopo e cobertura

Por padrao o annotation pass ignora:

```text
tmp/ target/ dist/ node_modules/ vendor/ generated/ .git/ .c2h/
probe-* fix-*
```

O contexto mostra:

```text
ARQUIVOS CRITICOS SEM ANOTACAO:
- app/server.js
- src/live/client.js
```

E o ranking considera:

```text
arquivo ativo
git diff
imports do ativo
cwd
risk high/medium
match da tarefa
densidade do why
```

Blocos `confidence=low` nao entram. Cada bloco carrega
`confidence` e `verified_by`.

## Qualidade do W5H2

O `why` de anotacao/edicao precisa ser tecnico e teleologico: proposito,
problema evitado, consequencia. `why_tem_consequencia` exige marcadores como
`porque`, `evita`, `impede`, `sem isso`, `falha`, `corrompe`. `why` mecanico
(`chama`, `registra`, `inicializa`) e rejeitado no `annotate_blocks`/`edit_file`.
O contexto ainda inclui why tecnico sem consequencia, mas ranqueia pior.

Blocos com `why` vazio nao entram no contexto nem no `read_file`. O `edit_file`
bloqueia com `E-WHY-REVIEW` se o bloco ainda nao tem why tecnico; use
`annotate_blocks` primeiro.

O agente pode atualizar anotacoes sem tocar no codigo:

```json
{"name":"annotate_blocks","args":{"blocks":[
  {"block_id":"doc.js::lib","what":"...","why":"read_file e async porque descomprime dados","when":"...","where":"...","who":"...","how":"...","how_much":"...","evidence":["doc.js:4-10"]}
]}}
```

Cada bloco tem `origin`: `structural`, `annotation`, `write_file` ou
`agent_annotation`.

O context compiler inclui comentarios/docstrings relevantes do arquivo, nao
apenas o W5H2. O annotation pass tambem recebe esses comentarios e pede why
tecnico.

## Tool `git`

O binario git instalado pode ser chamado sem shell:

```json
{"name":"git","args":{"args":["status"]}}
{"name":"git","args":{"args":["clone","--depth","1","https://github.com/...","destino"]}}
```

- leitura automatica: status/diff/log/show/branch/rev-parse/remote/tag/ls-files;
- escrita com confirmacao: clone/pull/fetch/add/commit/checkout/restore/switch/merge/rebase/reset/clean/push/init/mv/rm/stash;
- YOLO pula confirmacao;
- cwd no root do c2h;
- sem shell.

## Bypass de contexto via shell

O shell registra quando o agente usa comandos de leitura/escrita de codigo em
vez das tools de contexto:

```json
{"event":"shell_bypass","data":{"kind":"read_code","file":"moebius/app/server.js"}}
```

Leitura de codigo por shell e bloqueada com `E-SHELL-BYPASS`:

```text
E-SHELL-BYPASS: leitura de codigo por shell bloqueada: a.rs.
Use read_file(path="a.rs") ou find_text.
```

Detecta `type/cat/findstr/grep/rg/Get-Content/Select-String/head/tail` sobre
arquivos de codigo. Testes/build (`npm test`, `cargo test`, `node tools/smoke*`)
sao classificados como `test_build` e permitidos. Desconhecido e permitido com
confirmacao normal e log `shell_call.class=unknown`.

Redirecionamento `>`/`>>` para arquivo de codigo e bloqueado:

```text
E-SHELL-WRITE: redirecionamento para codigo bloqueado: a.rs.
Use write_file/replace_file.
```

Redirecionar para `tmp/*.log` continua permitido; `node tools/smoke.js > tmp/reg.log`
nao e classificado como escrita de codigo. O system prompt avisa para nao usar
shell na leitura/escrita de codigo.

## Visualizacao das tools

O chat mostra o alvo da tool:

```text
-> shell: git --version
-> read_file: player-test.html (1-120)
-> replace_file: src/index.js
-> edit_file: src/store.rs :: src/store.rs::fn::add
-> bd: ready
-> annotate_blocks: 20 blocos
```

O log `tool_call` guarda `command_head`/`command_chars` para shell e os
metadados redigidos, sem o conteudo completo.

## Shell: opt-in

O shell livre e **desligado por padrao**. Sem ele, o agente usa:

```text
read_file / find_text
write_file / replace_file / edit_file
run_test / run_script
git
bd
```

Para ligar:

```powershell
c2h --allow-shell
# ou
$env:C2H_ALLOW_SHELL="1"
```

No chat:

```text
/shell on
/shell off
/shell
```

Com shell off, a tool `shell` nem aparece no schema e chamadas retornam
`E-SHELL-DISABLED`.

## run_test / run_script

Crie `.c2h/commands.toml`:

```toml
[commands]
test = "node tools/smoke-live.js"
test_all = "node tools/smoke-mcp.js && node tools/smoke-doc-tools.js"
build = "npm run build"
smoke-live = "node tools/smoke-live.js"
smoke-doc = "node tools/smoke-doc-tools.js"
draw-cli = "node tools/draw-cli.js"
```

Tools:

```json
{"name":"run_test","arguments":{"name":"test"}}
{"name":"run_script","arguments":{"name":"draw-cli"}}
```

Sem shell string livre; o harness executa o comando nomeado, no root, e loga
`run_test`/`run_script` com exit code. Tambem aceita `.c2h/commands.json` como
fallback e `C2H_TEST_CMD` para `test`. BOM UTF-8 e tolerado.

## Tool `shell` (livre)

O agente pode executar comando de shell livre:

```json
{"name":"shell","args":{"command":"git --version","timeout_seconds":60}}
```

- roda via `cmd /C` (Windows) ou `sh -c`;
- cwd no root do c2h;
- captura stdout/stderr/exit;
- timeout 1..600s;
- pede confirmacao, exceto em YOLO;
- **executa com as permissoes do seu usuario**: nao ha sandbox/ACL.

## Tool `bd`

O agente pode executar somente o binario `bd`, sem shell:

```json
{"name":"bd","args":{"args":["ready"]}}
```

- `ready/list/show/status/prime/help/version` sao leitura;
- `init/create/update/close/delete/remember/dep/sync/import/export` pedem confirmacao;
- `cwd` e o root do c2h; o `bd` resolve o `.beads` no repo;
- sem pipes, sem redirect, sem shell;
- saida limitada a 8000 chars;
- em YOLO, escrita roda sem confirmacao.

## Limite de passos

O agent loop tem limite configuravel:

```powershell
$env:C2H_MAX_STEPS="24"
c2h
```

Padrao: 16. Se o limite chegar ao fim, o harness faz uma ultima chamada **sem
tools** pedindo ao modelo que responda com o que ja coletou. O evento
`agent_forced_final` registra isso. Para repos grandes, 24-32 pode ser melhor,
mas aumenta custo/latencia.

## YOLO

```powershell
c2h --yolo
# ou
$env:C2H_YOLO="1"
```

Tambem dentro do chat:

```text
/yolo
```

Nesse modo, `write_file`, `edit_file`, `/rm` e `/mv` nao pedem confirmacao.
Use com cuidado.

`c2h chat` abre o mesmo modo explicitamente. O histórico da conversa fica em
memória; nada é gravado em disco além dos logs de debug.

## Debug timeline

```bash
c2h --debug ask --file src/a.rs --apply "..."
c2h logs --limit 40
```

Grava `.c2h/logs/run-<timestamp>-<pid>.jsonl` com eventos:

```text
run_start, contract_attempt, provider_request, provider_mock,
contract_violations, contract_parse_error, apply_start, apply_write,
formatter_fail, test_gate_ok, test_gate_fail, annotate
```

O conteúdo do prompt não é gravado; só tamanhos, códigos e métricas. Use
`C2H_DEBUG=1` em vez de `--debug` para ativar por ambiente.

## Fuzz

```bash
c2h fuzz --iters 1000
```

Gera envelopes válidos, incompletos, truncados, com path traversal e lixo.
Não aplica patch: testa parser e validator contra panic.

## Laboratório

`lab/` é um projeto pequeno com um teste que falha de propósito:

```bash
cd lab
c2h annotate src/store.rs
c2h ask --file src/store.rs --apply "corrija titulo vazio e faca o teste passar"
C2H_TEST_CMD="cargo test" c2h apply resposta.json
```

Ele tem Rust, Python e HTML one-page para exercitar extractors e blocos.

## Build no_path

```powershell
pwsh -File scripts/build-no-path.ps1
```

Saída:

```text
harness/no_path/c2h.exe
```

Adicione `harness/no_path` ao PATH do usuário para testar a versão atualizada.

## Nota

Rust e Python usam tree-sitter/AST. TS, HTML, CSS e Markdown usam
extratores heurísticos por enquanto.
