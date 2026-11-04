//! Seguranca e privacidade (bead `c2-e9`).
//!
//! Regras:
//! - nada do workspace sai da maquina, e nada sensivel chega ao modelo;
//! - arquivos com nome sensivel (`.env`, chaves privadas, credenciais) sao
//!   pulados antes de qualquer leitura de conteudo;
//! - conteudo com segredo detectado e pulado ou mascarado, conforme
//!   `index.secret_policy` (`skip` por padrao, `mask` para mascarar);
//! - conteudo de arquivo e sempre DADO, nunca instrucao (ver `llm::build_prompt`).

use serde::{Deserialize, Serialize};

/// O que fazer com arquivo que contem segredo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SecretPolicy {
    /// Nao indexa o arquivo (padrao).
    #[default]
    Skip,
    /// Indexa, mas mascara o valor no conteudo enviado ao modelo.
    Mask,
}

/// Tipo de segredo detectado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    PrivateKey,
    AwsKey,
    ProviderToken,
    Jwt,
    PasswordLiteral,
    ConnectionString,
}

impl SecretKind {
    /// Rotulo usado na saida e no mascaramento.
    pub fn label(self) -> &'static str {
        match self {
            SecretKind::PrivateKey => "private-key",
            SecretKind::AwsKey => "aws-key",
            SecretKind::ProviderToken => "provider-token",
            SecretKind::Jwt => "jwt",
            SecretKind::PasswordLiteral => "password",
            SecretKind::ConnectionString => "connection-string",
        }
    }
}

/// Verdadeiro quando o nome do arquivo indica segredo/credencial.
pub fn is_sensitive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();

    const EXACT: &[&str] = &[
        ".env",
        ".netrc",
        ".npmrc",
        ".pypirc",
        ".git-credentials",
        "id_rsa",
        "id_dsa",
        "id_ecdsa",
        "id_ed25519",
        "credentials",
        "secrets",
        "keystore",
    ];
    if EXACT.iter().any(|candidate| lower == *candidate) {
        return true;
    }

    if lower.starts_with(".env.")
        || lower.starts_with("secrets.")
        || lower.starts_with("credentials.")
    {
        return true;
    }

    const SUFFIXES: &[&str] = &[
        ".pem",
        ".key",
        ".pfx",
        ".p12",
        ".keystore",
        ".jks",
        ".kdbx",
        ".asc",
        ".ppk",
        ".crt",
        ".der",
    ];
    SUFFIXES.iter().any(|suffix| lower.ends_with(suffix))
}

/// Detecta um segredo no conteudo, linha a linha.
pub fn detect(content: &str) -> Option<SecretKind> {
    for (index, line) in content.lines().enumerate() {
        if index > 2000 {
            break;
        }
        if let Some(kind) = detect_line(line) {
            return Some(kind);
        }
    }
    None
}

fn detect_line(line: &str) -> Option<SecretKind> {
    // Excecao explicita para fixtures de teste: `// c2:allow-secret`.
    if line.contains("c2:allow-secret") {
        return None;
    }
    let trimmed = line.trim();

    for marker in [
        "BEGIN RSA PRIVATE KEY",
        "BEGIN OPENSSH PRIVATE KEY",
        "BEGIN EC PRIVATE KEY",
        "BEGIN PRIVATE KEY",
        "BEGIN PGP PRIVATE KEY",
    ] {
        if trimmed.contains(marker) {
            return Some(SecretKind::PrivateKey);
        }
    }

    if has_token(trimmed, "AKIA", 16) {
        return Some(SecretKind::AwsKey);
    }
    if has_token(trimmed, "ghp_", 20)
        || has_token(trimmed, "github_pat_", 20)
        || has_token(trimmed, "xoxb-", 10)
        || has_token(trimmed, "sk-", 20)
    {
        return Some(SecretKind::ProviderToken);
    }
    if trimmed.contains("eyJ") && trimmed.matches('.').count() >= 2 {
        return Some(SecretKind::Jwt);
    }

    let lower = trimmed.to_ascii_lowercase();
    if [
        "postgres://",
        "postgresql://",
        "mysql://",
        "mongodb://",
        "mongodb+srv://",
        "redis://",
    ]
    .iter()
    .any(|scheme| lower.contains(scheme))
    {
        let after_scheme = lower
            .split("://")
            .nth(1)
            .unwrap_or_default()
            .split('@')
            .next()
            .unwrap_or_default();
        if after_scheme.contains(':') {
            return Some(SecretKind::ConnectionString);
        }
    }

    if password_assignment(trimmed) {
        return Some(SecretKind::PasswordLiteral);
    }

    None
}

/// `AKIA`/`ghp_` seguido de N caracteres de token.
fn has_token(line: &str, prefix: &str, min_rest: usize) -> bool {
    let Some(index) = line.find(prefix) else {
        return false;
    };
    let rest: String = line[index + prefix.len()..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    rest.len() >= min_rest
}

/// Palavras que, quando usadas como chave, indicam valor sensivel.
fn is_secret_key(word: &str) -> bool {
    matches!(
        word,
        "password"
            | "passwd"
            | "senha"
            | "secret"
            | "token"
            | "api_key"
            | "apikey"
            | "access_key"
            | "private_key"
            | "client_secret"
            | "auth_token"
            | "credential"
            | "credentials"
    )
}

/// `chave = "valor"` ou `chave: "valor"`, exigindo valor com cara de segredo.
fn password_assignment(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0usize;

    while index < chars.len() {
        if !chars[index].is_alphabetic() {
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_') {
            index += 1;
        }
        let word: String = chars[start..index]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
        if !is_secret_key(&word) {
            continue;
        }

        let rest: String = chars[index..].iter().collect();
        let value = rest
            .split_once('=')
            .map(|(_, value)| value)
            .or_else(|| rest.split_once(':').map(|(_, value)| value));
        let Some(value) = value else { continue };

        if let Some(literal) = quoted_value(value) {
            if looks_like_secret(&literal) {
                return true;
            }
        }
    }

    false
}

/// Valor entre aspas logo apos o separador.
fn quoted_value(rest: &str) -> Option<String> {
    let rest = rest.trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let body = &rest[quote.len_utf8()..];
    let end = body.find(quote)?;
    Some(body[..end].to_string())
}

/// Um literal so e tratado como segredo se tiver entropia suficiente: nao
/// marcamos `"private-key"`, `"secret_policy"` nem placeholders.
fn looks_like_secret(value: &str) -> bool {
    let value = value.trim();
    if value.len() < 12 || value.contains(' ') {
        return false;
    }
    if value.eq_ignore_ascii_case("[redacted]") {
        return false;
    }
    let has_digit = value.chars().any(|c| c.is_ascii_digit());
    let has_upper = value.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = value.chars().any(|c| c.is_ascii_lowercase());
    has_digit || (has_upper && has_lower)
}

/// Mascara os segredos do conteudo e devolve quantos trechos foram mascarados.
pub fn mask(content: &str) -> (String, usize) {
    let mut masked = 0usize;
    let mut in_private_key = false;
    let mut out = String::with_capacity(content.len());

    for line in content.lines() {
        let upper = line.to_ascii_uppercase();

        // Bloco de chave privada: mascara tudo ate a linha de END, senao o
        // corpo da chave (que sozinho nao "parece" segredo) vazaria.
        if in_private_key {
            masked += 1;
            out.push_str("[REDACTED private-key]\n");
            if upper.contains("END") && upper.contains("PRIVATE KEY") {
                in_private_key = false;
            }
            continue;
        }
        if upper.contains("BEGIN") && upper.contains("PRIVATE KEY") {
            in_private_key = true;
            masked += 1;
            out.push_str("[REDACTED private-key]\n");
            continue;
        }

        if detect_line(line).is_none() {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        masked += 1;
        out.push_str(&mask_line(line));
        out.push('\n');
    }

    (out, masked)
}

fn mask_line(line: &str) -> String {
    if detect_line(line) == Some(SecretKind::PrivateKey) && line.contains("BEGIN") {
        return "[REDACTED private-key]".to_string();
    }

    let mut out = line.to_string();
    for prefix in ["AKIA", "ghp_", "github_pat_", "xoxb-", "sk-"] {
        out = redact_token(&out, prefix);
    }
    out = redact_jwt(&out);
    out = redact_quoted_literals(&out);

    if detect_line(&out).is_some() {
        // Rede de seguranca: se ainda parece segredo, mascara a linha inteira.
        return "[REDACTED]".to_string();
    }
    out
}

fn redact_token(line: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(index) = rest.find(prefix) {
        out.push_str(&rest[..index]);
        out.push_str("[REDACTED]");
        let tail = &rest[index + prefix.len()..];
        let skip = tail
            .char_indices()
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '-'))
            .map(|(i, _)| i)
            .unwrap_or(tail.len());
        rest = &tail[skip..];
    }
    out.push_str(rest);
    out
}

fn redact_jwt(line: &str) -> String {
    if !line.contains("eyJ") || line.matches('.').count() < 2 {
        return line.to_string();
    }
    line.split_whitespace()
        .map(|word| {
            if word.contains("eyJ") && word.matches('.').count() >= 2 {
                "[REDACTED]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_quoted_literals(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let mut literal = String::new();
            let mut closed = false;
            for next in chars.by_ref() {
                if next == quote {
                    closed = true;
                    break;
                }
                literal.push(next);
            }
            if closed && literal.len() >= 8 {
                out.push(quote);
                out.push_str("[REDACTED]");
                out.push(quote);
            } else {
                out.push(quote);
                out.push_str(&literal);
                if closed {
                    out.push(quote);
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nomes_sensiveis_sao_reconhecidos() {
        for name in [
            ".env",
            ".env.local",
            ".env.production",
            "server.pem",
            "private.key",
            "id_rsa",
            "credentials.json",
            "secrets.yaml",
            "cert.pfx",
            ".netrc",
        ] {
            assert!(is_sensitive_name(name), "deveria ser sensivel: {name}");
        }
        for name in ["main.rs", "config.json", "README.md", "keys.md", "home.rs"] {
            assert!(!is_sensitive_name(name), "nao deveria ser sensivel: {name}");
        }
    }

    #[test]
    fn detecta_chave_privada_e_tokens() {
        assert_eq!(
            detect("-----BEGIN RSA PRIVATE KEY-----\nabc"),
            Some(SecretKind::PrivateKey)
        );
        assert_eq!(
            detect("aws = AKIAABCDEFGHIJKLMNOP"),
            Some(SecretKind::AwsKey)
        );
        assert_eq!(
            detect("token: ghp_abcdefghijklmnopqrstuvwx"),
            Some(SecretKind::ProviderToken)
        );
        assert_eq!(
            detect("url = postgres://usuario:senha123@localhost/db"),
            Some(SecretKind::ConnectionString)
        );
        assert_eq!(
            detect("password = \"super-secreta-123\""),
            Some(SecretKind::PasswordLiteral)
        );
        assert_eq!(detect("fn main() { println!(\"oi\"); }"), None);
    }

    #[test]
    fn nao_marca_codigo_que_apenas_menciona_password() {
        assert_eq!(detect("let password = user_input;"), None);
        assert_eq!(detect("if senha.is_empty() { return; }"), None);
    }

    #[test]
    fn nao_marca_falsos_positivos_do_proprio_codigo() {
        // Linhas reais que ja foram bloqueadas por engano no indice do C2.
        for line in [
            r#"                "secret_policy": config.index.secret_policy,"#,
            r#"        write(&base.join(".env"), b"SENHA=123456");"#,
            r#"            SecretKind::PrivateKey => "private-key","#,
            r#"    let token = generate_token();"#,
            r#"    pub secret_allow: Vec<String>,"#,
        ] {
            assert_eq!(detect(line), None, "falso positivo em: {line}");
        }
    }

    #[test]
    fn marcador_por_linha_libera_fixture() {
        let line = r#"let chave = "-----BEGIN RSA PRIVATE KEY-----"; // c2:allow-secret"#;
        assert_eq!(detect(line), None);
    }

    #[test]
    fn mascara_valores_mas_preserva_o_resto() {
        let content = "host = localhost\npassword = \"super-secreta-123\"\nporta = 5432\n";
        let (masked, count) = mask(content);
        assert_eq!(count, 1);
        assert!(masked.contains("host = localhost"));
        assert!(masked.contains("porta = 5432"));
        assert!(!masked.contains("super-secreta-123"));
        assert!(masked.contains("[REDACTED]"));
    }

    #[test]
    fn mascara_chave_privada_inteira() {
        let content = "-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n-----END-----\n";
        let (masked, count) = mask(content);
        assert!(count >= 1, "deve mascarar o bloco inteiro");
        assert!(masked.contains("[REDACTED private-key]"));
        assert!(!masked.contains("AAAA"), "corpo da chave nao pode vazar");
    }

    #[test]
    fn policy_serializa_em_minusculas() {
        let json = serde_json::to_string(&SecretPolicy::Mask).unwrap();
        assert_eq!(json, "\"mask\"");
        let parsed: SecretPolicy = serde_json::from_str("\"skip\"").unwrap();
        assert_eq!(parsed, SecretPolicy::Skip);
    }
}
