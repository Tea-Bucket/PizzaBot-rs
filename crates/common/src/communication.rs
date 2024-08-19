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
    EditOrder(OrderRequest),
    GetOrder(String), // Currently redundant, since client should keep track of the servers state
    RequestAll,

    SubscribeUpdates,
    UnsubscribeUpdates
}

#[derive(Serialize, Deserialize)]
pub enum ServerPackage<'a> {
    Response(Response<'a>),
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
pub enum Response<'a> {
    MakeOrder(MakeOrderResponse),
    EditOrder(EditOrderResponse),
    GetOrder(GetOrderResponse),
    Subscription(SubscriptionResponse<'a>)
}

#[derive(Serialize, Deserialize)]
pub enum MakeOrderResponse {
    Success,
    NameAlreadyRegistered,
}

#[derive(Serialize, Deserialize)]
pub enum EditOrderResponse {
    Success,
    NameNotFound,
}

#[derive(Serialize, Deserialize)]
pub enum GetOrderResponse {
    Success(FullOrder),
    NameNotFound,
}

#[derive(Serialize, Deserialize)]
pub enum SubscriptionResponse<'a> {
    Success(FullOrderData<'a>),
    AlreadySubscribed
}