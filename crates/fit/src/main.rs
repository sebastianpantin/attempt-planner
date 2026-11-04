//! Fits the planner's default make-rate curve to OpenPowerlifting results.
//!
//! Usage: `attempt-fit [--federation NAME] CSV`, where CSV is the OpenPowerlifting bulk CSV
//! from https://data.openpowerlifting.org, unzipped. `--federation IPF` keeps only meets of
//! the IPF and its member federations.

use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Instant;

use attempt_fit::calibrate::{MakeRates, calibrate_lifts};
use attempt_fit::data::raw_full_power_entries;
use attempt_fit::history::meet_to_meet;
use attempt_fit::stats::attempt_stats;
use attempt_sim::lift::Lift;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let mut federation = None;
    let mut path = None;
    while let Some(arg) = args.next() {
        if arg == "--federation" {
            federation = Some(
                args.next()
                    .ok_or("--federation needs a name, such as IPF")?,
            );
        } else {
            path = Some(PathBuf::from(arg));
        }
    }
    let path =
        path.ok_or("give the OpenPowerlifting bulk CSV, from https://data.openpowerlifting.org")?;

    let started = Instant::now();
    let entries =
        raw_full_power_entries(BufReader::new(File::open(&path)?), federation.as_deref())?;
    println!("make rates: opener, 2nd after make/miss, 3rd after make/miss (%)");
    let stats = Lift::ALL.map(|lift| {
        attempt_stats(entries.iter().map(|entry| *entry.attempts.get(lift)))
            .ok_or(format!("not enough {lift} attempts"))
    });
    let [squat, bench, deadlift] = stats;
    let stats = [squat?, bench?, deadlift?];
    let fits = calibrate_lifts(&stats.map(|s| (s.make_rates, s.jumps)));
    let total_spread = fits[0].params.spread;
    println!("spread of the day max {:.1}%", total_spread * 100.0);
    let history = meet_to_meet(&entries).ok_or("no lifter has two meets to compare")?;
    let day_form = history.mean_spread();
    println!(
        "  meet to meet ({} pairs): day form {:.1}% (squat {:.1}%, bench {:.1}%, deadlift {:.1}%)",
        history.pairs,
        day_form * 100.0,
        history.spread.squat * 100.0,
        history.spread.bench * 100.0,
        history.spread.deadlift * 100.0
    );
    println!(
        "  so expected-max uncertainty {:.1}%",
        (total_spread.powi(2) - day_form.powi(2)).max(0.0).sqrt() * 100.0
    );
    let [squat_bench, squat_deadlift, bench_deadlift] = history.correlations;
    println!(
        "correlation between lifts {:.2} (squat–bench {squat_bench:.2}, squat–deadlift \
         {squat_deadlift:.2}, bench–deadlift {bench_deadlift:.2})",
        history.mean_correlation()
    );
    let rates = |rates: MakeRates| rates.all().map(|r| format!("{:.1}", r * 100.0)).join(" ");
    for ((lift, stats), fit) in Lift::ALL.iter().zip(stats).zip(fits) {
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
    println!("took {:.1} s", started.elapsed().as_secs_f64());
    Ok(())
}
