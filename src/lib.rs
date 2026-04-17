use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, AudioContext, AnalyserNode};
use js_sys::Function;
use std::rc::Rc;
use std::cell::RefCell;

#[wasm_bindgen(start)]
pub fn main() {
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");

    let button = document
        .get_element_by_id("start")
        .expect("should have a start button")
        .dyn_into::<web_sys::HtmlButtonElement>()
        .expect("start should be a button");

    button.set_onclick(Some(
        &wasm_bindgen::closure::Closure::once(move || {
            wasm_bindgen_futures::spawn_local(async {
                if let Err(e) = start_visualizer().await {
                    web_sys::console::error_1(&format!("Error: {:?}", e).into());
                }
            });
        })
        .into_js_value()
        .dyn_into()
        .unwrap(),
    ));
}

async fn start_visualizer() -> Result<(), JsValue> {
    let window = window().ok_or("no window")?;
    let navigator = window.navigator();
    let media_devices = navigator.media_devices()?;

    let constraints = web_sys::MediaStreamConstraints::new();
    constraints.set_audio(&JsValue::from_bool(true));

    let stream = wasm_bindgen_futures::JsFuture::from(
        media_devices.get_user_media_with_constraints(&constraints)?
    )
    .await?
    .dyn_into::<web_sys::MediaStream>()?;

    let audio_ctx = AudioContext::new()?;
    let source = audio_ctx.create_media_stream_source(&stream)?;

    let analyser = audio_ctx.create_analyser()?;
    analyser.set_fft_size(512);

    source.connect_with_audio_node(&analyser)?;
    analyser.connect_with_audio_node(&audio_ctx.destination())?;

    start_animation_loop(&window, &analyser)?;

    Ok(())
}

fn start_animation_loop(window: &web_sys::Window, analyser: &AnalyserNode) -> Result<(), JsValue> {
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id("canvas")
        .ok_or("no canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let ctx = canvas
        .get_context("2d")?
        .ok_or("no 2d context")?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;

    let mut freq_data = vec![0u8; (analyser.fft_size() / 2) as usize];

    let closure: Rc<RefCell<Option<Function>>> = Rc::new(RefCell::new(None));
    let closure_clone = closure.clone();
    let closure_inner = closure.clone();
    let analyser_clone = analyser.clone();

    *closure_clone.borrow_mut() = Some(
        wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            analyser_clone.get_byte_frequency_data(&mut freq_data);

            ctx.set_fill_style_str("#000");
            ctx.fill_rect(0.0, 0.0, canvas.width() as f64, canvas.height() as f64);

            let bar_width = (canvas.width() as f64) / (freq_data.len() as f64);
            for (i, &value) in freq_data.iter().enumerate() {
                let hue = (i as f64 / freq_data.len() as f64 * 360.0) % 360.0;
                ctx.set_fill_style_str(&format!(
                    "hsl({}, 100%, {}%)",
                    hue,
                    50 + (value as f64 / 255.0 * 30.0) as u32
                ));

                let bar_height = (value as f64 / 255.0) * (canvas.height() as f64);
                let x = i as f64 * bar_width;
                let y = (canvas.height() as f64) - bar_height;

                ctx.fill_rect(x, y, bar_width - 1.0, bar_height);
            }

            let callback = closure_inner.borrow().as_ref().map(|f| f.clone());
            if let Some(callback) = callback {
                let _ = window.request_animation_frame(&callback);
            }
        }) as Box<dyn FnMut()>)
        .into_js_value()
        .dyn_into()
        .unwrap(),
    );

    if let Some(closure_fn) = closure.borrow().as_ref() {
        window.request_animation_frame(closure_fn)?;
    }

    Ok(())
}
