/// Stolen dirctly from [here](https://users.rust-lang.org/t/rust-wasm-async-sleeping-for-100-milli-seconds-goes-up-to-1-minute/81177)
/// Might need for some Dark Magic, at the time of writing this comment unsused
#[allow(unused)]
pub async fn sleep(millis: i32) {
    let mut cb = |resolve: js_sys::Function, _reject: js_sys::Function| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, millis)
            .unwrap();
    };
    let p = js_sys::Promise::new(&mut cb);
    wasm_bindgen_futures::JsFuture::from(p).await.unwrap();
}
