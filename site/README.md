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

## Ver localmente

```powershell
python -m http.server 8095 --directory site
# abra http://127.0.0.1:8095/
```

## Publicar

- **GitHub Pages**: Settings → Pages → branch `master`, pasta `/site`.
  (O workflow de deploy ainda não está escrito; ver `c2-e18.5`.)
- Qualquer host estático (Netlify, Cloudflare Pages, S3) aceita a pasta como está.

## Convenções

- **Só tokens** de `styles/tokens.css`; nenhuma cor/medida solta e nenhum Tailwind.
- Fonte de UI: sans do sistema. Código: **IBM Plex Mono** (woff2 local).
- Conteúdo em português; números sempre com fonte (`docs/bench-500.json`,
  `docs/bench-ab/`).
- Testemunhos entram com print + modelo + tarefa + data + citação em texto.

## Pendências

- Prints de testemunho (usuário vai fornecer) — hoje há placeholders.
- Imagens: telas Projetos/Engine/Busca e um GIF/AVIF do fluxo completo.
- Deploy automatizado e SEO (OG image, sitemap) — `c2-e18.5`.
