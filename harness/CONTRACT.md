# Contrato de resposta do c2h

Versão: `1`.

## Envelope

```json
{
  "contract_version": 1,
  "changes": [
    {
      "file": "src/pagamento.rs",
      "block_id": "src/pagamento.rs::fn::confirmar",
      "action": "update",
      "new_code": "fn confirmar() { ... }",
      "w5h2": {
        "what": "...",
        "why": "...",
        "when": "...",
        "where": "...",
        "who": "...",
        "how": "...",
        "how_much": "..."
      },
      "evidence": ["src/pagamento.rs:42"]
    }
  ]
}
```

## Regras

- `contract_version` deve ser `1`;
- `changes` não pode ser vazio;
- `file` é caminho relativo, sem `..`;
- `update`/`delete` exigem `block_id` existente no sidecar;
- `create`/`update` exigem `new_code`;
- `w5h2.what` ou `w5h2.why` obrigatório;
- `evidence` deve apontar para arquivo existente;
- a IA não escreve timestamp;
- a IA não escolhe comando shell.

## Erros

```text
E-SCHEMA-01  contrato incompatível
E-SCHEMA-02  changes vazio
E-PATH-01    caminho inválido/traversal
E-FILE-01    arquivo não existe
E-ACTION-01  action inválida
E-CODE-01    new_code vazio
E-BLOCK-01   block_id desconhecido
E-W5H2-01    what/why ausente
E-EVID-01    evidência inexistente
```

## Fluxo

```bash
c2h ask --file src/pagamento.rs --apply "adicione retry"
c2h validate-response resposta.json
c2h apply resposta.json --dry-run
c2h apply resposta.json
```

O `ask --file --apply` faz o loop:

```text
provider
  -> parse JSON
  -> validar
  -> se erro: reparo com violations (até 3)
  -> aplicar patch
  -> rustfmt
  -> atualizar sidecar
```

O `ask --file` sem `--apply` mostra a resposta e não altera arquivos.
