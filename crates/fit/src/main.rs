//! Fits the planner's default make-rate curve to OpenPowerlifting results.
//!
//! Usage: `attempt-fit CSV`, where CSV is the OpenPowerlifting bulk CSV from
//! https://data.openpowerlifting.org, unzipped.

use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Instant;

use attempt_fit::calibrate::{MakeRates, calibrate_lifts};
use attempt_fit::data::raw_full_power_attempts;
use attempt_fit::stats::attempt_stats;
use attempt_sim::lift::Lift;

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("give the OpenPowerlifting bulk CSV, from https://data.openpowerlifting.org")?;

    let started = Instant::now();
    let attempts = raw_full_power_attempts(BufReader::new(File::open(&path)?))?;
    println!("make rates: opener, 2nd after make/miss, 3rd after make/miss (%)");
    let stats = Lift::ALL.map(|lift| {
        attempt_stats(attempts.get(lift).iter().copied())
            .ok_or(format!("not enough {lift} attempts"))
    });
    let [squat, bench, deadlift] = stats;
    let stats = [squat?, bench?, deadlift?];
    let fits = calibrate_lifts(&stats.map(|s| (s.make_rates, s.jumps)));
    println!("day-form spread {:.1}%", fits[0].params.spread * 100.0);
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
