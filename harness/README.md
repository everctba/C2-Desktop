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
```

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
