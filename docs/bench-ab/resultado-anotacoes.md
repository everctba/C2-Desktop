# Medição: anotações de agente (`c2_annotate`)

**Data:** 2026-11-05 · **Bead:** c2-zup.3 · **Objetivo:** medir quantas
anotações um agente produz com a política teleológica e classificar a qualidade.

## Método

- 3 tarefas independentes em workspaces separados, cada uma com um agente
  **fresh** (mesmo modelo, sem memória das outras tarefas);
- projeto Rust sintético (5 arquivos) com metadados `.c2.json` plausíveis, mas
  **sem** a teleologia plantada (o agente tinha que descobri-la no código);
- instrução: protocolo C2 (ler `.c2.json` antes do código) + **pensamento
  teleológico antes de anotar** (por que existe? o que quebra se remover? o que
  o código não revela?) + máximo 2 anotações por tarefa; 0 era resposta válida;
- anotação via `c2 note add` (mesmo core da tool MCP `c2_annotate`).

### Tarefas

| # | Tarefa | Teleologia escondida |
|---|---|---|
| T1 | corrigir total do pedido dessincronizado | cache denormalizado + racional já documentado no código |
| T2 | corrigir dedupe do webhook | posse do `HashSet` era do chamador; `pedido_id` é armadilha; dedupe é memória de processo |
| T3 | implementar `aplicar_cupom` | `aplicar_desconto` pressupõe elegibilidade validada; `recalcular()` descarta desconto |

## Resultados

| Tarefa | Anotações | Kinds | Úteis (não óbvias) | Ruído |
|---|---:|---|---:|---:|
| T1 | 0 | — | — | 0 |
| T2 | 2 | gotcha, invariant | 2 | 0 |
| T3 | 2 | gotcha, invariant | 2 | 0 |
| **Total** | **4** | gotcha 2 · invariant 2 | **4 (100%)** | **0 (0%)** |

- **adesão:** 2 de 3 tarefas (67%) produziram anotação;
- **densidade:** média de **1,33 anotação/tarefa**, máximo 2 (limite respeitado);
- **ruído:** nenhuma anotação descrevendo o óbvio (ex.: “aplica desconto”);
- **T1 decidiu não anotar** com justificativa explícita: o racional já estava no
  comentário/doc do código — comportamento desejado (não duplicar o que o
  código já diz).

## Amostras (T2 e T3)

> **T2 · gotcha** — “O id do evento NÃO vem do corpo aqui… Como o HashSet era do
> chamador, um conjunto novo por entrega reabilitava o evento… `pedido_id` segue
> fora da chave de propósito: o mesmo pedido recebe eventos distintos.”

> **T2 · invariant** — “Dedupe é memória do processo: reinício, segundo worker
> ou segunda réplica começam vazios e reprocessam… para rodar com mais de uma
> instância, o id aceito precisa ir para um store compartilhado.”

> **T3 · gotcha** — “`aplicar_desconto` checa apenas `cupom.ativo`, não
> `minimo_centavos`… chamada direta concede desconto abaixo do mínimo.”

> **T3 · invariant** — “`recalcular()` zera o total… um desconto aplicado se
> perde se `recalcular()` rodar depois; nada no código liga `pedido.rs` a
> `cupom.rs`.”

## Leitura

O gatilho teleológico funcionou como filtro: os agentes não descreveram o que o
código faz (isso já está em `what`/`summary`) e sim o que **não** é derivável —
pré-condições implícitas, posse de estado, efeitos entre arquivos. O caso T1
mostra o outro lado: quando a explicação já está no código, 0 anotações é a
resposta correta.

## Limitações

- projeto sintético e 3 tarefas — não é uma amostra estatística;
- mesmo modelo/instruções; agentes de IDE podem ter atrito de permissão para
  escrita via MCP (aproximado aqui pelo CLI);
- não mede envelhecimento das anotações (stale) nem ganho para o próximo agente;
- notas foram revisadas manualmente (classificação “útil” é julgamento).

## Próximo passo

Repetir em 3–5 tarefas **reais** no repositório do C2 (ou projeto do usuário) e
comparar: notas por tarefa, taxa de ruído e quantas notam `correction`/`pending`.
