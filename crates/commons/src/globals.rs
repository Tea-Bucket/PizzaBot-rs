use serde::{Deserialize, Serialize};

#[derive(Serialize,Deserialize)]
pub struct WebSiteConfig
{
    pizza: PizzaConfig
    // TODO Other
}

#[derive(Serialize,Deserialize)]
pub struct PizzaConfig
{
    width_of_piece_in_cm: i8, 
    length_of_piece_in_cm: i8,
    price_per_piece:f32,
    pieces_per_pizza: i16,
}