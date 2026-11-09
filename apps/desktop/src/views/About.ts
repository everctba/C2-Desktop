import avatarUrl from "../assets/avatar.webp";
import paypalQrUrl from "../assets/paypal-qr.png";
import pixQrUrl from "../assets/pix-qr.png";
import { api, openExternal } from "../api";
import { t } from "../i18n";
import styles from "./About.module.css";
import { Button } from "../ui/Button";
import { Card, cardBody } from "../ui/Card";
import { Pill } from "../ui/Pill";

const PERFIL = {
  nome: "Everson Mayer",
  bio: "Criador do C2 — Comments 2.0 for AI. Desenvolvedor, designer e entusiasta de IA local.",
  github: "https://github.com/everctba",
  repo: "https://github.com/everctba/C2-Desktop",
  paypalEmail: "mayerdesigner@gmail.com",
  pixChave: "everson777@gmail.com",
  pixNome: "Everson Mayer",
};

async function copiar(texto: string): Promise<boolean> {
  try {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
    await writeText(texto);
    return true;
  } catch {
    try {
      await navigator.clipboard.writeText(texto);
      return true;
    } catch {
      return false;
    }
  }
}

function botaoCopiar(texto: string, rotulo = "Copiar"): HTMLButtonElement {
  const botao = Button({
    label: rotulo,
    size: "xs",
    variant: "secondary",
    onClick: async () => {
      const ok = await copiar(texto);
      botao.textContent = ok ? "Copiado" : "Falhou";
      setTimeout(() => {
        botao.textContent = rotulo;
      }, 1600);
    },
  });
  botao.style.width = "96px";
  botao.style.justifyContent = "center";
  return botao;
}

function cartaoDoacao(
  titulo: string,
  etiqueta: string,
  descricao: string,
  qr: string,
  chave: string,
  legenda: string,
): HTMLElement {
  const card = Card({ title: titulo, accent: 6 });
  const body = cardBody(card);

  const tag = document.createElement("span");
  tag.className = styles.k;
  tag.textContent = etiqueta;
  body.appendChild(tag);

  const texto = document.createElement("p");
  texto.textContent = descricao;
  body.appendChild(texto);

  const caixa = document.createElement("div");
  caixa.className = styles.qrBox;
  const img = document.createElement("img");
  img.src = qr;
  img.alt = `QR code de doação — ${titulo}`;
  img.className = styles.qr;
  caixa.appendChild(img);
  body.appendChild(caixa);

  const legendaEl = document.createElement("span");
  legendaEl.className = styles.legenda;
  legendaEl.textContent = legenda;
  body.appendChild(legendaEl);

  const linha = document.createElement("div");
  linha.className = styles.chaveLinha;
  const chaveEl = document.createElement("code");
  chaveEl.className = styles.chave;
  chaveEl.textContent = chave;
  linha.append(chaveEl, botaoCopiar(chave));
  body.appendChild(linha);

  return card;
}

export async function renderAbout(): Promise<{ element: HTMLElement }> {
  const page = document.createElement("div");
  page.className = styles.page;

  const header = document.createElement("div");
  header.className = styles.header;
  const titulo = document.createElement("h1");
  titulo.className = "pageTitle";
  titulo.textContent = t("Sobre");
  header.append(titulo);
  page.appendChild(header);

  // ----- perfil -----
  const perfilCard = Card({ title: t("Quem faz"), accent: 1 });
  const perfilBody = cardBody(perfilCard);
  const perfil = document.createElement("div");
  perfil.className = styles.perfil;

  const avatar = document.createElement("img");
  avatar.src = avatarUrl;
  avatar.alt = `Foto de ${PERFIL.nome}`;
  avatar.className = styles.avatar;
  perfil.appendChild(avatar);

  const infos = document.createElement("div");
  infos.className = styles.perfilInfo;
  const nome = document.createElement("h2");
  nome.className = styles.nome;
  nome.textContent = PERFIL.nome;
  const bio = document.createElement("p");
  bio.textContent = t(PERFIL.bio);
  const pills = document.createElement("div");
  pills.className = styles.pills;
  const versao = await api.appVersion().catch(() => "0.1.0");
  pills.append(
    Pill(`C2 v${versao}`, "accent", "xs"),
    Pill("open source", "neutral", "xs"),
    Pill("Rust + Tauri", "neutral", "xs"),
  );
  infos.append(nome, bio, pills);
  perfil.appendChild(infos);
  perfilBody.appendChild(perfil);

  const acoes = document.createElement("div");
  acoes.className = styles.acoes;
  acoes.append(
    Button({
      label: t("GitHub"),
      iconName: "projects",
      size: "xs",
      variant: "secondary",
      onClick: () => void openExternal(PERFIL.github),
    }),
    Button({
      label: t("Repositório do C2"),
      size: "xs",
      variant: "secondary",
      onClick: () => void openExternal(PERFIL.repo),
    }),
  );
  perfilBody.appendChild(acoes);
  page.appendChild(perfilCard);

  // ----- apoio -----
  const apoio = document.createElement("section");
  apoio.className = styles.apoio;
  const apoioTitulo = document.createElement("h2");
  apoioTitulo.textContent = t("Apoie o projeto");
  const apoioTexto = document.createElement("p");
  apoioTexto.textContent = t(
    "O C2 é gratuito e continua gratuito: nenhuma funcionalidade fica atrás de doação. " +
      "Se ele te economizou tempo, qualquer valor ajuda a manter o desenvolvimento. Doação opcional.",
  );
  apoio.append(apoioTitulo, apoioTexto);

  const grade = document.createElement("div");
  grade.className = styles.grade;
  grade.append(
    cartaoDoacao(
      "PayPal",
      t("internacional"),
      t("Cartão ou saldo PayPal, em dólar. Funciona fora do Brasil."),
      paypalQrUrl,
      PERFIL.paypalEmail,
      t("escaneie com o app do PayPal"),
    ),
    cartaoDoacao(
      "PIX",
      t("Brasil"),
      t("Pagamento instantâneo em reais, direto do seu banco."),
      pixQrUrl,
      PERFIL.pixChave,
      `PIX · ${PERFIL.pixNome}`,
    ),
  );
  apoio.appendChild(grade);
  page.appendChild(apoio);

  // ----- programa -----
  const programa = Card({ title: t("Programa"), accent: 3 });
  const programaBody = cardBody(programa);
  for (const [rotulo, valor] of [
    [t("Versão"), versao],
    [t("Tecnologia"), "Rust (c2-core + CLI) · llama.cpp · Tauri"],
    [t("Site"), "https://everctba.github.io/C2-Desktop/"],
    [t("Licença"), t("software gratuito e open source")],
  ] as Array<[string, string]>) {
    const linha = document.createElement("div");
    linha.className = styles.linha;
    const rotuloEl = document.createElement("span");
    rotuloEl.className = styles.rotulo;
    rotuloEl.textContent = rotulo;
    const valorEl = document.createElement("span");
    valorEl.textContent = valor;
    linha.append(rotuloEl, valorEl);
    programaBody.appendChild(linha);
  }
  page.appendChild(programa);

  return { element: page };
}
