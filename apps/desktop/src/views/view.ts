/** Resultado de uma tela: o elemento e, opcionalmente, como limpar efeitos. */
export interface View {
  element: HTMLElement;
  dispose?: () => void;
}
