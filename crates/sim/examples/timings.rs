//! Times each engine entry point on a typical lifter: `cargo run --release --example timings`.

use std::time::Instant;

use attempt_sim::evaluate::{EvaluateInput, LiftModel, Lifter, MeetDays, evaluate};
use attempt_sim::lift::PerLift;
use attempt_sim::model::{DayForm, MakeRateCurve};
use attempt_sim::plan::{Jumps, LiftPlan};
use attempt_sim::rules::Weight;
use attempt_sim::search::{ChangeInput, SearchInput, search, single_changes};
use attempt_sim::tradeoff::{TradeoffInput, tradeoff};

fn kg(kg: f64) -> Weight {
    Weight::from_kg(kg).expect("a valid weight")
}

fn lifter() -> Lifter {
    let lift = |max: f64, steepness: f64, technical_miss_chance: f64| LiftModel {
        expected_max: kg(max),
        curve: MakeRateCurve {
            ratio_at_half: 1.0,
            steepness,
            technical_miss_chance,
        },
    };
    Lifter {
        lifts: PerLift {
            squat: lift(200.0, 56.0, 0.0),
            bench: lift(130.0, 94.0, 0.03),
            deadlift: lift(240.0, 109.0, 0.01),
        },
        day_form: DayForm {
            spread: 0.03,
            max_uncertainty: 0.03,
            correlation: 0.4,
        },
    }
}

fn plan(opener: f64, second: f64, third: f64) -> LiftPlan {
    let jumps = |after_make| Jumps {
        after_make: kg(after_make),
        after_miss: Weight::ZERO,
    };
    LiftPlan {
        opener: kg(opener),
        second: jumps(second),
        third: jumps(third),
    }
}

fn plans() -> PerLift<LiftPlan> {
    PerLift {
        squat: plan(180.0, 10.0, 7.5),
        bench: plan(117.5, 7.5, 5.0),
        deadlift: plan(215.0, 15.0, 10.0),
    }
}

/// The fastest of five runs. Each run has a new seed, so it simulates its days afresh,
/// unless `warm`, when every run reuses the days of a first run.
fn time<T>(name: &str, simulations: u32, warm: bool, run: impl Fn(u32, u64) -> T) {
    let seed = |run: u64| if warm { 0 } else { run };
    if warm {
        run(simulations, seed(0));
    }
    let mut best = f64::INFINITY;
    for run_index in 1..=5 {
        let start = Instant::now();
        std::hint::black_box(run(simulations, seed(run_index)));
        best = best.min(start.elapsed().as_secs_f64());
    }
    let name = if warm {
        format!("{name} (warm)")
    } else {
        name.to_string()
    };
    println!(
        "{name:<24} {simulations:>9} meets {:>10.2} ms",
        best * 1000.0
    );
}

fn main() {
    let goal = Some(kg(550.0));
    for simulations in [20_000, 1_000_000] {
        time("meet days", simulations, false, |simulations, seed| {
            MeetDays::new(lifter(), simulations, seed)
        });
        for warm in [false, true] {
            time("evaluate", simulations, warm, |simulations, seed| {
                evaluate(&EvaluateInput {
                    lifter: lifter(),
                    plans: plans(),
                    goal,
                    simulations,
                    seed,
                })
            });
            time("tradeoff", simulations, warm, |simulations, seed| {
                tradeoff(&TradeoffInput {
                    lifter: lifter(),
                    goal,
                    bomb_out_cap: Some(0.05),
                    simulations,
                    seed,
                })
            });
            time("single changes", simulations, warm, |simulations, seed| {
                single_changes(&ChangeInput {
                    lifter: lifter(),
                    plans: plans(),
                    goal,
                    bomb_out_cap: Some(0.05),
                    simulations,
                    seed,
                })
            });
        }
    }
    time("search", 20_000, false, |simulations, seed| {
        search(&SearchInput {
            lifter: lifter(),
            start: Some(plans()),
            goal,
            bomb_out_cap: Some(0.05),
            simulations,
            seed,
        })
    });
}
