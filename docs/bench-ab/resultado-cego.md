# Rodada cega — agente externo (2026-11-04)

**Bead:** `c2-e17t6` · **Método:** dois subagentes novos, sem histórico do projeto e
sem acesso à rubrica/protocolo, recebendo as mesmas 5 tarefas.
**Projeto:** `examples/bench-ab/loja` (8 arquivos, 6,5 KB) — a condição A com
índice C2 (8 metadados) e a B com a pasta limpa.

| | A — com C2 | B — leitura direta |
|---|---|---|
| T1 ordem do cálculo | **2** | **2** |
| T2 total 437,40 | **2** | **2** |
| T3 `saturating_sub` | **2** | **2** |
| T4 `ceil` (não bancário) | **2** | **2** |
| T5 aviso de cupom | **2** | **2** |
| **Nota (0–10)** | **10** | **10** |
| Arquivos abertos (autodeclarado) | 9 | 8 |
| Comandos executados | 13 | 15 |
| Caracteres de contexto lidos | **~16.300** | **~8.500** |

## Leitura

**Acerto: empate em 10/10.** Duas respostas completas, com `arquivo:linha`,
cálculo e proposta de mudança correta. Não houve erro factual em nenhuma das
condições — em projeto pequeno, o C2 não melhora a compreensão.

**Custo: o C2 gastou ~1,9× mais contexto** (16,3 KB × 8,5 KB). Metadados +
status + greps + leitura de confirmação somam mais que ler 8 arquivos de 6,5 KB.
Isso **repete o resultado do piloto**: em projeto minúsculo, ler direto é mais
barato.

## O que foi além do gabarito (qualidade da análise)

- **B** encontrou uma falha real no teste do projeto: `imposto_centavos(1_234)`
  dá 99 tanto com `ceil` quanto com `round`, então o teste **não discrimina** o
  arredondamento que diz testar; o caso que separa é `1_013 → 82` (ceil) × 81
  (round). Achado que eu não tinha registrado.
- **A** citou o PRD e observou que `BEMVINDO10` usa subtração simples (segura
  por construção, mas não declarada), enquanto `FRETE20` depende do saturante.

## Limitações (por que isso ainda não fecha a pergunta)

1. **Uma rodada** — sem intervalo de confiança.
2. **Métricas autodeclaradas** pelos agentes — ordem de grandeza, não precisão.
3. **Mesmo modelo base** nas duas condições.
4. **Projeto pequeno**: é justamente o cenário onde o índice não compensa.
   A vantagem medida no corpus de 507 arquivos (consulta semântica em 86 ms /
   1,3 KB contra 0 resultados do grep lexical) **não** foi exercitada aqui.

## Próximo passo (ainda em `c2-e17t6`)

Repetir a rodada cega com um projeto de **100+ arquivos** e tarefas cujo
vocabulário não existe literalmente no código (ex.: "onde impede valores acima
do permitido"), que é onde o grep semântico mostrou vantagem. Se nesse cenário
o acerto empatar e o custo do C2 cair abaixo do direto, a hipótese H1 se
confirma; se empatar em custo também, o C2 não se paga e isso precisa ir para o
PRD e para a landing sem maquiar.
