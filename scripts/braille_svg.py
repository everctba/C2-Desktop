"""Converte a arte em Braille Unicode (assets/ascii/logo.txt) para SVG vetorial.

Cada caractere U+2800..U+28FF vira pontos desenhados (2 colunas x 4 linhas),
sem depender de fonte instalada. Texto (nao-Braille) e desenhado como <text>.
"""
import io

ENTRADA = r"site/assets/ascii/logo.txt"
SAIDA = r"site/assets/ascii/logo.svg"

CELULA_L = 10.0  # largura da celula
CELULA_A = 20.0  # altura da celula (terminal ~1:2)
RAIO = 1.75      # ponto pequeno com folga, como na referencia
MARGEM = 10.0
X = (2.5, 7.5)   # centros das 2 colunas (espacamento 5)
Y = (2.5, 7.5, 12.5, 17.5)  # centros das 4 linhas (espacamento 5)

linhas = io.open(ENTRADA, encoding="utf-8").read().splitlines()
largura = max(len(linha) for linha in linhas) * CELULA_L + 2 * MARGEM
altura = len(linhas) * CELULA_A + 2 * MARGEM

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
                    cx = MARGEM + c * CELULA_L + X[col]
                    cy = MARGEM + r * CELULA_A + Y[lin]
                    partes.append(f'<circle cx="{cx:g}" cy="{cy:g}" r="{RAIO:g}"/>')
        elif ch.strip():
            cx = MARGEM + c * CELULA_L
            cy = MARGEM + r * CELULA_A + 15.5
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
