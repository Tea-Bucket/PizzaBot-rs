use futures_util::{SinkExt, StreamExt};
use pizza_bot_rs_common::{communication::{ClientPackage, EditOrderResponse, FullOrderData, GetOrderResponse, MakeOrderResponse, Response, ServerPackage}, orders::{Order, OrderAmount, OrderRequest, OrderState, PizzaKind, PizzaKindArray, Preference}};
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
        Ok((stream, _)) => stream,
        Err(e) => {
            println!("WebSocket handshake for client failed with {e}!");
            return;
        }
    };

    let (rs, rr) = std::sync::mpsc::channel();

    let (sender, mut receiver) = ws_stream.split();

    struct Orders {
        state: OrderState,
        dirty: bool
    }

    impl Orders {
        fn print(&self) {
            println!("config: {:?}, valid: {}", self.state.config.0, self.state.distributions_valid);
            for ((info, order), distr) in self.state.order_infos.iter().zip(&self.state.orders).zip(&self.state.distributions) {
                println!("{}: (amounts: {:?}, preference: {}), given: {:?}, price: {}, paid: {}", info.name, order.amounts.0, order.preference, distr.0, info.price.cents as f32 / 100 as f32, info.has_paid)
            }
        }
    }

    let state;

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

        let Ok(all) = serde_json::from_str::<FullOrderData>(&init) else {
            // TODO handle malformed response
            return
        };

        state = Orders {
            state: OrderState::from_full_data(all),
            dirty: false,
        };
    }

    println!("Orders:");
    state.print();

    let state = Arc::new(Mutex::new(state));

    let sender = Arc::new(Mutex::new(sender));

    let mut recv_task = {
        let state = state.clone();
        let sender = sender.clone();
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
                            ServerPackage::Update { order, version, config, distributions, distributions_valid } => {
                                let mut state = state.lock().await;

                                if state.state.version + 1 != version {
                                    drop(state);

                                    let Ok(string) = serde_json::to_string(&ClientPackage::RequestAll) else {
                                        println!("Could not create request");
                                        break 'blk
                                    };

                                    if sender.lock().await
                                        .send(Message::Text(string))
                                        .await
                                        .is_err()
                                    {
                                        break 'blk
                                    }

                                    break 'blk
                                }

                                match state.state.order_infos.binary_search_by(|info| info.name.cmp(&order.info.name)) {
                                    Ok(index) => {
                                        state.state.order_infos[index] = order.info;
                                        state.state.orders[index] = order.order;
                                    },
                                    Err(index) => {
                                        state.state.order_infos.insert(index, order.info);
                                        state.state.orders.insert(index, order.order);
                                    },
                                }
                                state.state.config = config;
                                state.state.distributions = distributions.into_owned();
                                state.state.distributions_valid = distributions_valid;
                                state.dirty = true;
                                drop(state)
                            },
                            ServerPackage::All(all) => {
                                let mut state = state.lock().await;
                                state.state = OrderState::from_full_data(all);
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
                let mut state = state.lock().await;
                if state.dirty {
                    println!("\x1B[36m>>> Orders have changed.\x1B[37m");
                    state.dirty = false
                }
                drop(state)
            }

            println!("------------------------------------");
            println!("What do you want to do?");
            println!("(1) Make new order");
            println!("(2) Edit an order");
            println!("(3) Get an order");
            println!("(v) View orders");
            println!("(r) Reload");
            println!("(q) Exit");
            println!("------------------------------------");

            loop {
                buffer.clear();
                let Ok(_) = input.read_line(&mut buffer).await else {
                    break 'outer;
                };

                println!("\x1B[2J");
                match buffer.trim() {
                    "v" => {
                        let mut state = state.lock().await;
                        state.dirty = false;
                        state.print();
                        drop(state);
                        println!();
                        continue 'outer
                    },
                    "r" => continue 'outer,
                    "1" => {
                        let Some(mut request) = fun_name(&mut buffer, &mut input).await else {
                            break 'outer
                        };

                        let order = request.order;

                        loop {
                            let Ok(string) = serde_json::to_string(&ClientPackage::MakeOrder(request)) else {
                                println!("Could not create request");
                                break 'outer
                            };

                            if sender.lock().await
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
                                MakeOrderResponse::Success => println!("\x1B[32m>>> Request added successfully\x1B[37m"),
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

                                    request = OrderRequest {
                                        name: buffer.trim().to_owned(),
                                        order,
                                    };
                                    continue
                                },
                            }

                            break
                        }
                    },
                    "2" => {
                        let Some(mut request) = fun_name(&mut buffer, &mut input).await else {
                            break 'outer
                        };

                        let order = request.order;

                        loop {
                            let Ok(string) = serde_json::to_string(&ClientPackage::EditOrder(request)) else {
                                println!("Could not create request");
                                break 'outer
                            };

                            if sender.lock().await
                                .send(Message::Text(string))
                                .await
                                .is_err()
                            {
                                break 'outer
                            }

                            let Ok(response) = rr.recv() else {
                                break 'outer
                            };

                            let Response::EditOrder(response) = response else {
                                println!("Got invalid response try again later");
                                break
                            };

                            match response {
                                EditOrderResponse::Success => println!("\x1B[32m>>> Request edited successfully\x1B[37m"),
                                EditOrderResponse::NameNotFound => {
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

                                    request = OrderRequest {
                                        name: buffer.trim().to_owned(),
                                        order,
                                    };
                                    continue
                                },
                            }

                            break
                        }
                    },
                    "3" => {
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

                            if sender.lock().await
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

                    _ => println!("\x1B[31m>>> Invalid command\x1B[37m")
                }
                break
            }
        }

        println!("Terminating...");
        if let Err(e) = sender.lock().await
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

async fn fun_name(buffer: &mut String, input: &mut BufReader<tokio::io::Stdin>) -> Option<OrderRequest> {
    println!("name: ");

    buffer.clear();
    let Ok(_) = input.read_line(buffer).await else {
        return None
    };

    let name = buffer.trim().to_owned();

    let mut amounts = PizzaKindArray::splat(0);
    for i in 0..PizzaKind::Length {
        println!("amount of type {i}: ");

        let amount = loop {
            buffer.clear();
            let Ok(_) = input.read_line(buffer).await else {
                return None
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
        let Ok(_) = input.read_line(buffer).await else {
            return None
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

    return Some(OrderRequest {
        name,
        order: Order {
            amounts,
            preference,
        },
    })
}
