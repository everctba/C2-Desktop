//! Imposto com arredondamento para cima no centavo.

/// Alíquota em pontos percentuais.
pub const ALIQUOTA_PERCENTUAL: u64 = 8;

/// Imposto em centavos, arredondado **para cima** (nunca bancário).
///
/// `valor * 8%` costuma cair em frações de centavo: 1.234 * 8 / 100 = 98,72 →
/// cobra 99. É proposital: dinheiro no varejo arredonda para cima.
pub fn imposto_centavos(valor_centavos: u64) -> u64 {
    let bruto = valor_centavos as u128 * u128::from(ALIQUOTA_PERCENTUAL);
    let centavos = (bruto as f64 / 100.0).ceil() as u64;
    centavos
}

/// Valor após cupom + imposto.
pub fn aplicar_imposto(valor_apos_cupom_centavos: u64) -> u64 {
    valor_apos_cupom_centavos + imposto_centavos(valor_apos_cupom_centavos)
}
