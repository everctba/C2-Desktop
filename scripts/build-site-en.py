"""Gera as versoes EN estaticas do site (GitHub Pages), com hreflang.

Uso: python scripts/build-site-en.py
- le site/{index,ajuda,sobre}.html
- traduz textos/atributos com site/i18n/en.json
- ajusta caminhos relativos para site/en/*
- adiciona hreflang nas paginas PT e EN
- adiciona data-en-href/data-pt-href para o seletor navegar entre versoes
"""

import io
import json
import re
from pathlib import Path

from lxml import etree, html

RAIZ = Path(__file__).resolve().parent.parent
SITE = RAIZ / "site"
DESTINO = SITE / "en"
BASE = "https://everctba.github.io/C2-Desktop/"
PAGINAS = ["index.html", "ajuda.html", "sobre.html"]
SKIP = {"script", "style", "pre", "code"}

DICIONARIO = json.loads((SITE / "i18n" / "en.json").read_text(encoding="utf-8"))


def traduzir(texto: str) -> str:
    chave = texto.strip()
    if not chave:
        return texto
    traducao = DICIONARIO.get(chave)
    if not traducao:
        return texto
    prefixo = texto[: len(texto) - len(texto.lstrip())]
    sufixo = texto[len(texto.rstrip()) :]
    return prefixo + traducao + sufixo


def traduzir_arvore(arvore) -> None:
    for el in arvore.iter():
        if not isinstance(el.tag, str):
            continue
        if el.text and el.tag not in SKIP:
            el.text = traduzir(el.text)
        if el.tag not in SKIP:
            for attr in ("title", "placeholder", "aria-label", "alt"):
                valor = el.get(attr)
                if valor:
                    el.set(attr, traduzir(valor))
    # tails (texto depois de elementos inline)
    for el in arvore.iter():
        if el.tail and (el.getparent() is None or el.getparent().tag not in SKIP):
            el.tail = traduzir(el.tail)


def ajustar_urls(arvore) -> None:
    def ajustar(valor: str) -> str:
        if not valor:
            return valor
        if valor.startswith(("http://", "https://", "mailto:", "#", "data:", "//")):
            return valor
        if valor.startswith(("assets/", "styles/")):
            return "../" + valor
        if valor in ("i18n.js", "config.js", "news.json"):
            return "../" + valor
        return valor

    for el in arvore.iter():
        if not isinstance(el.tag, str):
            continue
        for attr in ("href", "src"):
            valor = el.get(attr)
            if valor:
                el.set(attr, ajustar(valor.strip()))


def garantir_head(arvore) -> None:
    head = arvore.find("head")
    if head is None:
        return
    # remove hreflang antigos
    for el in list(head):
        if el.tag == "link" and (el.get("hreflang") or "").strip():
            head.remove(el)


def adicionar_hreflang(caminho: Path, pagina: str, lingua: str) -> None:
    """Edita apenas o <head> (PT) ou o head da arvore ja gerada (EN)."""
    base = BASE
    if lingua == "pt-BR":
        pt_url = base + pagina
        en_url = base + "en/" + pagina
    else:
        pt_url = base + pagina
        en_url = base + "en/" + pagina
    trecho = (
        f'<link rel="alternate" hreflang="pt-BR" href="{pt_url}" />\n'
        f'<link rel="alternate" hreflang="en" href="{en_url}" />\n'
        f'<link rel="alternate" hreflang="x-default" href="{pt_url}" />\n'
    )
    if lingua == "pt-BR":
        texto = caminho.read_text(encoding="utf-8")
        if "hreflang=" not in texto:
            texto = texto.replace("</head>", trecho + "  </head>", 1)
        if "data-en-href" not in texto:
            texto = re.sub(
                r"<body([^>]*)>",
                lambda m: f'<body{m.group(1)} data-en-href="en/{pagina}">',
                texto,
                count=1,
            )
        caminho.write_text(texto, encoding="utf-8")
    else:
        # arvore ja em memoria: insere no head
        arvore = html.fromstring(caminho.read_text(encoding="utf-8"))
        garantir_head(arvore)
        head = arvore.find("head")
        if head is not None:
            for linha in trecho.strip().split("\n"):
                head.append(html.fromstring(linha))
        html_out = etree.tostring(arvore, method="html", encoding="unicode", doctype="<!DOCTYPE html>")
        caminho.write_text(html_out, encoding="utf-8")


def gerar(pagina: str) -> None:
    origem = SITE / pagina
    texto = origem.read_text(encoding="utf-8")
    arvore = html.fromstring(texto)

    # idioma + dados para o seletor navegar de volta
    arvore.set("lang", "en")
    body = arvore.find("body")
    if body is not None:
        body.set("data-pt-href", "../" + pagina)
        body.attrib.pop("data-en-href", None)

    traduzir_arvore(arvore)
    ajustar_urls(arvore)

    # metas de idioma
    for meta in arvore.xpath("//meta[@property='og:locale']"):
        meta.set("content", "en_US")
    for meta in arvore.xpath("//meta[@property='og:url']"):
        meta.set("content", BASE + "en/" + pagina)
    for meta in arvore.xpath("//meta[@name='description']"):
        # descricao longa: mantem (sem traducao confiavel no dicionario)
        pass
    canonical = arvore.xpath("//link[@rel='canonical']")
    for link in canonical:
        link.set("href", BASE + "en/" + pagina)

    # script de idioma relativo
    for script in arvore.xpath("//script[@src='i18n.js']"):
        script.set("src", "../i18n.js")

    DESTINO.mkdir(exist_ok=True)
    destino = DESTINO / pagina
    html_out = etree.tostring(arvore, method="html", encoding="unicode", doctype="<!DOCTYPE html>")

    # hreflang (EN)
    trecho = (
        f'<link rel="alternate" hreflang="pt-BR" href="{BASE + pagina}" />\n'
        f'<link rel="alternate" hreflang="en" href="{BASE + "en/" + pagina}" />\n'
        f'<link rel="alternate" hreflang="x-default" href="{BASE + pagina}" />\n'
    )
    if "hreflang=" not in html_out:
        html_out = html_out.replace("</head>", trecho + "  </head>", 1)
    destino.write_text(html_out, encoding="utf-8")
    print("gerado:", destino.relative_to(RAIZ))


def main() -> None:
    for pagina in PAGINAS:
        adicionar_hreflang(SITE / pagina, pagina, "pt-BR")
        gerar(pagina)


if __name__ == "__main__":
    main()
