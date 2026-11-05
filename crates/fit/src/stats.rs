//! What lifters do at meets: how often each attempt is made and how far they jump.

use std::ops::RangeInclusive;

use crate::calibrate::{Jumps, MakeRates};

/// Jumps outside this range, as a multiple of the attempt before, are taken to be
/// recording errors.
const PLAUSIBLE_JUMP: RangeInclusive<f64> = 0.9..=1.25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttemptStats {
    pub meets: usize,
    pub make_rates: MakeRates,
    /// Median jumps, split by whether the attempt before was made.
    pub jumps: Jumps,
}

/// Whether every attempt was recorded and each jump is plausible, with missed attempts
/// negative as OpenPowerlifting records them.
pub fn is_complete(attempts: [f64; 3]) -> bool {
    if !attempts.iter().all(|a| a.is_finite() && *a != 0.0) {
        return false;
    }
    let [first_kg, second_kg, third_kg] = attempts.map(f64::abs);
    PLAUSIBLE_JUMP.contains(&(second_kg / first_kg))
        && PLAUSIBLE_JUMP.contains(&(third_kg / second_kg))
}

/// Summarises one lift's attempts, three per meet, with missed attempts negative as
/// OpenPowerlifting records them. Skips meets with a missing or implausible attempt.
/// `None` when some kind of jump never happens.
pub fn attempt_stats(meets: impl IntoIterator<Item = [f64; 3]>) -> Option<AttemptStats> {
    let mut count = 0;
    let mut openers_made = 0;
    // Indexed by whether the attempt before was made: attempts, then makes.
    let mut second_attempts = [0usize; 2];
    let mut second_makes = [0usize; 2];
    let mut third_attempts = [0usize; 2];
    let mut third_makes = [0usize; 2];
    let mut second = [Vec::new(), Vec::new()];
    let mut third = [Vec::new(), Vec::new()];
    for attempts in meets {
        if !is_complete(attempts) {
            continue;
        }
        let [first_kg, second_kg, third_kg] = attempts.map(f64::abs);
        let (to_second, to_third) = (second_kg / first_kg, third_kg / second_kg);
        let [first_made, second_made, third_made] = attempts.map(|a| usize::from(a > 0.0));
        count += 1;
        openers_made += first_made;
        second_attempts[first_made] += 1;
        second_makes[first_made] += second_made;
        third_attempts[second_made] += 1;
        third_makes[second_made] += third_made;
        second[first_made].push(to_second);
        third[second_made].push(to_third);
    }
    let [second_after_miss, second_after_make] = second.map(median);
    let [third_after_miss, third_after_make] = third.map(median);
    let share = |made: usize, out_of: usize| made as f64 / out_of as f64;
    Some(AttemptStats {
        meets: count,
        make_rates: MakeRates {
            opener: share(openers_made, count),
            second_after_make: share(second_makes[1], second_attempts[1]),
            second_after_miss: share(second_makes[0], second_attempts[0]),
            third_after_make: share(third_makes[1], third_attempts[1]),
            third_after_miss: share(third_makes[0], third_attempts[0]),
        },
        jumps: Jumps {
            second_after_make: second_after_make?,
            second_after_miss: second_after_miss?,
            third_after_make: third_after_make?,
            third_after_miss: third_after_miss?,
        },
    })
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    match values.len() {
        0 => None,
        n if n % 2 == 1 => Some(values[middle]),
        _ => Some((values[middle - 1] + values[middle]) / 2.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_rates_are_split_by_the_attempt_before() {
        let stats = attempt_stats([
            [100.0, 105.0, -110.0],
            [100.0, -105.0, 105.0],
            [-100.0, 100.0, 105.0],
            [100.0, 105.0, 110.0],
        ])
        .unwrap();
        assert_eq!(stats.meets, 4);
        let third = 2.0 / 3.0;
        assert_eq!(
            stats.make_rates,
            MakeRates {
                opener: 0.75,
                second_after_make: third,
                second_after_miss: 1.0,
                third_after_make: third,
                third_after_miss: 1.0,
            }
        );
    }

    #[test]
    fn jumps_are_medians_split_by_the_previous_outcome() {
        let stats = attempt_stats([
            [100.0, 104.0, -110.0],
            [100.0, 106.0, 110.0],
            [100.0, 105.0, -112.5],
            [-100.0, 100.0, 104.0],
            [-100.0, 102.0, -106.0],
            [100.0, -105.0, 105.0],
        ])
        .unwrap();
        let close = |a: f64, b: f64| (a - b).abs() < 1e-12;
        assert!(close(stats.jumps.second_after_make, 1.05), "{stats:?}");
        assert!(close(stats.jumps.second_after_miss, 1.01), "{stats:?}");
        assert!(close(stats.jumps.third_after_make, 1.04), "{stats:?}");
        assert!(close(stats.jumps.third_after_miss, 1.0), "{stats:?}");
    }

    #[test]
    fn skips_meets_with_missing_or_implausible_attempts() {
        let stats = attempt_stats([
            [100.0, 105.0, -110.0],
            [-100.0, -100.0, 105.0],
            [100.0, 0.0, 110.0],
            [100.0, 210.0, 215.0],
            [100.0, f64::NAN, 110.0],
        ])
        .unwrap();
        assert_eq!(stats.meets, 2);
    }

    #[test]
    fn needs_every_kind_of_jump() {
        assert_eq!(attempt_stats([[100.0, 105.0, 110.0]]), None);
        assert_eq!(attempt_stats([]), None);
    }
}
