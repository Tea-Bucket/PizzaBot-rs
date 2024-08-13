use std::rc::Rc;

use gloo::{
    net::websocket::{futures::WebSocket, State},
    timers::callback::Interval,
};
use tracing::debug;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct WebSocketProviderProps {
    #[prop_or_default]
    pub children: Html,
}

pub enum WSConMsg {
    Connected(bool),
}

pub struct WebSocketProvider {
    ws: Rc<WebSocket>,
    connected: bool,
    pending_connection_poll: Option<Interval>,
}

impl Component for WebSocketProvider {
    type Message = WSConMsg;

    type Properties = WebSocketProviderProps;

    fn create(_ctx: &Context<Self>) -> Self {
        WebSocketProvider {
            ws: Rc::new(WebSocket::open("ws://0.0.0.0:8081/ws").unwrap()), //TODO add Error handling, this might crash if no connection could be established
            connected: false,
            pending_connection_poll: None,
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        //Test Implementation:
        html! {
            <>
            if self.connected
            {
                {"Websocket Connected"}
            }
            else
            {
                {"Websocket not Connected"}
            }
            </>
        }

        // Actual Implementation:
        //html!{<>{ ctx.props().children.clone() }
        //</>}
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            WSConMsg::Connected(b) => {
                if self.connected != b {
                    self.connected = b;
                    drop(self.pending_connection_poll.take());
                    return true;
                }
            }
        }
        false
    }

    fn changed(&mut self, _ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        true
    }

    fn rendered(&mut self, ctx: &Context<Self>, _first_render: bool) {
        if !self.connected {
            let cloned_ws = self.ws.clone();
            let ctx_link = ctx.link().clone();

            let interval = Interval::new(100, move || {
                debug!("Polling Websocket state");
                match cloned_ws.state() {
                    State::Connecting => (),
                    State::Open => ctx_link.send_message(WSConMsg::Connected(true)),
                    State::Closing => (),
                    State::Closed => (),
                }
            });
            self.pending_connection_poll = Some(interval);
        }
    }

    fn prepare_state(&self) -> Option<String> {
        None
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        debug!("WebSocketProvider destroyed")
    }
}
