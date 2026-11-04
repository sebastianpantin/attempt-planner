# attempt-planner

Choose openers and attempt jumps for a powerlifting meet by simulating the meet thousands of times. The simulator is Rust, compiled to WebAssembly, so everything runs in the browser.

**Try it: https://sebastianpantin.github.io/attempt-planner/**

![Changing the goal, then finding the best plan for it](docs/demo.gif)

## What it does

- **Evaluate a plan.** Enter each lift's expected max and a plan: an opener, then how much to jump for the 2nd and 3rd attempts after a make and after a miss. The page shows the average total, the chance of reaching a goal total, the chance of bombing out, the spread of totals, and each lift's median best, from 20,000 simulated meets (or 1,000,000 in precise mode). The average total and the two chances come with their Monte Carlo standard errors.
- **See the tradeoff.** A chart of a whole family of plans, from conservative to aggressive, by bomb-out chance and average total. It highlights the best plan at each level of risk and the best plan for the goal. Any highlighted plan can be filled in with one click.
- **Find the best plan.** A search for the openers and jumps with the best chance of reaching the goal, or the highest average total without a goal, optionally keeping the bomb-out chance under a cap.

Plans follow the IPF attempt rules: at least the 20 kg bar, 2.5 kg steps, and every attempt after a made one must be heavier.

## The model

- **Day form.** Nobody hits exactly their expected max on the day. Each simulated meet draws a day max for every lift: the expected max times 1 plus a normal draw with a spread of 3% by default. The three lifts share part of that draw (correlation 0.5 by default), because a good day for the squat tends to be a good day for the deadlift.
- **Make rate.** The chance of making an attempt falls along a logistic curve as the load nears and passes the day max. The curve has two numbers per lift: the load ÷ day max ratio with a 50% make rate, and how steeply the chance falls around it. Attempts are independent once the day max is drawn; there is no fatigue.
- **Plans.** The 2nd and 3rd attempts react to the attempt before: a jump after a make and a jump after a miss (0 repeats the weight). Missing all three attempts of a lift is a bomb-out, counted as a total of 0.
- **Same days for every plan.** Every plan is scored against the same seeded simulated days, so differences between plans are not luck of the draw. Each roll becomes the weight below which that attempt is made, so scoring a plan only compares weights.
- **Search.** Starts from the best plan in the tradeoff family, then changes one setting of one lift at a time and keeps any change that ranks higher, until a whole round changes nothing. It picks and reports plans on the same 20,000 meets, so its figures are slightly optimistic; precise mode re-checks a plan on more meets.

The day form and make-rate numbers are inputs under "Advanced".

### Where the default make-rate curve comes from

[OpenPowerlifting](https://www.openpowerlifting.org) records every attempt and whether it was made, but not anyone's max on the day, so the curve can't be read off the data directly. Instead it is calibrated to how often lifters make each attempt:

- From about 900,000 raw full-power meets per lift, measure how often the 1st, 2nd and 3rd attempts are made, and the median jump after a make and after a miss.
- Call the expected max the weight a lifter makes half the time on a normal day, so the curve's midpoint is 1. Give the day max the default 3% spread.
- For each lift, find the steepness, and where lifters open relative to their expected max, that make a lifter following the typical jumps succeed as often as the data says.

| Lift | Made 1st / 2nd / 3rd | Fitted steepness | Typical opener | Predicted 1st / 2nd / 3rd |
| --- | --- | --- | --- | --- |
| Squat | 88% / 84% / 66% | 19 | 87% | 91% / 80% / 68% |
| Bench | 92% / 81% / 47% | 59 | 91% | 98% / 79% / 49% |
| Deadlift | 95% / 89% / 62% | 49 | 89% | 99% / 88% / 63% |

The fit matches 2nd and 3rd attempts closely but makes openers look safer than they are, most clearly for the bench. Real meets have misses that have little to do with the load, like a missed command or a depth call, and the model has no term for them. Lifters also pick their jumps knowing how the day is going, which the typical plan ignores.

`crates/fit` does the calibration. It reads the Parquet file that [openpowerlifting-mcp](https://github.com/sebastianpantin/openpowerlifting-mcp) downloads and converts:

```sh
cargo run --release -p attempt-fit -- path/to/openpowerlifting.parquet
```

The engine takes plain data in and out, with no browser code, so other front ends can reuse it.

## Layout

- `crates/sim`: the simulator: rules, model, evaluation, the tradeoff sweep and the search. Plain Rust with no WebAssembly dependencies.
- `crates/sim-wasm`: `wasm-bindgen` bindings for the browser
- `crates/fit`: calibrates the default make-rate curve to OpenPowerlifting results
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
