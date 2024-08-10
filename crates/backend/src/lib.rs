#![allow(non_upper_case_globals)]
mod balancing;

use axum::{
    extract::{
        connect_info::ConnectInfo, ws::{Message, WebSocket, WebSocketUpgrade}, Request, State
    }, http::StatusCode, response::IntoResponse, routing::get, Router
};
use axum_extra::TypedHeader;
use futures::{stream::SplitSink, SinkExt, StreamExt};
use pizza_bot_rs_common::{communication::{self, EditOrderResponse, GetOrderResponse, MakeOrderResponse, Response, ServerPackage}, orders::{FullOrder, Order, OrderInfo, OrderState, Price}};
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

async fn send_serialized(message: impl serde::ser::Serialize, sender: &mut SplitSink<WebSocket, Message>) {
    let Ok(string) = serde_json::to_string(&message) else {
        // TODO handle, although currently the serializer should not be able to fail
        panic!("Could not create response");
    };
    sender.send(Message::Text(string)).await.expect("Could not send message");
}

fn broadcast_serialized(message: impl serde::ser::Serialize, sender: &broadcast::Sender<String>) {
    let Ok(string) = serde_json::to_string(&message) else {
        // TODO handle, although currently the serializer should not be able to fail
        panic!("Could not create response");
    };
    sender.send(string).expect("Could not send message");
}

async fn web_socket_thread(socket: WebSocket, who: SocketAddr, state: HandlerState) {
    let (mut sender, mut receiver) = socket.split();

    {   // Send initialize package
        let orders = state.orders.lock().await;
        let init = orders.to_full_data();

        send_serialized(&init, &mut sender).await;
    }

    let sender = Arc::new(Mutex::new(sender));

    let mut rx = state.broadcast.subscribe();

    // Send all broadcast through
    let mut send_task = {
        let sender = sender.clone();
        tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                if sender.lock().await.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        })
    };

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                // Entire communication over text, specifically common::ClientPackage/common::ServerPackage
                Message::Text(t) => 'blk: {
                    let Ok(request) = serde_json::from_str::<communication::ClientPackage>(&t) else {
                        // TODO handle malformed request
                        break 'blk
                    };

                    match request {
                        communication::ClientPackage::MakeOrder(order) => {
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

                            let mut sender = sender.lock().await;
                            send_serialized(ServerPackage::Response(Response::MakeOrder(response)), &mut sender).await;
                            drop(sender);
                        },
                        communication::ClientPackage::EditOrder(order) => {
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

                            let mut sender = sender.lock().await;
                            send_serialized(ServerPackage::Response(Response::EditOrder(response)), &mut sender).await;
                            drop(sender);
                        },
                        communication::ClientPackage::GetOrder(name) => {
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

                            let mut sender = sender.lock().await;
                            send_serialized(ServerPackage::Response(Response::GetOrder(response)), &mut sender).await;
                            drop(sender);
                        },
                        communication::ClientPackage::RequestAll => {
                            info!("Full state requested");

                            {   // Send initialize package
                                let orders = state.orders.lock().await;
                                let init = orders.to_full_data();

                                let mut sender = sender.lock().await;
                                send_serialized(&ServerPackage::All(init), &mut sender).await;
                                drop(sender);
                            }
                        }
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
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    };
}
