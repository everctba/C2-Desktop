/**
 * Preview isolado da página Sobre para inspecionar HTML/CSS no navegador.
 * Rode `pnpm dev` e abra http://127.0.0.1:1420/preview.html
 */
import "../styles/global.css";
import { initLang } from "../i18n";
import { renderAbout } from "../views/About";

async function main(): Promise<void> {
  await initLang();
  const { element } = await renderAbout();
  const app = document.getElementById("app")!;
  const wrap = document.createElement("main");
  wrap.className = "content";
  wrap.style.padding = "var(--space-l)";
  wrap.append(element);
  app.append(wrap);
}

void main();
