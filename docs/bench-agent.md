# Benchmark de adoção do C2

Tasks `c2-e54` e critérios de produto.

## Os três critérios

1. **Agente novo resolve a tarefa com menos leituras de código?**
2. **Agente usa o C2 sem ser lembrado?**
3. **Metadata reduz erro de modelo mental?**

## Como medir

### 1 e 2 — leituras e adoção

O C2 registra uso local em `.c2/logs/usage.jsonl`.

```bash
c2 stats
c2 stats --json
```

Campos:

```json
{
  "calls": 12,
  "code_reads": 3,
  "meta_reads": 4,
  "annotations": 1,
  "preflight_before_code": true,
  "by_actor": { "mcp": 10, "cli": 2 },
  "by_tool": { "c2_grep": 4, "c2_read": 3 }
}
```

- `code_reads`: quantas vezes a IA abriu código;
- `meta_reads`: metadados efetivamente lidos;
- `preflight_before_code`: usou `grep/related/similar/brief` **antes** da primeira leitura de código — indicador de adoção sem lembrete.

Comparação:

```text
baseline: agente sem C2/metadata
c2:       agente com C2 ativo
```

Meta: menos `code_reads` e mesma taxa de acerto.

### 3 — modelo mental

O harness cria quatro módulos com classificações conhecidas:

```text
audio, ui, mcp, cli
```

O agente deve preencher `answer.json`. Mede-se:

```text
correct / expected
```

Erro de modelo mental = classificação errada ou confiança em metadata errado.

## Rodar

```powershell
pwsh -File scripts/bench-agent.ps1 `
  -AgentCommand "pwsh -File C:\meu-agente.ps1" `
  -Mode c2

pwsh -File scripts/bench-agent.ps1 `
  -AgentCommand "pwsh -File C:\meu-agente.ps1" `
  -Mode baseline
```

O comando do agente recebe:

```text
-Root <projeto> -Prompt <TASK.md>
```

Saída: `docs/bench-agent.json`.

## Interpretação

```text
baseline 12 code_reads, 4/4 acertos
c2        3 code_reads, 4/4 acertos, preflight true
→ C2 pagou o custo
```

Se o C2 reduz leitura mas piora acerto, o metadata está enganando o agente.
Se o `preflight_before_code` é falso, o agente não adotou o C2 espontaneamente.

## Limitações

- o agente é externo; o harness não embute um modelo;
- um fixture pequeno não prova repo grande;
- rodar baseline e c2 em sessões separadas para evitar contaminação de contexto.
