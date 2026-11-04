use loja_bench::{coupons, pricing, tax, Item};

fn item(preco_centavos: u64, quantidade: u32) -> Item {
    Item {
        preco_unitario_centavos: preco_centavos,
        quantidade,
    }
}

#[test]
fn pedido_vazio_custa_zero() {
    assert_eq!(loja_bench::total_do_pedido(&[], None), 0);
}

#[test]
fn desconto_progressivo_respeita_faixas() {
    assert_eq!(pricing::percentual_por_subtotal(19_999), 0);
    assert_eq!(pricing::percentual_por_subtotal(20_000), 5);
    assert_eq!(pricing::percentual_por_subtotal(49_999), 5);
    assert_eq!(pricing::percentual_por_subtotal(50_000), 10);
}

#[test]
fn desconto_de_10_por_cento_em_500_reais() {
    // 500,00 → 10% → 450,00
    assert_eq!(pricing::aplicar_desconto_progressivo(50_000), 45_000);
}

#[test]
fn cupom_percentual_aplica_sobre_o_valor_ja_descontado() {
    // 500,00 - 10% = 450,00; BEMVINDO10 -> 405,00
    let total = loja_bench::total_do_pedido(&[item(50_000, 1)], Some("BEMVINDO10"));
    // 405,00 + 8% (32,40, exato) = 437,40
    assert_eq!(total, 43_740);
}

#[test]
fn cupom_fixo_nao_negativa_o_total() {
    // pedido de 10,00 com FRETE20: valor após cupom = 0
    assert_eq!(coupons::aplicar_cupom(1_000, Some("FRETE20")), 0);
    // e o total final é só o imposto de zero = zero
    assert_eq!(coupons::aplicar_cupom(0, Some("FRETE20")), 0);
}

#[test]
fn cupom_desconhecido_nao_muda_nada() {
    assert_eq!(coupons::aplicar_cupom(12_345, Some("NAOEXISTE")), 12_345);
}

#[test]
fn imposto_arredonda_para_cima() {
    // 1,00 * 8% = 0,08 exato; 12,34 * 8% = 0,9872 -> 0,99
    assert_eq!(tax::imposto_centavos(100), 8);
    assert_eq!(tax::imposto_centavos(1_234), 99);
}
