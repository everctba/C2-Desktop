/* Ícones 1-bit do chrome. Componentes individuais, currentColor (§13). */

export type IconName = "projects" | "engine" | "search" | "play" | "stop" | "refresh";

const PATHS: Record<IconName, string> = {
  // pasta
  projects: "M2 4h4l1 2h5v5H2z",
  // engrenagem simplificada
  engine: "M6 2h2l.3 1.6 1.4.6L11 3l1.4 1.4-1 1.3.6 1.4L14 7v2l-1.6.3-.6 1.4 1 1.3L11.4 13l-1.3-1-1.4.6L8.4 14h-2l-.3-1.6-1.4-.6-1.3 1L2 11.4l1-1.3-.6-1.4L1 8.4v-2l1.6-.3.6-1.4-1-1.3L3.6 2l1.3 1 1.4-.6z",
  // lupa
  search: "M6.5 2a4.5 4.5 0 1 1-2.8 8l-2.4 2.4 1 1L4.7 11A4.5 4.5 0 0 1 6.5 2zm0 1.6a2.9 2.9 0 1 0 0 5.8 2.9 2.9 0 0 0 0-5.8z",
  play: "M4 2l9 5-9 5z",
  stop: "M3 3h9v9H3z",
  refresh: "M8 2a6 6 0 1 1-5.7 4.1l1.6.6A4.3 4.3 0 1 0 8 3.7V6L4.5 3 8 0z",
};

export function icon(name: IconName, size = 16): string {
  const path = PATHS[name];
  return `<svg width="${size}" height="${size}" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="${path}" fill="currentColor"/></svg>`;
}
