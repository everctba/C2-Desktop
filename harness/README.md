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
- `config`

Ainda não implementado:

- contrato completo de resposta + reparo;
- aplicação de patch;
- formatter/testes/rollback;
- extractors AST completos;
- streaming;
- no_path automático no PATH.

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

O extractor atual é heurístico por linhas. A evolução é tree-sitter/AST e o
contrato de resposta com validação/reparo.
