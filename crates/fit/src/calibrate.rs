//! Calibrates the make-rate curve to how often lifters make each attempt.
//!
//! The planner calls a lifter's expected max the weight they make half the time on a
//! normal day, apart from technical misses, so the curve's midpoint is 1 by definition.
//! Lifters are assumed to follow the typical plan seen in the data: an opener at some
//! share of their expected max, then the typical jump after a make or a miss. Their day
//! max is their expected max times 1 plus a normal draw with the day-form spread. An
//! attempt is made when it is not a technical miss, a fixed chance whatever the load,
//! and the load is not too heavy for the day.
//!
//! The fit finds the steepness, technical-miss chance, day-form spread and opener share
//! whose predicted make rates come closest to the observed ones: the opener, and the 2nd
//! and 3rd attempts after a make and after a miss. How often a repeated weight is made
//! after a miss tells the two kinds of miss apart: after a technical miss the repeat is
//! usually made, after a strength miss on a bad day it usually is not.

use crate::minimize::minimize;

/// Each later attempt as a multiple of the attempt before it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Jumps {
    pub second_after_make: f64,
    pub second_after_miss: f64,
    pub third_after_make: f64,
    pub third_after_miss: f64,
}

/// Share of attempts made, the later ones split by whether the attempt before was made.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MakeRates {
    pub opener: f64,
    pub second_after_make: f64,
    pub second_after_miss: f64,
    pub third_after_make: f64,
    pub third_after_miss: f64,
}

impl MakeRates {
    pub fn all(&self) -> [f64; 5] {
        [
            self.opener,
            self.second_after_make,
            self.second_after_miss,
            self.third_after_make,
            self.third_after_miss,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// Steepness of the make-rate curve whose midpoint is the day max.
    pub steepness: f64,
    /// Chance of missing an attempt whatever the load.
    pub technical_miss_chance: f64,
    /// Standard deviation of the day max as a share of the expected max.
    pub spread: f64,
    /// The typical opener as a share of the expected max.
    pub opener_ratio: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    pub params: Params,
    pub predicted: MakeRates,
    /// Sum of squared differences between predicted and observed make rates.
    pub error: f64,
}

const DRAWS: usize = 121;
const DRAW_RANGE: f64 = 6.0;
const LIGHTEST_DAY: f64 = 0.01;

/// Points and weights that average a function over a standard normal draw.
fn normal_draws() -> impl Iterator<Item = (f64, f64)> {
    let width = 2.0 * DRAW_RANGE / (DRAWS - 1) as f64;
    let points = (0..DRAWS).map(move |i| -DRAW_RANGE + i as f64 * width);
    let density = |z: f64| (-z * z / 2.0).exp();
    let total: f64 = points.clone().map(density).sum();
    points.map(move |z| (z, density(z) / total))
}

/// Make rates for lifters with these parameters following the typical jumps, averaged
/// over day form and over the paths the plan takes after makes and misses.
pub fn predicted_make_rates(params: &Params, jumps: &Jumps) -> MakeRates {
    // Chances of each event and of each conditioning event, summed over day form.
    let mut opener = 0.0;
    let [mut second_made_after_make, mut second_made_after_miss] = [0.0; 2];
    let [mut third_made_after_make, mut third_made_after_miss] = [0.0; 2];
    let [mut second_made, mut second_missed] = [0.0; 2];
    for (z, weight) in normal_draws() {
        let day = (1.0 + params.spread * z).max(LIGHTEST_DAY);
        let make = |load: f64| {
            (1.0 - params.technical_miss_chance)
                / (1.0 + (params.steepness * (load / day - 1.0)).exp())
        };
        let first = make(params.opener_ratio);
        opener += weight * first;
        for (opener_made, path, second_load) in [
            (true, first, params.opener_ratio * jumps.second_after_make),
            (
                false,
                1.0 - first,
                params.opener_ratio * jumps.second_after_miss,
            ),
        ] {
            let second = make(second_load);
            if opener_made {
                second_made_after_make += weight * path * second;
            } else {
                second_made_after_miss += weight * path * second;
            }
            second_made += weight * path * second;
            second_missed += weight * path * (1.0 - second);
            third_made_after_make +=
                weight * path * second * make(second_load * jumps.third_after_make);
            third_made_after_miss +=
                weight * path * (1.0 - second) * make(second_load * jumps.third_after_miss);
        }
    }
    MakeRates {
        opener,
        second_after_make: second_made_after_make / opener,
        second_after_miss: second_made_after_miss / (1.0 - opener),
        third_after_make: third_made_after_make / second_made,
        third_after_miss: third_made_after_miss / second_missed,
    }
}

/// The steepness, technical-miss chance and opener share whose predicted make rates
/// come closest to `observed` for a given day-form spread: a coarse grid finds the
/// region, then Nelder-Mead follows the valley where steepness and technical-miss
/// chance trade off against each other.
pub fn calibrate_at_spread(observed: &MakeRates, jumps: &Jumps, spread: f64) -> Calibration {
    // Searched as log steepness and the square root of the technical-miss chance, so
    // every point is valid.
    let params = |[log_steepness, root_chance, opener_ratio]: [f64; 3]| Params {
        steepness: log_steepness.exp(),
        technical_miss_chance: (root_chance * root_chance).min(0.99),
        spread,
        opener_ratio,
    };
    let error =
        |point: [f64; 3]| squared_error(&predicted_make_rates(&params(point), jumps), observed);
    const CELLS: usize = 8;
    let span = |low: f64, high: f64| {
        (0..=CELLS).map(move |i| low + (high - low) * i as f64 / CELLS as f64)
    };
    let start = span(1.0_f64.ln(), 300.0_f64.ln())
        .flat_map(|s| span(0.0, 0.5).map(move |c| (s, c)))
        .flat_map(|(s, c)| span(0.6, 1.1).map(move |o| [s, c, o]))
        .min_by(|a, b| error(*a).total_cmp(&error(*b)))
        .expect("the grid has points");
    let point = minimize(error, start, [0.3, 0.05, 0.02]);
    Calibration {
        params: params(point),
        predicted: predicted_make_rates(&params(point), jumps),
        error: error(point),
    }
}

/// Calibrates every lift with one day-form spread, as the planner uses one spread for
/// all lifts: the spread with the lowest total error, each lift fitted at that spread.
pub fn calibrate_lifts(lifts: &[(MakeRates, Jumps)]) -> Vec<Calibration> {
    let at = |log_spread: f64| -> Vec<Calibration> {
        lifts
            .iter()
            .map(|(observed, jumps)| calibrate_at_spread(observed, jumps, log_spread.exp()))
            .collect()
    };
    let total = |log_spread: f64| at(log_spread).iter().map(|c| c.error).sum::<f64>();
    let [log_spread] = minimize(|[log_spread]| total(log_spread), [0.04_f64.ln()], [0.3]);
    at(log_spread)
}

fn squared_error(predicted: &MakeRates, observed: &MakeRates) -> f64 {
    let error: f64 = predicted
        .all()
        .iter()
        .zip(observed.all())
        .map(|(p, o)| (p - o).powi(2))
        .sum();
    // A rate conditioned on something that never happens is 0 / 0.
    if error.is_finite() {
        error
    } else {
        f64::INFINITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPICAL: Jumps = Jumps {
        second_after_make: 1.05,
        second_after_miss: 1.0,
        third_after_make: 1.04,
        third_after_miss: 1.0,
    };

    fn lifter(steepness: f64, technical_miss_chance: f64, spread: f64) -> Params {
        Params {
            steepness,
            technical_miss_chance,
            spread,
            opener_ratio: 0.9,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn a_certain_day_makes_every_attempt_under_the_max_unless_it_is_a_technical_miss() {
        let rates = predicted_make_rates(&lifter(10_000.0, 0.1, 0.0), &TYPICAL);
        for rate in rates.all() {
            assert!(close(rate, 0.9), "{rates:?}");
        }
    }

    #[test]
    fn a_certain_day_misses_every_attempt_over_the_max() {
        let jumps = Jumps {
            third_after_make: 1.08,
            ..TYPICAL
        };
        let lifter = Params {
            opener_ratio: 0.95,
            ..lifter(10_000.0, 0.1, 0.0)
        };
        let rates = predicted_make_rates(&lifter, &jumps);
        assert!(close(rates.third_after_make, 0.0), "{rates:?}");
        assert!(close(rates.third_after_miss, 0.9), "{rates:?}");
    }

    #[test]
    fn repeats_after_technical_misses_succeed_more_than_after_strength_misses() {
        let technical = predicted_make_rates(&lifter(60.0, 0.1, 0.03), &TYPICAL);
        let strength = predicted_make_rates(&lifter(8.0, 0.0, 0.03), &TYPICAL);
        assert!(
            technical.third_after_miss > strength.third_after_miss,
            "{technical:?} {strength:?}"
        );
    }

    #[test]
    fn rates_stay_probabilities() {
        for steepness in [2.0, 20.0, 200.0] {
            for chance in [0.0, 0.1] {
                let rates = predicted_make_rates(&lifter(steepness, chance, 0.1), &TYPICAL);
                assert!(
                    rates.all().iter().all(|r| (0.0..=1.0).contains(r)),
                    "{rates:?}"
                );
            }
        }
    }

    #[test]
    fn calibration_recovers_the_lifter_behind_the_rates() {
        let truth = lifter(40.0, 0.05, 0.05);
        let observed = predicted_make_rates(&truth, &TYPICAL);
        let fit = calibrate_at_spread(&observed, &TYPICAL, 0.05);
        assert!((fit.params.steepness - 40.0).abs() < 1.0, "{fit:?}");
        assert!(
            (fit.params.technical_miss_chance - 0.05).abs() < 0.002,
            "{fit:?}"
        );
        assert!((fit.params.opener_ratio - 0.9).abs() < 0.002, "{fit:?}");
        assert!(fit.error < 1e-8, "{fit:?}");
    }

    #[test]
    fn lifts_share_the_spread_that_fits_them_all() {
        let lifts: Vec<_> = [lifter(40.0, 0.05, 0.05), lifter(80.0, 0.01, 0.05)]
            .iter()
            .map(|truth| (predicted_make_rates(truth, &TYPICAL), TYPICAL))
            .collect();
        let fits = calibrate_lifts(&lifts);
        for fit in &fits {
            assert!((fit.params.spread - 0.05).abs() < 0.002, "{fit:?}");
        }
        assert!((fits[1].params.steepness - 80.0).abs() < 4.0, "{fits:?}");
    }
}
