#![allow(non_upper_case_globals)]
mod balancing;

use axum::{
    extract::{
        connect_info::ConnectInfo, ws::{Message, WebSocket, WebSocketUpgrade}, Request, State
    }, http::StatusCode, response::IntoResponse, routing::get, Router
};
use axum_extra::TypedHeader;
use futures::{stream::SplitSink, SinkExt, StreamExt};
use pizza_bot_rs_common::{communication::{self, EditOrderResponse, GetOrderResponse, MakeOrderResponse, Response, ServerPackage, SubscriptionResponse}, orders::{FullOrder, Order, OrderInfo, OrderRequest, OrderState, Price}};
use tokio::sync::{broadcast, Mutex};
use tracing::info;

use std::{borrow::Cow, net::SocketAddr, sync::Arc};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

trait OrderStateExt {
    fn try_add_order(&mut self, name: String, order: Order) -> Option<FullOrder>;
    fn try_edit_order(&mut self, name: String, order: Order) -> Option<FullOrder>;
    fn finalize_update(&mut self);
}

impl OrderStateExt for OrderState {
    fn try_add_order(&mut self, name: String, order: Order) -> Option<FullOrder> {
        match self.order_infos.binary_search_by(|info| info.name.cmp(&name)) {
            Ok(_) => return None,
            Err(index) => {
                let order = Order {
                    preference: order.preference.clamp(0.0, 1.0),
                    ..order
                };
                self.order_infos.insert(index, OrderInfo {
                    name,
                    has_paid: false,
                    price: Price { cents: 0 },
                });
                self.orders.insert(index, order);

                self.finalize_update();

                Some(FullOrder {
                    info: self.order_infos[index].clone(),
                    order,
                    distribution: self.distributions[index]
                })
            },
        }
    }

    fn try_edit_order(&mut self, name: String, order: Order) -> Option<FullOrder> {
        match self.order_infos.binary_search_by(|info| info.name.cmp(&name)) {
            Ok(index) => {
                let order = Order {
                    preference: order.preference.clamp(0.0, 1.0),
                    ..order
                };
                self.order_infos[index] = OrderInfo {
                    name,
                    has_paid: false,
                    price: Price { cents: 0 },
                };
                self.orders[index] = order;

                self.finalize_update();

                Some(FullOrder {
                    info: self.order_infos[index].clone(),
                    order,
                    distribution: self.distributions[index]
                })
            },
            Err(_) => None
        }
    }

    fn finalize_update(&mut self) {
        let (_, config, distributions, valid) = balancing::get_best(15, &self.orders);

        self.config = config;
        self.distributions = distributions;
        self.distributions_valid = valid;

        self.version += 1;
    }
}

struct AppState {
    orders: Mutex<OrderState>,
    broadcast: broadcast::Sender<String>
}

impl AppState {
    pub fn new(broadcast: broadcast::Sender<String>) -> Self {
        Self {
            orders: Mutex::new(OrderState::new(0)),
            broadcast
        }
    }
}

type HandlerState = Arc<AppState>;

pub async fn run() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug,backend=debug,tower_http=off".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let (tx, _) = broadcast::channel(16);

    let app = Router::new()
        .fallback(get(|_request: Request| async {
            StatusCode::NOT_FOUND
        }))
        .route("/ws", get(ws_handler))
        .with_state(Arc::new(AppState::new(tx)))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8081")
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

/// Upgrades a Websocket Connection
async fn ws_handler(
    ws: WebSocketUpgrade,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<HandlerState>,
) -> impl IntoResponse {
    let user_agent = if let Some(TypedHeader(user_agent)) = user_agent {
        user_agent.to_string()
    } else {
        String::from("Unknown browser")
    };
    info!("`{user_agent}` at {addr} connected.");

    ws.on_upgrade(move |socket| web_socket_thread(socket, addr, state))
}

async fn send_package(message: ServerPackage<'_>, sender: &Mutex<SplitSink<WebSocket, Message>>) {
    let Ok(string) = serde_json::to_string(&message) else {
        // TODO handle, although currently the serializer should not be able to fail
        panic!("Could not create response");
    };

    let mut sender = sender.lock().await;
    sender.send(Message::Text(string)).await.expect("Could not send message");
}

fn broadcast_serialized(message: ServerPackage<'_>, sender: &broadcast::Sender<String>) {
    let Ok(string) = serde_json::to_string(&message) else {
        // TODO handle, although currently the serializer should not be able to fail
        panic!("Could not create response");
    };

    // No need to handle result, since the only error it returns is the fact that there was no receiver
    let _ = sender.send(string);
}

async fn web_socket_thread(socket: WebSocket, who: SocketAddr, state: HandlerState) {
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(Mutex::new(sender));

    // Updater is only present if updates are requested
    let mut updater = None;

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            // Entire communication over text, specifically common::ClientPackage/common::ServerPackage
            Message::Text(t) => 'blk: {
                let Ok(request) = serde_json::from_str::<communication::ClientPackage>(&t) else {
                    // TODO handle malformed request
                    break 'blk
                };

                match request {
                    communication::ClientPackage::MakeOrder(order) => handle_make_order(order, &state, &sender).await,
                    communication::ClientPackage::EditOrder(order) => handle_edit_order(order, &state, &sender).await,
                    communication::ClientPackage::GetOrder(name) => handle_get_order(name, &state, &sender).await,
                    communication::ClientPackage::RequestAll => handle_request_all(&state, &sender).await,
                    communication::ClientPackage::SubscribeUpdates => handle_subscribe(&mut updater, &state, &sender).await,
                    communication::ClientPackage::UnsubscribeUpdates => handle_unsubscribe(&mut updater),
                }
            },
            Message::Close(c) => {
                if let Some(cf) = c {
                    info!(
                        "{} sent close with code {} and reason `{}`",
                        who, cf.code, cf.reason
                    );
                } else {
                    info!("{who} somehow sent close message without CloseFrame");
                }
                break
            },

            Message::Binary(_) |
            Message::Pong(_) |
            Message::Ping(_) => {}
        }
    }

    if let Some(updater) = updater {
        updater.abort();
    }
}

async fn handle_make_order(order: OrderRequest, state: &AppState, sender: &Mutex<SplitSink<WebSocket, Message>>) {
    info!("`{}` made request `(amount: {:?}, preference: {})`", order.name, order.order.amounts.0, order.order.preference);

    let mut orders = state.orders.lock().await;
    let success = orders.try_add_order(order.name, order.order);

    let response = match success {
        Some(full) => {
            broadcast_serialized(ServerPackage::Update {
                order: full,
                config: orders.config,

                version: orders.version,
                distributions: Cow::Borrowed(&orders.distributions),
                distributions_valid: orders.distributions_valid,
            }, &state.broadcast);
            drop(orders);
            MakeOrderResponse::Success
        },
        None => {
            drop(orders);
            MakeOrderResponse::NameAlreadyRegistered
        },
    };

    send_package(ServerPackage::Response(Response::MakeOrder(response)), sender).await;
}

async fn handle_edit_order(order: OrderRequest, state: &AppState, sender: &Mutex<SplitSink<WebSocket, Message>>) {
    info!("Order edit for `{}` with `(amount: {:?}, preference: {})` requested", order.name, order.order.amounts.0, order.order.preference);
    let mut orders = state.orders.lock().await;
    let success = orders.try_edit_order(order.name, order.order);
    let response = match success {
        Some(full) => {
            broadcast_serialized(ServerPackage::Update {
                order: full,
                config: orders.config,

                version: orders.version,
                distributions: Cow::Borrowed(&orders.distributions),
                distributions_valid: orders.distributions_valid,
            }, &state.broadcast);
            drop(orders);
            EditOrderResponse::Success
        },
        None => {
            drop(orders);
            EditOrderResponse::NameNotFound
        },
    };

    send_package(ServerPackage::Response(Response::EditOrder(response)), sender).await;
}

async fn handle_get_order(name: String, state: &AppState, sender: &Mutex<SplitSink<WebSocket, Message>>) {
    info!("Order for `{name}` requested");
    let orders = state.orders.lock().await;
    let response = match orders.order_infos.binary_search_by(|info| info.name.cmp(&name)) {
        Ok(index) => {
            let info = orders.order_infos[index].clone();
            let order = orders.orders[index];
            let distribution = orders.distributions[index];
            drop(orders);

            GetOrderResponse::Success(FullOrder {
                info,
                order,
                distribution
            })
        },
        Err(_) => {
            drop(orders);

            GetOrderResponse::NameNotFound
        },
    };
    send_package(ServerPackage::Response(Response::GetOrder(response)), sender).await;
}

async fn handle_request_all(state: &AppState, sender: &Mutex<SplitSink<WebSocket, Message>>) {
    info!("Full state requested");
    let orders = state.orders.lock().await;
    let init = orders.to_full_data();

    send_package(ServerPackage::All(init), sender).await;
}

async fn handle_subscribe(updater: &mut Option<tokio::task::JoinHandle<()>>, state: &AppState, sender: &Arc<Mutex<SplitSink<WebSocket, Message>>>) {
    if updater.is_none() {
        let mut rx = state.broadcast.subscribe();

        let orders = state.orders.lock().await;
        let init = orders.to_full_data();
        send_package(ServerPackage::Response(Response::Subscription(SubscriptionResponse::Success(init))), sender).await;
        drop(orders);

        let sender = sender.clone();
        // Send all broadcast through
        *updater = Some(tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                if sender.lock().await.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        }));
    } else {
        send_package(ServerPackage::Response(Response::Subscription(SubscriptionResponse::AlreadySubscribed)), sender).await;
    }
}

fn handle_unsubscribe(updater: &mut Option<tokio::task::JoinHandle<()>>) {
    if let Some(up) = updater {
        up.abort();
        *updater = None
    }
}
