import type { UsageEstimate } from "../types";
import styles from "./MemoryBar.module.css";

const gb = (mb: number) => `${(mb / 1024).toFixed(1)} GB`;

/**
 * Barra de memoria do modelo: pesos + KV cache + overhead contra a VRAM livre.
 * Cores só quando carregam significado (segmentos) e vermelho quando excede.
 */
export function memoryBar(estimate: UsageEstimate): HTMLElement {
  const wrapper = document.createElement("div");
  wrapper.style.display = "grid";
  wrapper.style.gap = "var(--space-2xs)";

  const bar = document.createElement("div");
  bar.className = styles.bar;
  bar.setAttribute("role", "img");
  bar.setAttribute(
    "aria-label",
    `Modelo ${gb(estimate.model_mb)}, contexto ${gb(estimate.kv_mb)}, total ${gb(estimate.total_mb)} de ${gb(estimate.available_mb)} livres`,
  );

  const escala = Math.max(estimate.total_mb, estimate.available_mb, 1);
  const excede = estimate.total_mb > estimate.available_mb && estimate.available_mb > 0;

  const segmento = (mb: number, classe: string) => {
    const el = document.createElement("div");
    el.className = `${styles.segment} ${classe}`;
    el.style.width = `${(mb / escala) * 100}%`;
    return el;
  };

  bar.appendChild(segmento(estimate.model_mb, styles.model));
  if (estimate.kv_mb > 0) bar.appendChild(segmento(estimate.kv_mb, styles.kv));
  if (estimate.overhead_mb > 0) bar.appendChild(segmento(estimate.overhead_mb, styles.overhead));
  if (excede) {
    const resto = estimate.total_mb - estimate.available_mb;
    bar.appendChild(segmento(resto, styles.overflow));
  }
  wrapper.appendChild(bar);

  const legend = document.createElement("div");
  legend.className = styles.legend;
  const item = (label: string, classe: string) => {
    const el = document.createElement("span");
    el.className = styles.key;
    const swatch = document.createElement("span");
    swatch.className = `${styles.swatch} ${classe}`;
    el.append(swatch, document.createTextNode(label));
    return el;
  };
  legend.append(
    item(`modelo ${gb(estimate.model_mb)}`, styles.model),
    item(`contexto ${gb(estimate.kv_mb)}`, styles.kv),
    item(`overhead ${gb(estimate.overhead_mb)}`, styles.overhead),
  );
  wrapper.appendChild(legend);

  const totals = document.createElement("div");
  totals.className = styles.totals;
  const total = document.createElement("span");
  total.textContent = `total estimado ${gb(estimate.total_mb)}`;
  const livre = document.createElement("span");
  livre.textContent = estimate.available_mb > 0 ? `VRAM livre ${gb(estimate.available_mb)}` : "GPU não detectada";

  if (estimate.fit === "fits") total.className = styles.ok;
  if (estimate.fit === "tight") total.className = styles.warning;
  if (estimate.fit === "too_big") total.className = styles.danger;
  totals.append(total, livre);
  wrapper.appendChild(totals);

  if (estimate.fit === "too_big") {
    const aviso = document.createElement("div");
    aviso.className = styles.danger;
    aviso.style.fontSize = "var(--text-2xs)";
    aviso.textContent =
      "Não cabe na VRAM livre: reduza o contexto/camadas na GPU ou libere memória antes de carregar.";
    wrapper.appendChild(aviso);
  } else if (estimate.fit === "tight") {
    const aviso = document.createElement("div");
    aviso.className = styles.warning;
    aviso.style.fontSize = "var(--text-2xs)";
    aviso.textContent = "Fica no limite da VRAM: parte do modelo pode ir para a RAM.";
    wrapper.appendChild(aviso);
  }

  return wrapper;
}

export function loadingBar(): HTMLElement {
  const wrapper = document.createElement("div");
  wrapper.style.display = "grid";
  wrapper.style.gap = "var(--space-2xs)";
  const bar = document.createElement("div");
  bar.className = styles.indeterminate;
  const label = document.createElement("div");
  label.className = styles.legend;
  label.textContent = "carregando o modelo na GPU…";
  wrapper.append(bar, label);
  return wrapper;
}
