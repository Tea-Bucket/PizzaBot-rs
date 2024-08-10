use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::orders::{Distribution, FullOrder, Order, OrderInfo, OrderRequest, PizzaAmount, PizzaKindArray};

#[derive(Serialize, Deserialize)]
pub struct Initialize<'a> {
    pub order_infos: Cow<'a, [OrderInfo]>,
    pub orders: Cow<'a, [Order]>,

    pub config: PizzaKindArray<PizzaAmount>,
    pub distributions: Cow<'a, [Distribution]>,
    pub valid_distributions: bool
}

#[derive(Serialize, Deserialize)]
pub enum ClientPackage {
    MakeOrder(OrderRequest),
    GetOrder(String),
}

#[derive(Serialize, Deserialize)]
pub enum ServerPackage<'a> {
    Response(Response),
    Update {
        order: FullOrder,
        config: PizzaKindArray<PizzaAmount>,
        distributions: Cow<'a, [Distribution]>,
        distributions_valid: bool
    }
}

#[derive(Serialize, Deserialize)]
pub enum Response {
    MakeOrder(MakeOrderResponse),
    GetOrder(GetOrderResponse),
}

#[derive(Serialize, Deserialize)]
pub enum MakeOrderResponse {
    Success,
    NameAlreadyRegistered,
}

#[derive(Serialize, Deserialize)]
pub enum GetOrderResponse {
    Success(FullOrder),
    NameNotFound,
}