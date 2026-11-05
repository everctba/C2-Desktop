# Medição 2: anotações de agente em tarefas reais (C2)

**Data:** 2026-11-05 · **Bead:** c2-zup.3 (2ª rodada) · **Objetivo:** medir
adesão e qualidade das anotações em tarefas reais do repositório do C2.

## Método

- 3 tarefas reais (fatias de `c2-kf1.2`), cada uma numa **cópia isolada do
  repositório** com agente fresh;
- protocolo C2: ler `<arquivo>.c2.json` antes do código, `c2 grep`/`c2 read`;
- política teleológica: só anotar o que não for derivável do código; máximo 2
  anotações; 0 era resposta válida;
- anotação via `c2 note add` (mesmo core da tool MCP `c2_annotate`).

### Tarefas

| # | Tarefa real |
|---|---|
| T1 | Card RUNTIME em Projetos no modo externo (`openai_compat`) — "gerenciado externamente" |
| T2 | `c2 status` humano: linha de provedor + custo configurado |
| T3 | Card Provedor de IA: resumo do custo configurado (só no modo nuvem) |

## Resultados

| Tarefa | Anotações | Kinds | Úteis | Ruído | Decisão |
|---|---:|---|---:|---:|---|
| T1 | 1 | gotcha | 1 | 0 | anotou |
| T2 | 0 | — | — | 0 | **não anotou** (justificado) |
| T3 | 2 | invariant, gotcha | 2 | 0 | anotou |
| **Total** | **3** | gotcha 2 · invariant 1 | **3/3** | **0/3** | 2/3 anotaram |

```text
adesão:      2 de 3 tarefas (67%)
densidade:   1,0 anotação/tarefa (limite 2 respeitado)
ruído:       0 (nenhuma anotação descrevendo o óbvio)
tempo:       ~10–12 min para as 3 tarefas em paralelo
```

## Amostras

**T1 · gotcha — o achado mais valioso**

> “Em `openai_compat`, `status.runtime` não descreve o servidor que o C2 usa:
> `healthy` vem de um GET em `model.health_url()` e, no modo externo, ela
> devolve o `base_url` cru — sem `/health` e sem API key. Logo o card pode dizer
> saudável por um 200 da raiz do provedor (falso positivo) ou parado com LM
> Studio/Ollama no ar. A validação real é o **Testar conexão**.”

**T3 · invariant**

> “‘Estimativa desligada’ só com os **dois** preços zero: `estimar_plano` zera
> apenas com entrada ≤ 0 **e** saída ≤ 0; checar por campo (`||`) faria a tela
> contradizer o backend.”

**T3 · gotcha**

> “O ‘teto por execução’ não é limite rígido: `indexer.rs` compara
> `cost_usd >= teto` **depois** de indexar cada arquivo; a execução pode passar
> do teto pelo custo do último arquivo.”

**T2 · decisão de não anotar**

> “Os quatro candidatos eram deriváveis do código… O que restou de não derivável
> era ruído de processo (ausência de build nesta cópia), não conhecimento sobre
> o código.”

## Leitura

- A adesão se manteve (67%) e o **filtro teleológico seguiu funcionando**: 0
  anotações óbvias, e a tarefa T2 recusou anotar com justificativa correta;
- as anotações apontaram **problemas reais** que não estavam no escopo das
  tarefas (runtime saudável falso; teto soft) — exatamente o tipo de
  conhecimento que se perde entre sessões;
- kinds usados: `gotcha` e `invariant`; `correction`/`pending` continuam sem
  aparecer em tarefas de implementação (esperado).

## Achados → tarefas de follow-up

1. **Runtime externo**: não usar `status.runtime` para decidir se o provedor
   está no ar; corrigir `health_url`/status no modo `openai_compat`.
2. **Teto de custo**: pré-checar a estimativa do próximo arquivo antes da
   chamada (ou documentar explicitamente como “teto aproximado”).
3. **Resumo de custo no card** (T3): promover o protótipo da cópia para o app.

## Limitações

- 3 tarefas, mesmo modelo/instruções, cópias sem build (agentes não compilaram);
- “útil” é julgamento manual; não mede o ganho para um agente futuro nem a
  obsolescência das notas.
