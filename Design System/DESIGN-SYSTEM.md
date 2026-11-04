# Design System — "Terminal Editorial"

> Extraído do código-fonte do [Instatic](https://github.com/CoreBunch/Instatic) (`src/styles/globals.css` + `src/ui/components/**`).
> Este documento é autossuficiente: dá para entregar a um time/dev/IA e pedir "siga exatamente isto".

**Em uma frase:** base **acromática** (preto/cinza), densidade de ferramenta profissional, **cor só quando carrega significado**, bordas quase invisíveis, raios pequenos no chrome e grandes nos cards, tipografia Inter com escala fluida e micro-tamanhos.

---

## Sumário

1. [Princípios](#1-princípios)
2. [Tokens (copiar e colar)](#2-tokens-copiar-e-colar)
3. [Cor](#3-cor)
4. [Tipografia](#4-tipografia)
5. [Espaçamento](#5-espaçamento)
6. [Raios, sombras, blur](#6-raios-sombras-blur)
7. [Superfícies](#7-superfícies)
8. [Componentes](#8-componentes)
9. [Layout e chrome](#9-layout-e-chrome)
10. [Movimento](#10-movimento)
11. [Acessibilidade](#11-acessibilidade)
12. [Regras de CSS (o que é proibido)](#12-regras-de-css-o-que-é-proibido)
13. [Ícones](#13-ícones)
14. [Adaptando para outro stack](#14-adaptando-para-outro-stack)
15. [Checklist de implementação](#15-checklist-de-implementação)
16. [Anti-padrões](#16-anti-padrões)

---

## 1. Princípios

1. **A base desaparece, o significado aparece.** Surfaces, bordas e texto padrão são neutros. Cor é reservada para identidade e estado. Se uma cor não comunica nada, ela não entra.
2. **Duas camadas de cor, separação dura.**
   - Camada 1 — **acromática**: `--bg-*`, `--text-*`, `--border-*`, `--overlay-*`.
   - Camada 2 — **semântica/categórica**: `--danger/--warning/--success`, `--accent-1..10`, neon do canvas.
3. **Hierarquia por superfície, não por borda.** Cards não têm borda: ficam sobre um pai mais escuro com 1px de gap. A "linha" é o gap.
4. **Inputs usam borda; cards não.** Campo = 1px de branco-alpha + fundo transparente + raio pill. Card = sem borda + raio 16px.
5. **Densidade de ferramenta.** Fonte 10–13px, linhas de 28px, botões de 26px. É intencional: é software de trabalho, não landing page. Ofereça um modo `comfortable` (36px) em vez de afrouxar o padrão.
6. **Identidade é cor, não rótulo.** Cada coleção/módulo/tag recebe um dos 10 accents de forma determinística (hash do nome → índice), com fundo sempre a 10% de alpha.
7. **O canvas tem paleta própria.** Verde neon = seleção, rosa neon = hover, laranja neon = "tudo que este seletor afeta". São as **únicas** cores cromáticas na tela de trabalho.
8. **Movimento discreto.** 80–150ms, `ease`, só em cor/fundo/opacidade. Nada de bounce, nada acima de 200ms, e tudo desligado em `prefers-reduced-motion`.

---

## 2. Tokens (copiar e colar)

Salve como `tokens.css` e importe antes de qualquer CSS de componente. **Nenhum componente deve conter hex/rgb/hsl hardcoded.**

### 2.1 Tema escuro (default) + escalas

```css
:root {
  color-scheme: dark;

  /* ── Fonte ─────────────────────────────────────────────── */
  --font-sans: "Inter Variable", system-ui, -apple-system, sans-serif;
  --font-mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;

  /* ── Escala tipográfica fluida (min 320px → max 1400px) ── */
  --text-3xs: clamp(8px,  calc(7.629px  + 0.095vw), 9px);
  --text-2xs: clamp(9px,  calc(8.629px  + 0.095vw), 10px);
  --text-xs:  clamp(10px, calc(9.629px  + 0.095vw), 11px);
  --text-s:   clamp(11px, calc(10.629px + 0.095vw), 12px);
  --text-m:   clamp(12px, calc(11.629px + 0.095vw), 13px);
  --text-l:   clamp(13px, calc(12.629px + 0.095vw), 14px);
  --text-xl:  clamp(14px, calc(13.257px + 0.19vw),  16px);
  --text-2xl: clamp(16px, calc(15.257px + 0.19vw),  18px);
  --text-3xl: clamp(18px, calc(17.257px + 0.19vw),  20px);
  --text-4xl: clamp(20px, calc(18.514px + 0.381vw), 24px);
  --text-5xl: clamp(24px, calc(22.514px + 0.381vw), 28px);
  --text-6xl: clamp(32px, calc(29.029px + 0.762vw), 40px);
  --text-7xl: clamp(40px, calc(34.057px + 1.524vw), 56px);

  /* ── Escala de espaço fluida ───────────────────────────── */
  --space-px:  1px;
  --space-4xs: clamp(2px,  calc(1.629px  + 0.095vw), 3px);
  --space-3xs: clamp(3px,  calc(2.629px  + 0.095vw), 4px);
  --space-2xs: clamp(4px,  calc(3.629px  + 0.095vw), 5px);
  --space-xs:  clamp(5px,  calc(4.629px  + 0.095vw), 6px);
  --space-s:   clamp(6px,  calc(5.257px  + 0.19vw),  8px);
  --space-m:   clamp(8px,  calc(7.257px  + 0.19vw),  10px);
  --space-l:   clamp(10px, calc(9.257px  + 0.19vw),  12px);
  --space-xl:  clamp(12px, calc(11.257px + 0.19vw),  14px);
  --space-2xl: clamp(14px, calc(13.257px + 0.19vw),  16px);
  --space-3xl: clamp(16px, calc(15.257px + 0.19vw),  18px);
  --space-4xl: clamp(18px, calc(17.257px + 0.19vw),  20px);
  --space-5xl: clamp(20px, calc(18.514px + 0.381vw), 24px);
  --space-6xl: clamp(24px, calc(22.514px + 0.381vw), 28px);
  --space-7xl: clamp(28px, calc(26.514px + 0.381vw), 32px);
  --space-8xl: clamp(32px, calc(29.029px + 0.762vw), 40px);
  --space-9xl: clamp(40px, calc(37.029px + 0.762vw), 48px);

  /* ── Superfícies (do mais fundo ao mais alto) ──────────── */
  --bg-body:      #000000;
  --bg-surface:   #1b1b1b;
  --bg-surface-2: #282828;
  --bg-surface-3: #323232;
  --bg-surface-4: #4a4a4a;
  --bg-surface-5: #605f5f;

  /* ── Bordas ────────────────────────────────────────────── */
  --border-subtle: #1f1f1f;
  --border-muted:  #2e2e2e;
  --border:        #333333;
  --border-strong: #4a4a4a;

  /* ── Texto ─────────────────────────────────────────────── */
  --text-bright:   #f4f4f5;
  --text:          #ededed;
  --text-muted:    #a1a1aa;
  --text-subtle:   #787878;
  --text-disabled: #52525b;

  /* ── Ação primária (pill inverso de alto contraste) ────── */
  --btn-primary-bg:        #ffffff;
  --btn-primary-bg-hover:  #e4e4e7;
  --btn-primary-bg-active: #d4d4d8;
  --btn-primary-fg:        #111827;

  /* ── Escala de overlay branco (sufixo = alpha × 100) ───── */
  --overlay:    #ffffff;
  --overlay-5:  rgba(255,255,255,0.05);
  --overlay-10: rgba(255,255,255,0.10);
  --overlay-20: rgba(255,255,255,0.20);
  --overlay-30: rgba(255,255,255,0.30);
  --overlay-40: rgba(255,255,255,0.40);
  --overlay-50: rgba(255,255,255,0.50);
  --overlay-60: rgba(255,255,255,0.60);
  --overlay-70: rgba(255,255,255,0.70);
  --overlay-80: rgba(255,255,255,0.80);
  --overlay-90: rgba(255,255,255,0.90);

  /* ── Scrim preto (sombras/backdrops) ───────────────────── */
  --scrim:    #000000;
  --scrim-10: rgba(0,0,0,0.10);
  --scrim-20: rgba(0,0,0,0.20);
  --scrim-30: rgba(0,0,0,0.30);
  --scrim-40: rgba(0,0,0,0.40);
  --scrim-50: rgba(0,0,0,0.50);
  --scrim-60: rgba(0,0,0,0.60);
  --scrim-70: rgba(0,0,0,0.70);
  --scrim-80: rgba(0,0,0,0.80);
  --scrim-90: rgba(0,0,0,0.90);

  /* ── Acentos de identidade (10) ────────────────────────── */
  --accent-1:  #8ee6c8;  --accent-1-10:  rgba(142,230,200,0.10);
  --accent-2:  #c8b6ff;  --accent-2-10:  rgba(200,182,255,0.10);
  --accent-3:  #9bdcff;  --accent-3-10:  rgba(155,220,255,0.10);
  --accent-4:  #ffc7a8;  --accent-4-10:  rgba(255,199,168,0.10);
  --accent-5:  #ffb6cd;  --accent-5-10:  rgba(255,182,205,0.10);
  --accent-6:  #b8f28b;  --accent-6-10:  rgba(184,242,139,0.10);
  --accent-7:  #f7df72;  --accent-7-10:  rgba(247,223,114,0.10);
  --accent-8:  #83e7ff;  --accent-8-10:  rgba(131,231,255,0.10);
  --accent-9:  #f0a6ff;  --accent-9-10:  rgba(240,166,255,0.10);
  --accent-10: #ff9f9f;  --accent-10-10: rgba(255,159,159,0.10);

  /* ── Estados semânticos ────────────────────────────────── */
  --danger:  #ef4444;
  --danger-light:   #f87171;
  --danger-lighter: #fca5a5;
  --danger-text:    #fecaca;
  --danger-5:  rgba(239,68,68,0.05);
  --danger-10: rgba(239,68,68,0.10);
  --danger-20: rgba(239,68,68,0.20);
  --danger-30: rgba(239,68,68,0.30);
  --danger-40: rgba(239,68,68,0.40);

  --warning: #f59e0b;
  --warning-text: #fde68a;
  --warning-10: rgba(245,158,11,0.10);
  --warning-20: rgba(245,158,11,0.20);
  --warning-30: rgba(245,158,11,0.30);

  --success: #34d399;
  --success-bright: #4ade80;
  --success-text: #d1fae5;
  --success-text-muted: #a7f3d0;
  --success-10: rgba(52,211,153,0.10);
  --success-20: rgba(52,211,153,0.20);
  --success-30: rgba(52,211,153,0.30);
  --success-40: rgba(52,211,153,0.40);

  --info-text: #c4b5fd;

  /* ── Raios ─────────────────────────────────────────────── */
  --radius-sm:      3px;
  --radius:         6px;
  --panel-radius:   12px;
  --card-radius:    16px;
  --input-radius:   1em;
  --radius-pill:    999px;
  --tooltip-radius: 6px;

  /* ── Scrollbar ─────────────────────────────────────────── */
  --scrollbar-size:  6px;
  --scrollbar-radius: var(--radius-sm);
  --scrollbar-track: transparent;
  --scrollbar-thumb:       color-mix(in srgb, var(--text-subtle) 58%, transparent);
  --scrollbar-thumb-hover: color-mix(in srgb, var(--text-muted)  72%, transparent);

  /* ── Sombras e foco ────────────────────────────────────── */
  --focus-ring: 0 0 0 1px var(--overlay-20);
  --panel-blur: 24px;
  --shadow-panel-inset-bottom: inset 0 -1px 0 var(--scrim-40);
  --shadow-panel-drop: 0 12px 40px rgba(0,0,0,0.65);
  --shadow-panel: var(--shadow-panel-inset-bottom), var(--shadow-panel-drop);
  --shadow-input-focus:
    inset 0 0 5px rgba(255,255,255,0.06),
    inset 0 -5px 8px rgba(255,255,255,0.09),
    0 0 5px rgba(0,0,0,0.24);
  --shadow-tooltip:
    0 4px 16px var(--scrim-50),
    inset 0 1px 0 var(--overlay-5);

  /* ── Keycap (atalhos) ──────────────────────────────────── */
  --kbd-face-top:    #434343;
  --kbd-face-bottom: #2f2f2f;
  --kbd-face-top-hover:    #4d4d4d;
  --kbd-face-bottom-hover: #363636;
  --kbd-border:   var(--border);
  --kbd-text:     var(--text-muted);
  --kbd-highlight: var(--overlay-5);
  --kbd-inner-shadow: var(--scrim-20);
  --kbd-edge: var(--scrim-30);

  /* ── Canvas (seleção/hover/seletor) ────────────────────── */
  --canvas-chrome-shadow: 0 12px 28px var(--scrim-30);
  --canvas-selection-ring-color: #39ff14;  /* verde neon */
  --canvas-hover-ring-color:     #ff2bd6;  /* rosa neon  */
  --canvas-selector-ring-color:  #ff8800;  /* laranja neon */
  --canvas-selection-ring: inset 0 0 0 1px var(--canvas-selection-ring-color);
  --canvas-hover-ring:     inset 0 0 0 1px var(--canvas-hover-ring-color);
  --canvas-selector-ring:  inset 0 0 0 1px var(--canvas-selector-ring-color);

  --tree-drop-line:         #5eead4;
  --tree-drop-line-shadow:  rgba(94,234,212,0.18);
  --tree-drop-inside-bg:    rgba(94,234,212,0.08);
  --tree-drop-inside-border: rgba(94,234,212,0.65);
  --tree-drop-invalid-bg:     rgba(239,68,68,0.06);
  --tree-drop-invalid-border: rgba(239,68,68,0.28);

  /* Placeholder listrado de "módulo vazio" (acromático) */
  --canvas-placeholder-bg:
    repeating-linear-gradient(
      -45deg,
      color-mix(in srgb, var(--text-subtle) 6%, transparent) 0 6px,
      transparent 6px 12px
    ),
    color-mix(in srgb, var(--text-subtle) 4%, transparent);

  /* ── Z-index ───────────────────────────────────────────── */
  --z-dropdown: 20;
  --spotlight-z-index: 9000;
  --toast-z-index: 10000;
  --tooltip-z-index: 10001;   /* tooltip nunca é coberto */
  --backdrop-blur: 8px;
  --spotlight-width: 640px;
  --progress-shimmer: var(--overlay-40);
}
```

### 2.2 Reset base

```css
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

html, body, #root { height: 100%; width: 100%; }

body {
  background: var(--bg-body);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: var(--text-m);
  -webkit-font-smoothing: antialiased;
}

/* Scrollbar fina e discreta */
* { scrollbar-width: thin; scrollbar-color: var(--scrollbar-thumb) var(--scrollbar-track); }
::-webkit-scrollbar { width: var(--scrollbar-size); height: var(--scrollbar-size); }
::-webkit-scrollbar-track { background: var(--scrollbar-track); }
::-webkit-scrollbar-thumb { background: var(--scrollbar-thumb); border-radius: var(--scrollbar-radius); }
::-webkit-scrollbar-thumb:hover { background: var(--scrollbar-thumb-hover); }
::-webkit-scrollbar-corner { background: var(--scrollbar-track); }

/* Movimento desligado para quem pediu */
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
    scroll-behavior: auto !important;
  }
}

/* Shimmer único de skeleton — 1.4s, sincronizado em toda a app */
@keyframes skeletonShimmer {
  0%   { background-position:  200% 0; }
  100% { background-position: -200% 0; }
}
```

### 2.3 Tema claro (opcional, mesmo contrato)

Ativado por `data-theme="light"` no `<html>` (ou `[data-editor-theme='light']`). Não reescreva componentes — só troque os tokens.

```css
[data-theme="light"] {
  color-scheme: light;

  --bg-body:      #e3e7ed;
  --bg-surface:   #ffffff;
  --bg-surface-2: #f6f7f9;
  --bg-surface-3: #eceff3;
  --bg-surface-4: #e1e5ea;
  --bg-surface-5: #d3d9e1;

  --border-subtle: #eef0f3;
  --border-muted:  #dce1e7;
  --border:        #cbd3dc;
  --border-strong: #aeb8c5;

  --text-bright:   #111827;
  --text:          #1f2937;
  --text-muted:    #4b5563;
  --text-subtle:   #6b7280;
  --text-disabled: #9ca3af;

  --btn-primary-bg:        #111827;
  --btn-primary-bg-hover:  #1f2937;
  --btn-primary-bg-active: #374151;
  --btn-primary-fg:        #ffffff;

  /* Overlays passam a ser tinta, não branco */
  --overlay:    #111827;
  --overlay-5:  rgba(17,24,39,0.05);
  --overlay-10: rgba(17,24,39,0.10);
  --overlay-20: rgba(17,24,39,0.20);
  --overlay-30: rgba(17,24,39,0.30);
  --overlay-40: rgba(17,24,39,0.40);
  --overlay-50: rgba(17,24,39,0.50);
  --overlay-60: rgba(17,24,39,0.60);
  --overlay-70: rgba(17,24,39,0.70);
  --overlay-80: rgba(17,24,39,0.80);
  --overlay-90: rgba(17,24,39,0.90);

  /* Acentos viram cores de FOREGROUND seguras (mais escuras) */
  --accent-1:  #047857;  --accent-1-10:  rgba(4,120,87,0.10);
  --accent-2:  #6d28d9;  --accent-2-10:  rgba(109,40,217,0.10);
  --accent-3:  #0369a1;  --accent-3-10:  rgba(3,105,161,0.10);
  --accent-4:  #9a3412;  --accent-4-10:  rgba(154,52,18,0.10);
  --accent-5:  #be123c;  --accent-5-10:  rgba(190,18,60,0.10);
  --accent-6:  #3f6212;  --accent-6-10:  rgba(63,98,18,0.10);
  --accent-7:  #713f12;  --accent-7-10:  rgba(113,63,18,0.10);
  --accent-8:  #0e7490;  --accent-8-10:  rgba(14,116,144,0.10);
  --accent-9:  #a21caf;  --accent-9-10:  rgba(162,28,175,0.10);
  --accent-10: #b91c1c;  --accent-10-10: rgba(185,28,28,0.10);

  --danger-text: #991b1b;
  --warning-text: #92400e;
  --success-text: #047857;
  --success-text-muted: #065f46;
  --info-text: #4338ca;

  --shadow-panel-inset-bottom: inset 0 -1px 0 var(--overlay-10);
  --shadow-panel-drop: 0 16px 42px rgba(15,23,42,0.18);
  --shadow-input-focus:
    inset 0 0 4px rgba(15,23,42,0.06),
    inset 0 -4px 8px rgba(15,23,42,0.08),
    0 0 0 1px rgba(15,23,42,0.14);

  --kbd-face-top:    #ffffff;
  --kbd-face-bottom: #e5e7eb;
  --kbd-face-top-hover:    #f8fafc;
  --kbd-face-bottom-hover: #dbe1e8;
  --kbd-highlight: rgba(255,255,255,0.85);
  --kbd-inner-shadow: rgba(15,23,42,0.10);
  --kbd-edge: rgba(15,23,42,0.18);

  --canvas-chrome-shadow: 0 12px 28px rgba(15,23,42,0.16);
}
```

### 2.4 Densidade e escala de texto (opcional, mas recomendado)

Três níveis de texto e dois de densidade — controlados por atributo no root, sem tocar em componente.

```css
/* data-text-scale="small" | "large" | "extra-large" */

/* densidade */
[data-density="comfortable"] { --tree-row-h: 36px; }
:root { --tree-row-h: 28px; }
```

---

## 3. Cor

### 3.1 Regra de ouro

```
┌──────────────────────────────────────────────────────────────┐
│ CAMADA 2 — SIGNIFICADO                                        │
│ --accent-1..10 (identidade)  |  danger/warning/success (estado)│
│ canvas neon (seleção/hover)                                   │
├──────────────────────────────────────────────────────────────┤
│ CAMADA 1 — BASE ACROMÁTICA                                    │
│ bg-body → bg-surface-5                                       │
│ text-bright → text-disabled                                  │
│ border-subtle → border-strong  |  overlay-5..90              │
└──────────────────────────────────────────────────────────────┘
```

**Uso permitido da cor:**

| Intenção | Token | Forma |
|---|---|---|
| Identidade de coleção/módulo | `--accent-N` + `--accent-N-10` | texto/borda na cor cheia, fundo a 10% |
| Erro / destrutivo | `--danger`, `--danger-10` | texto vermelho, fundo 10% |
| Aviso | `--warning`, `--warning-10` | idem |
| Sucesso / salvo | `--success`, `--success-10` | idem, ou só um dot de 6px |
| Foco | `--focus-ring` | `box-shadow: 0 0 0 1px var(--overlay-20)` |
| Seleção no canvas | `--canvas-selection-ring` | `box-shadow: inset 0 0 0 1px #39ff14` |

**Nunca:** gradiente decorativo, cor em superfície grande, cor em texto de corpo, cor sem estado por trás.

### 3.2 Acentos (hash determinístico)

```ts
// mesma string → mesma cor, sempre. Use em tags, avatares, módulos, coleções.
const ACCENTS = 10;
function accentFor(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return (h % ACCENTS) + 1; // 1..10
}
// uso: background: var(--accent-N-10); color: var(--accent-N);
```

### 3.3 Contraste

- Texto normal: `--text` sobre `--bg-surface` (≥ 12:1).
- Secundário: `--text-muted` (≥ 7:1), mínimo aceitável.
- Nunca abaixo de `--text-subtle` para informação essencial — é só para labels/uppercase de 10px.
- Acentos no dark são claros (funcionam como foreground); no light são escurecidos automaticamente pelo tema. **Nunca** use `--accent-N` como fundo de texto claro.

---

## 4. Tipografia

**Família:** `Inter Variable` (fallback `system-ui`). **Mono:** stack do sistema (`ui-monospace`). Sem Google Fonts em runtime — self-host.

| Token | px | Uso típico |
|---|---|---|
| `--text-3xs` | 8–9 | micro-badges, contadores |
| `--text-2xs` | 9–10 | pills xs, labels de árvore auxiliares |
| `--text-xs` | 10–11 | **padrão do chrome**: botões, inputs, tree rows |
| `--text-s` | 11–12 | corpo de painel, descrições |
| `--text-m` | 12–13 | corpo padrão do admin |
| `--text-l` | 13–14 | títulos de painel |
| `--text-xl` | 14–16 | título de página |
| `--text-2xl` | 16–18 | heading de seção |
| `--text-3xl`+ | 18–56 | hero / números de dashboard |

**Pesos:** 400 (raro), **500** (corpo/UI), **600** (label/título pequeno), **700** (botão, ênfase), **620** (pills — valor intermediário do Inter variável).

**Regras:**
- Títulos de widget: `font-size: var(--text-xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.07em; color: var(--text-subtle)`.
- Nunca `letter-spacing` negativo. Só uppercase usa tracking (0.05–0.08em).
- `line-height: 1` em linhas de árvore/toolbar; `1.4–1.5` em texto corrido; `1.3` em tooltip.
- Números em tabela/dashboard: `font-variant-numeric: tabular-nums`.
- Tamanho nunca em `px` hardcoded — sempre `var(--text-*)`.

---

## 5. Espaçamento

Base 4px "modulada" com escala fluida. **Sempre** `var(--space-*)`.

| Token | px | Uso |
|---|---|---|
| `--space-px` | 1 | hairline |
| `--space-4xs` | 2–3 | gap ícone↔texto em pill |
| `--space-3xs` | 3–4 | gap interno de botão |
| `--space-2xs` | 4–5 | padding vertical de input sm |
| `--space-xs` | 5–6 | padding horizontal de botão sm |
| `--space-s` | 6–8 | gap padrão entre itens |
| `--space-m` | 8–10 | padding de célula |
| `--space-l` | 10–12 | padding de painel |
| `--space-xl` | 12–14 | padding horizontal de header |
| `--space-2xl` | 14–16 | seções |
| `--space-3xl` | 16–18 | padding de card |
| `--space-4xl`–`9xl` | 18–48 | blocos grandes, dialogs, páginas |

**Densidade por componente (valores fixos, não tokens):**

| Elemento | Compacto | Comfortable |
|---|---|---|
| Tree row | 28px | 36px |
| Botão `sm` | 26px | — |
| Input `sm` | 28px | — |
| Item de menu | 32px | — |
| Alvo de toque (`lg`) | 44px | 44px |

---

## 6. Raios, sombras, blur

### Raios — escolha por função, não por gosto

| Token | Valor | Onde |
|---|---|---|
| `--radius-sm` | **3px** | badges minúsculos, kbd |
| `--radius` | **6px** | **botões**, pills, chips, menus, tooltip |
| `--panel-radius` | **12px** | painéis flutuantes, dialogs, janelas |
| `--card-radius` | **16px** | cards/widgets de dashboard |
| `--input-radius` | **1em** | inputs (acompanha o tamanho da fonte) |
| `--radius-pill` | 999px | botão primário, avatares |

> Input com raio `1em` + botão com raio `6px` é o par que dá o "cheiro" da interface. Mantenha.

### Sombras

```css
--focus-ring:                0 0 0 1px var(--overlay-20);
--shadow-panel:              inset 0 -1px 0 var(--scrim-40), 0 12px 40px rgba(0,0,0,0.65);
--shadow-tooltip:            0 4px 16px var(--scrim-50), inset 0 1px 0 var(--overlay-5);
--shadow-input-focus:        inset 0 0 5px rgba(255,255,255,0.06),
                             inset 0 -5px 8px rgba(255,255,255,0.09),
                             0 0 5px rgba(0,0,0,0.24);
--canvas-chrome-shadow:      0 12px 28px var(--scrim-30);
```

**Nunca** sombra colorida, nunca `box-shadow` decorativa. Sombra = elevação real (painel flutuante, menu, tooltip, input em foco).

### Blur

`--panel-blur: 24px` em painéis flutuantes; `--backdrop-blur: 8px` no backdrop de modais/spotlight. Nada de glassmorphism em conteúdo.

---

## 7. Superfícies

Hierarquia **estrita**. Cada nível só aparece sobre o anterior.

| Token | Dark | Light | Papel |
|---|---|---|---|
| `--bg-body` | `#000000` | `#e3e7ed` | fundo do app / canvas |
| `--bg-surface` | `#1b1b1b` | `#ffffff` | painel dockado, dialog, sidebar |
| `--bg-surface-2` | `#282828` | `#f6f7f9` | **card/tile**, header de dialog, input bg |
| `--bg-surface-3` | `#323232` | `#eceff3` | hover de card, linha zebrada |
| `--bg-surface-4` | `#4a4a4a` | `#e1e5ea` | estado ativo/pressionado |
| `--bg-surface-5` | `#605f5f` | `#d3d9e1` | raro — divisores fortes |

**Receitas canônicas:**

```css
/* Card / widget — sem borda, separado por gap */
.card {
  background: var(--bg-surface-2);
  border: 0;
  border-radius: var(--card-radius);
  padding: var(--space-3xl) var(--space-3xl) var(--space-2xl);
  transition: background 150ms ease;
}
.card:hover { background: var(--bg-surface-3); }

/* Grid de cards — o gap É a divisória */
.grid { display: grid; gap: 1px; background: var(--bg-surface); }

/* Painel / dialog flutuante */
.panel {
  background: var(--bg-surface);
  border-radius: var(--panel-radius);
  box-shadow: var(--shadow-panel);
  backdrop-filter: blur(var(--panel-blur));
}

/* Input — o único que usa borda */
.input {
  background: transparent;
  border: 1px solid var(--overlay-20);
  border-radius: var(--input-radius);
  color: var(--text);
}
.input:hover  { border-color: var(--overlay-30); }
.input:focus  { border-color: var(--overlay-50); box-shadow: var(--shadow-input-focus); }
```

---

## 8. Componentes

### 8.1 Button — a peça central

5 tamanhos, 4 variantes, 1 regra: **só uma ação primária por tela**.

| Variante | Fundo | Texto | Uso |
|---|---|---|---|
| `primary` | `--btn-primary-bg` (branco no dark) | `--btn-primary-fg` | a ação da tela (Salvar/Publicar) |
| `secondary` | `--overlay-10` | `--text-muted` | ações normais |
| `ghost` | transparente | `--text-muted` | toolbar, ícones |
| `destructive` | `--danger-10` | `--danger` | remover/descartar |

| Size | Altura | Padding X | Fonte | Raio |
|---|---|---|---|---|
| `micro` | 18px | `--space-3xs` | `--text-2xs` | 6px |
| `xs` | 22px | `--space-xs` | `--text-2xs` | 8px |
| `sm` *(default)* | 26px | `--space-s` | `--text-xs` | 6px |
| `md` | 32px | `--space-xl` | `--text-s` | 6px |
| `lg` | 44px | `--space-2xl` | `--text-m` | 6px |

```css
.btn {
  display: inline-flex; align-items: center; justify-content: center;
  gap: var(--space-3xs);
  border: 0;
  border-radius: var(--input-radius);
  font-family: inherit;
  font-size: var(--text-xs);
  font-weight: 700;
  white-space: nowrap;
  cursor: pointer;
  user-select: none;
  flex-shrink: 0;
  transition: color 150ms ease, background-color 150ms ease,
              border-color 150ms ease, opacity 150ms ease;
}
.btn:focus-visible { outline: none; box-shadow: var(--focus-ring); }
.btn:disabled     { opacity: 0.38; cursor: default; pointer-events: none; }

/* variantes */
.v-primary   { background: var(--btn-primary-bg); color: var(--btn-primary-fg); }
.v-primary:hover  { background: var(--btn-primary-bg-hover); }
.v-primary:active { background: var(--btn-primary-bg-active); }

.v-secondary { background: var(--overlay-10); color: var(--text-muted); }
.v-secondary:hover  { background: var(--overlay-20); color: var(--text); }
.v-secondary:active { background: var(--overlay-30); }

.v-ghost     { background: transparent; color: var(--text-muted); }
.v-ghost:hover  { background: var(--overlay-10); color: var(--text); }
.v-ghost:active { background: var(--overlay-20); }

.v-destructive { background: var(--danger-10); color: var(--danger); }
.v-destructive:hover  { opacity: 0.9; }

/* tamanhos */
.sm { height: 26px; padding: 0 var(--space-s); font-size: var(--text-xs); }
.md { height: 32px; padding: var(--space-xs) var(--space-xl); font-size: var(--text-s); }
.lg { height: 44px; padding: 0 var(--space-2xl); font-size: var(--text-m); }

/* icon-only = quadrado */
.iconOnly.sm { width: 28px; padding: 0; }
```

**Comportamentos obrigatórios:**
- `type="button"` por padrão (nunca submeter form sem querer).
- `iconOnly` **exige** `aria-label` (warning em dev).
- `busy` → troca o ícone por spinner, `aria-busy="true"`, bloqueia re-clique.
- `disabled` + tooltip → use `aria-disabled` (não `disabled`), senão o tooltip nunca aparece.
- `aria-expanded="true"` → **suprime** o tooltip (não cobrir o menu aberto).
- `aria-pressed` em toggles de toolbar; fundo `--overlay-20`.

### 8.2 Input

```css
.input {
  width: 100%;
  background: transparent;
  border: 1px solid var(--overlay-20);
  border-radius: var(--input-radius);
  color: var(--text);
  font-family: var(--font-sans);
  font-weight: 500;
  outline: none;
  transition: border-color 120ms ease;
}
.input::placeholder { color: var(--text-disabled); }
.size-xs { min-height: 24px; padding: var(--space-4xs) var(--space-xs); font-size: var(--text-xs); }
.size-sm { min-height: 28px; padding: var(--space-2xs) var(--space-s); font-size: var(--text-xs); }
.size-md { min-height: 32px; padding: var(--space-xs)  var(--space-m); font-size: var(--text-s); }
.invalid { border-color: var(--danger); }
```

Sempre com `<label>` visível acima (`ControlRow`/`FormField`), label em `--text-subtle` / `--text-2xs` / uppercase opcional.

### 8.3 Card / Widget

- `--bg-surface-2` sobre `--bg-surface`, gap 1px no grid pai.
- Raio 16px, **sem borda**.
- Hover sobe o tom (`--bg-surface-3`), nunca muda a borda.
- Título: uppercase 10px, `--text-subtle`, tracking 0.07em, com um **dot** de 6px no accent do card.
- Config: `padding: var(--space-3xl) var(--space-3xl) var(--space-2xl)`.

### 8.4 Painel / janela flutuante

```css
.panel {
  background: var(--bg-surface);
  border-radius: var(--panel-radius);
  box-shadow: var(--shadow-panel);
  contain: layout paint;
}
.panelHeader {
  background: var(--bg-surface-2);
  border-bottom: 1px solid var(--border);
  padding: var(--space-l) var(--space-xl);
  display: flex; align-items: center; justify-content: space-between;
}
```

### 8.5 Dialog

| Size | Width |
|---|---|
| `sm` | `min(360px, 100%)` |
| `md` | `min(440px, 100%)` |
| `lg` | `min(520px, 100%)` |
| `xl` | `min(640px, 100%)` |
| `2xl` | `min(820px, 100%)` |

```css
.backdrop {
  position: fixed; inset: 0; z-index: 95;
  display: grid; place-items: center;
  padding: var(--space-5xl);
  background: color-mix(in srgb, var(--bg-body) 50%, transparent);
}
.dialog {
  max-height: 80vh;
  border-radius: var(--panel-radius);
  background: var(--bg-surface);
  box-shadow: 0 18px 48px color-mix(in srgb, var(--bg-body) 28%, transparent);
  display: grid; grid-template-rows: auto 1fr auto;
  overflow: hidden;
}
```

### 8.6 Tooltip

- Fundo `--bg-body` (mais escuro que tudo), texto `--text-bright`, borda `--overlay-5`.
- Raio 6px, padding `--space-xs var(--space-m)`, fonte `--text-s`, `max-width: 240px` (280 se "wide").
- Seta: quadrado 6×6 rotacionado 45° com a mesma cor.
- `z-index: var(--tooltip-z-index)` (10001) — acima de modais.
- `pointer-events: none`, fade-in de 80ms **só depois de medir** (evita flash em 0,0).
- **Nunca** use `title` nativo.

### 8.7 TagPill (chip)

```css
.pill {
  display: inline-flex; align-items: center; gap: var(--space-3xs);
  border-radius: var(--radius);
  background: linear-gradient(0deg, var(--pill-bg), transparent);
  color: var(--pill-color);
  font-weight: 620;
  line-height: 1.4;
  max-width: 100%; overflow: hidden;
  transition: background-color 120ms ease, color 120ms ease;
}
[data-size="xs"] { min-height: 18px; padding: var(--space-px) var(--space-xs); font-size: var(--text-2xs); }
[data-size="sm"] { min-height: 20px; padding: 0 var(--space-s); font-size: var(--text-xs); }
```

Cor derivada do primeiro caractere/nome (ver §3.2), sempre `--accent-N-10` de fundo + `--accent-N` no texto.

### 8.8 Tree row (layers, navegação)

```css
.row {
  display: flex; align-items: center;
  height: var(--tree-row-h, 28px);       /* 36px em data-density="comfortable" */
  padding-left:  var(--tree-row-pl, var(--space-m));
  padding-right: var(--space-l);
  gap: var(--space-4xs);
  border-radius: 8px;
  color: var(--text-muted);
  font-size: var(--text-xs);
  font-weight: 500;
  line-height: 1;
  white-space: nowrap;
  user-select: none; cursor: pointer; outline: none;
  transition: background 80ms ease, color 80ms ease, opacity 80ms ease;
}
.row:hover    { background: var(--overlay-10); color: var(--text); }
.rowSelected  { background: var(--overlay-10); color: var(--text); }
.rowSelected::before {
  content: ""; position: absolute; inset-block: 6px; left: 0;
  width: 2px; background: var(--overlay-60);   /* barra de seleção */
}
```

- Indentação: `padding-left: calc(var(--space-m) + depth * 14px)`.
- Chevron 12px, ícone 14px, texto 12px.
- Drop de DnD: linha `--tree-drop-line` (teal) 2px; "dentro" = fundo `--tree-drop-inside-bg` + borda 1px.

### 8.9 SegmentedControl / Tabs

Container `--bg-surface-2`, raio 8px, 2px de padding. Item ativo = `--bg-surface-3` + `--text`, inativo = `--text-subtle` hover `--text-muted`. Sem underline, sem pill colorido.

### 8.10 Kbd

Fundo em gradiente `--kbd-face-top → --kbd-face-bottom`, borda `--kbd-border`, texto `--text-muted` 10px, raio 3px, sombra interna `--kbd-inner-shadow`.

### 8.11 Toast

`--bg-surface-2`, raio `--panel-radius`, sombra `--shadow-panel`, dot de 6px na cor do estado, `z-index: var(--toast-z-index)` (10000), entra/sai em 150ms, auto-dismiss 4s, pausa no hover.

---

## 9. Layout e chrome

### Shell de 3 colunas (padrão para ferramentas)

```
┌──────────┬──────────────────────────────────────┬───────────┐
│  Rail    │  Painel esquerdo (240–380px)         │  Direita  │
│  48px    │  resizable, colapsável               │  360–420  │
│  ícones  │                                      │  props    │
├──────────┴──────────────────────────────────────┴───────────┤
│  Toolbar 32–40px · borda inferior 1px · bg-surface           │
├──────────────────────────────────────────────────────────────┤
│  Área de trabalho (canvas) — --bg-body                       │
└──────────────────────────────────────────────────────────────┘
```

**Regras do chrome:**
- Rail: 48px, ícones de 18px, item ativo com barra de 2px e tint do accent do workspace.
- Painéis: header `--bg-surface-2` + borda inferior `--border`, tabs em SegmentedControl.
- Larguras persistidas em `localStorage` por workspace (hidrate **antes** do primeiro render para não haver layout shift).
- Divisores: 1px `--border`, com hit area de 5px.
- Toolbar: 32–40px, `--bg-surface`, borda inferior 1px, botões `sm`/`iconOnly`.

### Z-index (mapa global)

| Camada | z-index |
|---|---|
| Conteúdo / canvas | 0 |
| Dropdown | 20 |
| Toolbar | 30 |
| Rail | 55 |
| Sidebars / painéis dockados | 85 |
| Painéis flutuantes / properties | 90 |
| Editor de código sobre painéis | 95 |
| Popovers / menus da toolbar | 201 |
| Preview overlay | 400 |
| Spotlight / command palette | 9000 |
| Toast | 10000 |
| Tooltip | 10001 |

**Regra:** crie um *stacking context* isolado (`position: relative; z-index: 0`) para áreas com muitos overlays (canvas). Assim valores internos (50–60) nunca competem com o chrome.

### Atalhos / command palette

- `Cmd/Ctrl+K` global, 640px de largura, backdrop `--backdrop-blur`, resultados com `Kbd` à direita.
- Toda ação tem atalho listado no próprio item; nada de atalho escondido.

---

## 10. Movimento

| Duração | Onde |
|---|---|
| 80ms | hover de tree row, tooltip fade |
| 120ms | pill, input border |
| 150ms | botão, card, toast |
| 180ms | largura de sidebar |
| 1.4s | shimmer de skeleton (sincronizado) |

- Easing: `ease` (nunca `bounce`, `elastic`, `spring` em UI).
- Anime **só** `color`, `background-color`, `border-color`, `opacity`, `transform`, `width` (em painéis).
- Nunca anime `box-shadow`, `height` (prefira `transform: scaleY`), `top/left`.
- Tudo dentro de `prefers-reduced-motion` (§2.2).

---

## 11. Acessibilidade

- Foco sempre visível: `box-shadow: var(--focus-ring)` + `outline: none` (nunca remover sem substituto).
- `aria-label` obrigatório em botão icon-only.
- Toggle = `aria-pressed`; menu = `aria-expanded` + `aria-haspopup`; árvore = `role="tree"` + `role="treeitem"` + `aria-selected` + `aria-expanded` **no mesmo elemento** que tem `tabIndex`.
- Contraste mínimo 4.5:1 para texto (o sistema entrega ≥ 7:1).
- Alvos de toque: 44px (`lg`) em telas touch; chrome de desktop pode usar 26–28px.
- Nunca depender só de cor: erro tem ícone + texto.
- `aria-busy="true"` em regiões de loading (skeleton é decorativo).
- Navegação por teclado completa: Tab, ↑↓, Enter, Escape, Delete.

---

## 12. Regras de CSS (o que é proibido)

Estas regras são o que mantém o estilo coerente. Se for aplicar em outro projeto, **transforme cada uma em lint/teste**.

| Regra | Motivo |
|---|---|
| **CSS Modules** (`Component.module.css`) por componente | zero colisão, zero CSS morto |
| **Proibido Tailwind / utility classes** | o design é por componente + token, não por utilitário |
| **Proibido hex/rgb/hsl em CSS de componente** | só `var(--token)` |
| **Proibido `px` em fonte e espaço do chrome** | use `--text-*` / `--space-*` |
| **Proibido `!important`** | se precisou, a arquitetura está errada |
| **Proibido `style={{}}` inline** (exceto posição calculada: tooltip, overlay, transform) | quebra tema |
| **Proibido `var(--x, fallback)`** | fallback esconde token faltando |
| **Proibido `<button>` cru** em código de app | use o primitivo `Button` |
| **Proibido bibliotecas de UI** (Radix, MUI, shadcn, clsx, cva) | o sistema é próprio |
| **Proibido `title` nativo** | use o `Tooltip` |
| **Proibido ícone SVG inline** | use o pacote de ícones |

**Composição de classe:** helper de 3 linhas, sem dependência.

```ts
export function cn(...parts: Array<string | false | null | undefined>) {
  return parts.filter(Boolean).join(" ");
}
```

**Estrutura de arquivo por componente:**

```
Button/
├── Button.tsx
├── Button.module.css
└── index.ts
```

---

## 13. Ícones

- **Estilo:** pixel art / 1-bit, 14px no chrome, 16–18px em botões `md`/`lg`, 12px em chevrons.
- **Sempre** componentes importados individualmente (tree-shaking), nunca sprite ou string SVG.
- Cor = `currentColor`. Tamanho por `width/height` explícito.
- Um ícone por ação; não empilhe dois ícones no mesmo botão.
- Ícone + texto: gap `--space-3xs`.
- Acessibilidade: `aria-hidden="true"` quando há texto; `aria-label` no botão quando só ícone.

---

## 14. Adaptando para outro stack

### Tailwind
Mapeie tokens em `theme.extend` e **proíba** classes fora do mapa:

```js
theme: {
  extend: {
    colors: {
      body: 'var(--bg-body)', surface: 'var(--bg-surface)',
      'surface-2': 'var(--bg-surface-2)', 'surface-3': 'var(--bg-surface-3)',
      line: 'var(--border)', ink: 'var(--text)', muted: 'var(--text-muted)',
      subtle: 'var(--text-subtle)', danger: 'var(--danger)',
    },
    borderRadius: { sm: '3px', DEFAULT: '6px', panel: '12px', card: '16px', input: '1em' },
    boxShadow: { focus: 'var(--focus-ring)', panel: 'var(--shadow-panel)' },
  },
}
```
Mantenha as alturas fixas (`h-[26px]`, `h-[32px]`, `h-[44px]`) e a fonte em `text-[length:var(--text-xs)]`.

### CSS puro / outro framework
1. Cole `tokens.css` (§2) no root.
2. Traduza os componentes de §8 como classes `.ds-btn`, `.ds-input`, `.ds-card`.
3. Ative o tema trocando `data-theme` no `<html>`.
4. Rode o checklist abaixo como code review.

### React
- Primitivos em `src/ui/components/<Nome>/` com `index.ts` reexportando.
- Estado de UI local; estado de documento em store global.
- `memo` em itens de lista/tree + seletor por item (evita re-render O(N)).
- Interação contínua (drag, pan, zoom) escreve em `ref.style`, **não** em state.

---

## 15. Checklist de implementação

**Fundação**
- [ ] `tokens.css` importado antes de tudo; zero hex fora dele
- [ ] Inter Variable self-hosted + fallback `system-ui`
- [ ] Reset + scrollbar fina + `prefers-reduced-motion`
- [ ] Tema claro só por troca de tokens

**Chrome**
- [ ] Shell com rail 48px + 2 painéis + toolbar 32–40px
- [ ] Painéis com header `surface-2`, borda 1px, largura persistida
- [ ] Divisores de 1px com hit area de 5px
- [ ] Mapa de z-index documentado e respeitado

**Componentes**
- [ ] `Button` com 4 variantes × 5 tamanhos, `iconOnly` + `aria-label`, `busy`
- [ ] `Input` com raio `1em`, borda `overlay-20`, foco `overlay-50`
- [ ] `Card` sem borda, raio 16, hover sobe superfície
- [ ] `Dialog` com backdrop 50% e raio 12
- [ ] `Tooltip` acima de modais (10001), nunca `title`
- [ ] `TreeRow` 28px com indentação de 14px por nível
- [ ] `Kbd` e `TagPill` conforme §8

**Qualidade**
- [ ] Nenhum `!important`
- [ ] Nenhuma cor hardcoded
- [ ] Nenhum `<button>` cru
- [ ] Foco visível em 100% dos controles
- [ ] Contraste ≥ 7:1 no texto padrão
- [ ] Nenhuma animação acima de 200ms

---

## 16. Anti-padrões

| ❌ Não faça | ✅ Faça |
|---|---|
| Card com borda + sombra + raio grande | card sem borda sobre pai mais escuro, raio 16 |
| Botão colorido (azul/verde) | primário branco/ink; cor só para estado |
| `#333` direto no CSS | `var(--border)` |
| Fonte 16px no chrome | `--text-xs`/`--text-s` (10–12px) |
| Input com fundo `#fff` no dark | fundo transparente + borda alpha |
| Sombra colorida / glow decorativo | sombra neutra só para elevação |
| Hover que muda layout (`padding`, `border`) | só cor/fundo/opacidade |
| 6 tamanhos de fonte diferentes num painel | 2 no máximo (`xs` + `s`) |
| Ícone colorido por padrão | `currentColor` |
| Modal com `z-index: 99999` | mapa de z-index (§9) |
| Gradiente decorativo no header | `--bg-surface-2` chapado |
| Texto cinza claro (`#999`) sobre preto | `--text-muted` (`#a1a1aa`) |

---

*Documento derivado de `src/styles/globals.css`, `src/ui/components/**`, `docs/design.md` e `docs/editor.md` do Instatic (MIT).*
