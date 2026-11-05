//! Aplica o icone "C2" e metadados de versao no executavel do CLI (Windows).
//!
//! Usa `rc.exe` do Windows SDK. Se o compilador de recursos nao estiver
//! disponivel, o build continua (apenas sem icone), para nao quebrar maquinas
//! sem MSVC/SDK.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/c2.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // O `winresource` procura `rc.exe` no PATH; se nao achar, tenta os SDKs
    // instalados (registro + caminhos comuns) e adiciona a pasta ao PATH.
    if !tem_rc_no_path() {
        if let Some(pasta) = procurar_rc() {
            let atual = std::env::var_os("PATH").unwrap_or_default();
            let novo =
                std::env::join_paths(std::iter::once(pasta).chain(std::env::split_paths(&atual)))
                    .unwrap_or(atual);
            std::env::set_var("PATH", novo);
        }
    }

    let mut recurso = winresource::WindowsResource::new();
    recurso.set_icon("assets/c2.ico");
    recurso.set(
        "FileDescription",
        "C2 - contexto semantico de projeto para agentes de IA",
    );
    recurso.set("ProductName", "C2");
    recurso.set("LegalCopyright", "MIT OR Apache-2.0");
    if let Err(erro) = recurso.compile() {
        println!("cargo:warning=icone do CLI nao aplicado ({erro}); build segue sem recurso");
    }
}

fn tem_rc_no_path() -> bool {
    Command::new("rc.exe")
        .arg("/?")
        .output()
        .map(|saida| saida.status.code().is_some())
        .unwrap_or(false)
}

fn procurar_rc() -> Option<PathBuf> {
    let mut raizes: Vec<PathBuf> = Vec::new();
    for variavel in ["KitsRoot10", "WindowsSdkDir"] {
        if let Some(valor) = std::env::var_os(variavel) {
            raizes.push(PathBuf::from(valor));
        }
    }
    // registro: HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots
    if let Ok(saida) = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots",
            "/v",
            "KitsRoot10",
        ])
        .output()
    {
        if saida.status.success() {
            let texto = String::from_utf8_lossy(&saida.stdout);
            if let Some(linha) = texto.lines().find(|l| l.contains("KitsRoot10")) {
                if let Some(valor) = linha.split("REG_SZ").nth(1) {
                    raizes.push(PathBuf::from(valor.trim()));
                }
            }
        }
    }
    for padrao in [
        r"C:\Program Files (x86)\Windows Kits\10",
        r"C:\Program Files\Windows Kits\10",
        r"E:\Windows Kits\10",
        r"D:\Windows Kits\10",
    ] {
        raizes.push(PathBuf::from(padrao));
    }

    let mut encontrados: Vec<PathBuf> = Vec::new();
    for raiz in raizes {
        let bin = raiz.join("bin");
        let Ok(versoes) = std::fs::read_dir(&bin) else {
            continue;
        };
        for versao in versoes.flatten() {
            for arco in ["x64", "x86"] {
                let candidato = versao.path().join(arco).join("rc.exe");
                if candidato.is_file() {
                    encontrados.push(candidato);
                }
            }
        }
    }
    // prefere a versao mais recente do SDK
    encontrados.sort();
    encontrados
        .pop()
        .and_then(|caminho| caminho.parent().map(Path::to_path_buf))
}
