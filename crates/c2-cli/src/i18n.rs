//! Idioma do CLI (`c2 lang`, `--lang` e `language.json`).
//!
//! O instalador grava o idioma escolhido no mesmo `language.json` do desktop;
//! o CLI usa esse arquivo como default. `--lang` vale para a execução atual e
//! `c2 lang <en|pt-BR>` grava a preferência.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    PtBr,
}

static LANG: AtomicU8 = AtomicU8::new(2);

pub fn set(lang: Lang) {
    LANG.store(
        match lang {
            Lang::En => 0,
            Lang::PtBr => 1,
        },
        Ordering::SeqCst,
    );
}

pub fn atual() -> Option<Lang> {
    match LANG.load(Ordering::SeqCst) {
        0 => Some(Lang::En),
        1 => Some(Lang::PtBr),
        _ => None,
    }
}

pub fn en() -> bool {
    matches!(atual(), Some(Lang::En))
}

/// Texto curto no idioma atual.
pub fn t(en: &'static str, pt: &'static str) -> &'static str {
    if matches!(atual(), Some(Lang::En)) {
        en
    } else {
        pt
    }
}

pub fn parse(valor: &str) -> Option<Lang> {
    match valor {
        "en" | "en-US" | "english" => Some(Lang::En),
        "pt" | "pt-BR" | "pt-br" | "brazilianportuguese" => Some(Lang::PtBr),
        _ => None,
    }
}

pub fn nome(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "en",
        Lang::PtBr => "pt-BR",
    }
}

fn app_dir() -> Option<PathBuf> {
    if let Some(base) = std::env::var_os("APPDATA") {
        return Some(PathBuf::from(base).join("dev.c2.desktop"));
    }
    if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(base).join("dev.c2.desktop"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join("dev.c2.desktop"))
}

pub fn caminho() -> Option<PathBuf> {
    app_dir().map(|dir| dir.join("language.json"))
}

pub fn ler() -> Option<Lang> {
    let texto = std::fs::read_to_string(caminho()?).ok()?;
    let json: serde_json::Value = serde_json::from_str(&texto).ok()?;
    json.get("lang")
        .and_then(|valor| valor.as_str())
        .and_then(parse)
}

pub fn salvar(lang: Lang) -> Result<(), String> {
    let dir = app_dir().ok_or_else(|| "diretorio de configuracao indisponivel".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|erro| erro.to_string())?;
    let json = serde_json::json!({ "lang": nome(lang) });
    std::fs::write(
        dir.join("language.json"),
        serde_json::to_string_pretty(&json).map_err(|erro| erro.to_string())?,
    )
    .map_err(|erro| erro.to_string())
}

/// Resolve o idioma: `--lang` > language.json > português (compatibilidade).
pub fn iniciar(flag: Option<&str>) {
    if let Some(lang) = flag.and_then(parse) {
        set(lang);
    } else if let Some(lang) = ler() {
        set(lang);
    } else {
        set(Lang::PtBr);
    }
}
