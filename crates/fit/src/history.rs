//! How much a lifter's results move from one meet to the next, from lifters who competed
//! more than once.

use std::ops::RangeInclusive;

use attempt_sim::lift::{Lift, PerLift};

use crate::calibrate::OpenerMisses;
use crate::data::Entry;
use crate::stats::is_complete;

/// Meets closer than this are too close for a full peak in between, and meets further
/// apart mix in too much training progress.
const DAYS_BETWEEN_MEETS: RangeInclusive<i32> = 28..=120;

/// Scales a median absolute deviation to a standard deviation for normal data.
const MAD_TO_SD: f64 = 1.4826;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeetToMeet {
    /// Pairs of meets by the same lifter used.
    pub pairs: usize,
    /// Each lift's spread of one meet's best around the lifter's level, as a fraction.
    pub spread: PerLift<f64>,
    /// Correlation of the change in best between two lifts: squat–bench, squat–deadlift,
    /// bench–deadlift.
    pub correlations: [f64; 3],
}

impl MeetToMeet {
    pub fn mean_spread(&self) -> f64 {
        self.spread.mean()
    }

    pub fn mean_correlation(&self) -> f64 {
        self.correlations.iter().sum::<f64>() / 3.0
    }
}

/// Each lift's best made attempt, `None` unless every attempt was recorded and each lift
/// has a make.
fn bests(attempts: &PerLift<[f64; 3]>) -> Option<PerLift<f64>> {
    let mut bests = PerLift::from_fn(|_| 0.0);
    for (lift, attempts) in attempts.iter() {
        let best = attempts.iter().copied().fold(0.0, f64::max);
        if best == 0.0 || !attempts.iter().all(|kg| kg.is_finite() && *kg != 0.0) {
            return None;
        }
        *bests.get_mut(lift) = best;
    }
    Some(bests)
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

/// A robust standard deviation, so a few sandbagged or injured meets don't dominate.
fn robust_sd(values: &[f64]) -> f64 {
    let centre = median(&mut values.to_vec());
    let mut deviations: Vec<f64> = values.iter().map(|x| (x - centre).abs()).collect();
    MAD_TO_SD * median(&mut deviations)
}

fn correlation(xs: &[f64], ys: &[f64]) -> f64 {
    let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len() as f64;
    let (mean_x, mean_y) = (mean(xs), mean(ys));
    let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
    for (x, y) in xs.iter().zip(ys) {
        let (dx, dy) = (x - mean_x, y - mean_y);
        xy += dx * dy;
        xx += dx * dx;
        yy += dy * dy;
    }
    xy / (xx * yy).sqrt()
}

/// Compares each lifter's consecutive full meets 28 to 120 days apart. A change between two
/// meets carries two meets' worth of variation, so each spread is the robust spread of the
/// log change divided by √2. `None` with fewer than two pairs.
pub fn meet_to_meet(entries: &[Entry]) -> Option<MeetToMeet> {
    let mut meets: Vec<(&str, i32, PerLift<f64>)> = entries
        .iter()
        .filter_map(|entry| Some((entry.lifter.as_str(), entry.day?, bests(&entry.attempts)?)))
        .collect();
    meets.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));

    let mut changes = PerLift::from_fn(|_| Vec::new());
    for [(lifter, day, before), (next_lifter, next_day, after)] in meets.array_windows() {
        if lifter != next_lifter || !DAYS_BETWEEN_MEETS.contains(&(next_day - day)) {
            continue;
        }
        for lift in Lift::ALL {
            changes
                .get_mut(lift)
                .push((after.get(lift) / before.get(lift)).ln());
        }
    }
    let pairs = changes.squat.len();
    if pairs < 2 {
        return None;
    }
    Some(MeetToMeet {
        pairs,
        spread: changes.map(|_, changes| robust_sd(changes) / 2f64.sqrt()),
        correlations: [
            correlation(&changes.squat, &changes.bench),
            correlation(&changes.squat, &changes.deadlift),
            correlation(&changes.bench, &changes.deadlift),
        ],
    })
}

/// Each lift's [`OpenerMisses`] over every lifter's consecutive meets where the lift's
/// attempts are complete. Same-day entries, such as one meet in two divisions, count once.
pub fn opener_misses(entries: &[Entry]) -> PerLift<OpenerMisses> {
    let mut meets: Vec<(&str, i32, &PerLift<[f64; 3]>)> = entries
        .iter()
        .filter_map(|entry| Some((entry.lifter.as_str(), entry.day?, &entry.attempts)))
        .collect();
    meets.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    meets.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    // Indexed by whether the opener before was missed: meets, then missed openers.
    let mut counts = PerLift::from_fn(|_| [[0usize; 2]; 2]);
    for [(lifter, _, before), (next_lifter, _, after)] in meets.array_windows() {
        if lifter != next_lifter {
            continue;
        }
        for lift in Lift::ALL {
            let (before, after) = (before.get(lift), after.get(lift));
            if !is_complete(*before) || !is_complete(*after) {
                continue;
            }
            let missed_before = usize::from(before[0] < 0.0);
            let counts = &mut counts.get_mut(lift)[missed_before];
            counts[0] += 1;
            counts[1] += usize::from(after[0] < 0.0);
        }
    }
    counts.map(
        |_,
         [
            [after_make, missed_after_make],
            [after_miss, missed_after_miss],
        ]| {
            OpenerMisses {
                after_miss: *missed_after_miss as f64 / *after_miss as f64,
                after_make: *missed_after_make as f64 / *after_make as f64,
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(lifter: &str, day: i32, bests: [f64; 3]) -> Entry {
        let [squat, bench, deadlift] = bests.map(|best| [best * 0.9, best, -best * 1.02]);
        Entry {
            lifter: lifter.to_owned(),
            day: Some(day),
            attempts: PerLift {
                squat,
                bench,
                deadlift,
            },
        }
    }

    #[test]
    fn pairs_each_lifters_meets_in_date_order_within_the_window() {
        let entries = [
            entry("A", 100, [210.0, 105.0, 252.0]),
            entry("B", 0, [50.0, 50.0, 50.0]),
            entry("A", 0, [200.0, 100.0, 240.0]),
            entry("A", 300, [400.0, 400.0, 400.0]),
            entry("B", 20, [100.0, 100.0, 100.0]),
            entry("C", 50, [150.0, 150.0, 150.0]),
            entry("B", 60, [110.0, 99.0, 100.0]),
        ];
        let result = meet_to_meet(&entries).unwrap();
        assert_eq!(result.pairs, 2);
        let spread_of = |a: f64, b: f64| MAD_TO_SD * (a.ln() - b.ln()).abs() / 2.0 / 2f64.sqrt();
        assert!((result.spread.squat - spread_of(1.05, 1.1)).abs() < 1e-12);
        assert!((result.spread.bench - spread_of(1.05, 0.99)).abs() < 1e-12);
        assert!((result.spread.deadlift - spread_of(1.05, 1.0)).abs() < 1e-12);
        assert!((result.correlations[0] + 1.0).abs() < 1e-12);
        assert!((result.correlations[2] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn skips_meets_with_a_missing_attempt_or_a_bombed_lift() {
        let mut bombed = entry("A", 30, [210.0, 105.0, 250.0]);
        bombed.attempts.bench = [-110.0, -110.0, -110.0];
        let mut missing = entry("A", 40, [210.0, 105.0, 250.0]);
        missing.attempts.squat[2] = f64::NAN;
        let entries = [entry("A", 0, [200.0, 100.0, 240.0]), bombed, missing];
        assert_eq!(meet_to_meet(&entries), None);
    }

    #[test]
    fn counts_opener_misses_after_a_miss_and_after_a_make_at_the_meet_before() {
        let opener = |lifter: &str, day: i32, opener: f64| {
            let mut entry = entry(lifter, day, [100.0, 100.0, 100.0]);
            entry.attempts.squat[0] = opener;
            entry
        };
        let entries = [
            opener("A", 0, -90.0),
            opener("A", 50, -90.0),
            opener("A", 50, 90.0),
            opener("A", 100, 90.0),
            opener("A", 400, -90.0),
            opener("B", 0, 90.0),
            opener("C", 0, -90.0),
        ];
        let squat = opener_misses(&entries).squat;
        assert_eq!(squat.after_miss, 0.5);
        assert_eq!(squat.after_make, 1.0);
    }

    #[test]
    fn correlation_follows_the_sign_of_the_relationship() {
        let xs = [1.0, 2.0, 3.0, 4.0];
        assert!((correlation(&xs, &[2.0, 4.0, 6.0, 8.0]) - 1.0).abs() < 1e-12);
        assert!((correlation(&xs, &[8.0, 6.0, 4.0, 2.0]) + 1.0).abs() < 1e-12);
    }
}
