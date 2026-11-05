/** Bandeiras em SVG inline (emoji de bandeira nao renderiza no Windows). */

export function flagUS(size = 20): string {
  const largura = size;
  const altura = Math.round(size * 0.66);
  return `<svg width="${largura}" height="${altura}" viewBox="0 0 24 16" aria-hidden="true" style="display:block;border-radius:2px">
    <defs><clipPath id="us-clip"><rect width="24" height="16" rx="1.5"/></clipPath></defs>
    <g clip-path="url(#us-clip)">
      <rect width="24" height="16" fill="#fff"/>
      <g fill="#b22234">
        <rect y="0" width="24" height="1.23"/><rect y="2.46" width="24" height="1.23"/>
        <rect y="4.92" width="24" height="1.23"/><rect y="7.38" width="24" height="1.23"/>
        <rect y="9.85" width="24" height="1.23"/><rect y="12.31" width="24" height="1.23"/>
        <rect y="14.77" width="24" height="1.23"/>
      </g>
      <rect width="10" height="8.62" fill="#3c3b6e"/>
      <g fill="#fff">
        <circle cx="1.6" cy="1.5" r="0.5"/><circle cx="3.6" cy="1.5" r="0.5"/>
        <circle cx="5.6" cy="1.5" r="0.5"/><circle cx="7.6" cy="1.5" r="0.5"/>
        <circle cx="2.6" cy="3" r="0.5"/><circle cx="4.6" cy="3" r="0.5"/>
        <circle cx="6.6" cy="3" r="0.5"/><circle cx="8.6" cy="3" r="0.5"/>
        <circle cx="1.6" cy="4.5" r="0.5"/><circle cx="3.6" cy="4.5" r="0.5"/>
        <circle cx="5.6" cy="4.5" r="0.5"/><circle cx="7.6" cy="4.5" r="0.5"/>
        <circle cx="2.6" cy="6" r="0.5"/><circle cx="4.6" cy="6" r="0.5"/>
        <circle cx="6.6" cy="6" r="0.5"/><circle cx="8.6" cy="6" r="0.5"/>
      </g>
    </g>
  </svg>`;
}

export function flagBR(size = 20): string {
  const largura = size;
  const altura = Math.round(size * 0.66);
  return `<svg width="${largura}" height="${altura}" viewBox="0 0 24 16" aria-hidden="true" style="display:block;border-radius:2px">
    <defs><clipPath id="br-clip"><rect width="24" height="16" rx="1.5"/></clipPath></defs>
    <g clip-path="url(#br-clip)">
      <rect width="24" height="16" fill="#009b3a"/>
      <path d="M12 2.2 21.5 8 12 13.8 2.5 8z" fill="#fedf00"/>
      <circle cx="12" cy="8" r="3.4" fill="#002776"/>
      <path d="M8.8 7.4c2.2-.6 4.6-.2 6.4 1.1" stroke="#fff" stroke-width="0.8" fill="none"/>
    </g>
  </svg>`;
}
