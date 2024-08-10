use std::{borrow::Cow, iter::Sum, mem::MaybeUninit};

use serde::{Deserialize, Serialize};

use crate::communication::FullOrderData;

pub type OrderStateVersion = usize;
pub type PizzaAmount = u8;
pub type OrderAmount = usize;
pub type Preference = f32;
pub type Distribution = PizzaKindArray<OrderAmount>;

#[derive(Serialize, Deserialize, Clone)]
pub struct Price {
    pub cents: usize
}

pub struct OrderState {
    pub version: OrderStateVersion,

    pub order_infos: Vec<OrderInfo>,
    pub orders: Vec<Order>,

    pub config: PizzaKindArray<PizzaAmount>,
    pub distributions: Vec<Distribution>,
    pub distributions_valid: bool
}

impl OrderState {
    pub fn new(version: OrderStateVersion) -> Self {
        Self {
            version,

            order_infos: Vec::new(),
            orders: Vec::new(),

            config: PizzaKindArray::splat(0),
            distributions: Vec::new(),
            distributions_valid: true
        }
    }

    pub fn from_full_data(all: FullOrderData) -> Self {
        Self {
            version: all.version,
            order_infos: all.order_infos.into_owned(),
            orders: all.orders.into_owned(),
            config: all.config,
            distributions: all.distributions.into_owned(),
            distributions_valid: all.valid_distributions,
        }
    }

    pub fn to_full_data(&self) -> FullOrderData {
        FullOrderData {
            version: self.version,
            order_infos: Cow::Borrowed(&self.order_infos),
            orders: Cow::Borrowed(&self.orders),
            config: self.config,
            distributions: Cow::Borrowed(&self.distributions),
            valid_distributions: self.distributions_valid
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct OrderRequest {
    pub name: String,
    pub order: Order
}

#[derive(Serialize, Deserialize)]
pub struct FullOrder {
    pub info: OrderInfo,
    pub order: Order,
    pub distribution: Distribution
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
    pub amounts: Distribution,
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

    pub fn map<S>(self, f: impl FnMut(T) -> S) -> PizzaKindArray<S> {
        PizzaKindArray(self.0.map(f))
    }

    pub fn zip_map<S, R>(self, other: PizzaKindArray<S>, mut f: impl FnMut(T, S) -> R) -> PizzaKindArray<R> {
        let mut res: MaybeUninit<[R; PizzaKind::Length]> = MaybeUninit::uninit();
        for (i, (s, o)) in self.into_iter().zip(other).enumerate() {
            unsafe {
                res.assume_init_mut()[i] = f(s, o)
            }
        }

        PizzaKindArray(unsafe { res.assume_init() })
    }

    pub fn reduce(self, f: impl Fn(T, T) -> T) -> T {
        let Some(acc) = self.0.into_iter().reduce(f) else {unreachable!()};
        return acc
    }

    pub fn sum<S: Sum<T>>(self) -> S {
        self.into_iter().sum()
    }

    pub fn iter_mut(&mut self) -> <&mut Self as IntoIterator>::IntoIter {
        IntoIterator::into_iter(&mut self.0)
    }
}

impl<T> IntoIterator for PizzaKindArray<T> {
    type Item = T;

    type IntoIter = std::array::IntoIter<T, {PizzaKind::Length}>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a mut PizzaKindArray<T> {
    type Item = &'a mut T;

    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}