//! Fits the planner's default make-rate curve to OpenPowerlifting results.
//!
//! Usage: `attempt-fit [PARQUET] [--spread FRACTION]`. PARQUET defaults to `$OPL_PARQUET`,
//! the file that openpowerlifting-mcp's `scripts/fetch_data.py` writes. The spread is
//! the planner's default day-form spread, 0.03.

use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

use attempt_fit::calibrate::calibrate;
use attempt_fit::data::raw_full_power_attempts;
use attempt_fit::stats::attempt_stats;
use attempt_sim::lift::Lift;

const DEFAULT_SPREAD: f64 = 0.03;

fn main() -> Result<(), Box<dyn Error>> {
    let mut path = std::env::var_os("OPL_PARQUET").map(PathBuf::from);
    let mut spread = DEFAULT_SPREAD;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--spread" {
            spread = args.next().ok_or("--spread needs a value")?.parse()?;
        } else {
            path = Some(PathBuf::from(arg));
        }
    }
    let path = path.ok_or(
        "give the Parquet file or set OPL_PARQUET; openpowerlifting-mcp's \
         scripts/fetch_data.py writes it",
    )?;

    let started = Instant::now();
    let attempts = raw_full_power_attempts(&path)?;
    println!("day-form spread {spread}");
    println!(
        "{:<9} {:>7}  {:>17}  {:>23}  {:>9}  {:>6}  {:>17}",
        "lift", "meets", "made 1st/2nd/3rd", "jumps make, miss", "steepness", "opener", "predicted"
    );
    for lift in Lift::ALL {
        let stats = attempt_stats(attempts.get(lift).iter().copied())
            .ok_or(format!("not enough {lift} attempts"))?;
        let fit = calibrate(stats.make_rates, &stats.jumps, spread);
        let rates = |rates: [f64; 3]| rates.map(|r| format!("{:.1}", r * 100.0)).join("/");
        let jumps = stats.jumps;
        println!(
            "{:<9} {:>7}  {:>17}  {:>23}  {:>9.1}  {:>5.1}%  {:>17}",
            lift.to_string(),
            stats.meets,
            rates(stats.make_rates),
            format!(
                "{:.3},{:.3} {:.3},{:.3}",
                jumps.second_after_make,
                jumps.second_after_miss,
                jumps.third_after_make,
                jumps.third_after_miss
            ),
            fit.steepness,
            fit.opener_ratio * 100.0,
            rates(fit.predicted),
        );
    }
    println!("took {:.1} s", started.elapsed().as_secs_f64());
    Ok(())
}
