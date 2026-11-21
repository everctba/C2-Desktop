"""Relatorio simples do Taskline, usado para testar blocos Python no c2h."""

from __future__ import annotations

import json
from pathlib import Path


def carregar(caminho: Path) -> list[dict]:
    if not caminho.exists():
        return []
    return json.loads(caminho.read_text(encoding="utf-8")).get("tasks", [])


def resumo(tarefas: list[dict]) -> dict:
    abertas = [t for t in tarefas if not t.get("done")]
    concluidas = [t for t in tarefas if t.get("done")]
    return {"total": len(tarefas), "abertas": len(abertas), "concluidas": len(concluidas)}


def imprimir(tarefas: list[dict]) -> None:
    dados = resumo(tarefas)
    print(f"total={dados['total']} abertas={dados['abertas']} concluidas={dados['concluidas']}")


if __name__ == "__main__":
    imprimir(carregar(Path(".taskline.json")))
