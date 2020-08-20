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

        // Thanks mkeeter for the following hostname code:
        let location = global.doc.location().expect("Could not get doc location");
        let hostname = location.hostname()?;

        // Pick the port based on the connection type
        let (ws_protocol, ws_port) = if location.protocol()? == "https:" {
            ("wss", 5556)
        } else {
            ("ws", 5555)
        };
        let hostname = format!("{}://{}:{}", ws_protocol, hostname, ws_port);
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
    pub hand: Board,
    pub room_name: String,
    pub is_turn: bool,
    pub active_player: usize,
    pub players: Vec<String>,
    pub disconnected: Vec<usize>,
    // pub hand: Vec<Piece>,
    pub selected_piece: Option<Piece>,
    pub players_div: Element,
    pub board_div: Element,
    pub board_svg: Element,
    pub hand_div: Element,
    pub hand_svg: Element,
    pub on_board_click: JsClosure<PointerEvent>,
    pub on_board_move: JsClosure<PointerEvent>,
    pub on_board_leave: JsClosure<Event>,
    pub on_hand_click: JsClosure<PointerEvent>,
    pub on_hand_move: JsClosure<PointerEvent>,
    pub on_hand_leave: JsClosure<Event>,
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

        let board_div = global.doc.get_element_by_id("board").unwrap();
        // let board_svg = global.doc.get_element_by_id("board_svg").unwrap();

        let hand_div = global.doc.get_element_by_id("hand").unwrap();
        // let hand_svg = global.doc.get_element_by_id("hand_svg").unwrap();

        let players_div = global.doc.get_element_by_id("players").unwrap();

        let board = Board::new(15, 25, &board_div, "board");
        let board_svg = board_div.get_elements_by_tag_name("svg").item(0).unwrap();

        let hand = Board::new(5, 25, &hand_div, "hand");
        let hand_svg = hand_div.get_elements_by_tag_name("svg").item(0).unwrap();

        let on_board_click = set_event_cb(&board_svg, "click", move |e: PointerEvent| {
            e.prevent_default();
            STATE.lock().unwrap().on_board_click(e.x(), e.y())
        });

        let on_board_move = set_event_cb(&board_svg, "mousemove", move |e: PointerEvent| {
            e.prevent_default();
            STATE.lock().unwrap().on_board_move(e.x(), e.y())
        });

        let on_board_leave = set_event_cb(&board_svg, "mouseleave", move |e: Event| {
            e.prevent_default();
            STATE.lock().unwrap().on_board_leave()
        });

        let on_hand_click = set_event_cb(&hand_svg, "click", move |e: PointerEvent| {
            e.prevent_default();
            STATE.lock().unwrap().on_hand_click(e.x(), e.y())
        });

        let on_hand_move = set_event_cb(&hand_svg, "mousemove", move |e: PointerEvent| {
            e.prevent_default();
            STATE.lock().unwrap().on_hand_move(e.x(), e.y())
        });

        let on_hand_leave = set_event_cb(&hand_svg, "mouseleave", move |e: Event| {
            e.prevent_default();
            STATE.lock().unwrap().on_hand_leave()
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

        let mut this = Self {
            ws,
            global,
            board,
            hand,
            room_name: String::new(),
            is_turn,
            active_player: 0,
            players: Vec::new(),
            disconnected: Vec::new(),
            selected_piece: None,
            board_div,
            board_svg,
            hand_div,
            hand_svg,
            players_div,
            on_board_click,
            on_board_move,
            on_board_leave,
            on_hand_click,
            on_hand_move,
            on_hand_leave,
            on_end_turn,
            on_window_resize,
        };

        this.update_players();

        Ok(this)
    }

    fn on_joined_room(
        &mut self,
        room_name: String,
        players: Vec<String>,
        mut hand: Vec<Piece>,
        pieces_remaining: usize,
        board: BTreeMap<Coord, Piece>,
    ) -> JsResult<()> {
        hand.sort();

        self.global
            .doc
            .get_element_by_id("room")
            .unwrap()
            .set_inner_html(&room_name);

        self.global
            .doc
            .get_element_by_id("pieces_remaining")
            .unwrap()
            .set_inner_html(&format!("{}", pieces_remaining));

        *self.board.grid_mut() = board;
        self.room_name = room_name;
        self.players = players;

        self.hand.insert_as_hand(&hand);

        self.board.rerender();
        self.hand.rerender();
        self.update_players();

        console_log!(
            "[{}] {:?} pieces, {:?}",
            self.room_name,
            self.hand.grid().len(),
            self.players
        );

        Ok(())
    }

    fn update_players(&mut self) {
        let mut inner_html = String::new();

        for (i, player) in self.players.iter().enumerate() {
            if i == self.active_player {
                inner_html.push_str(&format!(
                    "<tr><td class=\"active_player\">{}</td></tr>",
                    player
                ));
            } else if self.disconnected.contains(&i) {
                inner_html.push_str(&format!(
                    "<tr><td class=\"disconnected\">{}</td></tr>",
                    player
                ));
            } else {
                inner_html.push_str(&format!("<tr><td>{}</td></tr>", player));
            }
        }

        inner_html = format!("<table>{}</table>", inner_html);
        self.players_div.set_inner_html(&inner_html);
    }

    fn on_board_click(&mut self, x: i32, y: i32) -> JsResult<()> {
        let rect = self.board_svg.get_bounding_client_rect();
        let x = x - rect.x() as i32;
        let y = y - rect.y() as i32;

        let coord = self.board.world_to_grid(x, y);
        console_log!("Board Click: ({}, {})", coord.0, coord.1);

        // The player has clicked and wants to place a piece:
        if let Some(piece) = self.selected_piece {
            console_log!("placing piece: {:?}", piece);

            if self.board.contains(coord) {
                // user is trying to place on another tile, don't let them
                console_log!("piece already there");
            } else if !self.is_turn {
                self.global.window.alert_with_message(
                    "You cannot place on the board when it is not your turn.",
                )?;
            } else {
                // Player is placing on board and it's their turn, place
                // the piece and send the message.
                let _ = self.board.world_insert(x, y, piece);
                self.send_message(ClientMessage::Place(coord, piece))?;
                self.selected_piece = None;
            }
        } else {
            // Player wants to pickup a piece
            if self.is_turn {
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
        let rect = self.board_svg.get_bounding_client_rect();
        let x = x - rect.x() as i32;
        let y = y - rect.y() as i32;

        if let Some(piece) = self.selected_piece {
            if !self.board.world_contains(x, y) {
                self.board.world_render_highlight(x, y, &piece);
            }
        }

        Ok(())
    }

    fn on_board_leave(&mut self) -> JsResult<()> {
        self.board.remove_highlight();
        Ok(())
    }

    fn on_hand_click(&mut self, x: i32, y: i32) -> JsResult<()> {
        let rect = self.hand_svg.get_bounding_client_rect();
        let x = x - rect.x() as i32;
        let y = y - rect.y() as i32;

        let coord = self.board.world_to_grid(x, y);
        console_log!("Hand Click: ({}, {})", coord.0, coord.1);

        // The player has clicked and wants to place a piece in their hand:
        if let Some(piece) = self.selected_piece {
            console_log!("placing piece: {:?}", piece);
            if self.board.contains(coord) {
                // user is trying to place on another tile, don't let them
                console_log!("piece already there");
            } else {
                // Player is placing on board and it's in their hand, always succeed
                let _ = self.hand.world_insert(x, y, piece);
                self.selected_piece = None;
            }
        } else if let Some(piece) = self.hand.grid_remove(coord) {
            // Player wants to pickup a piece in their hand
            self.selected_piece = Some(piece);
        } else {
            console_log!("no piece there");
        }

        console_log!("Hand: {:?}", self.hand.grid());

        self.hand.rerender();

        Ok(())
    }

    fn on_hand_move(&mut self, x: i32, y: i32) -> JsResult<()> {
        let rect = self.hand_svg.get_bounding_client_rect();
        let x = x - rect.x() as i32;
        let y = y - rect.y() as i32;

        if let Some(piece) = self.selected_piece {
            if !self.hand.world_contains(x, y) {
                self.hand.world_render_highlight(x, y, &piece);
            }
        }

        Ok(())
    }

    fn on_hand_leave(&mut self) -> JsResult<()> {
        self.hand.remove_highlight();
        Ok(())
    }

    fn on_draw_piece(&mut self, piece: Piece) -> JsResult<()> {
        self.hand.insert_into_hand(piece);
        self.hand.rerender();

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
        next_player: usize,
        pieces_remaining: usize,
        board: BTreeMap<Coord, Piece>,
    ) -> JsResult<()> {
        console_log!("Turn Finished for {}", ending_player);
        console_log!("{} drew? {}", ending_player, ending_drew);
        console_log!("{} is the next player", self.players[next_player]);
        console_log!("There are {} pieces remaining", pieces_remaining);
        console_log!("board: {:?}", board);

        self.active_player = next_player;

        self.global
            .doc
            .get_element_by_id("current_player")
            .unwrap()
            .set_inner_html(&format!("{}", self.players[next_player]));

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

        self.update_players();
        self.rerender();

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

    pub fn on_player_disconnected(&mut self, idx: usize) -> JsResult<()> {
        console_log!("on_player_disconnected");
        self.disconnected.push(idx);

        self.update_players();

        Ok(())
    }

    pub fn on_current_player(&mut self, idx: usize) -> JsResult<()> {
        self.global
            .doc
            .get_element_by_id("current_player")
            .unwrap()
            .set_inner_html(&format!("{}", self.players[idx]));

        self.global
            .doc
            .get_element_by_id("last_player")
            .unwrap()
            .set_inner_html("N/A");

        self.active_player = idx;
        self.update_players();

        Ok(())
    }

    pub fn on_player_reconnected(&mut self, idx: usize) -> JsResult<()> {
        for i in 0..self.disconnected.len() {
            if self.disconnected[i] == idx {
                self.disconnected.swap_remove(i);
                break;
            }
        }

        self.update_players();

        Ok(())
    }

    pub fn on_player_won(&mut self, name: String) -> JsResult<()> {
        self.global
            .window
            .alert_with_message(&format!("{} won the game! Refresh to play again!", name))
    }

    pub fn on_window_resize(&mut self) -> JsResult<()> {
        // console_log!("resize");
        // self.board.resize();
        // self.hand.resize();
        // Ok(())
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
        self.hand.rerender();
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
            on_joined_room(room_name: String, players: Vec<String>, hand: Vec<Piece>, pieces_left: usize, board: BTreeMap<Coord, Piece>),
            on_board_click(x: i32, y: i32),
            on_board_move(x: i32, y: i32),
            on_hand_click(x: i32, y: i32),
            on_hand_move(x: i32, y: i32),
            on_board_leave(),
            on_hand_leave(),
            on_turn_start(),
            on_turn_finished(ending_player: String, ending_drew: bool, next_player: usize, pieces_remaining: usize, board: BTreeMap<Coord, Piece>),
            on_player_joined(name: String),
            on_draw_piece(piece: Piece),
            on_piece_place(coord: Coord, piece: Piece),
            on_pickup(coord: Coord, piece: Piece),
            on_player_disconnected(idx: usize),
            on_player_reconnected(idx: usize),
            on_current_player(idx: usize),
            on_player_won(name: String),
            on_invalid_board(),
            on_end_turn(),
            on_end_turn_valid(),
            on_window_resize(),
        ]
    );
}

unsafe impl Send for State {}
