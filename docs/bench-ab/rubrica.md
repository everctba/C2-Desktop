# Rubrica e gabarito — benchmark A/B

Cada tarefa vale **2 pontos**: 2 = completo e correto com arquivo:linha;
1 = parcial (regra certa sem citar arquivo, ou arquivo certo com detalhe errado);
0 = errado/sem evidência.

## T1 — Ordem de cálculo (2 pts)

**Resposta esperada:** o total é `subtotal → desconto progressivo por volume →
cupom → imposto`, nessa ordem.

- `src/lib.rs` (`total_do_pedido`): encadeia `pricing::aplicar_desconto_progressivo`,
  `coupons::aplicar_cupom`, `tax::aplicar_imposto`.
- `src/pricing.rs` (`percentual_por_subtotal`): faixas de 5% e 10%.
- `src/coupons.rs` (`aplicar_cupom`): `BEMVINDO10` = 10%, `FRETE20` = 20,00.
- `src/tax.rs` (`aplicar_imposto`): 8% sobre o valor após cupom.

**Perde ponto:** dizer que o cupom vem antes do desconto, ou omitir o imposto.

## T2 — Composição numérica (2 pts)

**Resposta esperada:** `R$ 437,40` (43 740 centavos).

Conta: 500,00 → −10% = 450,00 → −10% do cupom = 405,00 → +8% = 437,40.
Aceitar 437,40; **não** aceitar 405,00 (esqueceu imposto) nem 486,00.
Evidência: `tests/pedido.rs` (`cupom_percentual_aplica_sobre_o_valor_ja_descontado`).

## T3 — Borda do cupom fixo (2 pts)

**Resposta esperada:** `coupons.rs` usa `saturating_sub(2_000)`, que trava em
zero em vez de estourar; logo R$ 10,00 com `FRETE20` vira 0 e o total é só o
imposto de zero (zero).

**Perde ponto:** falar de "validação" que não existe no código.

## T4 — Arredondamento do imposto (2 pts)

**Resposta esperada:** **não** usa arredondamento bancário. `tax.rs` faz
`(bruto / 100.0).ceil()`, ou seja, **para cima**: 12,34 × 8% = 0,9872 → cobra
0,99.

**Perde ponto:** dizer `round()`/bancário ou não citar o arquivo.

## T5 — Mudança de código (2 pts)

**Resposta esperada:** alterar `src/coupons.rs` no braço `_ =>` para emitir
aviso (ex.: `eprintln!`) e adicionar teste em `tests/pedido.rs` cobrindo
"cupom desconhecido mantém o valor e emite aviso". Não é necessário mudar
`lib.rs` (a orquestração não muda).

**Perde ponto:** propor mexer em `lib.rs`/`tax.rs` sem necessidade, ou não
indicar onde testar.

## Pontuação por rodada

```text
nota = soma das 5 tarefas (0–10)
erro_sem_suporte = afirmações que citam arquivo errado ou regra invertida
```

## Armadilha extra (não pontua, mas qualifica a resposta)

Pergunta opcional: "trocar a ordem de cupom e desconto muda algum pedido?"
**Sim**: 1 × R$ 500,00 com `FRETE20` → atual 450,00 − 20,00 = 430,00 + 8% =
464,40; invertido 480,00 − 10% = 432,00 + 8% = 466,56. Diferença de R$ 2,16.
Quem responde "não muda" demonstra leitura rasa.
