//! Loja de exemplo do benchmark A/B: preço final de um pedido.
//!
//! As regras vivem em `pricing` (desconto por volume), `coupons` (cupons) e
//! `tax` (imposto com arredondamento para cima). A ordem importa.

pub mod coupons;
pub mod pricing;
pub mod tax;

/// Item de um pedido.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Item {
    pub preco_unitario_centavos: u64,
    pub quantidade: u32,
}

impl Item {
    pub fn total_centavos(&self) -> u64 {
        self.preco_unitario_centavos.saturating_mul(u64::from(self.quantidade))
    }
}

/// Calcula o total final em centavos.
///
/// Ordem: subtotal → desconto progressivo → cupom → imposto.
pub fn total_do_pedido(itens: &[Item], cupom: Option<&str>) -> u64 {
    let subtotal: u64 = itens.iter().map(Item::total_centavos).sum();

    let apos_desconto = pricing::aplicar_desconto_progressivo(subtotal);
    let apos_cupom = coupons::aplicar_cupom(apos_desconto, cupom);
    tax::aplicar_imposto(apos_cupom)
}
