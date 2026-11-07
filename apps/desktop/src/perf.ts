/**
 * Diagnostico de desempenho (bead c2-csp).
 *
 * Eventos de boot sao gravados sempre em `%APPDATA%/dev.c2.desktop/perf.log`
 * (JSONL). O probe de mouse/clique so liga com `localStorage.setItem("c2.perf","1")`.
 */
import { api } from "./api";

const CHAVE = "c2.perf";

export function perfAtivo(): boolean {
  try {
    return localStorage.getItem(CHAVE) === "1";
  } catch {
    return false;
  }
}

export function perf(evento: string, dados: Record<string, unknown> = {}): void {
  if (!perfAtivo()) return;
  enviar(evento, dados);
}

/** Marcas de boot sempre registradas (poucas linhas, uteis para cold start). */
export function perfBoot(evento: string, dados: Record<string, unknown> = {}): void {
  enviar(evento, dados);
}

function enviar(evento: string, dados: Record<string, unknown>): void {
  try {
    void api
      .perfLog(evento, { ...dados, js_ms: Math.round(performance.now()) })
      .catch(() => {});
  } catch {
    /* Tauri ainda nao disponivel */
  }
}

let probeIniciado = false;

export function iniciarProbeUI(): void {
  if (!perfAtivo() || probeIniciado) return;
  probeIniciado = true;

  let down: { x: number; y: number; t: number } | null = null;
  window.addEventListener(
    "pointerdown",
    (evento) => {
      down = { x: evento.clientX, y: evento.clientY, t: performance.now() };
    },
    true,
  );
  window.addEventListener(
    "click",
    (evento) => {
      const clique = performance.now();
      const inicio = down;
      down = null;
      requestAnimationFrame(() => {
        const frame = performance.now();
        perf("ui_click", {
          x: evento.clientX,
          y: evento.clientY,
          down_to_click_ms: inicio ? Math.round(clique - inicio.t) : null,
          click_to_frame_ms: Math.round(frame - clique),
        });
      });
    },
    true,
  );
}
