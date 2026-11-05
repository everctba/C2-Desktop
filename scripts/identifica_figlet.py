import io

import pyfiglet

arte = io.open(r"site/assets/ascii/logo-ascii.txt", encoding="utf-8").read().splitlines()
alvo = [l.rstrip() for l in arte[:4]]
largura = max(len(l) for l in alvo)


def normalizar(linhas):
    linhas = [l.rstrip() for l in linhas]
    linhas = [l for l in linhas if l.strip()]
    if not linhas:
        return []
    m = max(len(l) for l in linhas)
    return [l.ljust(m) for l in linhas]


def similaridade(a, b):
    alt = min(len(a), len(b))
    larg = max(len(x) for x in a)
    pontos = 0
    total = 0
    for i in range(alt):
        for j in range(larg):
            ca = a[i][j] if j < len(a[i]) else " "
            cb = b[i][j] if j < len(b[i]) else " "
            if ca != " " or cb != " ":
                total += 1
                if ca == cb:
                    pontos += 1
    return pontos / total if total else 0.0


resultados = []
for fonte in pyfiglet.FigletFont.getFonts():
    try:
        for texto in ("C2", "C2 AI", "C2 2.0", "C2 COMMENTS", "C2 Comments"):
            saida = pyfiglet.figlet_format(texto, font=fonte).splitlines()
            saida = normalizar(saida)
            if not saida:
                continue
            nota = similaridade(alvo, saida)
            resultados.append((nota, fonte, texto, saida))
    except Exception:
        continue

resultados.sort(key=lambda r: -r[0])
for nota, fonte, texto, saida in resultados[:6]:
    print(f"--- {nota:.2%} | fonte={fonte} | texto={texto!r}")
    print("\n".join(saida))
    print()
