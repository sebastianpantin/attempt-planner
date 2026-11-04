# attempt-planner

A Monte Carlo simulator for choosing openers and attempt jumps at a powerlifting meet. The core is Rust, compiled to WebAssembly so it runs in the browser.

Early stage. Right now the repo has the attempt rules and the build pipeline. The simulation itself comes next.

## The model (planned)

- Each lift gets three attempts, each at least the 20 kg bar and in 2.5 kg steps. After a miss the next attempt may repeat the weight or go heavier; after a make it must go heavier. Missing all three attempts in a lift means no total.
- On meet day the lifter's true max for each lift is uncertain, so every simulated meet draws a day form per lift.
- The chance of making an attempt depends on how heavy it is relative to that day's max. The curve can be fitted to the lifter's own training data or to [OpenPowerlifting](https://www.openpowerlifting.org) attempt results.

For a given attempt plan the simulator reports the expected total, the chance of reaching a goal total and the chance of bombing out. Comparing plans shows the tradeoff between aggressive and conservative jumps.

## Layout

- `crates/sim`: the simulator, plain Rust with no WebAssembly dependencies
- `crates/sim-wasm`: `wasm-bindgen` bindings for the browser
- `web`: the static page served on GitHub Pages

## Development

### With Docker only

No Rust toolchain needed.

```sh
docker build -t attempt-planner . && docker run --rm -p 8000:80 attempt-planner
```

Then open http://localhost:8000. Run the checks (fmt, clippy, tests) with:

```sh
docker build --target test .
```

### With Rust installed

```sh
cargo test --workspace
```

To run the page, build the WebAssembly package into `web/pkg` and serve `web/`. This needs the `wasm32-unknown-unknown` target and [wasm-pack](https://github.com/drager/wasm-pack):

```sh
wasm-pack build crates/sim-wasm --target web --out-dir ../../web/pkg
python3 -m http.server -d web
```

`docker build --target pkg --output web/pkg .` builds the same package if you have Rust but not the wasm tools.

## License

MIT. See [LICENSE](LICENSE).
