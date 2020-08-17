use wasm_svg_graphics::prelude::SVGElem;
use web_sys::{Document, Element};

use crate::JsResult;

pub trait AsSVG {
    fn as_svg(&self, width: i32, height: i32) -> SVGElem;
}

trait DocExt {
    fn create_svg_element(&self, t: &str) -> JsResult<Element>;
}

impl DocExt for Document {
    fn create_svg_element(&self, t: &str) -> JsResult<Element> {
        self.create_element_ns(Some("http://www.w3.org/2000/svg"), t)
    }
}
