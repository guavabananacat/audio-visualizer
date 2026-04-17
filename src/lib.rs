use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, AudioContext, AnalyserNode};
use js_sys::Function;
use std::rc::Rc;
use std::cell::RefCell;

const BINS_PER_OCTAVE: i32 = 12;
const F_MIN: f64 = 20.0;
const F_MAX: f64 = 20000.0;
const DB_MIN: f64 = -60.0;
const DB_MAX: f64 = 0.0;

struct Resonator {
    s1: f64,
    s2: f64,
    coeff: f64,
    r: f64,
    r2: f64,
    // multiply raw power by this to normalize to input amplitude²
    norm: f64,
}

impl Resonator {
    fn new(freq: f64, sample_rate: f64, q: f64) -> Self {
        let omega = 2.0 * std::f64::consts::PI * freq / sample_rate;
        let r = (-std::f64::consts::PI * freq / (q * sample_rate)).exp();
        // |H(e^{jω₀})|² = 1 / ((1-r)² · (1 - 2r·cos(2ω₀) + r²))
        let gain_sq = 1.0
            / ((1.0 - r).powi(2) * (1.0 - 2.0 * r * (2.0 * omega).cos() + r * r));
        Self {
            s1: 0.0,
            s2: 0.0,
            coeff: 2.0 * omega.cos(),
            r,
            r2: r * r,
            norm: 1.0 / gain_sq,
        }
    }

    #[inline]
    fn process(&mut self, x: f64) {
        let s0 = x + self.r * self.coeff * self.s1 - self.r2 * self.s2;
        self.s2 = self.s1;
        self.s1 = s0;
    }

    // Returns amplitude in units of input signal (0..1 for 0 dBFS sine)
    fn amplitude(&self) -> f64 {
        let p = (self.s1 * self.s1 + self.s2 * self.s2
            - self.r * self.coeff * self.s1 * self.s2)
            .max(0.0);
        (p * self.norm).sqrt()
    }
}

fn build_bank(sample_rate: f64) -> Vec<Resonator> {
    let q = 1.0 / (2.0_f64.powf(1.0 / BINS_PER_OCTAVE as f64) - 1.0);
    let f_top = F_MAX.min(sample_rate * 0.499);
    let n_bins = (BINS_PER_OCTAVE as f64 * (f_top / F_MIN).log2()).ceil() as usize;
    (0..n_bins)
        .map(|k| {
            let freq = F_MIN * 2.0_f64.powf(k as f64 / BINS_PER_OCTAVE as f64);
            Resonator::new(freq, sample_rate, q)
        })
        .collect()
}

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
    analyser.set_fft_size(1024);

    source.connect_with_audio_node(&analyser)?;

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

    let bank = build_bank(sample_rate);
    start_animation_loop(&window, &analyser, bank)?;

    Ok(())
}

fn start_animation_loop(
    window: &web_sys::Window,
    analyser: &AnalyserNode,
    bank: Vec<Resonator>,
) -> Result<(), JsValue> {
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id("canvas")
        .ok_or("no canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let ctx = canvas
        .get_context("2d")?
        .ok_or("no 2d context")?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;

    let fft_size = analyser.fft_size() as usize;
    let samples = vec![0.0f32; fft_size];
    let bank = Rc::new(RefCell::new(bank));
    let samples = Rc::new(RefCell::new(samples));

    let closure: Rc<RefCell<Option<Function>>> = Rc::new(RefCell::new(None));
    let closure_clone = closure.clone();
    let closure_inner = closure.clone();
    let analyser_clone = analyser.clone();
    let window_clone = window.clone();
    let canvas_clone = canvas.clone();
    let ctx_clone = ctx.clone();

    *closure_clone.borrow_mut() = Some(
        wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            analyser_clone.get_float_time_domain_data(&mut samples.borrow_mut());

            {
                let mut bank = bank.borrow_mut();
                let samples = samples.borrow();
                for &s in samples.iter() {
                    for res in bank.iter_mut() {
                        res.process(s as f64);
                    }
                }
            }

            ctx_clone.set_fill_style_str("#000");
            ctx_clone.fill_rect(0.0, 0.0, canvas_clone.width() as f64, canvas_clone.height() as f64);

            let canvas_w = canvas_clone.width() as usize;
            let canvas_h = canvas_clone.height() as f64;
            let bank = bank.borrow();
            let n_bins = bank.len() as f64;
            let log_ratio = (F_MAX / F_MIN).ln();

            for x in 0..canvas_w {
                let t = x as f64 / canvas_w as f64;
                let freq = F_MIN * (t * log_ratio).exp();
                let k = (BINS_PER_OCTAVE as f64 * (freq / F_MIN).log2())
                    .round()
                    .clamp(0.0, n_bins - 1.0) as usize;

                let amp = bank[k].amplitude();
                let db = 20.0 * amp.max(1e-9).log10();
                let v = ((db - DB_MIN) / (DB_MAX - DB_MIN)).clamp(0.0, 1.0);
                let brightness = (v * 255.0) as u8;

                ctx_clone.set_fill_style_str(&format!(
                    "rgb({brightness},{brightness},{brightness})"
                ));
                ctx_clone.fill_rect(x as f64, 0.0, 1.0, canvas_h);
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
