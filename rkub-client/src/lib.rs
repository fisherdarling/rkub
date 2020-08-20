#![allow(unused_unsafe)]
#![allow(deprecated)]
mod board;
mod states;
mod svg;

use chrono::Utc;

use std::sync::Mutex;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{convert::FromWasmAbi, JsCast};
use web_sys::EventTarget;

use crate::states::*;

use rkub_common::ServerMessage;

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
    ($($t:tt)*) => (unsafe { crate::log(&format!("[{}] {}", crate::timestamp(), &format_args!($($t)*).to_string())) })
}

fn timestamp() -> String {
    Utc::now().format("%T%.3f").to_string()
}

fn on_message(msg: ServerMessage) -> JsResult<()> {
    match msg {
        ServerMessage::Pong => {
            console_log!("Server: Pong");
            Ok(())
        }
        ServerMessage::JoinedRoom {
            room_name,
            players,
            hand,
            pieces_remaining,
            board,
        } => crate::STATE.lock().unwrap().on_joined_room(
            room_name,
            players,
            hand,
            pieces_remaining,
            board,
        ),
        ServerMessage::TurnFinished {
            ending_player,
            ending_drew,
            next_player,
            pieces_remaining,
            board,
        } => crate::STATE.lock().unwrap().on_turn_finished(
            ending_player,
            ending_drew,
            next_player,
            pieces_remaining,
            board,
        ),
        ServerMessage::PlayerWon(name) => crate::STATE.lock().unwrap().on_player_won(name),
        ServerMessage::CurrentPlayer(idx) => crate::STATE.lock().unwrap().on_current_player(idx),
        ServerMessage::PlayerJoined(name) => crate::STATE.lock().unwrap().on_player_joined(name),
        ServerMessage::DrawPiece(piece) => crate::STATE.lock().unwrap().on_draw_piece(piece),
        ServerMessage::Place(coord, piece) => {
            crate::STATE.lock().unwrap().on_piece_place(coord, piece)
        }
        ServerMessage::Pickup(coord, piece) => crate::STATE.lock().unwrap().on_pickup(coord, piece),
        ServerMessage::InvalidBoardState => crate::STATE.lock().unwrap().on_invalid_board(),
        ServerMessage::StartTurn => crate::STATE.lock().unwrap().on_turn_start(),
        ServerMessage::EndTurnValid => crate::STATE.lock().unwrap().on_end_turn_valid(),
        ServerMessage::PlayerDisconnected(idx) => {
            crate::STATE.lock().unwrap().on_player_disconnected(idx)
        }
        ServerMessage::PlayerReconnected(idx) => {
            crate::STATE.lock().unwrap().on_player_reconnected(idx)
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

#[wasm_bindgen(start)]
pub fn main() -> JsResult<()> {
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
