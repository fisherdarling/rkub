use wasm_svg_graphics::prelude::*;
use std::collections::HashMap;

use crate::svg::AsSVG;
use rkub_common::{Color, Piece, Game};

const CELL_WIDTH: usize = 40;
const CELL_HEIGHT: usize = 50;

pub struct Board {
    grid: HashMap<(i32, i32), Piece>,
    played_pieces: Vec<LocatedPiece>,
    renderer: SVGRenderer,
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
            grid: HashMap::new(),
            played_pieces: Vec::new(),
            renderer,
        }
    }

    pub fn render(&mut self) {
        for located in &self.played_pieces {
            self.renderer.render(located.piece.as_svg(), (located.x, located.y));
        }
    }

    pub fn render_pieces(&mut self, pieces: &[Piece]) {
        let mut pieces = pieces.iter();
        let cols = 4;
        let rows = pieces.len() / cols + 1;

        for col in 0..cols {
            for row in 0..rows {
                if let Some(piece) = pieces.next() {
                    let svg = piece.as_svg();

                    self.renderer.render(svg, ((col * CELL_WIDTH) as f32, (row * CELL_HEIGHT) as f32));
                } else {
                    return;
                }
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

pub struct LocatedGroup {
    pub x: f32,
    pub y: f32,
    pub pieces: Piece,
}

impl AsSVG for Piece {
    fn as_svg(&self) -> SVGElem {
        let color = self.color.to_string();
        let number = self.num.to_string();

        let background = SVGElem::new(Tag::Rect)
            .set(Attr::Class, "piece_tile")
            .set(Attr::Width, CELL_WIDTH)
            .set(Attr::Height, CELL_HEIGHT)
            .set(Attr::X, 0)
            .set(Attr::Y, 0);

        let num = SVGElem::new(Tag::Text)
            .set(Attr::Fill, color)
            .set(Attr::Transform, "scale(1, 2)")
            .set(Attr::X, CELL_WIDTH / 2)
            .set(Attr::Y, CELL_HEIGHT / 4)
            .set(Attr::DominantBaseline, "central")
            .set(Attr::TextAnchor, "middle")
            .set(Attr::Class, "piece_text")
            .set(Attr::TextLength, CELL_WIDTH - 5)
            .set(Attr::LengthAdjust, "spacingAndGlyphs")
            .set_inner(&number);

        let piece = SVGElem::new(Tag::G).append(background).append(num);

        piece
    }
}
