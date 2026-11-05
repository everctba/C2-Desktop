import { api } from "../api";
import type { McpStatus } from "../types";
import { Button } from "../ui/Button";
import { Card, cardBody } from "../ui/Card";
import { Pill } from "../ui/Pill";
import type { View } from "./view";
import styles from "./views.module.css";

export interface McpContext {
  root: string;
}

/**
 * Painel MCP: mostra o estado real do servidor fazendo o handshake
 * `initialize` + `tools/list` contra `c2 mcp` (c2-e16t5).
 */
export async function renderMcp(ctx: McpContext): Promise<View> {
  const page = document.createElement("div");
  page.className = styles.page;

  const header = document.createElement("div");
  header.className = styles.header;
  const title = document.createElement("h1");
  title.className = "pageTitle";
  title.textContent = "MCP";
  const hint = document.createElement("p");
  hint.className = "pageHint";
  hint.textContent =
    "Servidor MCP por STDIO para agentes de IA. Somente leitura: grep, read, related e map sobre o Core, com resposta limitada.";
  header.append(title, hint);
  page.appendChild(header);

  const estado = document.createElement("div");
  estado.className = styles.actions;
  const pilha = Pill("testando…", "neutral");
  estado.appendChild(pilha);
  const botao = Button({
    label: "Testar conexão",
    iconName: "refresh",
    size: "sm",
    variant: "secondary",
  });
  estado.appendChild(botao);
  page.appendChild(estado);

  const servidorCard = Card({ title: "Servidor", accent: 4 });
  const servidorCorpo = cardBody(servidorCard);
  page.appendChild(servidorCard);

  const ferramentasCard = Card({ title: "Ferramentas expostas", accent: 2 });
  const ferramentasCorpo = cardBody(ferramentasCard);
  page.appendChild(ferramentasCard);

  const conectarCard = Card({ title: "Conectar um agente", accent: 6 });
  const conectarCorpo = cardBody(conectarCard);
  page.appendChild(conectarCard);

  const campo = (rotulo: string, valor: string, mono = false): HTMLElement => {
    const linha = document.createElement("div");
    linha.className = styles.field;
    const label = document.createElement("span");
    label.className = styles.fieldLabel;
    label.textContent = rotulo;
    const texto = document.createElement("span");
    texto.className = mono ? styles.mono : styles.fieldValue;
    texto.textContent = valor;
    linha.append(label, texto);
    return linha;
  };

  const desenhar = (status: McpStatus) => {
    const disponivel = status.available;
    const atual = Pill(
      disponivel ? "servidor pronto" : "indisponível",
      disponivel ? "success" : "danger",
    );
    estado.replaceChild(atual, estado.firstElementChild ?? atual);
    servidorCorpo.replaceChildren();
    servidorCorpo.appendChild(
      campo("protocolo", status.protocol_version ?? "—", true),
    );
    servidorCorpo.appendChild(campo("servidor", status.server_name ?? "—"));
    servidorCorpo.appendChild(campo("binário", status.binary ?? "não encontrado", true));
    if (status.error) {
      servidorCorpo.appendChild(campo("erro", status.error));
    }

    ferramentasCorpo.replaceChildren();
    if (status.tools.length === 0) {
      const vazio = document.createElement("div");
      vazio.className = "empty";
      vazio.textContent = "nenhuma ferramenta retornada pelo servidor";
      ferramentasCorpo.appendChild(vazio);
    }
    for (const ferramenta of status.tools) {
      const linha = document.createElement("div");
      linha.className = styles.field;
      const nome = document.createElement("span");
      nome.className = styles.mono;
      nome.textContent = ferramenta.name;
      const descricao = document.createElement("span");
      descricao.className = styles.fieldValue;
      descricao.textContent = ferramenta.description;
      linha.append(nome, descricao);
      ferramentasCorpo.appendChild(linha);
    }

    conectarCorpo.replaceChildren();
    const binario = status.binary ?? "<caminho-do-c2>/cli/c2.exe";
    const config = {
      mcpServers: {
        c2: {
          command: binario,
          args: ["--root", ctx.root, "mcp"],
        },
      },
    };
    const texto = JSON.stringify(config, null, 2);
    const bloco = document.createElement("div");
    bloco.className = styles.configCard;

    const topo = document.createElement("div");
    topo.className = styles.configTopo;
    const rotulo = document.createElement("span");
    rotulo.className = styles.configLabel;
    rotulo.textContent = "configuração do cliente MCP";
    const copiar = Button({
      label: "Copiar MCP",
      iconName: "copy",
      size: "sm",
      variant: "secondary",
      onClick: async () => {
        try {
          // plugin nativo do Tauri: copia sem o prompt de permissao do WebView2
          const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
          await writeText(texto);
        } catch {
          try {
            await navigator.clipboard.writeText(texto);
          } catch {
            copiar.title = "Não foi possível copiar";
            return;
          }
        }
        const rotuloBotao = copiar.lastElementChild as HTMLElement | null;
        copiar.title = "Copiado!";
        if (rotuloBotao) rotuloBotao.textContent = "Copiado";
        setTimeout(() => {
          copiar.title = "Copiar configuração MCP";
          if (rotuloBotao) rotuloBotao.textContent = "Copiar MCP";
        }, 1600);
      },
    });
    copiar.title = "Copiar configuração MCP";
    copiar.setAttribute("aria-label", "Copiar configuração MCP");
    // tamanho fixo: o botao nao muda de largura quando o texto troca
    copiar.style.width = "118px";
    copiar.style.flex = "0 0 auto";
    copiar.style.justifyContent = "center";
    topo.append(rotulo, copiar);

    const pre = document.createElement("pre");
    pre.className = styles.pre;
    pre.textContent = texto;

    bloco.append(topo, pre);
    conectarCorpo.appendChild(bloco);
    const nota = document.createElement("p");
    nota.className = "pageHint";
    nota.textContent =
      "Cole no seu cliente MCP (Claude Desktop, Cursor, etc.). O servidor é somente leitura, exceto `c2_annotate` (anotações no .c2.json do arquivo). Nada é enviado para fora da máquina.";
    conectarCorpo.appendChild(nota);
  };

  const testar = async () => {
    pilha.replaceWith(Pill("testando…", "neutral"));
    try {
      const status = await api.mcpSmoke(ctx.root);
      desenhar(status);
    } catch (erro) {
      desenhar({
        available: false,
        binary: null,
        protocol_version: null,
        server_name: null,
        tools: [],
        error: String(erro),
      });
    }
  };

  botao.addEventListener("click", () => void testar());
  // Estado real na entrada da tela.
  void testar();

  return { element: page };
}
