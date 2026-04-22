use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

// A macro #[wasm_bindgen] exporta essa função para o JavaScript
#[wasm_bindgen]
pub fn start_engine(canvas_id: &str) -> Result<(), JsValue> {
    // 1. Pega a janela e o documento do navegador
    let window = web_sys::window().expect("Sem janela global");
    let document = window.document().expect("Sem documento");

    // 2. Acha o canvas pelo ID
    let canvas = document.get_element_by_id(canvas_id)
        .expect("Canvas não encontrado")
        .dyn_into::<HtmlCanvasElement>()?;

    // 3. Pega o contexto 2D (aqui no futuro será o seu contexto wgpu/WebGL)
    let context = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;

    // 4. Pinta o canvas de azul via Rust!
    context.set_fill_style(&JsValue::from_str("#0055ff"));
    context.fill_rect(0.0, 0.0, canvas.width().into(), canvas.height().into());

    Ok(())
}