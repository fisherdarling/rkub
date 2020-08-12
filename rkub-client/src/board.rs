use wasm_svg_graphics::prelude::*;
use std::collections::BTreeMap;

use crate::svg::AsSVG;
use rkub_common::{Color, Piece, Game};

// const CELL_WIDTH: usize = 40;
// const CELL_HEIGHT: usize = 50;

const COLS: i32 = 25;
const ROWS: i32 = 20;

pub struct Board {
    grid: BTreeMap<(i32, i32), Piece>,
    // played_pieces: Vec<LocatedPiece>,
    // hand_pieces: Vec<LocatedPiece>,
    renderer: SVGRenderer,
    cell_width: i32,
    cell_height: i32,
    last_highlight: Option<(i32, i32)>,
}

impl Board {
    pub fn new() -> Self {
        let document = web_sys::window().unwrap().document().unwrap();
        let board = document.get_element_by_id("board").unwrap();

        let width = board.client_width();
        let height = board.client_height();
        let renderer = SVGRenderer::new("board").expect("Unable to create renderer");
        renderer.adjust_viewbox(0, 0, width, height);

        Self {
            grid: BTreeMap::new(),
            // played_pieces: Vec::new(),
            // hand_pieces: Vec::new(),
            renderer,
            cell_width: width / COLS,
            cell_height: height / ROWS,
            last_highlight: None,
        }
    }

    pub fn resize(&mut self) {
        let document = web_sys::window().unwrap().document().unwrap();
        let board = document.get_element_by_id("board").unwrap();

        let width = board.client_width();
        let height = board.client_height();

        self.cell_width = width / COLS;
        self.cell_height = height / ROWS;

        self.renderer.adjust_viewbox(0, 0, width, height);
        self.rerender();
    }

    pub fn grid(&self) -> &BTreeMap<(i32, i32), Piece> {
        &self.grid
    }

    pub fn played_grid(&self) -> BTreeMap<(i32, i32), Piece> {
        self.grid.iter().filter(|((x, y), _)| *y < ROWS - 5).map(|((x, y), p)| ((*x, *y), *p)).collect()
    }

    pub fn render(&mut self) {
        for ((grid_x, grid_y), piece) in self.grid.iter() {
            self.renderer.render(piece.as_svg(self.cell_width, self.cell_height), 
                ((grid_x * self.cell_width) as f32, (grid_y * self.cell_height) as f32)
            );
        }
    }

    pub fn render_hand_line(&mut self) {
        let grid_x_1 = 0;
        let grid_y_1 = (ROWS - 5) * self.cell_height;
        let grid_x_2 = COLS * self.cell_width;
        let line = SVGElem::new(Tag::Line)
            .set(Attr::X1, 0)
            .set(Attr::Y1, -5)
            .set(Attr::X2, grid_x_2)
            .set(Attr::Y2, -5)
            .set(Attr::Stroke, "black")
            .set(Attr::StrokeWidth, 4);

        self.renderer.render(line, (grid_x_1 as f32, grid_y_1 as f32));
    }

    pub fn rerender(&mut self) {
        self.renderer.clear();
        self.render();
        self.render_hand_line();
    }

    pub fn render_pieces(&mut self, pieces: &[Piece]) {
        let mut pieces = pieces.iter();
        let cols = 4;
        let rows = pieces.len() / cols + 1;

        for col in 0..cols {
            for row in 0..rows {
                if let Some(piece) = pieces.next() {
                    let svg = piece.as_svg(self.cell_width, self.cell_height);

                    self.renderer.render(svg, ((col * self.cell_width as usize) as f32, (row * self.cell_height as usize) as f32));
                } else {
                    return;
                }
            }
        }
    }

    pub fn in_hand(&self, grid_x: i32, grid_y: i32) -> bool {
        grid_x > 0 && grid_y >= ROWS - 5
    }

    pub fn world_in_hand(&self, world_x: i32, world_y: i32) -> bool {
        let grid_x = world_x / self.cell_width;
        let grid_y = world_y / self.cell_height;

        self.in_hand(grid_x, grid_y)
    }

    pub fn world_contains(&self, world_x: i32, world_y: i32) -> bool {
        let grid_x = world_x / self.cell_width;
        let grid_y = world_y / self.cell_height;

        self.grid.contains_key(&(grid_x, grid_y))    
    }

    pub fn world_to_grid(&self, world_x: i32, world_y: i32) -> (i32, i32) {
        (world_x / self.cell_width, world_y / self.cell_height)
    }

    pub fn world_render_highlight(&mut self, world_x: i32, world_y: i32, piece: &Piece) {
        let grid_x = world_x / self.cell_width;
        let grid_y = world_y / self.cell_height;

        if self.last_highlight != Some((grid_x, grid_y)) {
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
            self.renderer.render(piece, ((grid_x * self.cell_width) as f32, (grid_y * self.cell_height) as f32));

            self.last_highlight = Some((grid_x, grid_y));
        }
    }

    pub fn remove_piece_at(&mut self, world_x: i32, world_y: i32) -> Option<(Piece, bool)> {
        let grid_x = world_x / self.cell_width;
        let grid_y = world_y / self.cell_height;
        
        let in_hand = grid_y >= ROWS - 5;

        self.grid.remove(&(grid_x, grid_y)).map(|p| (p, in_hand))
    }

    pub fn world_insert(&mut self, world_x: i32, world_y: i32, piece: Piece) -> bool {
        let grid_x = world_x / self.cell_width;
        let grid_y = world_y / self.cell_height;

        self.grid.insert((grid_x, grid_y), piece);

        grid_y >= ROWS - 5
    }

    pub fn insert_as_hand(&mut self, pieces: &[Piece]) {
        let mut x = 0;
        let mut y = ROWS - 5;

        for piece in pieces {
            self.grid.insert((x, y), *piece);
            
            if x + 1 > COLS {
                y = (y + 1) % ROWS;
            }
            
            x = (x + 1) % COLS; 
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
            .set(Attr::Transform, "scale(1, 2)")
            .set(Attr::X, width / 2)
            .set(Attr::Y, height / 4)
            .set(Attr::DominantBaseline, "central")
            .set(Attr::TextAnchor, "middle")
            .set(Attr::Class, "piece_text")
            .set(Attr::TextLength, width - 5)
            .set(Attr::LengthAdjust, "spacingAndGlyphs")
            .set_inner(&number);

        let piece = SVGElem::new(Tag::G).append(background).append(num);

        piece
    }
}
