//! Helpers para executar processos de console a partir do app gráfico sem
//! abrir janela (bead de UX: "terminal piscando ao abrir o projeto").
//!
//! No Windows, um app de GUI que executa um programa de console ganha uma
//! janela nova. `CREATE_NO_WINDOW` cria o processo sem console visível.

use std::process::{Child, Command, ExitStatus, Output};

/// Configura `Command` para não abrir janela de console.
#[cfg(windows)]
pub fn sem_janela(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

/// No-op fora do Windows (o conceito de console separado não existe).
#[cfg(not(windows))]
pub fn sem_janela(_command: &mut Command) {}

/// `Command::output` sem janela.
pub fn output(mut command: Command) -> std::io::Result<Output> {
    sem_janela(&mut command);
    command.output()
}

/// `Command::status` sem janela.
pub fn status(mut command: Command) -> std::io::Result<ExitStatus> {
    sem_janela(&mut command);
    command.status()
}

/// `Command::spawn` sem janela (o filho continua rodando em background).
pub fn spawn(mut command: Command) -> std::io::Result<Child> {
    sem_janela(&mut command);
    command.spawn()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn executa_sem_abrir_janela() {
        let mut comando = Command::new("cmd");
        comando.args(["/c", "echo c2-sem-janela"]);
        let saida = output(comando).expect("rodar cmd");
        assert!(saida.status.success());
        assert!(String::from_utf8_lossy(&saida.stdout).contains("c2-sem-janela"));
    }
}
