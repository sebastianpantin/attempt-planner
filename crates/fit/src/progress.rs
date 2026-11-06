//! How much stronger lifters get from one meet to the next, from lifters' consecutive
//! complete meets.
//!
//! The change between two meets is the mean over the lifts of the log change in the best
//! made attempt. Lifters gain a step at the next meet whatever the time between, larger
//! early on as they learn to compete, and a yearly rate on top that slows with
//! experience and stops after a while. Each experience group gets its own least-squares
//! line in the time between the meets, capped at the one cap that fits all groups best.

use attempt_sim::past_meets::{EXPERIENCE_GROUPS, Progress, Step, years_between};

use crate::attempts::Meet;
use crate::history::bests;

/// The caps on the years between meets tried, every two weeks or so up to 3 years.
const CAPS: std::ops::RangeInclusive<usize> = 2..=75;
const CAP_STEP_YEARS: f64 = 0.04;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgressFit {
    /// Pairs of consecutive meets used, by meets done before the second.
    pub pairs: [usize; EXPERIENCE_GROUPS],
    pub progress: Progress,
}

/// One pair of consecutive meets: the meets done before the second, the years between them
/// and the change.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Change {
    group: usize,
    years: f64,
    log_change: f64,
}

fn changes(lifters: &[Vec<Meet>]) -> Vec<Change> {
    lifters
        .iter()
        .flat_map(|meets| {
            meets
                .array_windows()
                .enumerate()
                .filter_map(|(done, [before, after])| {
                    let before_kg = bests(&before.attempts)?;
                    let log_change = bests(&after.attempts)?
                        .map(|lift, kg| (kg / before_kg.get(lift)).ln())
                        .mean();
                    Some(Change {
                        group: done.min(EXPERIENCE_GROUPS - 1),
                        years: years_between(before.day, after.day),
                        log_change,
                    })
                })
        })
        .collect()
}

/// Sums for a least-squares line.
#[derive(Debug, Clone, Copy, Default)]
struct Line {
    n: f64,
    x: f64,
    y: f64,
    xx: f64,
    xy: f64,
}

impl Line {
    fn add(&mut self, x: f64, y: f64) {
        self.n += 1.0;
        self.x += x;
        self.y += y;
        self.xx += x * x;
        self.xy += x * y;
    }

    /// Intercept and slope, `None` when the points can't fix a line.
    fn fit(&self) -> Option<(f64, f64)> {
        let spread = self.n * self.xx - self.x * self.x;
        (spread > 0.0).then(|| {
            let slope = (self.n * self.xy - self.x * self.y) / spread;
            ((self.y - slope * self.x) / self.n, slope)
        })
    }
}

/// Each group's step with the yearly gain capped at `most_years`.
fn steps(changes: &[Change], most_years: f64) -> Option<[Step; EXPERIENCE_GROUPS]> {
    let mut lines = [Line::default(); EXPERIENCE_GROUPS];
    for change in changes {
        lines[change.group].add(change.years.min(most_years), change.log_change);
    }
    let mut steps = [Step {
        at_next_meet: 0.0,
        per_year: 0.0,
    }; EXPERIENCE_GROUPS];
    for (step, line) in steps.iter_mut().zip(lines) {
        let (at_next_meet, per_year) = line.fit()?;
        *step = Step {
            at_next_meet,
            per_year,
        };
    }
    Some(steps)
}

fn residual(progress: &Progress, change: &Change) -> f64 {
    change.log_change - progress.at_next_meet(change.group + 1, change.years, 0.0)
}

/// How much the spread of the changes grows with the capped years between the meets: the
/// spread of lifters' own yearly gain. Each group's own variance at no time apart, from
/// day form and the step, is left out by comparing changes within a group.
fn yearly_spread(progress: &Progress, changes: &[Change]) -> f64 {
    let mut lines = [Line::default(); EXPERIENCE_GROUPS];
    for change in changes {
        let years = progress.counted_years(change.years);
        lines[change.group].add(years * years, residual(progress, change).powi(2));
    }
    let (mut covariance, mut variance) = (0.0, 0.0);
    for line in lines.iter().filter(|line| line.n > 0.0) {
        covariance += line.xy - line.x * line.y / line.n;
        variance += line.xx - line.x * line.x / line.n;
    }
    (covariance / variance).max(0.0).sqrt()
}

/// The progress that best explains the changes between `lifters`' consecutive meets,
/// `None` when some group has too few pairs to fit.
pub(crate) fn fit_progress(lifters: &[Vec<Meet>]) -> Option<ProgressFit> {
    let changes = changes(lifters);
    let squared_error = |progress: &Progress| -> f64 {
        changes
            .iter()
            .map(|change| residual(progress, change).powi(2))
            .sum()
    };
    let (progress, _) = CAPS
        .filter_map(|cap| {
            let most_years = cap as f64 * CAP_STEP_YEARS;
            let progress = Progress {
                steps: steps(&changes, most_years)?,
                most_years,
                yearly_spread: 0.0,
            };
            Some((progress, squared_error(&progress)))
        })
        .min_by(|(_, a), (_, b)| a.total_cmp(b))?;
    let mut pairs = [0; EXPERIENCE_GROUPS];
    for change in &changes {
        pairs[change.group] += 1;
    }
    Some(ProgressFit {
        pairs,
        progress: Progress {
            yearly_spread: yearly_spread(&progress, &changes),
            ..progress
        },
    })
}

#[cfg(test)]
mod tests {
    use attempt_sim::lift::PerLift;
    use attempt_sim::past_meets::DAYS_PER_YEAR;
    use rand::{RngExt, SeedableRng};
    use rand_distr::StandardNormal;
    use rand_xoshiro::Xoshiro256PlusPlus;

    use super::*;

    fn truth() -> Progress {
        Progress {
            steps: std::array::from_fn(|group| Step {
                at_next_meet: 0.03 / (group + 1) as f64,
                per_year: 0.05 - 0.01 * group as f64,
            }),
            most_years: 1.0,
            yearly_spread: 0.05,
        }
    }

    /// Lifters with eight meets 1 to 30 months apart who progress as `progress` says, each
    /// with a yearly gain of their own, give or take 1% from meet to meet.
    fn simulated_lifters(progress: &Progress, count: usize) -> Vec<Vec<Meet>> {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
        (0..count)
            .map(|_| {
                let own_gain = progress.yearly_spread * rng.sample::<f64, _>(StandardNormal);
                let (mut day, mut log_level) = (0, 0.0);
                (0..8)
                    .map(|done| {
                        if done > 0 {
                            let days = rng.random_range(30..900);
                            day += days;
                            let years = f64::from(days) / DAYS_PER_YEAR;
                            log_level += progress.at_next_meet(done, years, own_gain);
                        }
                        let noise: f64 = rng.sample(StandardNormal);
                        let best = 100.0 * (log_level + 0.01 * noise).exp();
                        Meet {
                            day,
                            attempts: PerLift::from_fn(|_| [best * 0.9, -best, best]),
                        }
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn recovers_the_steps_cap_and_spread_behind_simulated_meets() {
        let truth = truth();
        let fit = fit_progress(&simulated_lifters(&truth, 40_000)).unwrap();
        assert_eq!(fit.pairs[0], 40_000);
        assert_eq!(fit.pairs[EXPERIENCE_GROUPS - 1], 80_000);
        let found = fit.progress;
        for (found, truth) in found.steps.iter().zip(truth.steps) {
            assert!(
                (found.at_next_meet - truth.at_next_meet).abs() < 0.002,
                "{fit:?}"
            );
            assert!((found.per_year - truth.per_year).abs() < 0.004, "{fit:?}");
        }
        assert!((found.most_years - truth.most_years).abs() < 0.1, "{fit:?}");
        assert!(
            (found.yearly_spread - truth.yearly_spread).abs() < 0.01,
            "{fit:?}"
        );
    }

    #[test]
    fn skips_pairs_with_a_lift_never_made() {
        let meet = |day, bench: [f64; 3]| Meet {
            day,
            attempts: PerLift {
                squat: [100.0, 105.0, 110.0],
                bench,
                deadlift: [100.0, 105.0, 110.0],
            },
        };
        let lifter = vec![
            meet(0, [60.0, 62.5, 65.0]),
            meet(100, [-70.0, -70.0, -70.0]),
            meet(200, [60.0, 62.5, 65.0]),
        ];
        assert!(changes(&[lifter]).is_empty());
    }
}
