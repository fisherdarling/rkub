use std::sync::Mutex;
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, MessageEvent, MouseEvent, WebSocket, Window, Event, PointerEvent};

use crate::STATE;
use crate::board::Board;
use crate::{console_log, log, set_event_cb, timestamp};
use rkub_common::{ClientMessage, Color, Piece, ServerMessage};

type JsResult<T> = Result<T, JsValue>;
type JsError = Result<(), JsValue>;
type JsClosure<T> = Closure<dyn FnMut(T) -> JsError>;

macro_rules! methods {
    ($($sub:ident => [$($name:ident($($var:ident: $type:ty),*)),+ $(,)?]),+
       $(,)?) =>
    {
        $($(
        pub fn $name(&mut self, $($var: $type),* ) -> JsError {
            match self {
                State::$sub(s) => s.$name($($var),*),
                _ => panic!("Invalid state transition"),
            }
        }
        )+)+
    }
}

macro_rules! transitions {
    ($($sub:ident => [$($name:ident($($var:ident: $type:ty),*)
                        -> $into:ident),+ $(,)?]),+$(,)?) =>
    {
        $($(
        pub fn $name(&mut self, $($var: $type),* ) -> JsError {
            console_log!("t: {}", stringify!($name));
            let s = std::mem::replace(self, State::Empty);
            match s {
                State::$sub(s) => *self = State::$into(s.$name($($var),*)?),
                _ => panic!("Invalid state"),
            }
            Ok(())
        }
        )+)+
    }
}

#[derive(Debug)]
pub struct Global {
    pub doc: Document,
    pub window: Window,
}

#[derive(Debug)]
pub struct CreateOrJoin {
    global: Global,
}

impl CreateOrJoin {
    pub fn new(global: Global) -> JsResult<CreateOrJoin> {
        let doc: &Document = &global.doc;

        let html = doc.get_element_by_id("create_or_join").unwrap();
        html.toggle_attribute("hidden")?;

        let button = doc.get_element_by_id("join_button").unwrap();
        set_event_cb(&button, "click", |e: MouseEvent| {
            console_log!("join_button clicked");

            {
                STATE.lock().unwrap().on_join_start().unwrap();
            }

            Ok(())
        })
        .forget();

        Ok(CreateOrJoin { global })
    }

    pub fn on_join_start(self) -> JsResult<Connecting> {
        let html = self.global.doc.get_element_by_id("create_or_join").unwrap();
        html.toggle_attribute("hidden")?;

        Connecting::new(self.global)
    }
}

#[derive(Debug)]
pub struct Connecting {
    pub global: Global,
    pub ws: WebSocket,
}

impl Connecting {
    pub fn new(global: Global) -> JsResult<Self> {
        let html = global.doc.get_element_by_id("connecting").unwrap();
        html.toggle_attribute("hidden")?;

        let hostname = format!("{}://{}:{}", "ws", "localhost", "5555");
        console_log!("Host: {}", hostname);

        // Set up the websocket
        let ws = WebSocket::new(&hostname)?;
        set_event_cb(&ws, "open", move |_: JsValue| {
            console_log!("WS Connected");

            {
                STATE.lock().unwrap().on_connected().unwrap();
            }

            Ok(())
        })
        .forget();

        Ok(Connecting { global, ws })
    }

    pub fn on_connected(self) -> JsResult<Playing> {
        let html = self.global.doc.get_element_by_id("connecting").unwrap();
        html.toggle_attribute("hidden")?;

        Playing::new(self.global, self.ws)
    }
}

// #[derive(Debug)]
pub struct Playing {
    pub ws: WebSocket,
    pub global: Global,
    pub board: Board,
    pub room_name: String,
    pub is_turn: bool,
    pub pieces_placed: u8,
    pub players: Vec<String>,
    pub hand: Vec<Piece>,
    pub selected_piece: Option<Piece>,
    pub board_div: Element,
    pub chat_div: Element,
    pub players_div: Element,
    pub on_board_click: JsClosure<PointerEvent>,
    pub on_board_move: JsClosure<PointerEvent>,
    pub on_end_turn: JsClosure<PointerEvent>
}

impl Playing {
    pub fn new(global: Global, ws: WebSocket) -> JsResult<Self> {
        // Display the game board:
        let html = global.doc.get_element_by_id("playing").unwrap();
        html.toggle_attribute("hidden")?;
        
        // We have connected so setup the websocket heartbeat:
        // crate::create_heartbeat()?;

        // Handle websocket message:
        set_event_cb(&ws, "message", move |e: MessageEvent| {
            let msg: ServerMessage = serde_json::from_str(&e.data().as_string().unwrap())
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
            crate::on_message(msg)
        })
        .forget();

        // Handle websocket error:
        set_event_cb(&ws, "error", move |e: Event| {
            console_log!("WS Error: {:?}", e);
            Ok(())
        })
        .forget();

        // Handle websocket close:
        set_event_cb(&ws, "close", move |e: Event| {
            console_log!("WS Closed: {:?}", e);
            Ok(())
        })
        .forget();

        let board = Board::new();
        
        let board_div = global.doc.get_element_by_id("board").unwrap();
        let chat_div = global.doc.get_element_by_id("chat").unwrap();
        let players_div = global.doc.get_element_by_id("players").unwrap();
        
        console_log!("Board Div Child: {:?}", board_div.first_child());

        let svg = board_div.get_elements_by_tag_name("svg").item(0).unwrap();
        let on_board_click = set_event_cb(&svg, "click", move |e: PointerEvent| {
            e.prevent_default();
            STATE.lock().unwrap().on_board_click(e.x(), e.y())
        });

        let on_board_move = set_event_cb(&svg, "mousemove", move |e: PointerEvent| {
            e.prevent_default();
            STATE.lock().unwrap().on_board_move(e.x(), e.y())
        });

        let end_turn = global.doc.get_element_by_id("end_turn").unwrap();
        let on_end_turn = set_event_cb(&end_turn, "click", move |e: PointerEvent| {
            e.prevent_default();
            STATE.lock().unwrap().on_end_turn()
        });
        
        console_log!("sending join message");

        let join_message = serde_json::to_string(&ClientMessage::CreateRoom("fisher".to_string())).unwrap();
        ws.send_with_str(&join_message)?;

        Ok(Self {
            ws,
            global,
            board,
            room_name: String::new(),
            is_turn: true,
            pieces_placed: 0,
            players: Vec::new(),
            hand: Vec::new(),
            selected_piece: None,
            board_div,
            chat_div,
            players_div,
            on_board_click,
            on_board_move,
            on_end_turn,
        })
    }

    fn on_joined_room(&mut self, room_name: String, players: Vec<String>, mut hand: Vec<Piece>) -> JsResult<()> {
        hand.sort();
        
        self.board.insert_as_hand(&hand);

        self.room_name = room_name;
        self.players = players;
        self.hand = hand;
        
        self.board.rerender();
        // for piece in

        console_log!("[{}] {:?} pieces, {:?}", self.room_name, self.hand.len(), self.players);

        Ok(())
    }

    fn on_board_click(&mut self, x: i32, y: i32) -> JsResult<()> {
        console_log!("Board Click: ({}, {})", x, y);

        if !self.is_turn {
            console_log!("not your turn");
            return Ok(());
        }

        // The player has clicked and wants to place a piece:
        if let Some(piece) = self.selected_piece {
            console_log!("placing piece: {:?}", piece);
            
            if self.board.world_contains(x, y) {
                // User is trying to place on another tile, don't let them
                console_log!("piece already there");
            } else {
                let in_hand = self.board.world_insert(x, y, piece);
    
                if in_hand {
                    self.hand.push(piece);
                    self.pieces_placed -= 1;
                } else {
                    self.pieces_placed += 1;
                }
    
                self.selected_piece = None;
            }
        } else {
            if let Some((piece, in_hand)) = self.board.remove_piece_at(x, y) {
                console_log!("picked up: {:?}, in hand: {}", piece, in_hand);

                if in_hand {
                    self.hand.remove(self.hand.iter().position(|x| *x == piece).expect("piece not in hand"));
                }

                self.selected_piece = Some(piece);
            } else {
                console_log!("no piece there");
            }
        }

        self.board.rerender();
        console_log!("hand: {:?}", self.hand);

        Ok(())
    }

    fn on_board_move(&mut self, x: i32, y: i32) -> JsResult<()> {
        if let Some(piece) = self.selected_piece {
            if !self.board.world_contains(x, y) {
                self.board.world_render_highlight(x, y, &piece);
            }
        }
        
        Ok(())
    }

    fn on_end_turn(&mut self) -> JsResult<()> {
        console_log!("on_end_turn");

        Ok(())
    }

    pub fn send_ping(&mut self) -> JsResult<()> {
        let msg = serde_json::to_string(&ClientMessage::Ping).unwrap();
        self.ws.send_with_str(&msg)
    }

    pub fn rerender(&mut self) {
        self.board.rerender();
    }
}

// #[derive(Debug)]
pub enum State {
    Empty,
    Connecting(Connecting),
    CreateOrJoin(CreateOrJoin),
    Playing(Playing),
}

impl State {
    transitions!(
        CreateOrJoin => [
            on_join_start() -> Connecting,
        ],
        Connecting => [
            on_connected() -> Playing,
        ],
    );

    methods!(
        Playing => [
            send_ping(),
            on_joined_room(room_name: String, players: Vec<String>, hand: Vec<Piece>),
            on_board_click(x: i32, y: i32),
            on_board_move(x: i32, y: i32),
            on_end_turn(),
        ]
    );
}

unsafe impl Send for State {}
