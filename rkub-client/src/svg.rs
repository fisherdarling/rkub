use web_sys::{
    Document, Element, EventTarget, FileReader, MessageEvent, ProgressEvent, WebSocket, Window,
};
use wasm_svg_graphics::prelude::SVGElem;

use crate::{JsError, JsResult};
use rkub_common::Piece;

pub trait AsSVG {
    fn as_svg(&self) -> SVGElem;
}


trait DocExt {
    fn create_svg_element(&self, t: &str) -> JsResult<Element>;
}

impl DocExt for Document {
    fn create_svg_element(&self, t: &str) -> JsResult<Element> {
        self.create_element_ns(Some("http://www.w3.org/2000/svg"), t)
    }
}

pub fn create_piece_svg(document: &Document, piece: &Piece) -> JsResult<Element> {
    let piece = document.create_svg_element("rect")?;
    piece.set_attribute("width", "5em")?;
    piece.set_attribute("height", "10em")?;
    piece.set_attribute("x", "200")?;
    piece.set_attribute("y", "200")?;
    piece.set_attribute("fill", "red")?;

    Ok(piece)
}
