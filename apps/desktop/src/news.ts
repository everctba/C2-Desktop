import { api, hasTauri, openExternal } from "./api";
import styles from "./views/News.module.css";
import { createDialog } from "./ui/Dialog";
import { Button } from "./ui/Button";

/**
 * Popup de novidades ("What's New").
 *
 * - conteudo vem de um `news.json` estatico (sem backend, sem release);
 * - mostra no maximo 1 post, no maximo 1 vez;
 * - falha em silencio (offline/JSON invalido nao mostra nada);
 * - links so http/https/mailto; texto nunca vira innerHTML.
 */

const URL_PADRAO = "https://everctba.github.io/C2-Desktop/news.json";
const MIN_ABERTURAS = 5;
const INTERVALO_CHECAGEM_MS = 24 * 60 * 60 * 1000;
const ATRASO_MS = 3000;

interface NewsState {
  launches: number;
  lastSeenDate: string | null;
  lastCheckAt: number;
}

interface NewsPost {
  id: string;
  title: string;
  description: string;
  date: string;
  image?: string;
  url?: string;
}

const ESTADO_VAZIO: NewsState = { launches: 0, lastSeenDate: null, lastCheckAt: 0 };

function lerEstado(bruto: string | null): NewsState {
  if (!bruto) return { ...ESTADO_VAZIO };
  try {
    const valor = JSON.parse(bruto.replace(/^\uFEFF/, "")) as Partial<NewsState>;
    return {
      launches: Number.isFinite(valor.launches) ? Number(valor.launches) : 0,
      lastSeenDate:
        typeof valor.lastSeenDate === "string" && Number.isFinite(Date.parse(valor.lastSeenDate))
          ? valor.lastSeenDate
          : null,
      lastCheckAt: Number.isFinite(valor.lastCheckAt) ? Number(valor.lastCheckAt) : 0,
    };
  } catch {
    return { ...ESTADO_VAZIO };
  }
}

async function salvarEstado(estado: NewsState): Promise<void> {
  try {
    await api.newsStateSave(JSON.stringify(estado));
  } catch {
    /* best-effort: estado local e opcional */
  }
}

function urlPermitida(url: string): boolean {
  const minuscula = url.trim().toLowerCase();
  return (
    minuscula.startsWith("http://") ||
    minuscula.startsWith("https://") ||
    minuscula.startsWith("mailto:")
  );
}

function postsValidos(bruto: string): NewsPost[] {
  let valor: unknown;
  try {
    valor = JSON.parse(bruto.replace(/^\uFEFF/, ""));
  } catch {
    return [];
  }
  const posts = (valor as { posts?: unknown })?.posts;
  if (!Array.isArray(posts)) return [];

  const validos: NewsPost[] = [];
  for (const item of posts) {
    if (!item || typeof item !== "object") continue;
    const post = item as Record<string, unknown>;
    if (
      typeof post.id !== "string" ||
      typeof post.title !== "string" ||
      typeof post.description !== "string" ||
      typeof post.date !== "string" ||
      !Number.isFinite(Date.parse(post.date))
    ) {
      continue;
    }
    validos.push({
      id: post.id,
      title: post.title.trim(),
      description: post.description.trim(),
      date: post.date,
      image: typeof post.image === "string" && post.image.trim() ? post.image.trim() : undefined,
      url: typeof post.url === "string" && post.url.trim() ? post.url.trim() : undefined,
    });
  }
  return validos;
}

function resolverUrl(base: string, relativo: string): string | null {
  try {
    const url = new URL(relativo, base).toString();
    return urlPermitida(url) ? url : null;
  } catch {
    return null;
  }
}

/** Renderiza a descricao escapando texto e criando apenas links http/https/mailto. */
function renderDescricao(destino: HTMLElement, texto: string, base: string): void {
  const padrao = /\[([^\]]+)\]\(([^)\s]+)\)/g;
  let ultimo = 0;
  let encontrado: RegExpExecArray | null;
  while ((encontrado = padrao.exec(texto)) !== null) {
    const antes = texto.slice(ultimo, encontrado.index);
    if (antes) destino.appendChild(document.createTextNode(antes));

    const rotulo = encontrado[1];
    const href = encontrado[2];
    const url = resolverUrl(base, href);
    if (url && urlPermitida(url)) {
      const link = document.createElement("a");
      link.href = url;
      link.textContent = rotulo;
      link.target = "_blank";
      link.rel = "noopener noreferrer";
      link.addEventListener("click", (evento) => {
        evento.preventDefault();
        void openExternal(url);
      });
      destino.appendChild(link);
    } else {
      destino.appendChild(document.createTextNode(rotulo));
    }
    ultimo = padrao.lastIndex;
  }
  const resto = texto.slice(ultimo);
  if (resto) destino.appendChild(document.createTextNode(resto));
}

function mostrarNovidade(post: NewsPost, base: string, aoFechar: () => void): void {
  const handle = createDialog({
    title: "O que há de novo",
    onClose: aoFechar,
  });
  handle.element.classList.add(styles.modal);
  handle.body.classList.add(styles.body);

  if (post.image) {
    const imagem = resolverUrl(base, post.image);
    if (imagem) {
      const img = document.createElement("img");
      img.className = styles.capa;
      img.alt = post.title;
      img.src = imagem;
      img.addEventListener("error", () => img.remove());
      handle.body.appendChild(img);
    }
  }

  const conteudo = document.createElement("div");
  conteudo.className = styles.conteudo;

  const selo = document.createElement("span");
  selo.className = styles.selo;
  selo.textContent = "Novidade!";
  conteudo.appendChild(selo);

  const titulo = document.createElement("h2");
  titulo.className = styles.titulo;
  titulo.textContent = post.title;
  conteudo.appendChild(titulo);

  const descricao = document.createElement("p");
  descricao.className = styles.descricao;
  renderDescricao(descricao, post.description, base);
  conteudo.appendChild(descricao);

  handle.body.appendChild(conteudo);

  const fechar = Button({
    label: "Fechar",
    variant: "ghost",
    onClick: () => handle.close(),
  });
  handle.footer.appendChild(fechar);

  if (post.url) {
    const url = resolverUrl(base, post.url);
    if (url && urlPermitida(url)) {
      const verMais = Button({
        label: "Ver mais",
        variant: "primary",
        onClick: () => {
          void openExternal(url);
          handle.close();
        },
      });
      handle.footer.appendChild(verMais);
      verMais.focus();
    } else {
      fechar.focus();
    }
  } else {
    fechar.focus();
  }
}

async function checarNovidades(): Promise<void> {
  const estado = lerEstado(await api.newsStateLoad().catch(() => null));
  estado.launches += 1;
  await salvarEstado(estado);

  if (estado.launches < MIN_ABERTURAS) return;
  if (Date.now() - estado.lastCheckAt < INTERVALO_CHECAGEM_MS) return;

  // Registra a tentativa (mesmo se falhar) para nao tentar em loop.
  estado.lastCheckAt = Date.now();
  await salvarEstado(estado);

  const url = (await api.newsUrl().catch(() => null)) ?? URL_PADRAO;
  const bruto = await api.newsFetch(url).catch(() => null);
  if (!bruto) return;

  const posts = postsValidos(bruto);
  if (posts.length === 0) return;
  posts.sort((a, b) => Date.parse(b.date) - Date.parse(a.date));
  const post = posts[0];

  // Primeira execucao: grava a base e NAO mostra.
  if (!estado.lastSeenDate) {
    estado.lastSeenDate = post.date;
    await salvarEstado(estado);
    return;
  }

  if (Date.parse(post.date) <= Date.parse(estado.lastSeenDate)) return;

  let fechado = false;
  mostrarNovidade(post, url, () => {
    if (fechado) return;
    fechado = true;
    estado.lastSeenDate = post.date;
    void salvarEstado(estado);
  });
}

/** Agenda a checagem depois do boot (nunca durante o splash). */
export function initNews(): void {
  if (!hasTauri) return;
  // Evita competir com a primeira acao do usuario.
  window.setTimeout(() => {
    void checarNovidades().catch(() => {
      /* falha em silencio */
    });
  }, ATRASO_MS);
}
