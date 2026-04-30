#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::future_to_promise;

mod error;
mod pixel;
mod color;
mod image;
mod pipeline;
#[cfg(target_arch = "wasm32")]
mod viewport;
mod approx;

#[cfg(target_arch = "wasm32")]
use crate::viewport::renderer::Renderer;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static VIEWPORT: RefCell<Option<Rc<Renderer>>> = const { RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
fn viewport_get() -> Option<Rc<Renderer>> {
    VIEWPORT.with(|v| v.borrow().as_ref().cloned())
}

#[cfg(target_arch = "wasm32")]
fn viewport_set(r: Rc<Renderer>) {
    VIEWPORT.with(|v| *v.borrow_mut() = Some(r));
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct PixorsEngine;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl PixorsEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        web_sys::console::log_1(&JsValue::from_str("PixorsEngine initialized via WASM!"));
        Self
    }

    pub fn init_viewport(
        &self,
        _canvas: web_sys::HtmlCanvasElement,
        width: u32,
        height: u32,
    ) -> js_sys::Promise {
        if viewport_get().is_some() {
            return js_sys::Promise::resolve(&JsValue::UNDEFINED);
        }
        future_to_promise(async move {
            let renderer = Renderer::new(width, height)
                .await
                .map_err(|e: String| JsValue::from_str(&e))?;
            viewport_set(Rc::new(renderer));
            Ok(JsValue::UNDEFINED)
        })
    }

    pub fn resize_viewport(&self, width: u32, height: u32) {
        if let Some(vp) = viewport_get() {
            vp.queue_resize(width, height);
        }
    }

    pub fn render(&self) -> js_sys::Promise {
        let vp = match viewport_get() {
            Some(v) => v,
            None => return js_sys::Promise::reject(&JsValue::from_str("Viewport not init")),
        };
        future_to_promise(async move { vp.render().await.map(JsValue::from) })
    }
}
