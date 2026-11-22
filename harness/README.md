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

Ainda pendente:

- extractors AST para Rust/Python; TS/HTML/CSS ainda heurísticos;
- gate de testes configurável via `C2H_TEST_CMD`;
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
