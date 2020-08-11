use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    CreateRoom(String),
    JoinRoom(String, String),
    Ready(String),
    Ping,
    Close,
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    JoinedRoom {
        room_name: String,
        players: Vec<String>,
        hand: Vec<Piece>,
    },
    StartGame,
    PlayerJoined(String),
    GameAlreadyStarted(String),
    Pong,
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Color {
    Red = 0,
    Blue = 1,
    Yellow = 2,
    Black = 3,
    Joker = 4,
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let string = match self {
            Color::Red => "red",
            Color::Blue => "blue",
            Color::Yellow => "yellow",
            Color::Black => "black",
            Color::Joker => "n/a",
        };

        write!(f, "{}", string)
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    pub color: Color,
    pub num: u8,
}

impl Piece {
    pub fn new(color: Color, num: u8) -> Self {
        Self { color, num }
    }

    pub fn joker() -> Self {
        Piece::new(Color::Joker, std::u8::MAX)
    }
}

#[derive(Default, Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct Group {
    id: usize,
    pieces: Vec<Piece>,
}

impl Group {
    pub fn first_non_joker(&self) -> usize {
        self.pieces
            .iter()
            .enumerate()
            .find(|(_, p)| p.color != Color::Joker)
            .map(|(i, _)| i)
            .unwrap()
    }

    pub fn is_valid(&self) -> bool {
        if self.pieces.len() < 3 {
            return false;
        }

        self.is_valid_run() || self.is_valid_combo()
    }

    pub fn is_valid_run(&self) -> bool {
        let first_idx = self.first_non_joker();

        if first_idx == self.pieces.len() - 1 {
            return true;
        }

        let first_piece = self.pieces[first_idx];

        let check_color = first_piece.color;
        let mut start = first_piece.num;

        for Piece { color, num } in &self.pieces[first_idx + 1..] {
            if *color == Color::Joker {
                start += 1;
                continue;
            }

            if *color != check_color || *num != start + 1 {
                return false;
            }

            start += 1;
        }

        true
    }

    pub fn is_valid_combo(&self) -> bool {
        let mut seen = [false; 4];
        let first_idx = self.first_non_joker();
        let check_num = self.pieces[0].num;

        if first_idx == self.pieces.len() - 1 {
            return true;
        }

        for Piece { color, num } in &self.pieces[first_idx + 1..] {
            if *color == Color::Joker {
                continue;
            }

            if seen[*color as usize] || *num != check_num {
                return false;
            }

            seen[*color as usize] = true;
        }

        true
    }
}

#[derive(Default, Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Game {
    next_group: usize,
    groups: HashMap<usize, Group>,
    remaining_pieces: Vec<Piece>,
}

impl Game {
    pub fn new() -> Self {
        let mut game = Self {
            next_group: 1,
            groups: HashMap::new(),
            remaining_pieces: Game::create_pieces(),
        };

        game.shuffle();

        game
    }

    pub fn shuffle(&mut self) {
        use rand::seq::SliceRandom;

        self.remaining_pieces.shuffle(&mut rand::thread_rng());
    }

    pub fn create_pieces() -> Vec<Piece> {
        let mut pieces = Vec::new();

        for i in 1..=13 {
            pieces.push(Piece::new(Color::Red, i));
            pieces.push(Piece::new(Color::Blue, i));
            pieces.push(Piece::new(Color::Yellow, i));
            pieces.push(Piece::new(Color::Black, i));
        }

        pieces
    }

    pub fn deal(&mut self, count: usize) -> Vec<Piece> {
        if count > self.remaining_pieces.len() {
            std::mem::take(&mut self.remaining_pieces)
        } else {
            self.remaining_pieces
                .split_off(self.remaining_pieces.len() - count)
        }
    }

    pub fn deal_piece(&mut self) -> Option<Piece> {
        self.remaining_pieces.pop()
    }
}
