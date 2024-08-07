use serde::{Deserialize, Serialize};

/// An individual order of a User
#[derive(Serialize, Deserialize)]
pub struct Order {
    pub name: String,
    pub base: OrderBase,
    pub has_paid: bool,
    //TODO more fields
}

/// Base Order
#[derive(Serialize, Deserialize)]
pub struct OrderBase {
    //preference: TODO type?
    pub price: f32,
}
