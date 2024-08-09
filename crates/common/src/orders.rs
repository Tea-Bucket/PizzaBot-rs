use std::mem::MaybeUninit;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Price {
    pub cents: usize
}

pub type OrderAmount = usize;
pub type Preference = f32;

#[derive(Serialize, Deserialize)]
pub struct OrderRequest {
    pub name: String,
    pub order: Order
}

#[derive(Serialize, Deserialize)]
pub struct FullOrder {
    pub info: OrderInfo,
    pub order: Order
}

/// An individual order of a User
#[derive(Serialize, Deserialize, Clone)]
pub struct OrderInfo {
    pub name: String,
    pub has_paid: bool,
    pub price: Price,
}

/// Base Order
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Order {
    pub amounts: PizzaKindArray<OrderAmount>,
    pub preference: Preference
}

pub enum PizzaKind {
    Meat,
    Vegetarian,
    Vegan
}

impl PizzaKind {
    pub const Length: usize = 3;
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct PizzaKindArray<T>(pub [T; PizzaKind::Length]);

impl<T> PizzaKindArray<T> {
    pub fn splat(value: T) -> Self where T: Clone {
        let mut values: MaybeUninit<[T; PizzaKind::Length]> = MaybeUninit::uninit();
        unsafe {
            if PizzaKind::Length == 0 {
                return Self(values.assume_init())
            }

            for i in 1..PizzaKind::Length {
                values.assume_init_mut()[i] = value.clone()
            }
            values.assume_init_mut()[0] = value;
            return Self(values.assume_init())
        }
    }
}