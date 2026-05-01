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
mod checkerboard;

#[cfg(target_arch = "wasm32")]
use crate::viewport::GpuViewport;

const IMG_W: u32 = 2048;
const IMG_H: u32 = 1536;
const TILE_SIZE: u32 = 256;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static VIEWPORT: RefCell<Option<Rc<RefCell<GpuViewport>>>> = const { RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
fn vp_with<R>(f: impl FnOnce(&mut GpuViewport) -> R) -> Option<R> {
    VIEWPORT.with(|v| {
        v.borrow().as_ref().and_then(|cell| {
            cell.try_borrow_mut().ok().as_deref_mut().map(f)
        })
    })
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
        Self
    }

    pub fn init_viewport(&self, width: u32, height: u32) -> js_sys::Promise {
        future_to_promise(async move {
            let vp = GpuViewport::new(width, height, IMG_W, IMG_H, TILE_SIZE)
                .await
                .map_err(|e| JsValue::from_str(&e))?;
            VIEWPORT.with(|v| *v.borrow_mut() = Some(Rc::new(RefCell::new(vp))));
            Ok(JsValue::UNDEFINED)
        })
    }

    pub fn pan(&self, dx: f32, dy: f32) {
        vp_with(|v| { v.prepare_frame(); v.pan(dx, dy); });
    }

    pub fn zoom_at(&self, factor: f32, anchor_x: f32, anchor_y: f32) {
        vp_with(|v| { v.prepare_frame(); v.zoom_at(factor, anchor_x, anchor_y); });
    }

    pub fn fit(&self) {
        vp_with(|v| { v.prepare_frame(); v.fit(); });
    }

    pub fn resize(&self, width: u32, height: u32) {
        vp_with(|v| v.resize(width, height));
    }

    pub fn render(&self) -> js_sys::Promise {
        let vp = VIEWPORT.with(|v| v.borrow().as_ref().map(Rc::clone));
        match vp {
            Some(vp) => {
                future_to_promise(async move {
                    let map = {
                        let vp_ref = vp.borrow();
                        let atlas_bg = &vp_ref.atlas.bind_group;
                        let atlas_vb = &vp_ref.atlas.vertex_buffer;
                        let atlas_vc = vp_ref.atlas.vertex_count;
                        vp_ref.renderer.submit(|rpass| {
                            if atlas_vc == 0 { return; }
                            rpass.set_vertex_buffer(0, atlas_vb.slice(..));
                            rpass.set_bind_group(1, atlas_bg, &[]);
                            rpass.draw(0..atlas_vc, 0..1);
                        })
                    };
                    map.await.map_err(|e| JsValue::from_str(&format!("map: {:?}", e)))?;
                    vp.borrow().renderer.read_pixels().map(JsValue::from)
                })
            }
            None => js_sys::Promise::reject(&JsValue::from_str("Viewport not initialized")),
        }
    }
}