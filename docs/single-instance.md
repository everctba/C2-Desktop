# Instância única e foco

Task `c2-d6l`.

## Problema

`c2 init` abria o desktop e um `c2 open` depois iniciava outro processo. Cada
processo tinha WebView, engine, tray e logs próprios.

## Solução

### App

- uma instância primária reserva uma porta TCP loopback por usuário;
- grava `instance.json` com `{port, token}`;
- segunda execução envia `focus` e termina antes de criar janela/engine.

### CLI

`c2 open`:

```text
1. lê instance.json
2. conecta em 127.0.0.1:<port>
3. envia {token, action:"focus", root}
4. se ok, retorna sem spawnar
5. se não houver instância, spawna o app
```

### Foco

A instância primária:

```text
window.show()
window.unminimize()
window.set_focus()
```

Se a janela estiver no tray ou minimizada, ela volta. Em Windows/Linux o
compositor ainda pode negar foco; nesse caso não se abre uma segunda janela.

## Arquivos

```text
%APPDATA%\dev.c2.desktop\instance.json       Windows
$XDG_RUNTIME_DIR/dev.c2.desktop/instance.json Linux (fallback /tmp)
```

O token é local e o listener só aceita `127.0.0.1`.

## Limitações

- uma instância por usuário/sessão;
- foco em Wayland é best effort;
- se o app morrer sem limpar o arquivo, o próximo launch reescreve;
- `open-root` é entregue ao frontend via evento Tauri.
