"""Converte a arte em Braille Unicode (assets/ascii/logo.txt) para SVG vetorial.

Cada caractere U+2800..U+28FF vira pontos desenhados (2 colunas x 4 linhas),
sem depender de fonte instalada. Texto (nao-Braille) e desenhado como <text>.
"""
import io

ENTRADA = r"site/assets/ascii/logo.txt"
SAIDA = r"site/assets/ascii/logo.svg"

CELULA_L = 10.0  # largura da celula
CELULA_A = 20.0  # altura da celula (terminal ~1:2)
RAIO = 2.3
X = (3.0, 7.0)   # centros das 2 colunas
Y = (4.5, 9.5, 14.5, 19.5)  # centros das 4 linhas

linhas = io.open(ENTRADA, encoding="utf-8").read().splitlines()
largura = max(len(linha) for linha in linhas) * CELULA_L
altura = len(linhas) * CELULA_A

partes = []
for r, linha in enumerate(linhas):
    for c, ch in enumerate(linha):
        codigo = ord(ch)
        if 0x2800 <= codigo <= 0x28FF:
            bits = codigo - 0x2800
            for bit in range(8):
                if bits & (1 << bit):
                    col = 0 if bit in (0, 1, 2, 6) else 1
                    lin = bit if bit < 3 else (3 if bit >= 6 else bit - 3)
                    cx = c * CELULA_L + X[col]
                    cy = r * CELULA_A + Y[lin]
                    partes.append(f'<circle cx="{cx:g}" cy="{cy:g}" r="{RAIO:g}"/>')
        elif ch.strip():
            cx = c * CELULA_L
            cy = r * CELULA_A + 15.5
            partes.append(
                f'<text x="{cx:g}" y="{cy:g}" font-family="Consolas, &apos;Courier New&apos;, monospace" '
                f'font-size="14" fill="#9bdcff">{ch.replace("&", "&amp;").replace("<", "&lt;")}</text>'
            )

svg = (
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {largura:g} {altura:g}" '
    f'width="{largura:g}" height="{altura:g}" role="img" '
    f'aria-label="C2 — Comments 2.0 for AI">\n'
    f'  <g fill="#9bdcff">\n'
    + "\n".join("    " + parte for parte in partes)
    + "\n  </g>\n</svg>\n"
)
io.open(SAIDA, "w", encoding="utf-8", newline="\n").write(svg)
print(f"svg: {SAIDA} ({largura:g}x{altura:g}, {len(partes)} elementos)")
