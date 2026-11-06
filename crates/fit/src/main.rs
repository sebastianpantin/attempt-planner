//! Fits the planner's default make-rate curve to OpenPowerlifting results.
//!
//! Usage: `attempt-fit [--federation NAME] [--sex M|F] [--backtest] CSV`, where CSV is the
//! OpenPowerlifting bulk CSV from https://data.openpowerlifting.org, unzipped.
//! `--federation IPF` keeps only meets of the IPF and its member federations, and `--sex F`
//! only women's entries. `--backtest` fits to nine lifters in ten and scores the
//! predictions on the latest meet of the rest.

use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Instant;

use attempt_fit::backtest::{
    BIN_EDGES, Backtest, PAST_MISS_GROUPS, SCENARIOS, Score, Scores, backtest, is_held_out,
};
use attempt_fit::calibrate::MakeRates;
use attempt_fit::data::{Entry, Filter, raw_full_power_entries};
use attempt_fit::fitted::{Fitted, fit};
use attempt_fit::progress::ProgressFit;
use attempt_sim::lift::Lift;
use attempt_sim::past_meets::{EXPERIENCE_GROUPS, PROGRESS, Progress};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let (mut federation, mut sex, mut path) = (None, None, None);
    let mut run_backtest = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--federation" => {
                federation = Some(
                    args.next()
                        .ok_or("--federation needs a name, such as IPF")?,
                )
            }
            "--sex" => sex = Some(args.next().ok_or("--sex needs M or F")?),
            "--backtest" => run_backtest = true,
            _ => path = Some(PathBuf::from(arg)),
        }
    }
    let filter = Filter {
        federation: federation.as_deref(),
        sex: sex.as_deref(),
    };
    let path =
        path.ok_or("give the OpenPowerlifting bulk CSV, from https://data.openpowerlifting.org")?;

    let started = Instant::now();
    let entries = raw_full_power_entries(BufReader::new(File::open(&path)?), filter)?;
    let (held_out, fitted_to): (Vec<Entry>, Vec<Entry>) = entries
        .into_iter()
        .partition(|entry| run_backtest && is_held_out(&entry.lifter));
    let fitted = fit(&fitted_to)?;
    print_fit(&fitted);
    println!("fitted in {:.1} s", started.elapsed().as_secs_f64());
    if run_backtest {
        print_backtest(&backtest(&fitted, &held_out));
    }
    println!("took {:.1} s", started.elapsed().as_secs_f64());
    Ok(())
}

fn print_fit(fitted: &Fitted) {
    println!("make rates: opener, 2nd after make/miss, 3rd after make/miss (%)");
    println!(
        "spread of the day max {:.1}%",
        fitted.total_spread() * 100.0
    );
    let history = fitted.history;
    let calibrated = fitted.calibrated_model().day_form;
    println!(
        "  meet to meet ({} pairs): day form {:.1}% (squat {:.1}%, bench {:.1}%, deadlift {:.1}%)",
        history.pairs,
        calibrated.spread * 100.0,
        history.spread.squat * 100.0,
        history.spread.bench * 100.0,
        history.spread.deadlift * 100.0
    );
    println!(
        "  so expected-max uncertainty {:.1}%",
        calibrated.max_uncertainty * 100.0
    );
    let [squat_bench, squat_deadlift, bench_deadlift] = history.correlations;
    println!(
        "correlation between lifts {:.2} (squat–bench {squat_bench:.2}, squat–deadlift \
         {squat_deadlift:.2}, bench–deadlift {bench_deadlift:.2})",
        history.mean_correlation()
    );
    print_progress(&fitted.progress);
    let rates = |rates: MakeRates| rates.all().map(|r| format!("{:.1}", r * 100.0)).join(" ");
    for lift in Lift::ALL {
        let (stats, fit) = (fitted.stats.get(lift), fitted.calibrations.get(lift));
        let jumps = stats.jumps;
        let params = fit.params;
        println!("{lift}: {} meets", stats.meets);
        println!(
            "  jumps after make/miss: 2nd {:.3}/{:.3}, 3rd {:.3}/{:.3}",
            jumps.second_after_make,
            jumps.second_after_miss,
            jumps.third_after_make,
            jumps.third_after_miss
        );
        println!("  observed:  {}", rates(stats.make_rates));
        println!("  predicted: {}", rates(fit.predicted));
        println!(
            "  steepness {:.1}, technical miss chance {:.1}%, typical opener {:.1}%",
            params.steepness,
            params.technical_miss_chance * 100.0,
            params.opener_ratio * 100.0
        );
    }
    let attempts = fitted.attempts;
    println!(
        "fitted to the attempts of {} lifters' latest meets, log loss {:.4}",
        attempts.lifters, attempts.log_loss
    );
    for lift in Lift::ALL {
        let curve = attempts.model.curves.get(lift);
        let choice = attempts.load_choices.get(lift);
        let [a, b, c, d] = choice.log_shares.map(|share| share.exp() * 100.0);
        println!(
            "  {lift}: steepness {:.1}, technical miss chance {:.1}%, typical opener {:.1}%; 2nd \
             at {a:.1}% / {b:.1}% and 3rd at {c:.1}% / {d:.1}% of the day max after a make / \
             miss, give or take {:.1}%",
            curve.steepness,
            curve.technical_miss_chance * 100.0,
            attempts.model.typical_opener.get(lift) * 100.0,
            choice.spread * 100.0
        );
    }
}

/// Prints the fitted progress as `PROGRESS` in the engine takes it, rounded as there, and
/// how far the shipped one is from it.
fn print_progress(fit: &ProgressFit) {
    let progress = fit.progress;
    println!(
        "progress from meet to meet, the yearly gain carried at most {:.2} years, a lifter's \
         own give or take {:.1}% a year",
        progress.most_years,
        progress.yearly_spread * 100.0
    );
    for (group, (step, pairs)) in progress.steps.iter().zip(fit.pairs).enumerate() {
        let done = if group + 1 == EXPERIENCE_GROUPS {
            format!("{}+", group + 1)
        } else {
            (group + 1).to_string()
        };
        println!(
            "  after {done} meets ({pairs} pairs): {:.2}% at the next meet and {:.2}% a year",
            step.at_next_meet * 100.0,
            step.per_year * 100.0
        );
    }
    println!("  as PROGRESS in crates/sim/src/past_meets.rs:");
    println!("{}", progress_constant(&progress));
    println!(
        "  the shipped PROGRESS differs by at most {:.4}",
        largest_difference(&progress, &PROGRESS)
    );
}

fn progress_constant(progress: &Progress) -> String {
    let steps: String = progress
        .steps
        .iter()
        .map(|step| {
            format!(
                "        Step {{\n            at_next_meet: {:.4},\n            per_year: {:.4},\n        }},\n",
                step.at_next_meet, step.per_year
            )
        })
        .collect();
    format!(
        "pub const PROGRESS: Progress = Progress {{\n    steps: [\n{steps}    ],\n    most_years: {:.2},\n    yearly_spread: {:.3},\n}};",
        progress.most_years, progress.yearly_spread
    )
}

fn largest_difference(a: &Progress, b: &Progress) -> f64 {
    a.steps
        .iter()
        .zip(&b.steps)
        .flat_map(|(a, b)| [a.at_next_meet - b.at_next_meet, a.per_year - b.per_year])
        .chain([
            a.most_years - b.most_years,
            a.yearly_spread - b.yearly_spread,
        ])
        .map(f64::abs)
        .fold(0.0, f64::max)
}

fn print_backtest(backtest: &Backtest) {
    println!(
        "backtest: latest meet of {} held-out lifters, {} skipped",
        backtest.meets, backtest.skipped
    );
    print_score("share made, any load", &backtest.baseline);
    for (scenario, scores) in SCENARIOS.iter().zip(&backtest.scenarios) {
        print_score(scenario.name, &scores.all);
    }
    for (scenario, scores) in SCENARIOS.iter().zip(&backtest.scenarios) {
        println!("{}", scenario.name);
        print_scores(scores);
    }
}

fn print_score(name: &str, score: &Score) {
    println!(
        "  {name:<44} {:>6} attempts  made {:>5.1}%  predicted {:>5.1}%  log loss {:.4}  brier {:.4}",
        score.attempts,
        score.observed() * 100.0,
        score.predicted() * 100.0,
        score.log_loss(),
        score.brier()
    );
}

fn print_scores(scores: &Scores) {
    print_score("model", &scores.all);
    let attempts = [
        "opener",
        "2nd after make",
        "2nd after miss",
        "3rd after make",
        "3rd after miss",
    ];
    for lift in Lift::ALL {
        for (name, score) in attempts.iter().zip(scores.attempts.get(lift)) {
            print_score(&format!("{lift} {name}"), score);
        }
    }
    println!("  by predicted chance");
    for (edges, score) in BIN_EDGES.windows(2).zip(&scores.bins) {
        print_score(
            &format!("{:.0}–{:.0}%", edges[0] * 100.0, edges[1] * 100.0),
            score,
        );
    }
    println!("  by attempts at the lift missed in past meets");
    for lift in Lift::ALL {
        for (misses, score) in scores.past_misses.get(lift).iter().enumerate() {
            let misses = if misses + 1 == PAST_MISS_GROUPS {
                format!("{misses}+")
            } else {
                misses.to_string()
            };
            print_score(&format!("{lift}, {misses} missed"), score);
        }
    }
}
