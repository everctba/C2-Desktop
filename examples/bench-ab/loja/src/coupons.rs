//! Cupons de desconto.

/// Aplica o cupom sobre o valor já descontado (em centavos).
///
/// - `BEMVINDO10`: 10% de desconto
/// - `FRETE20`: R$ 20,00 fixos
/// - qualquer outro: sem efeito
/// - nunca devolve valor negativo
pub fn aplicar_cupom(valor_centavos: u64, cupom: Option<&str>) -> u64 {
    let Some(codigo) = cupom else {
        return valor_centavos;
    };

    match codigo.trim().to_ascii_uppercase().as_str() {
        "BEMVINDO10" => valor_centavos - valor_centavos / 10,
        "FRETE20" => valor_centavos.saturating_sub(2_000),
        _ => valor_centavos,
    }
}
