import { api } from "../api";
import { t } from "../i18n";
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
  /// Abre a tela de Configuracoes (ex.: configurar provedor de nuvem).
  openSettings: () => void;
}

/**
 * Assistente de primeiros passos (c2-e16 / primeira execucao):
 * 1) baixar o modelo, 2) abrir a pasta do projeto, 3) selecionar o .gguf.
 */
export function openWelcome(
  handlers: WelcomeHandlers,
  modo: "local" | "nuvem" = "local",
): void {
  const dialog = createDialog({
    title: t("Bem-vindo ao C2"),
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

  const TOTAL = modo === "nuvem" ? 2 : 3;
  const rotulos = modo === "nuvem"
    ? [t("Nuvem"), t("Projeto")]
    : [t("Modelo"), t("Projeto"), t("Selecionar")];

  const renderPassos = () => {
    passo.innerHTML = "";
    for (let indice = 0; indice < TOTAL; indice++) {
      const ativo = indice === atual;
      const feito =
        indice < atual || (modo === "local" && indice === 2 && modeloEscolhido);
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

    if (modo === "nuvem" && atual === 0) {
      corpo.appendChild(titulo(t("Conecte seu provedor (API Key)")));
      corpo.appendChild(
        texto(
          "O C2 fala o padrao OpenAI-compatible: OpenAI, Groq, DeepSeek, OpenRouter " +
            "ou um servidor local (LM Studio/Ollama/vLLM). A chave fica no cofre do " +
            "sistema, nunca no config. O conteudo dos arquivos sai da maquina.",
        ),
      );
      const acoesNuvem = document.createElement("div");
      acoesNuvem.className = styles.actions;
      acoesNuvem.appendChild(
        Button({
          label: t("Abrir Configuracoes"),
          iconName: "settings",
          size: "xs",
          variant: "primary",
          onClick: () => handlers.openSettings(),
        }),
      );
      acoesNuvem.appendChild(
        Button({
          label: t("Testar conexao"),
          size: "xs",
          variant: "secondary",
          onClick: async () => {
            const raiz = handlers.root();
            if (!raiz) {
              const aviso = texto("Abra a pasta do projeto primeiro (proximo passo).");
              aviso.style.color = "var(--warning-100)";
              corpo.appendChild(aviso);
              return;
            }
            const resultado = await api.providerTest(raiz);
            const feedback = texto(
              resultado.ok
                ? `provedor ok: ${resultado.provider} · ${resultado.model} · ${resultado.latency_ms} ms`
                : `falhou: ${resultado.error ?? "erro desconhecido"}`,
            );
            feedback.style.color = resultado.ok ? "var(--success)" : "var(--danger)";
            corpo.appendChild(feedback);
          },
        }),
      );
      corpo.appendChild(acoesNuvem);
      corpo.appendChild(
        texto(
          "Salve a chave em Config → Provedor de IA e clique em Testar conexao. " +
            "Depois siga para o projeto.",
        ),
      );
    }

    if (modo === "local" && atual === 0) {
      corpo.appendChild(titulo(t("Baixe um modelo local (GGUF)")));
      corpo.appendChild(
        texto(
          "O C2 roda o modelo na sua máquina — nada sai do seu computador. " +
            "Recomendamos o Gemma 4 E2B em Q4_K_M (~3,2 GB), que cabe numa GPU de 8 GB.",
        ),
      );
      const instrucoes = modelInstructions();
      if (instrucoes instanceof HTMLDetailsElement) instrucoes.open = true;
      corpo.appendChild(instrucoes);

      // Sem GPU (ou VRAM insuficiente): sugerir CPU ou provedor de nuvem.
      const avisoGpu = document.createElement("div");
      corpo.appendChild(avisoGpu);
      void api
        .gpuInfo()
        .then((gpu) => {
          const suficiente = gpu !== null && gpu.total_mb >= 4096;
          if (suficiente) {
            const nota = texto(
              t("GPU detectada") +
                ": " +
                gpu!.name +
                " (" +
                Math.round(gpu!.total_mb / 1024) +
                " GB).",
            );
            nota.style.opacity = "0.7";
            avisoGpu.appendChild(nota);
            return;
          }
          const box = document.createElement("div");
          box.className = styles.gpuAviso;
          const tituloBox = document.createElement("strong");
          tituloBox.textContent = gpu
            ? "VRAM insuficiente para o modelo recomendado"
            : "Nenhuma GPU detectada";
          const explica = document.createElement("p");
          explica.textContent =
            "Rodar na CPU está desativado nesta versão: o modelo inteiro iria para a " +
            "RAM (~4 GB no 4B Q4 em contexto 8k) e cada arquivo levaria minutos. " +
            "Use uma GPU ou um provedor OpenAI-compatible com a sua API key " +
            "(OpenAI, Groq, DeepSeek…). No modo nuvem o conteúdo dos arquivos sai da " +
            "máquina — o aviso aparece na tela Config.";
          const acoes = document.createElement("div");
          acoes.className = styles.actions;
          acoes.appendChild(
            Button({
              label: t("Configurar nuvem"),
              size: "xs",
              variant: "primary",
              onClick: () => handlers.openSettings(),
            }),
          );
          const notaCpu = document.createElement("p");
          notaCpu.style.margin = "0";
          notaCpu.style.fontSize = "var(--text-xs)";
          notaCpu.style.opacity = "0.8";
          notaCpu.textContent =
            "A opção de rodar na CPU está desativada nesta versão. " +
            "GPU AMD/Intel pode não ser detectada aqui — nesse caso, use o modo nuvem.";
          box.append(tituloBox, explica, acoes, notaCpu);
          avisoGpu.appendChild(box);
        })
        .catch(() => {
          /* sem deteccao: segue o fluxo normal */
        });
    }

    if (atual === 1) {
      corpo.appendChild(titulo(t("Escolha a pasta do projeto")));
      corpo.appendChild(
        texto(
          "O C2 grava os metadados ao lado de cada arquivo (<arquivo>.c2.json). " +
            "Escolha a pasta que você quer indexar.",
        ),
      );
      const raiz = handlers.root();
      if (raiz) {
        const campo = document.createElement("div");
        campo.className = styles.field;
        const rotulo = document.createElement("span");
        rotulo.className = styles.fieldLabel;
        rotulo.textContent = t("pasta selecionada");
        const valor = document.createElement("span");
        valor.className = styles.mono;
        valor.textContent = raiz;
        campo.append(rotulo, valor);
        corpo.appendChild(campo);
        corpo.appendChild(
          botaoConteudo(
            Button({
              label: t("Trocar pasta"),
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
              label: t("Abrir projeto…"),
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
      corpo.appendChild(titulo(t("Selecione o arquivo .gguf")));
      corpo.appendChild(
        texto(
          "Escolha o modelo que você baixou. O C2 lê apenas o cabeçalho para validar " +
            "(nome, quantização e se cabe na VRAM) e guarda só o caminho — o arquivo não é copiado.",
        ),
      );

      const raiz = handlers.root();
      if (!raiz) {
        corpo.appendChild(
          texto(t("Escolha a pasta do projeto antes de selecionar o modelo.")),
        );
      } else {
        if (modeloEscolhido) {
          const campo = document.createElement("div");
          campo.className = styles.field;
          const rotulo = document.createElement("span");
          rotulo.className = styles.fieldLabel;
          rotulo.textContent = t("modelo selecionado");
          const valor = document.createElement("span");
          valor.className = styles.mono;
          valor.textContent = modeloEscolhido;
          campo.append(rotulo, valor);
          corpo.appendChild(campo);
        }
        corpo.appendChild(
          botaoConteudo(
            Button({
              label: modeloEscolhido ? t("Trocar modelo…") : t("Selecionar .gguf…"),
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
      label: t("Voltar"),
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
      label: t("Pular"),
      variant: "ghost",
      onClick: () => {
        dialog.close();
        handlers.dismiss();
      },
    });

    const avancar = Button({
      label: atual === TOTAL - 1 ? t("Ir para a Engine") : t("Avançar"),
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
      modo === "nuvem"
        ? atual === 1 && !handlers.root()
        : (atual === 1 && !handlers.root()) || (atual === 2 && !modeloEscolhido);

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
  el.textContent = t(conteudo);
  el.style.margin = "0";
  el.style.fontSize = "var(--text-xs)";
  el.style.lineHeight = "1.5";
  el.style.color = "var(--text-muted)";
  return el;
}

