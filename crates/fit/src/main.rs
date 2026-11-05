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
    BIN_EDGES, Backtest, PAST_MISS_GROUPS, Score, Scores, backtest, is_held_out,
};
use attempt_fit::calibrate::MakeRates;
use attempt_fit::data::{Entry, Filter, raw_full_power_entries};
use attempt_fit::fitted::{Fitted, fit};
use attempt_sim::lift::Lift;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let (mut federation, mut sex, mut path) = (None, None, None);
    let mut scores = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--federation" => {
                federation = Some(
                    args.next()
                        .ok_or("--federation needs a name, such as IPF")?,
                )
            }
            "--sex" => sex = Some(args.next().ok_or("--sex needs M or F")?),
            "--backtest" => scores = true,
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
    if scores {
        let (held_out, fitted_to): (Vec<Entry>, Vec<Entry>) = entries
            .into_iter()
            .partition(|entry| is_held_out(&entry.lifter));
        let fitted = fit(&fitted_to)?;
        print_fit(&fitted);
        print_backtest(&backtest(&fitted, &held_out));
    } else {
        print_fit(&fit(&entries)?);
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
    let day_form = fitted.day_form();
    println!(
        "  meet to meet ({} pairs): day form {:.1}% (squat {:.1}%, bench {:.1}%, deadlift {:.1}%)",
        history.pairs,
        day_form.spread * 100.0,
        history.spread.squat * 100.0,
        history.spread.bench * 100.0,
        history.spread.deadlift * 100.0
    );
    println!(
        "  so expected-max uncertainty {:.1}%",
        day_form.max_uncertainty * 100.0
    );
    let [squat_bench, squat_deadlift, bench_deadlift] = history.correlations;
    println!(
        "correlation between lifts {:.2} (squat–bench {squat_bench:.2}, squat–deadlift \
         {squat_deadlift:.2}, bench–deadlift {bench_deadlift:.2})",
        history.mean_correlation()
    );
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
}

fn print_backtest(backtest: &Backtest) {
    println!(
        "backtest: latest meet of {} held-out lifters, {} skipped",
        backtest.meets, backtest.skipped
    );
    print_score("share made, any load", &backtest.baseline);
    for (name, scores) in [
        ("from past meets", &backtest.past_meets),
        ("from past meets and the opener", &backtest.with_opener),
    ] {
        println!("expected maxes {name}");
        print_scores(scores);
    }
}

fn print_score(name: &str, score: &Score) {
    println!(
        "  {name:<24} {:>6} attempts  made {:>5.1}%  predicted {:>5.1}%  log loss {:.4}  brier {:.4}",
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
