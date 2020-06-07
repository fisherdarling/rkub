mod utils;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

// use web_sys::console::log_1;s

#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
pub struct IntervalHandle {
    interval_id: i32,
    _closure: Closure<FnMut()>,
}

impl Drop for IntervalHandle {
    fn drop(&mut self) {
        let window = web_sys::window().unwrap();
        window.clear_interval_with_handle(self.interval_id);
    }
}

#[wasm_bindgen]
pub fn run() -> Result<IntervalHandle, JsValue> {
    let heartbeat = Closure::wrap(Box::new(|| {
        web_sys::console::log_1(&"<3".into());
    }) as Box<FnMut()>);

    let window = web_sys::window().unwrap();
    let id = window.set_interval_with_callback_and_timeout_and_arguments_0(
        heartbeat.as_ref().unchecked_ref(),
        1_000,
    )?;

    Ok(IntervalHandle {
        interval_id: id,
        _closure: heartbeat,
    })
}   
