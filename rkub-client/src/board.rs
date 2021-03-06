use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use wasm_svg_graphics::prelude::*;

use crate::svg::AsSVG;
use rkub_common::{Color, Coord, Piece};

// const CELL_WIDTH: usize = 40;
// const CELL_HEIGHT: usize = 50;

// const self.cols: i32 = 25;
// const self.rows: i32 = 20;

pub struct Board {
    grid: BTreeMap<Coord, Piece>,
    // played_pieces: Vec<LocatedPiece>,
    // hand_pieces: Vec<LocatedPiece>,
    renderer: SVGRenderer,
    root_name: &'static str,
    rows: i32,
    cols: i32,
    cell_width: i32,
    cell_height: i32,
    last_highlight: Option<Coord>,
}

impl Board {
    pub fn new(
        rows: i32,
        cols: i32,
        root_element: &web_sys::Element,
        root_name: &'static str,
    ) -> Self {
        let width = root_element.client_width();
        let height = root_element.client_height();
        let renderer = SVGRenderer::new(root_name).expect("Unable to create renderer");
        renderer.adjust_viewbox(0, 0, width, height);

        Self {
            grid: BTreeMap::new(),
            renderer,
            root_name,
            rows,
            cols,
            cell_width: width / cols,
            cell_height: height / rows,
            last_highlight: None,
        }
    }

    pub fn resize(&mut self) {
        let document = web_sys::window().unwrap().document().unwrap();
        let root: web_sys::HtmlElement = document
            .get_element_by_id(self.root_name)
            .unwrap()
            .dyn_into()
            .unwrap();

        let width = root.client_width() as i32;
        let height = root.client_height() as i32;

        self.cell_width = width / self.cols;
        self.cell_height = height / self.rows;

        crate::console_log!("new viewbox: ({}, {})", width, height);

        self.renderer.adjust_viewbox(0, 0, width, height);
        self.rerender();
    }

    pub fn grid(&self) -> &BTreeMap<Coord, Piece> {
        &self.grid
    }

    pub fn grid_mut(&mut self) -> &mut BTreeMap<Coord, Piece> {
        &mut self.grid
    }

    pub fn remove_highlight(&mut self) {
        self.last_highlight = None;
        self.rerender();
    }

    pub fn played_grid(&self) -> BTreeMap<Coord, Piece> {
        self.grid
            .iter()
            .filter(|(Coord(_x, y), _)| *y < self.rows - 5)
            .map(|(Coord(x, y), p)| (Coord(*x, *y), *p))
            .collect()
    }

    pub fn render(&mut self) {
        for (Coord(grid_x, grid_y), piece) in self.grid.iter() {
            self.renderer.render(
                piece.as_svg(self.cell_width, self.cell_height),
                (
                    (grid_x * self.cell_width) as f32,
                    (grid_y * self.cell_height) as f32,
                ),
            );
        }
    }

    pub fn rerender(&mut self) {
        self.renderer.clear();
        self.render();
    }

    pub fn render_pieces(&mut self, pieces: &[Piece]) {
        let mut pieces = pieces.iter();
        let cols = 4;
        let rows = pieces.len() / cols + 1;

        for col in 0..cols {
            for row in 0..rows {
                if let Some(piece) = pieces.next() {
                    let svg = piece.as_svg(self.cell_width, self.cell_height);

                    self.renderer.render(
                        svg,
                        (
                            (col * self.cell_width as usize) as f32,
                            (row * self.cell_height as usize) as f32,
                        ),
                    );
                } else {
                    return;
                }
            }
        }
    }

    pub fn contains(&self, coord: Coord) -> bool {
        self.grid.contains_key(&coord)
    }

    pub fn world_contains(&self, world_x: i32, world_y: i32) -> bool {
        let grid_x = world_x / self.cell_width;
        let grid_y = world_y / self.cell_height;

        self.grid.contains_key(&Coord(grid_x, grid_y))
    }

    pub fn world_to_grid(&self, world_x: i32, world_y: i32) -> Coord {
        Coord(world_x / self.cell_width, world_y / self.cell_height)
    }

    pub fn world_render_highlight(&mut self, world_x: i32, world_y: i32, piece: &Piece) {
        let coord = self.world_to_grid(world_x, world_y);

        if self.last_highlight != Some(coord) {
            let background = SVGElem::new(Tag::Rect)
                .set(Attr::Fill, "lightgrey")
                .set(Attr::Width, self.cell_width)
                .set(Attr::Height, self.cell_height)
                .set(Attr::X, 0)
                .set(Attr::Y, 0);

            let num = SVGElem::new(Tag::Text)
                .set(Attr::Fill, piece.color)
                .set(Attr::Transform, "scale(1, 2)")
                .set(Attr::X, self.cell_width / 2)
                .set(Attr::Y, self.cell_height / 4)
                .set(Attr::DominantBaseline, "central")
                .set(Attr::TextAnchor, "middle")
                .set(Attr::Class, "piece_text")
                .set(Attr::TextLength, self.cell_width - 5)
                .set(Attr::LengthAdjust, "spacingAndGlyphs")
                .set_inner(&piece.num.to_string());

            let piece = SVGElem::new(Tag::G).append(background).append(num);

            self.rerender();
            self.renderer.render(
                piece,
                (
                    (coord.0 * self.cell_width) as f32,
                    (coord.1 * self.cell_height) as f32,
                ),
            );

            self.last_highlight = Some(coord);
        }
    }

    pub fn remove_piece_at(&mut self, world_x: i32, world_y: i32) -> Option<Piece> {
        let coord = self.world_to_grid(world_x, world_y);
        self.grid.remove(&coord)
    }

    pub fn grid_remove(&mut self, coord: Coord) -> Option<Piece> {
        crate::console_log!("grid_remove: {:?}, {:?}", coord, self.grid.get(&coord));
        self.grid.remove(&coord)
    }

    pub fn world_insert(&mut self, world_x: i32, world_y: i32, piece: Piece) -> Option<Piece> {
        let coord = self.world_to_grid(world_x, world_y);
        self.grid.insert(coord, piece)
    }

    pub fn grid_insert(&mut self, coord: Coord, piece: Piece) -> Option<Piece> {
        self.grid.insert(coord, piece)
    }

    pub fn insert_as_hand(&mut self, pieces: &[Piece]) {
        let mut red = pieces.iter().filter(|p| p.color == Color::Red);
        let mut blue = pieces.iter().filter(|p| p.color == Color::Blue);
        let mut yellow = pieces.iter().filter(|p| p.color == Color::Yellow);
        let mut black = pieces
            .iter()
            .filter(|p| p.color == Color::Black || p.color == Color::Joker);

        for x in 0..self.cols - 1 {
            if let Some(&p) = red.next() {
                self.grid.insert(Coord(x, 0), p);
            }

            if let Some(&p) = blue.next() {
                self.grid.insert(Coord(x, 1), p);
            }

            if let Some(&p) = yellow.next() {
                self.grid.insert(Coord(x, 2), p);
            }

            if let Some(&p) = black.next() {
                self.grid.insert(Coord(x, 3), p);
            }
        }
    }

    pub fn insert_into_hand(&mut self, piece: Piece) {
        let y = match piece.color {
            Color::Red => 0,
            Color::Blue => 1,
            Color::Yellow => 2,
            Color::Black | Color::Joker => 3,
        };

        for x in 0..self.cols - 1 {
            if !self.grid.contains_key(&Coord(x, y)) {
                self.grid.insert(Coord(x, y), piece);
                break;
            }
        }
    }
}

#[derive(Debug)]
pub struct LocatedPiece {
    pub x: f32,
    pub y: f32,
    pub piece: Piece,
}

impl AsSVG for Piece {
    fn as_svg(&self, width: i32, height: i32) -> SVGElem {
        let color = self.color.to_string();
        let number = self.num.to_string();

        let background = SVGElem::new(Tag::Rect)
            .set(Attr::Class, "piece_tile")
            .set(Attr::Width, width)
            .set(Attr::Height, height)
            .set(Attr::X, 0)
            .set(Attr::Y, 0);

        let num = SVGElem::new(Tag::Text)
            .set(Attr::Fill, color)
            .set(Attr::Transform, "scale(1, 1.5)")
            .set(Attr::X, width / 2)
            .set(Attr::Y, height / 3)
            .set(Attr::DominantBaseline, "central")
            .set(Attr::TextAnchor, "middle")
            .set(Attr::Class, "piece_text")
            .set(Attr::TextLength, width - (width / 5))
            .set(Attr::LengthAdjust, "spacingAndGlyphs")
            .set_inner(&number);

        let piece = SVGElem::new(Tag::G).append(background).append(num);

        piece
    }
}
