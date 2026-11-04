# Projeto de exemplo — loja (benchmark A/B)

Projeto pequeno de propósito, mas com regras que **não cabem num arquivo só**.
Serve para comparar "agente com C2" vs "agente lendo o projeto direto".

## Contexto

Uma loja calcula o preço final de um pedido. As regras estão espalhadas por
quatro arquivos e dependem da **ordem** de aplicação.

## Regras de negócio

1. O **subtotal** é a soma de `preco_unitario * quantidade` de cada item.
2. **Desconto progressivo por volume** (em `pricing.rs`):
   - subtotal ≥ 500 → 10%
   - subtotal ≥ 200 → 5%
   - abaixo disso → 0%
3. **Cupom** (em `coupons.rs`) é aplicado **depois** do desconto progressivo:
   - `BEMVINDO10` → 10% sobre o valor já descontado
   - `FRETE20` → R$ 20 fixos
   - cupom desconhecido → sem efeito
   - o cupom **nunca** leva o total abaixo de zero
4. **Imposto** (em `tax.rs`): 8% sobre o valor após cupom, **arredondado para
   cima** no centavo (não é arredondamento bancário).
5. O total final = valor após cupons + imposto.

## Invariantes escondidas (é aqui que ler direto costuma falhar)

- Trocar a ordem desconto/cupom muda o resultado em pedidos de borda.
- `FRETE20` em pedido pequeno pode zerar o valor (não pode ficar negativo).
- O arredondamento do imposto é **para cima** (0,005 vira 0,01), diferente de
  `round()` do Rust.
- Pedido vazio é válido e custa `0.00`.

## Como o benchmark usa

O agente recebe uma tarefa (ex.: "adicione a regra X" ou "por que o pedido Y
saiu com esse total?") e precisa responder com arquivo:linha. A condição com
C2 pode usar `c2 status`, `c2 grep`, `.c2.json`, `c2 related` e `c2 read`; a
condição sem C2 lê os arquivos que quiser.
