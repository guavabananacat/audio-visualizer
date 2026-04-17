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

    let overlay = document
        .get_element_by_id("start-overlay")
        .expect("should have start-overlay")
        .dyn_into::<web_sys::HtmlElement>()
        .expect("start-overlay should be an HtmlElement");

    let overlay_clone = overlay.clone();
    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
        let _ = overlay_clone.style().set_property("display", "none");
        wasm_bindgen_futures::spawn_local(async {
            if let Err(e) = start_visualizer().await {
                web_sys::console::error_1(&format!("Error: {:?}", e).into());
            }
        });
    }) as Box<dyn FnMut()>);
    overlay.set_onclick(Some(closure.as_ref().dyn_ref().unwrap()));
    closure.forget();
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
    let sample_rate = audio_ctx.sample_rate() as f64;
    let source = audio_ctx.create_media_stream_source(&stream)?;

    let analyser = audio_ctx.create_analyser()?;
    analyser.set_fft_size(512);

    source.connect_with_audio_node(&analyser)?;
    analyser.connect_with_audio_node(&audio_ctx.destination())?;

    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id("canvas")
        .ok_or("no canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let w = window.inner_width()?.as_f64().unwrap_or(800.0) as u32;
    let h = window.inner_height()?.as_f64().unwrap_or(400.0) as u32;
    canvas.set_width(w);
    canvas.set_height(h);

    let canvas_resize = canvas.clone();
    let window_resize = window.clone();
    let resize_closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
        let w = window_resize.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(800.0) as u32;
        let h = window_resize.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(400.0) as u32;
        canvas_resize.set_width(w);
        canvas_resize.set_height(h);
    }) as Box<dyn FnMut()>);
    window.add_event_listener_with_callback("resize", resize_closure.as_ref().dyn_ref().unwrap())?;
    resize_closure.forget();

    start_animation_loop(&window, &analyser, sample_rate)?;

    Ok(())
}

fn start_animation_loop(window: &web_sys::Window, analyser: &AnalyserNode, sample_rate: f64) -> Result<(), JsValue> {
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id("canvas")
        .ok_or("no canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let ctx = canvas
        .get_context("2d")?
        .ok_or("no 2d context")?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;

    let fft_size = analyser.fft_size();
    let mut freq_data = vec![0u8; (fft_size / 2) as usize];

    let closure: Rc<RefCell<Option<Function>>> = Rc::new(RefCell::new(None));
    let closure_clone = closure.clone();
    let closure_inner = closure.clone();
    let analyser_clone = analyser.clone();
    let window_clone = window.clone();
    let canvas_clone = canvas.clone();
    let ctx_clone = ctx.clone();

    *closure_clone.borrow_mut() = Some(
        wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            analyser_clone.get_byte_frequency_data(&mut freq_data);

            ctx_clone.set_fill_style_str("#000");
            ctx_clone.fill_rect(0.0, 0.0, canvas_clone.width() as f64, canvas_clone.height() as f64);

            let n_bins = freq_data.len();
            let canvas_w = canvas_clone.width() as f64;
            let canvas_h = canvas_clone.height() as f64;
            let f_min = 20.0f64;
            let f_max = (sample_rate / 2.0).min(20000.0);
            let log_min = f_min.ln();
            let log_max = f_max.ln();
            // Each bar maps a log-spaced frequency range to one pixel column
            let n_bars = canvas_clone.width() as usize;
            for bar in 0..n_bars {
                let t_low = bar as f64 / n_bars as f64;
                let t_high = (bar + 1) as f64 / n_bars as f64;
                let f_low = (log_min + t_low * (log_max - log_min)).exp();
                let f_high = (log_min + t_high * (log_max - log_min)).exp();

                let bin_low = ((f_low * n_bins as f64 * 2.0) / sample_rate) as usize;
                let bin_high = ((f_high * n_bins as f64 * 2.0) / sample_rate) as usize;
                let bin_low = bin_low.min(n_bins - 1);
                let bin_high = bin_high.min(n_bins).max(bin_low + 1);

                let value = freq_data[bin_low..bin_high]
                    .iter()
                    .map(|&v| v as f64)
                    .fold(0.0f64, f64::max);

                let hue = (bar as f64 / n_bars as f64 * 360.0) % 360.0;
                ctx_clone.set_fill_style_str(&format!(
                    "hsl({}, 100%, {}%)",
                    hue,
                    50 + (value / 255.0 * 30.0) as u32
                ));

                let bar_height = (value / 255.0) * canvas_h;
                let x = bar as f64;
                let y = canvas_h - bar_height;
                ctx_clone.fill_rect(x, y, 1.0, bar_height);
            }

            let callback = closure_inner.borrow().as_ref().map(|f| f.clone());
            if let Some(callback) = callback {
                let _ = window_clone.request_animation_frame(&callback);
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
