use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    CreateRoom(String),
    JoinRoom(String, String),
    Ready(String),
    PlayedPieces(BTreeMap<(i32, i32), Piece>, Vec<Piece>),
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
    DrawPiece(Piece),
    TurnFinished {
        ending_player: String,
        ending_drew: bool,
        next_player: String,
        pieces_remaining: usize,
        board: BTreeMap<(i32, i32), Piece>,
    },
    InvalidPlay(BTreeMap<(i32, i32), Piece>),
    StartTurn,
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

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
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

impl fmt::Debug for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.color, self.num)
    }
}

#[derive(Default, Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct Group(Vec<Piece>);

impl Group {
    pub fn first_non_joker(&self) -> usize {
        self.0
            .iter()
            .enumerate()
            .find(|(_, p)| p.color != Color::Joker)
            .map(|(i, _)| i)
            .unwrap()
    }

    pub fn is_valid(&self) -> bool {
        if self.0.len() < 3 {
            return false;
        }

        self.is_valid_run() || self.is_valid_combo()
    }

    pub fn is_valid_run(&self) -> bool {
        let first_idx = self.first_non_joker();

        if first_idx == self.0.len() - 1 {
            return true;
        }

        let first_piece = self.0[first_idx];

        let check_color = first_piece.color;
        let mut start = first_piece.num;

        for Piece { color, num } in &self.0[first_idx + 1..] {
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
        let check_num = self.0[0].num;

        if first_idx == self.0.len() - 1 {
            return true;
        }

        for Piece { color, num } in &self.0[first_idx + 1..] {
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
    grid: BTreeMap<(i32, i32), Piece>,
    remaining_pieces: Vec<Piece>,
}

impl Game {
    pub fn new() -> Self {
        let mut game = Self {
            grid: BTreeMap::new(),
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

    pub fn remaining_pieces(&self) -> &[Piece] {
        &self.remaining_pieces
    }

    pub fn board(&self) -> &BTreeMap<(i32, i32), Piece> {
        &self.grid
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

    pub fn set_board(&mut self, grid: BTreeMap<(i32, i32), Piece>) {
        self.grid = grid;
    }

    pub fn is_valid_board(grid: &BTreeMap<(i32, i32), Piece>) -> (bool, Vec<Group>) {
        let mut current_group: Option<Group> = None;
        let mut groups: Vec<Group> = Vec::new();

        let min_x = grid.iter().map(|(k, _)| k.0).min().unwrap_or_default();
        let min_y = grid.iter().map(|(k, _)| k.1).min().unwrap_or_default();

        let max_x = grid.iter().map(|(k, _)| k.0).max().unwrap_or_default();
        let max_y = grid.iter().map(|(k, _)| k.1).max().unwrap_or_default();

        println!("({}, {}) ({}, {})", min_x, min_y, max_x, max_y);

        for y in min_y..=max_y {
            if let Some(group) = current_group.take() {
                groups.push(group);
            }

            for x in min_x..=max_x {
                if let Some(piece) = grid.get(&(x, y)) {
                    current_group
                        .get_or_insert(Group(Vec::new()))
                        .0
                        .push(*piece);
                } else if let Some(group) = current_group.take() {
                    groups.push(group);
                }
            }
        }

        if let Some(group) = current_group {
            groups.push(group);
        }

        let is_valid = groups.iter().all(Group::is_valid);

        (is_valid, groups)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_board_valid() {
        let mut grid = BTreeMap::new();

        grid.insert((10, 8), Piece::new(Color::Yellow, 2));
        grid.insert((11, 8), Piece::new(Color::Yellow, 3));
        grid.insert((12, 8), Piece::new(Color::Yellow, 4));

        let (is_valid, groups) = Game::is_valid_board(&grid);

        println!("{:?}", groups);

        assert!(is_valid);
        assert_eq!(
            groups,
            &[Group(vec![
                Piece::new(Color::Yellow, 2),
                Piece::new(Color::Yellow, 3),
                Piece::new(Color::Yellow, 4)
            ])]
        );
    }
}
