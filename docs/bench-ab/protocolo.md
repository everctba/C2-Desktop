# Benchmark A/B — C2 versus leitura direta

**Beads:** `c2-e17` (épico) · `c2-e17t1` (protocolo) · `c2-e17t2` (projeto de exemplo)
**Projeto de teste:** [`examples/bench-ab/loja`](../examples/bench-ab/loja/PRD.md)

## Pergunta

Usar o C2 (metadados + grep semântico + grafo) faz um agente de IA compreender
um projeto **com menos leitura e menos erro** do que ler os arquivos direto?

## Hipóteses

| # | Hipótese | Como é medida |
|---|---|---|
| H1 | Com C2 o agente gasta **menos tokens** para responder | soma de tokens de prompt+completion e de chamadas de ferramenta |
| H2 | Com C2 o agente comete **menos erros factuais** | rubrica por tarefa (0–2 pontos), com arquivo:linha obrigatório |
| H3 | Com C2 chega à resposta correta **mais rápido** | tempo até a resposta final |

Hipótese nula: C2 não muda nada (custo/erro equivalentes).

## Condições

**A — com C2.** O agente pode usar:

```text
c2 status
c2 grep "<tarefa>" --limit 5
cat <arquivo>.c2.json        (metadado: what/why/goal/summary/responsibility/links)
c2 related <arquivo> --json
c2 read <arquivo> --lines A:B
```

**B — sem C2.** Só leitura direta (`ralph`/`view`/`grep` de arquivos). Nada de
`.c2.json`, nem do binário `c2`.

> Importante: as duas condições usam **o mesmo modelo, mesma temperatura (0),
> mesmo harness e o mesmo prompt base**. Só muda o material de contexto
> disponível.

## Tarefas (5)

1. **Ordem de cálculo** — "Em que ordem o preço final é calculado e onde está
   cada etapa?"
2. **Composição numérica** — "Pedido de 1 × R$ 500,00 com cupom `BEMVINDO10`:
   qual o total final?"
3. **Borda** — "Por que um pedido de R$ 10,00 com `FRETE20` não fica negativo?"
4. **Detalhe fácil de errar** — "O imposto usa arredondamento bancário?
   Justifique com o código."
5. **Mudança de código** — "Adicione um aviso (sem mudar o valor) quando o
   cupom for desconhecido. Quais arquivos tocar e como testar?"

Cada resposta precisa citar **arquivo:linha**; sem isso, vale no máximo 1 ponto.

## Métricas por rodada

| Métrica | Como coletar |
|---|---|
| tokens de entrada/saída | relatório do harness/modelo |
| arquivos abertos | contador do agente |
| chamadas de ferramenta | idem |
| acertos | rubrica (0–2 por tarefa, 0–10 por rodada) |
| erros sem suporte | afirmação sem arquivo:linha ou com regra invertida |
| tempo até a resposta | cronômetro do harness |

## Procedimento

1. `pwsh -File scripts/bench-ab.ps1 -Condition both` prepara duas cópias limpas
   do projeto (uma com índice C2, outra sem `.c2/`).
2. Rodar as 5 tarefas na condição **A** (3 rodadas) e na **B** (3 rodadas),
   alternando a ordem para reduzir efeito de aprendizado.
3. Registrar cada rodada no JSON gerado (`bench-ab-resultado.json`).
4. `c2-e17t5`: calcular média/desvio por condição, comparar H1–H3 e escrever o
   relatório final (incluir o custo de indexar: uma vez ~3 s/arquivo).

## Critério de sucesso do experimento

- H1: redução ≥ 30% de tokens na média, sem perda de acerto; **ou**
- H2: ganho ≥ 1 ponto médio na rubrica (0–10); **ou**
- H3: tempo médio ≥ 30% menor com acerto igual ou melhor.

Se nenhuma hipótese se confirmar, o resultado é honesto e vira material para a
landing (`c2-e18`) e para o PRD — não se maquia número.
