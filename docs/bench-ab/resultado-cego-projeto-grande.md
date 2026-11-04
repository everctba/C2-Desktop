# Rodada cega — segundo projeto (repositório do C2)

**Bead:** `c2-e17t6` · **Data:** 2026-11-04
**Projeto:** o próprio repositório do C2 — 202 arquivos, 1,70 MB, 129 metadados.
**Método:** dois subagentes novos, sem histórico do projeto e sem ver a rubrica.
Mesmas 5 tarefas. Condição A: pasta indexada + CLI do C2. Condição B: cópia sem
`.c2/` e sem `.c2.json`, só leitura/grep textual.

## Tarefas e gabarito

| Tarefa | Resposta correta |
|---|---|
| T1 | `search.rs:25/27/29` — `W_PATH 3.0`, `W_SYMBOL 2.0`, `W_SEMANTIC 1.5` (+ `PARTIAL 0.4`) |
| T2 | `indexer.rs:243-267` detecta órfão com mesmo hash → `Decision::Renamed`; `382-411` move metadado e religa links, sem LLM |
| T3 | `instalar.ps1:96-141` grava `Path`/`User` + `WM_SETTINGCHANGE`; `desinstalar.ps1:25-40` remove só `\cli` |
| T4 | `lib.rs:541-649` (`mcp_smoke`) spawna `c2 mcp`, faz `initialize`+`tools/list`, mata; `Mcp.ts:143-162` consome |
| T5 | `security.rs:95` detecta; `indexer.rs:479-508` com `Skip` bloqueia sem gravar, com `Mask` envia `[REDACTED]` |

## Resultado

| | A — com C2 | B — leitura direta |
|---|---|---|
| T1 | 2 | 2 |
| T2 | 2 | 2 |
| T3 | 2 | 2 |
| T4 | 2 | 2 |
| T5 | 2 | 2 |
| **Nota (0–10)** | **10** | **10** |
| Arquivos abertos | 13 | 12 |
| Comandos | 29 | 20 |
| Caracteres lidos | **~168.000** | **~157.000** |

Ambas as respostas citaram os pesos, as linhas e os ramos corretos; as duas
acertaram 10/10. Nenhuma alucinação em nenhuma condição.

## Leitura (mesma conclusão do piloto, agora com 200+ arquivos)

1. **Acerto empatado.** Para perguntas com resposta localizável por leitura
   dirigida, o C2 não melhora a correção — os dois agentes são competentes.
2. **Custo ligeiramente maior com C2** (168 KB vs 157 KB; 29 vs 20 comandos).
   O metadado e as saídas de CLI somam; o agente A também abriu 13 arquivos.
3. **A vantagem medida do C2 não é "gastar menos" em tarefas dirigidas** — é
   **achar o que o vocabulário não revela**: no corpus de 507 arquivos, a
   consulta "onde impede valores acima do permitido" achou em 86 ms/1,3 KB,
   enquanto o grep lexical devolveu 0 resultados e exigiria varrer 995 KB.

## O que este experimento não prova

- **Menos erro em tarefa real longa** (refatorar, adicionar feature): aqui as
  tarefas eram de localização/entendimento pontual.
- **Ganho de tokens**: para perguntas dirigidas, o caminho mais barato é ler os
  3–5 arquivos certos; o C2 só ganha quando o agente não sabe quais são.
- **Variância**: uma rodada por condição, métricas autodeclaradas.

## Conclusão honesta do programa A/B

```text
C2 vence:  recuperação por significado, projeto médio/grande, agente sem mapa
Empata:    acerto em perguntas dirigidas (e custa um pouco mais de contexto)
Não vence: projeto pequeno, pergunta cujo alvo o agente já sabe onde está
```

Isso é o que vai para a landing e para o PRD — inclusive a parte em que o C2
**não** se paga.

## Evidências

- Piloto (projeto pequeno): [resultado-piloto.md](resultado-piloto.md)
- Rodada cega (projeto pequeno): [resultado-cego.md](resultado-cego.md)
- Protocolo e rubrica: [protocolo.md](protocolo.md), [rubrica.md](rubrica.md)
