use tracing_subscriber::fmt::format::{FmtSpan, Pretty};
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::prelude::*;
use yew::prelude::*;
use yew_router::prelude::*;

mod components;
mod misc_func;

use components::ws::WebSocketProvider;

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
    #[at("/admin")]
    Admin,
    #[not_found]
    #[at("/404")]
    NotFound,
}

fn switch(routes: Route) -> Html {
    match routes {
        Route::Home => html! { <h1>{ "Home" }</h1> },
        Route::Admin => html! {
            <h1>{ "PlaceHolder" }</h1>
        },
        Route::NotFound => html! { <h1>{ "404" }</h1> },
    }
}

#[function_component(Content)]
fn content() -> Html {
    html! {<div>{"Done!"}</div>}
}

#[function_component]
fn Header() -> Html {
    html! {
        <header style="background-color: #333; color: white; padding: 1rem;">
            <nav style="display: flex; justify-content: space-between; align-items: center;">
                <div>
                    <h1 style="margin: 0;">{ "PizzaBotTemplate" }</h1>
                </div>
                <ul style="list-style: none; display: flex; margin: 0;">
                    <li style="margin-right: 1rem;"><a style="color: white; text-decoration: none;" href="">{ "Home" }</a></li>
                    <li style="margin-right: 1rem;"><a style="color: white; text-decoration: none;" href="">{ "Template" }</a></li>
                    <li style="margin-right: 1rem;"><a style="color: white; text-decoration: none;" href="admin">{ "Admin" }</a></li>
                    <li><a style="color: white; text-decoration: none;" href="">{ "Template" }</a></li>
                </ul>
            </nav>
        </header>
    }
}

#[function_component]
fn Footer() -> Html {
    html! {
        <footer style="background-color: #333; color: white; padding: 1rem; margin-top: 2rem;">
            <div style="display: flex; justify-content: space-between; align-items: center;">
                <p style="margin: 0;">{ "© 2024 🍕. All rights 🍕." }</p>
                <ul style="list-style: none; display: flex; margin: 0;">
                    <li style="margin-right: 1rem;"><a style="color: white; text-decoration: none;" href="">{ "Template" }</a></li>
                    <li style="margin-right: 1rem;"><a style="color: white; text-decoration: none;" href="">{ "Template" }</a></li>
                    <li><a style="color: white; text-decoration: none;" href="">{ "Template" }</a></li>
                </ul>
            </div>
        </footer>
    }
}

#[function_component]
fn App() -> Html {


    html! {<>

        <Header/>
        <WebSocketProvider>

            <BrowserRouter>
                <Switch<Route> render={switch} />
            </BrowserRouter>

        </WebSocketProvider>
        <Footer/>

        </>
    }
}

fn main() {
    // We start the logger here, LATER: Include Options to turn this off
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_timer(UtcTime::rfc_3339())
        .with_writer(tracing_web::MakeConsoleWriter)
        .with_span_events(FmtSpan::ACTIVE);
    let perf_layer = tracing_web::performance_layer().with_details_from_fields(Pretty::default());

    let filter_layer = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "debug,yew=info".into());

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(perf_layer)
        .with(filter_layer)
        .init();

    // Start the Render
    yew::Renderer::<App>::new().render();
}
