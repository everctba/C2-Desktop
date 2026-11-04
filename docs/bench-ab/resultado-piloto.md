# Resultado do piloto A/B — C2 versus leitura direta

**Data:** 2026-11-04 · **Beads:** `c2-e17t5` (análise), `c2-e17t6` (repetir com agente externo)
**Protocolo:** [protocolo.md](protocolo.md) · **Rubrica:** [rubrica.md](rubrica.md)
**Projeto pequeno:** `examples/bench-ab/loja` · **Corpus grande:** 500 arquivos sintéticos (`scripts/bench.ps1`)

> **Aviso de método:** este é um **piloto**, uma rodada, executado pelo próprio
> autor do projeto. Isso enviesa a etapa de "compreensão" (eu já sei as
> respostas), então a nota de acerto não diferencia as condições. O que o piloto
> mede de verdade é **custo de contexto** e **recuperação da informação**.

## 1. Projeto pequeno (7 arquivos, 6,5 KB)

| Métrica | A — com C2 | B — leitura direta |
|---|---|---|
| Custo de indexação | **28,3 s** · 8 chamadas · 4.985 tokens | 0 |
| Dados consultados para responder as 5 tarefas | **15,9 KB** (`status` 2,4 KB + 5 greps 10,1 KB + metadados 3,4 KB) | **6,5 KB** (ler os 6 arquivos-fonte) |
| Ferramentas | 7 | 6 leituras |
| Acerto na rubrica (0–10) | 10 | 10 |

**Leitura:** em projeto minúsculo o C2 **não compensa** — o metadado
(~500–800 B por arquivo) é quase do tamanho do código (600–1.600 B), e ainda se
paga a indexação. Ler tudo direto é mais barato.

## 2. Corpus grande (507 arquivos, 995 KB)

Consulta semântica cujas palavras **não existem literalmente** no código:
`"onde impede valores acima do permitido"`.

| Métrica | A — com C2 (grep semântico) | B — grep lexical direto |
|---|---|---|
| Resultado | **achou** (`src/mod01/arquivo_001.rs`, score 10,20) | **0 matches** |
| Custo do contexto | **1,3 KB** | 0 B (mas sem resposta) |
| Tempo da consulta | **86 ms** | 150 ms |
| Para responder, teria que | — | ler até **995 KB** (o corpus inteiro) ou adivinhar outras palavras |

**Leitura:** quando o vocabulário da tarefa não casa com o código, a busca
lexical falha e o agente é forçado a varrer o projeto. O C2 responde com 1,3 KB
em 86 ms.

## 3. Onde o custo está

```text
C2 = custo fixo alto (indexação) + custo marginal baixo (consulta)
direto = custo zero fixo + custo marginal alto (ler arquivos)
```

Com os números medidos nesta sessão:

- indexação: **~4,4 s/arquivo** (Gemma 4 E2B Q4, GPU 8 GB) — 501 arquivos em 36,7 min, uma vez;
- consulta com C2: **1–2 KB** e **60–90 ms** (grep); metadado de ~650 B por arquivo;
- leitura direta: custo = tamanho dos arquivos que o agente precisa abrir.

Ponto de equilíbrio aproximado, assumindo que o agente precise de ~10% dos
arquivos para responder e que um arquivo médio tenha 2 KB:

```text
projeto pequeno   (≤ 20 arquivos, arquivos ~700 B): ler direto ganha
projeto médio     (100–500 arquivos):               C2 ganha (grep + metadado)
projeto grande    (milhares de arquivos):           C2 ganha muito (varredura inviável)
```

## 4. O que o piloto NÃO responde

- **Taxa de erro**: com o autor respondendo, as duas condições acertaram 10/10.
  Precisa de agente **sem conhecimento prévio** e de uma rodada cega.
- **Tokens reais do modelo**: usei bytes de contexto como proxy; o número de
  tokens depende do tokenizador do modelo do harness.
- **Variância**: uma rodada só, sem intervalo de confiança.

## 5. Próximo passo (`c2-e17t6`)

1. Rodar com **agente externo** (outro modelo/harness, sem ver este relatório).
2. 3 rodadas por condição, alternando a ordem, no projeto `loja` **e** num
   projeto de 100+ arquivos.
3. Medir tokens reais (prompt+completion), chamadas de ferramenta, tempo e a
   rubrica de 0–10.
4. Se a variância for alta, repetir com o segundo projeto antes de concluir.

## 6. Como reproduzir

```powershell
pwsh -File scripts/bench-ab.ps1            # prepara as duas pastas e indexa A
# condicao B: ler os arquivos direto; condicao A: c2 status/grep/metadado/read
pwsh -File scripts/bench.ps1 -Files 500    # corpus grande para a consulta semantica
```
