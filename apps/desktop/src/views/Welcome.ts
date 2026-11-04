import { Button } from "../ui/Button";
import { createDialog } from "../ui/Dialog";
import { Pill } from "../ui/Pill";
import { modelInstructions } from "./Projects";
import styles from "./views.module.css";

export interface WelcomeHandlers {
  /// Caminho do workspace atual ("" quando nenhum).
  root: () => string;
  /// Abre o seletor de pasta e devolve o caminho escolhido (ou null).
  pickProject: () => Promise<string | null>;
  /// Abre o seletor de .gguf, valida, grava e devolve o caminho (ou null).
  chooseModel: () => Promise<string | null>;
  /// Navega para a Engine (fim do assistente).
  finish: () => void;
  /// Fecha marcando como concluido (nao aparece de novo).
  dismiss: () => void;
}

/**
 * Assistente de primeiros passos (c2-e16 / primeira execucao):
 * 1) baixar o modelo, 2) abrir a pasta do projeto, 3) selecionar o .gguf.
 */
export function openWelcome(handlers: WelcomeHandlers): void {
  const dialog = createDialog({
    title: "Bem-vindo ao C2",
    onClose: () => handlers.dismiss(),
  });
  dialog.element.style.width = "min(620px, 100%)";

  const passo = document.createElement("div");
  passo.style.display = "flex";
  passo.style.gap = "var(--space-2xs)";
  passo.style.alignItems = "center";
  dialog.body.appendChild(passo);

  const corpo = document.createElement("div");
  corpo.style.display = "grid";
  corpo.style.gap = "var(--space-s)";
  dialog.body.appendChild(corpo);

  let atual = 0;
  let modeloEscolhido: string | null = null;

  const TOTAL = 3;
  const rotulos = ["Modelo", "Projeto", "Selecionar"];

  const renderPassos = () => {
    passo.innerHTML = "";
    for (let indice = 0; indice < TOTAL; indice++) {
      const ativo = indice === atual;
      const feito = indice < atual || (indice === 2 && modeloEscolhido);
      passo.appendChild(
        Pill(
          `${indice + 1}. ${rotulos[indice]}`,
          ativo ? "accent" : feito ? "success" : "neutral",
          "xs",
        ),
      );
    }
  };

  const construir = () => {
    renderPassos();
    corpo.innerHTML = "";
    dialog.footer.innerHTML = "";

    if (atual === 0) {
      corpo.appendChild(titulo("Baixe um modelo local (GGUF)"));
      corpo.appendChild(
        texto(
          "O C2 roda o modelo na sua máquina — nada sai do seu computador. " +
            "Recomendamos o Gemma 4 E2B em Q4_K_M (~3,2 GB), que cabe numa GPU de 8 GB.",
        ),
      );
      const instrucoes = modelInstructions();
      if (instrucoes instanceof HTMLDetailsElement) instrucoes.open = true;
      corpo.appendChild(instrucoes);
    }

    if (atual === 1) {
      corpo.appendChild(titulo("Abra a pasta do projeto"));
      corpo.appendChild(
        texto(
          "O C2 grava os metadados ao lado de cada arquivo (<arquivo>.c2.json) e o índice em .c2/. " +
            "Escolha a pasta que você quer indexar.",
        ),
      );
      const raiz = handlers.root();
      if (raiz) {
        const campo = document.createElement("div");
        campo.className = styles.field;
        const rotulo = document.createElement("span");
        rotulo.className = styles.fieldLabel;
        rotulo.textContent = "pasta selecionada";
        const valor = document.createElement("span");
        valor.className = styles.mono;
        valor.textContent = raiz;
        campo.append(rotulo, valor);
        corpo.appendChild(campo);
        corpo.appendChild(
          botaoConteudo(
            Button({
              label: "Trocar pasta",
              size: "xs",
              variant: "secondary",
              onClick: async () => {
                const escolhida = await handlers.pickProject();
                if (escolhida) construir();
              },
            }),
          ),
        );
      } else {
        corpo.appendChild(
          botaoConteudo(
            Button({
              label: "Abrir projeto…",
              iconName: "projects",
              variant: "primary",
              onClick: async () => {
                const escolhida = await handlers.pickProject();
                if (escolhida) construir();
              },
            }),
          ),
        );
      }
    }

    if (atual === 2) {
      corpo.appendChild(titulo("Selecione o arquivo .gguf"));
      corpo.appendChild(
        texto(
          "Escolha o modelo que você baixou. O C2 lê apenas o cabeçalho para validar " +
            "(nome, quantização e se cabe na VRAM) e guarda só o caminho — o arquivo não é copiado.",
        ),
      );

      const raiz = handlers.root();
      if (!raiz) {
        corpo.appendChild(
          texto("Abra a pasta do projeto antes de selecionar o modelo."),
        );
      } else {
        if (modeloEscolhido) {
          const campo = document.createElement("div");
          campo.className = styles.field;
          const rotulo = document.createElement("span");
          rotulo.className = styles.fieldLabel;
          rotulo.textContent = "modelo selecionado";
          const valor = document.createElement("span");
          valor.className = styles.mono;
          valor.textContent = modeloEscolhido;
          campo.append(rotulo, valor);
          corpo.appendChild(campo);
        }
        corpo.appendChild(
          botaoConteudo(
            Button({
              label: modeloEscolhido ? "Trocar modelo…" : "Selecionar .gguf…",
              iconName: "projects",
              variant: "primary",
              onClick: async () => {
                const escolhido = await handlers.chooseModel();
                if (escolhido) {
                  modeloEscolhido = escolhido;
                  construir();
                }
              },
            }),
          ),
        );
      }
    }

    /* rodape */
    const voltar = Button({
      label: "Voltar",
      variant: "ghost",
      onClick: () => {
        if (atual > 0) {
          atual -= 1;
          construir();
        }
      },
    });
    voltar.disabled = atual === 0;

    const pular = Button({
      label: "Pular",
      variant: "ghost",
      onClick: () => {
        dialog.close();
        handlers.dismiss();
      },
    });

    const avancar = Button({
      label: atual === TOTAL - 1 ? "Ir para a Engine" : "Avançar",
      variant: "primary",
      onClick: () => {
        if (atual === TOTAL - 1) {
          dialog.close();
          handlers.finish();
          return;
        }
        atual += 1;
        construir();
      },
    });
    avancar.disabled =
      (atual === 1 && !handlers.root()) || (atual === 2 && !modeloEscolhido);

    dialog.footer.append(voltar, pular, avancar);
  };

  construir();
}

function botaoConteudo(botao: HTMLButtonElement): HTMLElement {
  botao.style.width = "fit-content";
  return botao;
}

function titulo(texto: string): HTMLElement {
  const el = document.createElement("h2");
  el.textContent = texto;
  el.style.margin = "0";
  el.style.fontSize = "var(--text-s)";
  el.style.fontWeight = "700";
  el.style.color = "var(--text-bright)";
  return el;
}

function texto(conteudo: string): HTMLElement {
  const el = document.createElement("p");
  el.textContent = conteudo;
  el.style.margin = "0";
  el.style.fontSize = "var(--text-xs)";
  el.style.lineHeight = "1.5";
  el.style.color = "var(--text-muted)";
  return el;
}

