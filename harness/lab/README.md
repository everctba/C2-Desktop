# Taskline Lab

Projeto pequeno para testar o `c2h`.

## Rodar

```bash
cargo run -- add "comprar cafe"
cargo run -- list
cargo run -- done 1
cargo run -- html > page.html
cargo test
```

## Estado inicial

O laboratorio tem problemas de proposito para o harness resolver:

- `Store::add("")` aceita titulo vazio; o teste `rejeita_titulo_vazio` falha;
- o HTML de `html` pode ficar desatualizado em relacao ao `page.html`;
- a persistencia usa `.taskline.json` no diretorio atual.

## Tarefas para experimentar

1. Corrigir `add` para rejeitar titulo vazio e fazer o teste passar.
2. Fazer `load` lidar com JSON corrompido sem panic.
3. Adicionar `remove <id>`.
4. Garantir que `done` persiste apos reiniciar.

## Por que este projeto

- Rust com blocos pequenos (funcoes e structs);
- HTML one-page com secoes `data-c2-block`;
- testes que falham de forma deterministica;
- nenhum servidor, banco ou API externa.

## Com o harness

```bash
c2h annotate src/store.rs
c2h context src/store.rs
c2h ask --file src/store.rs --apply "corrija o titulo vazio e faca o teste passar"
C2H_TEST_CMD="cargo test" c2h apply resposta.json
```

O `C2H_TEST_CMD` roda `cargo test` depois do patch e faz rollback se falhar.
