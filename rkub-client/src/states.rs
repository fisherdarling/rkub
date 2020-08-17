use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    Document, Element, Event, HtmlInputElement, MessageEvent, MouseEvent, PointerEvent, WebSocket,
    Window,
};

use crate::board::Board;
use crate::STATE;
use crate::{console_log, set_event_cb};
use rkub_common::{ClientMessage, Coord, Game, Piece, ServerMessage};

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
    join_cb: JsClosure<MouseEvent>,
    create_cb: JsClosure<MouseEvent>,
}

impl CreateOrJoin {
    pub fn new(global: Global) -> JsResult<CreateOrJoin> {
        let doc: &Document = &global.doc;

        let html = doc.get_element_by_id("create_or_join").unwrap();
        html.toggle_attribute("hidden")?;

        let join_button = doc.get_element_by_id("join_room").unwrap();
        let join_cb = set_event_cb(&join_button, "click", |_e: MouseEvent| {
            console_log!("join_button clicked");

            let window = web_sys::window().unwrap();
            let room_input: HtmlInputElement = window
                .document()
                .unwrap()
                .get_element_by_id("input_room")
                .unwrap()
                .dyn_into()?;

            let name_input: HtmlInputElement = window
                .document()
                .unwrap()
                .get_element_by_id("input_name")
                .unwrap()
                .dyn_into()?;

            let room_name = room_input.value();
            let player_name = name_input.value();

            if room_name.is_empty() {
                window.alert_with_message("Please enter a valid room ID")?;
            } else {
                if player_name.is_empty() {
                    window.alert_with_message("Please enter name")?;
                } else {
                    STATE
                        .lock()
                        .unwrap()
                        .on_join_start(player_name, room_name)?;
                }
            }

            Ok(())
        });

        let create_button = doc.get_element_by_id("create_room").unwrap();
        let create_cb = set_event_cb(&create_button, "click", |_e: MouseEvent| {
            console_log!("create_button clicked");

            let window = web_sys::window().unwrap();

            let name_input: HtmlInputElement = window
                .document()
                .unwrap()
                .get_element_by_id("input_name")
                .unwrap()
                .dyn_into()?;

            let player_name = name_input.value();
            if player_name.is_empty() {
                window.alert_with_message("please enter a name")?;
            } else {
                STATE.lock().unwrap().on_create_start(player_name)?;
            }

            Ok(())
        });

        Ok(CreateOrJoin {
            global,
            join_cb,
            create_cb,
        })
    }

    pub fn on_join_start(self, player_name: String, room_name: String) -> JsResult<Connecting> {
        let html = self.global.doc.get_element_by_id("create_or_join").unwrap();
        html.set_attribute("style", "display:none")?;
        // html.

        Connecting::new(self.global, player_name, Some(room_name))
    }

    pub fn on_create_start(self, player_name: String) -> JsResult<Connecting> {
        let html = self.global.doc.get_element_by_id("create_or_join").unwrap();
        html.set_attribute("style", "display:none")?;

        Connecting::new(self.global, player_name, None)
    }
}

#[derive(Debug)]
pub struct Connecting {
    pub global: Global,
    pub ws: WebSocket,
    pub player_name: String,
    pub room_name: Option<String>,
}

impl Connecting {
    pub fn new(global: Global, player_name: String, room_name: Option<String>) -> JsResult<Self> {
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

        Ok(Connecting {
            global,
            ws,
            player_name,
            room_name,
        })
    }

    pub fn on_connected(self) -> JsResult<Playing> {
        let html = self.global.doc.get_element_by_id("connecting").unwrap();
        html.toggle_attribute("hidden")?;

        Playing::new(self.global, self.ws, self.player_name, self.room_name)
    }
}

// #[derive(Debug)]
pub struct Playing {
    pub ws: WebSocket,
    pub global: Global,
    pub board: Board,
    pub room_name: String,
    pub is_turn: bool,
    // pub played_pieces: BTreeMap<Coord, Piece>,
    pub players: Vec<String>,
    pub hand: Vec<Piece>,
    pub selected_piece: Option<Piece>,
    pub board_div: Element,
    pub players_div: Element,
    pub on_board_click: JsClosure<PointerEvent>,
    pub on_board_move: JsClosure<PointerEvent>,
    pub on_end_turn: JsClosure<PointerEvent>,
    pub on_window_resize: JsClosure<Event>,
}

impl Playing {
    pub fn new(
        global: Global,
        ws: WebSocket,
        player_name: String,
        room_name: Option<String>,
    ) -> JsResult<Self> {
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

        let window = &global.window;
        let on_window_resize = set_event_cb(window, "resize", move |e: Event| {
            e.prevent_default();
            STATE.lock().unwrap().on_window_resize()
        });

        console_log!("sending join message");

        let mut is_turn = false;
        if let Some(room_name) = room_name {
            let join_message =
                serde_json::to_string(&ClientMessage::JoinRoom(player_name, room_name)).unwrap();
            ws.send_with_str(&join_message)?;
        } else {
            let join_message =
                serde_json::to_string(&ClientMessage::CreateRoom(player_name)).unwrap();
            ws.send_with_str(&join_message)?;
            console_log!("created room");

            is_turn = true;
        }

        console_log!("is turn: {}", is_turn);

        let this = Self {
            ws,
            global,
            board,
            room_name: String::new(),
            is_turn,
            // played_pieces: BTreeMap::new(),
            players: Vec::new(),
            hand: Vec::new(),
            selected_piece: None,
            board_div,
            players_div,
            on_board_click,
            on_board_move,
            on_end_turn,
            on_window_resize,
        };

        Ok(this)
    }

    fn on_joined_room(
        &mut self,
        room_name: String,
        players: Vec<String>,
        mut hand: Vec<Piece>,
    ) -> JsResult<()> {
        hand.sort();

        self.global
            .doc
            .get_element_by_id("room")
            .unwrap()
            .set_inner_html(&room_name);

        self.board.insert_as_hand(&hand);

        self.room_name = room_name;
        self.players = players;
        self.hand = hand;

        self.board.rerender();
        self.update_players();

        console_log!(
            "[{}] {:?} pieces, {:?}",
            self.room_name,
            self.hand.len(),
            self.players
        );

        Ok(())
    }

    fn update_players(&mut self) {
        let mut inner_html = String::new();

        for player in &self.players {
            inner_html.push_str(&format!("<tr><td>{}</td></tr>", player));
        }

        inner_html = format!("<table>{}</table>", inner_html);
        self.players_div.set_inner_html(&inner_html);
    }

    fn on_board_click(&mut self, x: i32, y: i32) -> JsResult<()> {
        let rect = self.board_div.get_bounding_client_rect();
        let x = x - rect.x() as i32;
        let y = y - rect.y() as i32;

        console_log!("Board Click: ({}, {})", x, y);

        // The player has clicked and wants to place a piece:
        if let Some(piece) = self.selected_piece {
            console_log!("placing piece: {:?}", piece);

            if self.board.world_contains(x, y) {
                // User is trying to place on another tile, don't let them
                console_log!("piece already there");
            } else {
                let coord = self.board.world_to_grid(x, y);

                console_log!(
                    "in_hand?: ({}, {}) => {}",
                    coord.0,
                    coord.1,
                    self.board.in_hand(coord.0, coord.1)
                );

                // Player is placing in hand, always succeeds:
                if self.board.in_hand(coord.0, coord.1) {
                    let _ = self.board.world_insert(x, y, piece);
                    self.hand.push(piece);
                } else if !self.is_turn {
                    // Player is placing on board and it's not their turn
                    self.global.window.alert_with_message(
                        "You cannot place on the board when it is not your turn.",
                    )?;

                    return Ok(());
                } else {
                    // Player is placing on board and it's their turn, place
                    // the piece and send the message.
                    let _ = self.board.world_insert(x, y, piece);
                    self.send_message(ClientMessage::Place(coord, piece))?;
                }

                self.selected_piece = None;
            }
        } else {
            let coord = self.board.world_to_grid(x, y);

            console_log!(
                "in_hand?: ({}, {}) => {}",
                coord.0,
                coord.1,
                self.board.in_hand(coord.0, coord.1)
            );

            // A pickup in the player's hand always succeeds
            if self.board.in_hand(coord.0, coord.1) {
                if let Some(piece) = self.board.grid_remove(coord) {
                    self.hand.remove(
                        self.hand
                            .iter()
                            .position(|x| *x == piece)
                            .expect("piece not in hand"),
                    );
                    self.selected_piece = Some(piece);
                }
            } else if self.is_turn {
                if let Some(piece) = self.board.grid_remove(coord) {
                    // Tell the server we picked up the piece.
                    self.send_message(ClientMessage::Pickup(coord, piece))?;
                    self.selected_piece = Some(piece);
                } else {
                    console_log!("no piece there");
                }
            }
        }

        self.board.rerender();

        Ok(())
    }

    fn on_board_move(&mut self, x: i32, y: i32) -> JsResult<()> {
        let rect = self.board_div.get_bounding_client_rect();
        let x = x - rect.x() as i32;
        let y = y - rect.y() as i32;

        if let Some(piece) = self.selected_piece {
            if !self.board.world_contains(x, y) {
                self.board.world_render_highlight(x, y, &piece);
            }
        }

        Ok(())
    }

    fn on_draw_piece(&mut self, piece: Piece) -> JsResult<()> {
        self.hand.push(piece);
        self.board.insert_into_hand(piece);
        self.board.rerender();

        Ok(())
    }

    fn on_invalid_board(&mut self) -> JsResult<()> {
        self.global
            .window
            .alert_with_message("The board is in an invalid state")
    }

    fn on_piece_place(&mut self, coord: Coord, piece: Piece) -> JsResult<()> {
        if !self.is_turn {
            console_log!("place: {:?} {:?}", coord, piece);

            if let Some(old) = self.board.grid_insert(coord, piece) {
                if !self.is_turn {
                    console_log!("[ERROR] overwriting piece: {:?}", old);
                }
            }

            self.board.rerender();
        }

        Ok(())
    }

    fn on_pickup(&mut self, coord: Coord, piece: Piece) -> JsResult<()> {
        if !self.is_turn {
            console_log!("pickup: {:?} {:?}", coord, piece);

            if let Some(removed) = self.board.grid_remove(coord) {
                console_log!("{:?}: removed {:?}, expected {:?}", coord, removed, piece);
            }

            self.board.rerender();
        }

        Ok(())
    }

    fn on_end_turn(&mut self) -> JsResult<()> {
        console_log!("on_end_turn");
        self.send_message(ClientMessage::EndTurn)
    }

    fn on_turn_finished(
        &mut self,
        ending_player: String,
        ending_drew: bool,
        next_player: String,
        pieces_remaining: usize,
        board: BTreeMap<Coord, Piece>,
    ) -> JsResult<()> {
        console_log!("Turn Finished for {}", ending_player);
        console_log!("{} drew? {}", ending_player, ending_drew);
        console_log!("{} is the next player", next_player);
        console_log!("There are {} pieces remaining", pieces_remaining);
        console_log!("board: {:?}", board);

        self.global
            .doc
            .get_element_by_id("current_player")
            .unwrap()
            .set_inner_html(&format!("{}", next_player));

        self.global
            .doc
            .get_element_by_id("last_player")
            .unwrap()
            .set_inner_html(&format!("{}", ending_player));

        self.global
            .doc
            .get_element_by_id("pieces_remaining")
            .unwrap()
            .set_inner_html(&format!("{}", pieces_remaining));

        Ok(())
    }

    pub fn on_turn_start(&mut self) -> JsResult<()> {
        self.is_turn = true;
        Ok(())
    }

    pub fn on_end_turn_valid(&mut self) -> JsResult<()> {
        self.is_turn = false;
        Ok(())
    }

    pub fn on_player_joined(&mut self, name: String) -> JsResult<()> {
        console_log!("{} joined", name);

        self.players.push(name);
        self.update_players();

        Ok(())
    }

    // pub fn on_game_won(&mut self, winner: String) -> JsResult<()> {
    //     // self
    // }

    pub fn on_window_resize(&mut self) -> JsResult<()> {
        console_log!("resize");
        self.board.resize();
        Ok(())
    }

    fn send_message(&mut self, msg: ClientMessage) -> JsResult<()> {
        let msg = serde_json::to_string(&msg).unwrap();
        self.ws.send_with_str(&msg)
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
            on_join_start(name: String, room: String) -> Connecting,
            on_create_start(name: String) -> Connecting,
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
            on_turn_start(),
            on_turn_finished(ending_player: String, ending_drew: bool, next_player: String, pieces_remaining: usize, board: BTreeMap<Coord, Piece>),
            on_player_joined(name: String),
            on_draw_piece(piece: Piece),
            on_piece_place(coord: Coord, piece: Piece),
            on_pickup(coord: Coord, piece: Piece),
            // on_game_won(winner: String),
            on_invalid_board(),
            on_end_turn(),
            on_end_turn_valid(),
            on_window_resize(),
        ]
    );
}

unsafe impl Send for State {}
