use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::orders::{FullOrder, Order, OrderInfo, OrderRequest};

#[derive(Serialize, Deserialize)]
pub struct Initialize<'a> {
    pub order_infos: Cow<'a, [OrderInfo]>,
    pub orders: Cow<'a, [Order]>,
}

#[derive(Serialize, Deserialize)]
pub enum ClientPackage {
    MakeOrder(OrderRequest),
    GetOrder(String),
}

#[derive(Serialize, Deserialize)]
pub enum ServerPackage {
    Response(Response),
    Update(FullOrder)
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