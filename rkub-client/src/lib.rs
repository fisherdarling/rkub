mod board;
mod states;
mod svg;
mod utils;
mod piece_view;

use chrono::{DateTime, Utc};
use log::*;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{convert::FromWasmAbi, JsCast};
use web_sys::{
    Document, Element, EventTarget, FileReader, MessageEvent, ProgressEvent, WebSocket, Window,
};

use crate::states::*;
use crate::svg::AsSVG;
use rkub_common::{ClientMessage, ServerMessage, Piece, Color};

#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// Thanks mkeeter for the callback boilerplate!!!
//
// Boilerplate to wrap and bind a callback.
// The resulting callback must be stored for as long as it may be used.
pub type JsResult<T> = Result<T, JsValue>;
pub type JsError = Result<(), JsValue>;
pub type JsClosure<T> = Closure<dyn FnMut(T) -> JsError>;

#[must_use]
fn build_cb<F, T>(f: F) -> JsClosure<T>
where
    F: FnMut(T) -> JsError + 'static,
    T: FromWasmAbi + 'static,
{
    Closure::wrap(Box::new(f) as Box<dyn FnMut(T) -> JsError>)
}

#[must_use]
fn set_event_cb<E, F, T>(obj: &E, name: &str, f: F) -> JsClosure<T>
where
    E: JsCast + Clone + std::fmt::Debug,
    F: FnMut(T) -> JsError + 'static,
    T: FromWasmAbi + 'static,
{
    let cb = build_cb(f);
    
    let target = obj
        .dyn_ref::<EventTarget>()
        .expect("Could not convert into `EventTarget`");
    target
        .add_event_listener_with_callback(name, cb.as_ref().unchecked_ref())
        .expect("Could not add event listener");

    cb
}

// lifted from the `console_log` example
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(a: &str);
}

#[macro_export]
macro_rules! console_log {
    ($($t:tt)*) => (crate::log(&format!("[{}] {}", crate::timestamp(), &format_args!($($t)*).to_string())))
}

fn timestamp() -> String {
    Utc::now().format("%T%.3f").to_string()
}

fn on_message(msg: ServerMessage) -> JsResult<()> {
    match msg {
        ServerMessage::JoinedRoom { room_name, players, hand } => {
            crate::STATE.lock().unwrap().on_joined_room(room_name, players, hand)
        }
        ServerMessage::TurnFinished {
            ending_player, ending_drew, next_player,
            pieces_remaining, played_pieces
        } => {
            crate::STATE.lock().unwrap().on_turn_finished(ending_player, ending_drew, next_player, pieces_remaining, played_pieces)
        }
        ServerMessage::Pong => {
            console_log!("Server: Pong");
            Ok(())
        }
        _ => {
            console_log!("unhandled message: {:?}", msg);
            Ok(())
        }
    }
}

lazy_static::lazy_static! {
    pub static ref STATE: Mutex<State> = Mutex::new(State::Empty);
}

#[wasm_bindgen]
pub fn run() -> JsResult<()> {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::default());

    console_log!("Starting Application");

    let window = web_sys::window().unwrap();
    let doc = window.document().unwrap();

    let global = Global { window, doc };
    let create_or_join = CreateOrJoin::new(global).unwrap();
    *STATE.lock().unwrap() = State::CreateOrJoin(create_or_join);

    Ok(())
}

pub fn create_heartbeat() -> JsResult<()> {
    console_log!("Creating Heartbeat");
    let heartbeat = Closure::wrap(Box::new(|| {
        console_log!("Client: Ping");
        {
            let mut lock = STATE.lock().unwrap();
            lock.send_ping().unwrap();
        }
    }) as Box<dyn FnMut()>);

    let window = web_sys::window().unwrap();
    let _id = window.set_interval_with_callback_and_timeout_and_arguments_0(
        heartbeat.as_ref().unchecked_ref(),
        3_000,
    )?;

    console_log!("Forgetting Heartbeat");
    heartbeat.forget();

    Ok(())
}
