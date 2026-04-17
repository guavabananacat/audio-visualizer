# Audio Visualizer

A real-time audio visualizer built with Rust and WebAssembly. Captures microphone input and renders a frequency bar visualization on an HTML5 canvas.

## Demo

[https://guavabananacat.github.io/audio-visualizer/](https://guavabananacat.github.io/audio-visualizer/)

## Requirements

- [Rust](https://rustup.rs/)
- [Trunk](https://trunkrs.dev/) (`cargo install trunk`)
- `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)

## Development

```sh
trunk serve
```

## Build

```sh
trunk build --release --dist docs
```

## Stack

- Rust + `wasm-bindgen` for WASM
- Web Audio API (`AnalyserNode`) for FFT frequency data
- Canvas 2D for rendering
