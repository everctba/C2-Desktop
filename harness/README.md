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
/file <arquivo> /drop /clear /status /help /sair
```

## Context compiler automatico

Antes de cada mensagem no chat, o harness monta contexto com:

```text
projeto, cwd, arquivo ativo
blocos/W5H2 relevantes (ate 5 arquivos)
imports do arquivo ativo
git diff --stat
```

O contexto entra no system message; nao depende mais de `/file`. Ele inclui
`ARQUIVOS NO CWD` reais e ignora sidecars de arquivos removidos. O log registra
`context_compiled` com arquivos, blocos e caracteres.

Se `read_file` receber caminho inexistente, o erro volta como resultado de
ferramenta com a lista de arquivos disponiveis; o agente pode tentar de novo em
vez de abortar.

Ferramentas `read_file`/`list_dir` tambem devolvem anotacoes/status do sidecar:

- `read_file`: codigo + W5H2 completo (what/why/when/where/who/how/how_much);
- `list_dir`: nome + `[sidecar status=... blocks=...]`.

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
arquivo > 120 linhas com 1 bloco
arquivo > 150 linhas com menos de 2 blocos
bloco cobrindo > 150 linhas
```

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
