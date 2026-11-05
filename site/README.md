# Site do C2 (landing + ajuda)

Pasta organizada para o material público do produto. É **estático**: não tem
build, bundler nem dependência instalada.

```text
site/
├── index.html          landing (hero, problema, como funciona, números, instalar,
│                       privacidade, janela, testemunhos, FAQ)
├── ajuda.html          guia completo (requisitos, instalação, uso, telas, CLI,
│                       config, MCP, dados, erros e soluções, FAQ, desinstalar)
├── styles/
│   ├── tokens.css      Design System "Terminal Editorial" (copiado do app)
│   ├── fonts.css       IBM Plex Mono embutida (SIL OFL)
│   └── site.css        layout da landing e da ajuda
└── assets/
    ├── fonts/          woff2 (sem CDN; funciona offline)
    └── img/            capturas do app
```

## Logo e ícone

| Arquivo | Uso |
|---|---|
| `assets/img/logo-horizontal.png` | logo horizontal com fundo transparente (nav, sobre, site escuro) |
| `assets/img/logo-horizontal-dark.png` | logo sobre fundo escuro (README/GitHub, tema claro e escuro) |
| `assets/img/logo-horizontal-light.png` | variante invertida para fundo claro |
| `assets/img/favicon.ico`, `favicon-32.png`, `apple-touch-icon.png` | favicon das páginas |
| `assets/img/og-image.png` (1200×630) | preview de link (Open Graph/Twitter) |
| `assets/ascii/logo-ascii.txt` | arte ASCII usada no hero (ASCII puro, alinha em qualquer mono) |

Fontes originais em `ARTES/` (PSD + PNG). O ícone do executável é gerado com
`pnpm tauri icon ARTES/icone-c2-1024.png` (saída em `apps/desktop/src-tauri/icons`).

## Ver localmente

```powershell
python -m http.server 8095 --directory site
# abra http://127.0.0.1:8095/
```

## Publicar

- **GitHub Pages**: Settings → Pages → branch `master`, pasta `/site`.
  (O workflow de deploy ainda não está escrito; ver `c2-e18.5`.)
- Qualquer host estático (Netlify, Cloudflare Pages, S3) aceita a pasta como está.

## Configurar perfil e doações

Tudo fica em **`site/config.js`** (nada sai do navegador):

```js
window.C2_SITE = {
  nome: "Everson Mayer",
  github: "everctba",        // foto/nome abrem este perfil
  bio: "...",
  paypal: "everctba",        // paypal.me/usuario; vazio esconde o cartão
  pixChave: "chave@exemplo", // copia-e-cola; vazio esconde o cartão
  pixNome: "Nome do titular",
  githubSponsors: "",        // opcional, ex.: https://github.com/sponsors/everctba
};
```

Foto: coloque `site/assets/img/avatar.jpg` — se não existir, aparece a inicial
do nome. Os botões US$ 5/10/15 montam `paypal.me/<usuario>/<valor>`.

## Ruído animado\n\nO fundo usa ssets/img/ruido.gif (mesmo do app) em ody::before:\nackground-size: 180px, opacity: 0.25, mix-blend-mode: screen. O screen\né necessário porque overlay sobre preto puro não altera nada; por isso o\nody usa #0b0b0b. Respeita prefers-reduced-motion.\n\n## Convenções

- **Só tokens** de `styles/tokens.css`; nenhuma cor/medida solta e nenhum Tailwind.
- Fonte de UI: sans do sistema. Código: **IBM Plex Mono** (woff2 local).
- Conteúdo em português; números sempre com fonte (`docs/bench-500.json`,
  `docs/bench-ab/`).
- Testemunhos entram com print + modelo + tarefa + data + citação em texto.

## Pendências

- Prints de testemunho (usuário vai fornecer) — hoje há placeholders.
- Imagens: telas Projetos/Engine/Busca e um GIF/AVIF do fluxo completo.
- Deploy automatizado e SEO (OG image, sitemap) — `c2-e18.5`.

## Idioma (PT/EN)

O site tem seletor flutuante (bolinha com bandeira + menu) nas três páginas.
PT fica na raiz; EN é gerado em `site/en/` a partir do dicionário
`site/i18n/en.json`:

```powershell
python scripts/build-site-en.py   # gera site/en/{index,ajuda,sobre}.html + hreflang
```

- o dicionário fica em `site/i18n.js` (fonte) e `site/i18n/en.json` (gerado);
- `hreflang` pt-BR/en/x-default é aplicado nas páginas PT e EN;
- o seletor navega para `/en/...` quando a versão existe (melhor SEO);
- `?lang=en` também abre a versão EN;
- ao mudar texto PT, rode o script e commite `site/en/` junto.

Obs.: a tradução de parágrafos longos com links inline ainda é parcial
(acompanhada em `c2-gj4.4`).
