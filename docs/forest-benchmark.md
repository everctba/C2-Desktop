# Benchmark da floresta

Task `c2-jgf.8`.

O benchmark cria um projeto sintético com inconsistências deliberadas e verifica
se o forest as expõe.

## O que é plantado

- README cita `src/nao_existe.rs`;
- README afirma que o projeto não usa embeddings, mas existe
  `crates/c2-core/src/embedding.rs`;
- uma nota `gotcha` em `src/pagamento.rs`;
- uma nota `pending` em `src/pedido.rs`;
- dois domínios com dependência entre si.

## O que é medido

```text
divergências encontradas
domínios sintetizados
notas incorporadas
tamanho do brief de onboarding
brief contém o gotcha?
```

## Rodar

```powershell
pwsh -File scripts/bench-forest.ps1
```

Saída: `docs/bench-forest.json`.

## Perguntas de referência

| Pergunta | Esperado |
|---|---|
| Quais domínios existem? | pagamento e pedido |
| Quais gotchas antes de mexer? | retry do pagamento |
| O README cita arquivo inexistente? | sim, divergência |
| O README ainda diz “sem embeddings”? | sim, divergência |
| Um brief de 2 minutos cabe? | sim, abaixo do budget |

## Limitação

É um corpus pequeno e determinístico. Serve para regressão de detecção e de
estrutura, não como métrica de qualidade geral.
