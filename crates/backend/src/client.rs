use futures_util::{SinkExt, StreamExt};
use pizza_bot_rs_common::{communication::{GetOrderResponse, Initialize, MakeOrderResponse, ClientPackage, Response, ServerPackage}, orders::{Order, OrderAmount, OrderInfo, OrderRequest, PizzaKind, PizzaKindArray, Preference}};
use tokio::{io::{AsyncBufReadExt, BufReader}, sync::Mutex};
use std::{borrow::Cow, sync::Arc};

use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::{frame::coding::CloseCode, CloseFrame, Message},
};

const SERVER: &str = "ws://127.0.0.1:8081/ws";

#[tokio::main]
async fn main() {
    spawn_client().await;
}

async fn spawn_client() {
    let ws_stream = match connect_async(SERVER).await {
        Ok((stream, _)) => {
            println!("Handshake for client has been completed");
            stream
        }
        Err(e) => {
            println!("WebSocket handshake for client failed with {e}!");
            return;
        }
    };

    let (rs, rr) = std::sync::mpsc::channel();

    let (mut sender, mut receiver) = ws_stream.split();

    struct Orders {
        order_infos: Vec<OrderInfo>,
        orders: Vec<Order>,
        dirty: bool
    }

    impl Orders {
        fn print_if_changed(&self) {
            if self.dirty {
                println!("Orders have changed");
                self.print()
            }
        }

        fn print(&self) {
            for (info, order) in self.order_infos.iter().zip(&self.orders) {
                println!("{}: (amounts: {:?}, preference: {}), price: {}, paid: {}", info.name, order.amounts.0, order.preference, info.price.cents as f32 / 100 as f32, info.has_paid)
            }
        }
    }

    let mut state = Orders {
        order_infos: Vec::new(),
        orders: Vec::new(),
        dirty: false
    };

    {
        let Some(Ok(msg)) = receiver.next().await else {
            return
        };

        let init = match msg {
            Message::Text(t) => t,
            Message::Close(c) => {
                if let Some(cf) = c {
                    println!(
                        ">>> got close with code {} and reason `{}`",
                        cf.code, cf.reason
                    );
                } else {
                    println!(">>> somehow got close message without CloseFrame");
                }

                return
            },

            Message::Binary(_) |
            Message::Pong(_) |
            Message::Ping(_) => return,

            Message::Frame(_) => {
                unreachable!("This is never supposed to happen")
            }
        };

        let Ok(response) = serde_json::from_str::<Initialize>(&init) else {
            // TODO handle malformed response
            return
        };

        state.order_infos = response.order_infos.into_owned();
        state.orders = response.orders.into_owned();
    }

    println!("Orders:");
    state.print();

    let state = Arc::new(Mutex::new(state));

    let mut recv_task = {
        let state = state.clone();
        tokio::spawn(async move {
            while let Some(Ok(msg)) = receiver.next().await {
                match msg {
                    Message::Text(t) => 'blk: {
                        let Ok(response) = serde_json::from_str::<ServerPackage>(&t) else {
                            // TODO handle malformed response
                            break 'blk
                        };

                        match response {
                            ServerPackage::Response(response) => {
                                if rs.send(response).is_err() {
                                    return
                                }
                            },
                            ServerPackage::Update(full) => {
                                let mut state = state.lock().await;
                                match state.order_infos.binary_search_by(|info| info.name.cmp(&full.info.name)) {
                                    Ok(index) => {
                                        state.order_infos[index] = full.info;
                                        state.orders[index] = full.order
                                    },
                                    Err(index) => {
                                        state.order_infos.insert(index, full.info);
                                        state.orders.insert(index, full.order)
                                    },
                                }
                                state.dirty = true;
                                drop(state)
                            }
                        }
                    },
                    Message::Close(c) => {
                        if let Some(cf) = c {
                            println!(
                                ">>> got close with code {} and reason `{}`",
                                cf.code, cf.reason
                            );
                        } else {
                            println!(">>> somehow got close message without CloseFrame");
                        }

                        return
                    },

                    Message::Binary(_) |
                    Message::Pong(_) |
                    Message::Ping(_) => {},

                    Message::Frame(_) => {
                        unreachable!("This is never supposed to happen")
                    }
                }
            }
        })
    };

    let mut send_task = tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let mut input = BufReader::new(stdin);

        let mut buffer = String::new();

        'outer:
        loop {
            {
                let state = state.lock().await;
                state.print_if_changed();
                drop(state)
            }

            println!("What do you want to do?");
            println!("(1) Make new order");
            println!("(2) Get an order");
            println!("(r) Reload");
            println!("(q) Exit");

            loop {
                buffer.clear();
                let Ok(_) = input.read_line(&mut buffer).await else {
                    break 'outer;
                };

                match buffer.trim() {
                    "r" => continue 'outer,
                    "1" => {
                        println!("name: ");

                        buffer.clear();
                        let Ok(_) = input.read_line(&mut buffer).await else {
                            break 'outer;
                        };

                        let mut name = buffer.trim().to_owned();

                        let mut amounts = PizzaKindArray::splat(0);
                        for i in 0..PizzaKind::Length {
                            println!("amount of type {i}: ");

                            let amount = loop {
                                buffer.clear();
                                let Ok(_) = input.read_line(&mut buffer).await else {
                                    break 'outer;
                                };

                                match buffer.trim().parse::<OrderAmount>() {
                                    Ok(amount) => break amount,
                                    Err(err) => {
                                        println!("Error {}", err);
                                        println!("Invalid input. Please input a non-negative integer: ");
                                        continue
                                    }
                                }
                            };

                            amounts.0[i] = amount
                        }

                        println!("preference (from shape = 0 to amount = 1): ");

                        let preference = loop {
                            buffer.clear();
                            let Ok(_) = input.read_line(&mut buffer).await else {
                                break 'outer;
                            };

                            let Ok(preference) = buffer.trim().parse::<Preference>() else {
                                println!("Invalid input. Please input a number: ");
                                continue
                            };

                            if preference < 0.0 || preference > 1.0 {
                                println!("Invalid input. Must be in 0..1: ");
                                continue
                            }

                            break preference
                        };

                        loop {
                            let Ok(string) = serde_json::to_string(&ClientPackage::MakeOrder(OrderRequest {
                                name,
                                order: Order {
                                    amounts,
                                    preference,
                                },
                            })) else {
                                println!("Could not create request");
                                break 'outer
                            };

                            if sender
                                .send(Message::Text(string))
                                .await
                                .is_err()
                            {
                                break 'outer
                            }

                            let Ok(response) = rr.recv() else {
                                break 'outer
                            };

                            let Response::MakeOrder(response) = response else {
                                println!("Got invalid response try again later");
                                break
                            };

                            match response {
                                MakeOrderResponse::Success => println!("Request added successfully"),
                                MakeOrderResponse::NameAlreadyRegistered => {
                                    println!("Name already exists. Do you want to try again? (y/n):");

                                    loop {
                                        buffer.clear();
                                        let Ok(_) = input.read_line(&mut buffer).await else {
                                            break 'outer;
                                        };

                                        match buffer.trim() {
                                            "y" => break,
                                            "n" => continue 'outer,

                                            _ => {
                                                println!("Invalid command");
                                                continue
                                            }
                                        }
                                    }

                                    println!("Type in a new name:");

                                    buffer.clear();
                                    let Ok(_) = input.read_line(&mut buffer).await else {
                                        break 'outer;
                                    };

                                    name = buffer.trim().to_owned();
                                    continue
                                },
                            }

                            break
                        }
                    },
                    "2" => {
                        println!("name: ");

                        buffer.clear();
                        let Ok(_) = input.read_line(&mut buffer).await else {
                            break 'outer;
                        };

                        let mut name = buffer.trim().to_owned();

                        loop {
                            let Ok(string) = serde_json::to_string(&ClientPackage::GetOrder(name)) else {
                                println!("Could not create request");
                                break 'outer
                            };

                            if sender
                                .send(Message::Text(string))
                                .await
                                .is_err()
                            {
                                break 'outer
                            }

                            let Ok(response) = rr.recv() else {
                                break 'outer
                            };

                            let Response::GetOrder(response) = response else {
                                println!("Got invalid response try again later");
                                break
                            };

                            let order = match response {
                                GetOrderResponse::Success(order) => order,
                                GetOrderResponse::NameNotFound => {
                                    println!("Name does not exist. Do you want to try again? (y/n):");

                                    loop {
                                        buffer.clear();
                                        let Ok(_) = input.read_line(&mut buffer).await else {
                                            break 'outer;
                                        };

                                        match buffer.trim() {
                                            "y" => break,
                                            "n" => continue 'outer,

                                            _ => {
                                                println!("Invalid command");
                                                continue
                                            }
                                        }
                                    }

                                    println!("Type in a new name:");
                                    buffer.clear();
                                    let Ok(_) = input.read_line(&mut buffer).await else {
                                        break 'outer;
                                    };

                                    name = buffer.trim().to_owned();
                                    continue
                                },
                            };

                            println!("{}: (amounts: {:?}, preference: {}), price: {}, paid: {}", order.info.name, order.order.amounts.0, order.order.preference, order.info.price.cents as f32 / 100 as f32, order.info.has_paid);

                            break
                        }
                    },
                    "q" => break 'outer,

                    _ => {
                        buffer.clear();
                    }
                }
                break
            }
        }

        println!("Terminating...");
        if let Err(e) = sender
            .send(Message::Close(Some(CloseFrame {
                code: CloseCode::Normal,
                reason: Cow::from("Termination"),
            })))
            .await
        {
            println!("Could not send Close due to {e:?}, probably it is ok?");
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    };
}
