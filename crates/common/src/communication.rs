use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::orders::{Distribution, FullOrder, Order, OrderInfo, OrderRequest, OrderStateVersion, PizzaAmount, PizzaKindArray};

#[derive(Serialize, Deserialize)]
pub struct FullOrderData<'a> {
    pub version: OrderStateVersion,

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
    RequestAll,
}

#[derive(Serialize, Deserialize)]
pub enum ServerPackage<'a> {
    Response(Response),
    Update {
        order: FullOrder,

        version: OrderStateVersion,
        config: PizzaKindArray<PizzaAmount>,
        distributions: Cow<'a, [Distribution]>,
        distributions_valid: bool
    },
    All(FullOrderData<'a>)
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