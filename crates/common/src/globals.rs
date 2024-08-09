use serde::{Deserialize, Serialize};

use crate::orders::Price;

#[derive(Serialize, Deserialize)]
pub struct WebSiteConfig {
    pizza: PizzaConfig, // TODO Other
}

#[derive(Serialize, Deserialize)]
pub struct PizzaConfig {
    width_of_piece_in_cm: u8,
    length_of_piece_in_cm: u8,
    price_per_piece: Price,
    pieces_per_pizza: u16,
}
