# attempt-planner

A Monte Carlo simulator for choosing openers and attempt jumps at a powerlifting meet. The core is Rust, compiled to WebAssembly so it runs in the browser.

Early stage. The page evaluates an attempt plan, charts the tradeoff between bomb-out risk and average total across a family of plans, and searches for the plan with the best chance of reaching a goal, optionally under a bomb-out cap. The default make-rate curve is still a placeholder.

## The model

- Each lift gets three attempts, each at least the 20 kg bar and in 2.5 kg steps. After a miss the next attempt may repeat the weight or go heavier; after a make it must go heavier. Missing all three attempts in a lift means no total.
- On meet day the lifter's true max for each lift is uncertain, so every simulated meet draws a day form per lift.
- The chance of making an attempt depends on how heavy it is relative to that day's max, along a logistic curve. The default curve is a hand-picked placeholder for now; fitting it to [OpenPowerlifting](https://www.openpowerlifting.org) attempt results comes later.

For a given attempt plan the simulator reports the expected total, the chance of reaching a goal total and the chance of bombing out. Comparing plans shows the tradeoff between aggressive and conservative jumps.

## Layout

- `crates/sim`: the simulator, plain Rust with no WebAssembly dependencies
- `crates/sim-wasm`: `wasm-bindgen` bindings for the browser
- `web`: the page served on GitHub Pages, built with Vite and TypeScript. The simulation runs in a Web Worker so the page stays responsive.

## Development

### With Docker only

No Rust toolchain needed.

```sh
docker build -t attempt-planner . && docker run --rm -p 8000:80 attempt-planner
```

Then open http://localhost:8000. Run the checks with:

```sh
docker build --target test .   # Rust: fmt, clippy, tests, WebAssembly tests in Node
docker build --target web .    # page: format, type-check, tests, build
```

### With Rust installed

```sh
cargo test --workspace
```

To work on the page, build the WebAssembly package into `web/pkg`, then start the Vite dev server. This needs Node 24, the `wasm32-unknown-unknown` target and [wasm-pack](https://github.com/drager/wasm-pack):

```sh
wasm-pack build crates/sim-wasm --target web --out-dir ../../web/pkg
cd web
npm install
npm run dev
```

`docker build --target pkg --output web/pkg .` builds the same package if you have Rust but not the wasm tools. In `web`, `npm run check` checks formatting and types, `npm run format` formats, and `npm test` runs the tests.

## License

MIT. See [LICENSE](LICENSE).
