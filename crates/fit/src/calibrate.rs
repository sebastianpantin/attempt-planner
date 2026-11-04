//! Calibrates the make-rate curve to how often lifters make each attempt.
//!
//! The planner calls a lifter's expected max the weight they make half the time on a
//! normal day, so the curve's midpoint is 1 by definition and only its steepness is
//! fitted. Lifters are assumed to follow the typical plan seen in the data: an opener at
//! some share of their expected max, then the typical jump after a make or a miss. Their
//! day max moves by the planner's day-form spread. The fit finds the steepness and
//! opener share whose predicted make rates for the 1st, 2nd and 3rd attempts are closest
//! to the observed ones.

/// Each later attempt as a multiple of the attempt before it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Jumps {
    pub second_after_make: f64,
    pub second_after_miss: f64,
    pub third_after_make: f64,
    pub third_after_miss: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    /// Steepness of the make-rate curve whose midpoint is the day max.
    pub steepness: f64,
    /// The typical opener as a share of the expected max.
    pub opener_ratio: f64,
    pub predicted: [f64; 3],
    /// Sum of squared differences between predicted and observed make rates.
    pub error: f64,
}

const DRAWS: usize = 201;
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

/// Chance of making each of the three attempts, averaged over day form and over the
/// paths the plan takes after makes and misses.
pub fn predicted_make_rates(
    steepness: f64,
    opener_ratio: f64,
    jumps: &Jumps,
    spread: f64,
) -> [f64; 3] {
    let mut rates = [0.0; 3];
    for (z, weight) in normal_draws() {
        let day = (1.0 + spread * z).max(LIGHTEST_DAY);
        let make = |load: f64| 1.0 / (1.0 + (steepness * (load / day - 1.0)).exp());
        let opener = make(opener_ratio);
        let mut second = 0.0;
        let mut third = 0.0;
        for (second_load, second_path) in [
            (opener_ratio * jumps.second_after_make, opener),
            (opener_ratio * jumps.second_after_miss, 1.0 - opener),
        ] {
            let second_made = make(second_load);
            second += second_path * second_made;
            third += second_path
                * (second_made * make(second_load * jumps.third_after_make)
                    + (1.0 - second_made) * make(second_load * jumps.third_after_miss));
        }
        for (rate, value) in rates.iter_mut().zip([opener, second, third]) {
            *rate += weight * value;
        }
    }
    rates
}

/// The steepness and opener share whose predicted make rates come closest to `observed`.
pub fn calibrate(observed: [f64; 3], jumps: &Jumps, spread: f64) -> Calibration {
    let error = |log_steepness: f64, opener_ratio: f64| {
        let predicted = predicted_make_rates(log_steepness.exp(), opener_ratio, jumps, spread);
        predicted
            .iter()
            .zip(observed)
            .map(|(p, o)| (p - o).powi(2))
            .sum::<f64>()
    };
    const CELLS: usize = 20;
    let (mut steepness_range, mut opener_range) = ((1.0_f64.ln(), 300.0_f64.ln()), (0.6, 1.1));
    let mut best = (0.0, 0.0, f64::INFINITY);
    for _ in 0..15 {
        let at = |(low, high): (f64, f64), i: usize| low + (high - low) * i as f64 / CELLS as f64;
        for i in 0..=CELLS {
            for j in 0..=CELLS {
                let (s, o) = (at(steepness_range, i), at(opener_range, j));
                let e = error(s, o);
                if e < best.2 {
                    best = (s, o, e);
                }
            }
        }
        let around = |(low, high): (f64, f64), centre: f64| {
            let reach = 2.0 * (high - low) / CELLS as f64;
            (centre - reach, centre + reach)
        };
        steepness_range = around(steepness_range, best.0);
        opener_range = around(opener_range, best.1);
    }
    let (log_steepness, opener_ratio, error) = best;
    Calibration {
        steepness: log_steepness.exp(),
        opener_ratio,
        predicted: predicted_make_rates(log_steepness.exp(), opener_ratio, jumps, spread),
        error,
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

    fn assert_close(actual: [f64; 3], expected: [f64; 3]) {
        let close = actual
            .iter()
            .zip(expected)
            .all(|(a, e)| (a - e).abs() < 1e-9);
        assert!(close, "{actual:?} != {expected:?}");
    }

    #[test]
    fn a_certain_day_makes_every_attempt_under_the_max() {
        assert_close(
            predicted_make_rates(10_000.0, 0.9, &TYPICAL, 0.0),
            [1.0, 1.0, 1.0],
        );
    }

    #[test]
    fn a_certain_day_misses_an_attempt_over_the_max_and_keeps_missing() {
        let jumps = Jumps {
            third_after_make: 1.08,
            ..TYPICAL
        };
        assert_close(
            predicted_make_rates(10_000.0, 0.9, &jumps, 0.0),
            [1.0, 1.0, 0.0],
        );
    }

    #[test]
    fn heavier_attempts_are_made_less_often() {
        let [opener, second, third] = predicted_make_rates(25.0, 0.9, &TYPICAL, 0.03);
        assert!(
            opener > second && second > third,
            "{opener} {second} {third}"
        );
    }

    #[test]
    fn rates_stay_probabilities() {
        for steepness in [2.0, 20.0, 200.0] {
            for rate in predicted_make_rates(steepness, 0.95, &TYPICAL, 0.1) {
                assert!((0.0..=1.0).contains(&rate));
            }
        }
    }

    #[test]
    fn calibration_recovers_the_curve_behind_the_rates() {
        let observed = predicted_make_rates(25.0, 0.9, &TYPICAL, 0.03);
        let fit = calibrate(observed, &TYPICAL, 0.03);
        assert!((fit.steepness - 25.0).abs() < 0.5, "{fit:?}");
        assert!((fit.opener_ratio - 0.9).abs() < 0.002, "{fit:?}");
        assert!(fit.error < 1e-6, "{fit:?}");
    }
}
