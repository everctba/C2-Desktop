//! Desconto progressivo por volume.

/// Percentual de desconto conforme o subtotal (em centavos).
///
/// - >= 500,00 → 10%
/// - >= 200,00 → 5%
/// - abaixo disso → 0%
pub fn percentual_por_subtotal(subtotal_centavos: u64) -> u64 {
    if subtotal_centavos >= 50_000 {
        10
    } else if subtotal_centavos >= 20_000 {
        5
    } else {
        0
    }
}

/// Aplica o desconto progressivo e devolve o valor em centavos.
pub fn aplicar_desconto_progressivo(subtotal_centavos: u64) -> u64 {
    let percentual = percentual_por_subtotal(subtotal_centavos);
    subtotal_centavos - subtotal_centavos * percentual / 100
}
