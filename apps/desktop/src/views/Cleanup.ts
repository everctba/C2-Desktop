import { api } from "../api";
import { t } from "../i18n";
import { Button } from "../ui/Button";
import { createDialog } from "../ui/Dialog";
import { Input } from "../ui/Input";

export interface CleanupHandlers {
  /** Mostra mensagens curtas de status (ex.: "132 metadados serão removidos"). */
  setStatus: (texto: string) => void;
  /** Chamado depois que a remoção termina (para recarregar o status). */
  onDone: (removidos: number) => void;
}

/**
 * Fluxo unico de limpeza de metadados: dry-run, aviso destrutivo e
 * confirmacao digitando DELETE em maiusculo.
 */
export async function limparMetadados(root: string, handlers: CleanupHandlers): Promise<void> {
  if (!root) return;

  let plano: { found: number };
  try {
    plano = await api.cleanerPlan(root);
  } catch (erro) {
    handlers.setStatus(`${t("falha ao limpar")}: ${String(erro)}`);
    return;
  }

  if (plano.found === 0) {
    handlers.setStatus(t("nenhum metadado para limpar"));
    return;
  }

  const handle = createDialog({ title: t("Limpar metadados?") });
  const corpo = document.createElement("div");
  corpo.style.display = "grid";
  corpo.style.gap = "var(--space-s)";

  const contagem = document.createElement("p");
  contagem.style.margin = "0";
  contagem.style.color = "var(--warning)";
  contagem.textContent = `${plano.found} ${t("metadados serão removidos")}`;
  corpo.appendChild(contagem);

  const aviso = document.createElement("p");
  aviso.className = "pageHint";
  aviso.style.margin = "0";
  aviso.style.maxWidth = "none";
  aviso.textContent = t(
    "O código e o .c2/config.json não são alterados. Esta ação não pode ser desfeita; reindexe depois.",
  );
  corpo.appendChild(aviso);

  const confirmacao = Input({
    label: t("Digite DELETE (maiúsculo) para confirmar"),
    placeholder: "DELETE",
    mono: true,
  });
  corpo.appendChild(confirmacao.root);

  const remover = Button({
    label: t("Remover metadados"),
    variant: "destructive",
    disabled: true,
    onClick: async () => {
      if (confirmacao.input.value !== "DELETE") return;
      handle.close();
      handlers.setStatus(t("removendo metadados…"));
      try {
        const resultado = await api.cleanerRun(root);
        let texto = `${resultado.removed} ${t("metadados removidos")}`;
        if (resultado.failed.length > 0) {
          texto += ` · ${resultado.failed.length} ${t("falhas")}`;
        }
        handlers.setStatus(texto);
        handlers.onDone(resultado.removed);
      } catch (erro) {
        handlers.setStatus(`${t("falha ao limpar")}: ${String(erro)}`);
      }
    },
  });

  confirmacao.input.addEventListener("input", () => {
    remover.disabled = confirmacao.input.value !== "DELETE";
  });
  confirmacao.input.addEventListener("keydown", (evento) => {
    if (evento.key === "Enter" && confirmacao.input.value === "DELETE") {
      remover.click();
    }
  });

  handle.body.appendChild(corpo);
  handle.footer.append(
    Button({
      label: t("Cancelar"),
      variant: "ghost",
      onClick: () => handle.close(),
    }),
    remover,
  );
}
