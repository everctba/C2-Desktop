//! Configuracao do workspace (`.c2/config.json`).

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{C2Error, IoContext, Result};
use crate::security::SecretPolicy;

/// Versao do formato de configuracao.
pub const CONFIG_VERSION: u32 = 1;

/// Parameteros do runtime de inferencia (`llama-server`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    /// Nome enviado no campo `model` da API OpenAI-compat.
    pub name: String,
    /// Caminho do GGUF; `None` usa o que o servidor ja estiver servindo.
    pub path: Option<String>,
    /// Caminho do binario do llama-server; `None` busca automaticamente.
    pub binary: Option<String>,
    /// Diretorios extras com DLLs do runtime (ex.: vendor CUDA).
    pub dll_dirs: Vec<String>,
    /// Permite usar os backends do LM Studio como fallback de desenvolvimento.
    /// Desligado por padrao: o pacote embarca o runtime proprio (c2-e12.1).
    pub allow_lmstudio: bool,
    /// Host do llama-server.
    pub host: String,
    /// Porta HTTP local do llama-server.
    pub port: u16,
    pub ctx_size: u32,
    pub threads: u32,
    pub gpu_layers: u32,
    pub temperature: f64,
    pub timeout_secs: u64,
    pub max_retries: u32,
    /// Desliga o raciocinio do modelo (`enable_thinking: false`).
    /// Medido com Gemma 4 E2B: 1,4 s por arquivo contra 9,2 s pensando.
    pub disable_thinking: bool,
    /// Teto de tokens da resposta de metadado.
    pub max_tokens: u32,
    /// Teto maior usado quando a resposta anterior foi cortada por limite.
    pub retry_max_tokens: u32,
    /// Teto de caracteres do arquivo enviados no prompt. O cliente ainda
    /// reduz isso se o servidor reclamar de contexto.
    pub max_input_chars: usize,
}

impl ModelConfig {
    /// URL do endpoint OpenAI-compat de chat.
    pub fn chat_url(&self) -> String {
        format!("http://{}:{}/v1/chat/completions", self.host, self.port)
    }

    /// URL do health check do llama-server.
    pub fn health_url(&self) -> String {
        format!("http://{}:{}/health", self.host, self.port)
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            name: "local-model".to_string(),
            path: None,
            binary: None,
            dll_dirs: Vec::new(),
            allow_lmstudio: false,
            host: "127.0.0.1".to_string(),
            port: 8080,
            ctx_size: 8192,
            threads: 0,
            gpu_layers: 99,
            temperature: 0.3,
            timeout_secs: 120,
            max_retries: 2,
            disable_thinking: true,
            max_tokens: 900,
            retry_max_tokens: 1600,
            max_input_chars: 16000,
        }
    }
}

/// Regras de descoberta e indexacao.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexConfig {
    /// Arquivos maiores que isto sao pulados (nunca vao ao modelo).
    pub max_file_bytes: u64,
    /// Extensoes indexaveis; vazio = qualquer arquivo de texto.
    pub extensions: Vec<String>,
    /// Diretorios ignorados, alem do que o `.gitignore`/`.c2ignore` diz.
    pub excluded_dirs: Vec<String>,
    /// Respeitar `.gitignore` e `.git/info/exclude`.
    pub respect_gitignore: bool,
    /// Teto de arestas por arquivo, para nao inflar o metadado.
    pub max_links_per_file: usize,
    /// O que fazer com arquivo que contem segredo.
    pub secret_policy: SecretPolicy,
    /// Instrucoes base enviadas ao modelo (o conteudo do arquivo e anexado
    /// depois, sempre delimitado e marcado como dado). Vazio = invalido.
    #[serde(default = "default_prompt")]
    pub prompt: String,
    /// Caminhos (prefixo relativo) liberados da checagem de segredo, para
    /// arquivos com fixtures de teste. Use com cuidado.
    pub secret_allow: Vec<String>,
    /// Teto de bytes de conteudo enviados ao modelo por ciclo (0 = ilimitado).
    pub max_total_bytes_per_run: u64,
    /// Regras de import por regex para linguagens sem extrator nativo.
    pub link_fallback: Vec<FallbackRule>,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: 256 * 1024,
            extensions: default_extensions(),
            excluded_dirs: default_excluded_dirs(),
            respect_gitignore: true,
            max_links_per_file: 64,
            secret_policy: SecretPolicy::Skip,
            prompt: DEFAULT_PROMPT.to_string(),
            secret_allow: Vec::new(),
            max_total_bytes_per_run: 64 * 1024 * 1024,
            link_fallback: Vec::new(),
        }
    }
}

/// Instrucoes padrao enviadas ao modelo para gerar o metadado semantico.
/// Editavel via `c2 config set index.prompt "<texto>"` ou pela tela Config.
pub const DEFAULT_PROMPT: &str = "\
Voce e um arquiteto de software. Analise o arquivo abaixo e descreva a responsabilidade dele no projeto.
Responda APENAS com um objeto JSON valido, sem markdown, sem cercas de codigo e sem texto fora do JSON.
Formato exato: {\"what\":\"...\",\"why\":\"...\",\"goal\":\"...\",\"summary\":\"...\",\"responsibility\":\"...\"}
Regras: frases curtas; what = do que trata; why = por que existe; goal = o que garante; summary = responsabilidade em uma frase; responsibility = o que e de sua propriedade.";

fn default_prompt() -> String {
    DEFAULT_PROMPT.to_string()
}

/// Extensoes de texto indexaveis por padrao.
pub fn default_extensions() -> Vec<String> {
    [
        "rs", "toml", "lock", "json", "jsonl", "md", "txt", "sh", "bash", "ps1", "bat", "cmd",
        "py", "js", "mjs", "cjs", "ts", "tsx", "jsx", "vue", "svelte", "html", "htm", "css",
        "scss", "less", "sql", "yml", "yaml", "xml", "ini", "cfg", "conf", "env", "c", "h", "cpp",
        "hpp", "cc", "go", "java", "kt", "kts", "cs", "rb", "php", "lua", "swift", "ex", "exs",
        "erl", "hs", "ml", "scala", "dart", "zig", "nim", "r", "jl", "pl", "tf", "hcl", "proto",
        "graphql",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Diretorios ignorados por padrao.
pub fn default_excluded_dirs() -> Vec<String> {
    [
        ".git",
        ".beads",
        ".c2",
        "node_modules",
        "target",
        "dist",
        "build",
        "out",
        "__pycache__",
        ".venv",
        "venv",
        "vendor",
        ".idea",
        ".vscode",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Parametros do laco do indexador.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    /// Intervalo do polling barato (mtime + tamanho).
    pub poll_interval_secs: u64,
    /// Quantos arquivos processar por ciclo (0 = todos).
    pub batch_size: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 5,
            batch_size: 0,
        }
    }
}

/// Parametros da busca semantica.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    /// Sinonimos no formato `termo=sin1,sin2`, aplicados na consulta e nos
    /// metadados (ex.: `auth=login,sessao,token`).
    pub synonyms: Vec<String>,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            synonyms: vec![
                "auth=login,sessao,session,token,autenticacao".to_string(),
                "db=banco,database,storage,persistencia".to_string(),
                "ui=frontend,interface,view,tela".to_string(),
                "erro=falha,error,exception".to_string(),
                "config=configuracao,settings,ajuste".to_string(),
                "busca=search,grep,pesquisa".to_string(),
                "indice=index,indexacao,indexer".to_string(),
            ],
        }
    }
}

/// Parametros de log em arquivo (`c2-e10t1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// Nivel minimo: error, warn, info ou debug.
    pub level: String,
    /// Tamanho maximo antes de rotacionar.
    pub max_file_bytes: u64,
    /// Quantos arquivos rotacionados manter.
    pub keep: usize,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            max_file_bytes: 1024 * 1024,
            keep: 5,
        }
    }
}

/// Regra de fallback para import de linguagem nao suportada (c2-e6t5).
///
/// `regex` precisa ter um grupo de captura 1 com o alvo (arquivo/modulo).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallbackRule {
    /// Extensao do arquivo de origem, sem ponto (ex.: "lua").
    pub extension: String,
    /// Regex com grupo 1 = caminho do alvo.
    pub regex: String,
}

/// Configuracao completa do workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema_version: u32,
    pub model: ModelConfig,
    pub index: IndexConfig,
    pub engine: EngineConfig,
    pub search: SearchConfig,
    pub log: LogConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_VERSION,
            model: ModelConfig::default(),
            index: IndexConfig::default(),
            engine: EngineConfig::default(),
            search: SearchConfig::default(),
            log: LogConfig::default(),
        }
    }
}

impl Config {
    /// Valida limites e coerencia dos valores.
    pub fn validate(&self) -> Result<()> {
        let invalid = |msg: String| Err(C2Error::InvalidConfig(msg));

        if self.model.port == 0 {
            return invalid("model.port nao pode ser 0".to_string());
        }
        if self.model.ctx_size < 512 {
            return invalid("model.ctx_size deve ser >= 512".to_string());
        }
        if !(0.0..=2.0).contains(&self.model.temperature) {
            return invalid("model.temperature deve estar entre 0 e 2".to_string());
        }
        if self.model.timeout_secs == 0 {
            return invalid("model.timeout_secs deve ser > 0".to_string());
        }
        if self.model.host.trim().is_empty() {
            return invalid("model.host nao pode ser vazio".to_string());
        }
        if self.model.max_tokens < 64 {
            return invalid("model.max_tokens deve ser >= 64".to_string());
        }
        if self.index.prompt.trim().is_empty() {
            return invalid("index.prompt nao pode ser vazio".to_string());
        }
        if self.model.max_input_chars < 1000 {
            return invalid("model.max_input_chars deve ser >= 1000".to_string());
        }
        if self.model.retry_max_tokens < self.model.max_tokens {
            return invalid("model.retry_max_tokens deve ser >= model.max_tokens".to_string());
        }
        if self.index.max_file_bytes == 0 {
            return invalid("index.max_file_bytes deve ser > 0".to_string());
        }
        if self.engine.poll_interval_secs == 0 {
            return invalid("engine.poll_interval_secs deve ser > 0".to_string());
        }
        if self.log.keep == 0 {
            return invalid("log.keep deve ser >= 1".to_string());
        }
        if self.log.max_file_bytes < 1024 {
            return invalid("log.max_file_bytes deve ser >= 1024".to_string());
        }
        if !matches!(
            self.log.level.to_ascii_lowercase().as_str(),
            "error" | "warn" | "warning" | "info" | "debug"
        ) {
            return invalid("log.level deve ser error, warn, info ou debug".to_string());
        }
        Ok(())
    }

    /// Le e valida a configuracao do disco.
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).at(path)?;
        let config: Config = serde_json::from_slice(&bytes)?;
        config.validate()?;
        Ok(config)
    }

    /// Serializa com indentacao.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(C2Error::from)
    }

    /// Escrita atomica, igual ao metadado.
    pub fn write_atomic(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .ok_or_else(|| C2Error::InvalidConfig(format!("caminho invalido: {path:?}")))?;
        fs::create_dir_all(parent).at(parent)?;

        let json = self.to_json()?;
        let temp = parent.join(format!(".c2-config-tmp-{}", std::process::id()));
        {
            let mut file = fs::File::create(&temp).at(&temp)?;
            file.write_all(json.as_bytes()).at(&temp)?;
            file.write_all(b"\n").at(&temp)?;
            file.sync_all().at(&temp)?;
        }
        if let Err(err) = fs::rename(&temp, path) {
            let _ = fs::remove_file(&temp);
            return Err(C2Error::io(path, err));
        }
        Ok(())
    }

    /// Le um valor por chave pontuada (`model.port`, `index.max_file_bytes`).
    pub fn get(&self, key: &str) -> Option<String> {
        let value = serde_json::to_value(self).ok()?;
        let mut current = &value;
        for part in key.split('.') {
            current = current.get(part)?;
        }
        Some(match current {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    }

    /// Aplica um valor textual na chave pontuada, respeitando o tipo atual.
    pub fn set(&mut self, key: &str, raw: &str) -> Result<()> {
        let mut value = serde_json::to_value(&*self)?;
        set_in_value(&mut value, key, raw)?;
        *self = serde_json::from_value(value)?;
        self.validate()
    }
}

fn set_in_value(root: &mut serde_json::Value, key: &str, raw: &str) -> Result<()> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return Err(C2Error::InvalidConfig("chave vazia".to_string()));
    }

    let mut current = root;
    for part in &parts[..parts.len() - 1] {
        current = current
            .get_mut(*part)
            .ok_or_else(|| C2Error::InvalidConfig(format!("chave desconhecida: {key}")))?;
        if !current.is_object() {
            return Err(C2Error::InvalidConfig(format!("chave desconhecida: {key}")));
        }
    }

    let last = parts[parts.len() - 1];
    let slot = current
        .get_mut(last)
        .ok_or_else(|| C2Error::InvalidConfig(format!("chave desconhecida: {key}")))?;

    let new_value = match slot {
        serde_json::Value::Bool(_) => serde_json::Value::Bool(parse_bool(raw)?),
        serde_json::Value::Number(_) => parse_number(raw)?,
        serde_json::Value::String(_) => serde_json::Value::String(raw.to_string()),
        serde_json::Value::Array(_) => parse_array(raw),
        serde_json::Value::Null => serde_json::Value::String(raw.to_string()),
        _ => {
            return Err(C2Error::InvalidConfig(format!(
                "nao e possivel definir a chave {key}"
            )))
        }
    };
    *slot = new_value;
    Ok(())
}

fn parse_bool(raw: &str) -> Result<bool> {
    match raw.to_ascii_lowercase().as_str() {
        "true" | "1" | "sim" | "yes" | "on" => Ok(true),
        "false" | "0" | "nao" | "no" | "off" => Ok(false),
        _ => Err(C2Error::InvalidConfig(format!(
            "valor booleano invalido: {raw}"
        ))),
    }
}

fn parse_number(raw: &str) -> Result<serde_json::Value> {
    if let Ok(int) = raw.parse::<i64>() {
        return Ok(serde_json::Value::from(int));
    }
    if let Ok(float) = raw.parse::<f64>() {
        if let Some(number) = serde_json::Number::from_f64(float) {
            return Ok(serde_json::Value::Number(number));
        }
    }
    Err(C2Error::InvalidConfig(format!(
        "valor numerico invalido: {raw}"
    )))
}

fn parse_array(raw: &str) -> serde_json::Value {
    let items: Vec<serde_json::Value> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::Value::String(s.to_string()))
        .collect();
    serde_json::Value::Array(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p =
            std::env::temp_dir().join(format!("c2-config-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn defaults_sao_validos_e_serializaveis() {
        let config = Config::default();
        config.validate().unwrap();
        let json = config.to_json().unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn roundtrip_em_disco() {
        let base = dir("disk");
        let path = base.join("config.json");
        let mut config = Config::default();
        config.model.port = 9000;
        config.write_atomic(&path).unwrap();
        assert_eq!(Config::read(&path).unwrap().model.port, 9000);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn get_e_set_por_chave() {
        let mut config = Config::default();
        assert_eq!(config.get("model.port").as_deref(), Some("8080"));
        config.set("model.port", "9090").unwrap();
        assert_eq!(config.model.port, 9090);
        config.set("index.respect_gitignore", "false").unwrap();
        assert!(!config.index.respect_gitignore);
        config.set("index.max_file_bytes", "1024").unwrap();
        assert_eq!(config.index.max_file_bytes, 1024);
        config.set("index.extensions", "rs,md").unwrap();
        assert_eq!(config.index.extensions, vec!["rs", "md"]);
        assert!(config.get("nao.existe").is_none());
        assert!(config.set("nao.existe", "1").is_err());
    }

    #[test]
    fn valores_invalidos_sao_recusados() {
        let mut config = Config::default();
        assert!(config.set("model.temperature", "9.0").is_err());
        assert!(config.set("model.port", "abc").is_err());
        config.model.port = 0;
        assert!(config.validate().is_err());
    }
}
