//! Estimates a lifter's expected maxes from the attempts they took at past meets.
//!
//! Each meet's day max is the lifter's level on that date moved by day form. The level
//! grows from meet to meet as lifters' levels typically do (see [`PROGRESS`]), plus a
//! yearly gain of the lifter's own. The estimate is the level today and the own gain that
//! best explain which attempts were made and missed, given the make-rate curves. Openers
//! also count as evidence: lifters tend to open at a typical share of their max, which
//! keeps the estimate finite for a lifter who never misses.

use std::fmt;

use crate::lift::{Lift, PerLift};
use crate::minimize::minimize;
use crate::model::{DayForm, MakeRateCurve};
use crate::normal::normal_draws;

/// How far a lifter's opener usually strays from the typical share of their max.
const OPENER_SPREAD: f64 = 0.05;
/// The page never sets the uncertainty of the expected max below this.
const LEAST_SHARED_UNCERTAINTY: f64 = 0.005;
pub const DAYS_PER_YEAR: f64 = 365.25;

/// Years from one day to another, both in days since 1970-01-01.
pub fn years_between(from_day: i32, to_day: i32) -> f64 {
    f64::from(to_day - from_day) / DAYS_PER_YEAR
}
const LIGHTEST_DAY: f64 = 0.01;
const PARAMETERS: usize = 4;
const OWN_GAIN: usize = 3;
/// Lifters are told apart by meets done, 1 to 5 and then 6 or more.
pub const EXPERIENCE_GROUPS: usize = 6;

/// How much stronger a lifter gets after a meet, as a log ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    /// At the next meet, whenever it comes.
    pub at_next_meet: f64,
    /// For each year until then.
    pub per_year: f64,
}

/// How much stronger lifters get from one meet to the next before their own meets say
/// otherwise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    /// By meets done, see [`EXPERIENCE_GROUPS`].
    pub steps: [Step; EXPERIENCE_GROUPS],
    /// Years between two meets after which the yearly gain stops: lifters plateau, detrain
    /// or get hurt.
    pub most_years: f64,
    /// Standard deviation of a lifter's own yearly gain around the typical one.
    pub yearly_spread: f64,
}

impl Progress {
    /// The step after `meets_done` meets.
    pub fn step(&self, meets_done: usize) -> Step {
        self.steps[meets_done.clamp(1, EXPERIENCE_GROUPS) - 1]
    }

    /// The years of yearly gain counted in `years` between two meets.
    pub fn counted_years(&self, years: f64) -> f64 {
        years.min(self.most_years)
    }

    /// How much stronger, as a log ratio, a lifter who gains `own_gain` a year more than
    /// typical is at the meet after their `meets_done`th, `years` later.
    pub fn at_next_meet(&self, meets_done: usize, years: f64, own_gain: f64) -> f64 {
        let step = self.step(meets_done);
        step.at_next_meet + (step.per_year + own_gain) * self.counted_years(years)
    }
}

/// Measured by the fit crate from lifters' consecutive complete meets in IPF federations:
/// the next meet's best lifts against the last's. A new lifter is 3% stronger at their
/// next meet even a month later, as they learn to compete, and 5% more for each year
/// between; after six meets the step is about 1% and the yearly gain is gone.
pub const PROGRESS: Progress = Progress {
    steps: [
        Step {
            at_next_meet: 0.0314,
            per_year: 0.048,
        },
        Step {
            at_next_meet: 0.0207,
            per_year: 0.0304,
        },
        Step {
            at_next_meet: 0.0179,
            per_year: 0.019,
        },
        Step {
            at_next_meet: 0.0161,
            per_year: 0.011,
        },
        Step {
            at_next_meet: 0.016,
            per_year: 0.0028,
        },
        Step {
            at_next_meet: 0.0122,
            per_year: -0.0068,
        },
    ],
    most_years: 1.16,
    yearly_spread: 0.078,
};

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct PastAttempt {
    pub kg: f64,
    pub made: bool,
}

/// One past meet: its date and each lift's attempts in order. A lift the lifter did not
/// do has no attempts.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct PastMeet {
    /// Days since 1970-01-01.
    pub day: i32,
    pub attempts: PerLift<Vec<PastAttempt>>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct MaxEstimateInput {
    pub curves: PerLift<MakeRateCurve>,
    /// Typical opener as a share of the expected max, for each lift.
    pub typical_opener: PerLift<f64>,
    /// Day-to-day spread of the day max (see `DayForm::spread`).
    pub day_form_spread: f64,
    pub meets: Vec<PastMeet>,
    /// The day to estimate the maxes for, in days since 1970-01-01.
    pub on_day: i32,
    /// Meets the lifter did before the ones in `meets`, as typical progress slows with
    /// meets done. 0 when absent: `meets` are all the lifter's meets.
    #[cfg_attr(feature = "serde", serde(default))]
    pub meets_before: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct LiftEstimate {
    pub expected_max_kg: f64,
    /// Standard deviation of the estimate, as a fraction of it. Grows with the time since
    /// the lift's latest meet.
    pub uncertainty: f64,
    /// Meets with at least one attempt at this lift.
    pub meets: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct MaxEstimate {
    /// `None` for a lift with no attempts.
    pub lifts: PerLift<Option<LiftEstimate>>,
    /// How much stronger the lifter gets in a year now, as a fraction, shared by all lifts:
    /// the typical gain after as many meets, and the lifter's own.
    pub yearly_progress: f64,
    /// One uncertainty of the expected max for every lift, as the planner takes it: the
    /// average over the estimated lifts, and at least 0.5%.
    pub shared_uncertainty: f64,
}

impl MaxEstimate {
    /// The estimate after also taking planned openers as evidence, one per lift. See
    /// [`LiftEstimate::with_opener`].
    pub fn with_openers(&self, openers_kg: &PerLift<f64>, typical_opener: &PerLift<f64>) -> Self {
        let lifts = self.lifts.map(|lift, estimate| {
            estimate.map(|estimate| {
                estimate.with_opener(*openers_kg.get(lift), *typical_opener.get(lift))
            })
        });
        MaxEstimate {
            shared_uncertainty: shared_uncertainty(&lifts),
            lifts,
            yearly_progress: self.yearly_progress,
        }
    }
}

/// See [`MaxEstimate::shared_uncertainty`].
fn shared_uncertainty(lifts: &PerLift<Option<LiftEstimate>>) -> f64 {
    let estimated: Vec<f64> = Lift::ALL
        .iter()
        .filter_map(|&lift| Some(lifts.get(lift).as_ref()?.uncertainty))
        .collect();
    let mean = estimated.iter().sum::<f64>() / estimated.len() as f64;
    // An uncertainty that could not be worked out stays NaN rather than the surest value.
    if mean.is_nan() {
        mean
    } else {
        mean.max(LEAST_SHARED_UNCERTAINTY)
    }
}

impl LiftEstimate {
    /// The estimate after also taking a planned opener as evidence: an opener that strays
    /// from the typical share of the max as much as openers do.
    pub fn with_opener(&self, opener_kg: f64, typical_opener: f64) -> LiftEstimate {
        let from_opener = (opener_kg / typical_opener).ln();
        let [past_weight, opener_weight] = [self.uncertainty, OPENER_SPREAD].map(|sd| sd.powi(-2));
        let weight = past_weight + opener_weight;
        let log_kg =
            (past_weight * self.expected_max_kg.ln() + opener_weight * from_opener) / weight;
        LiftEstimate {
            expected_max_kg: log_kg.exp(),
            uncertainty: weight.powf(-0.5),
            meets: self.meets,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstimateProblem {
    NoAttempts,
    InvalidAttempt,
    InvalidCurve { lift: Lift },
    InvalidOpener { lift: Lift },
    InvalidSpread,
}

impl fmt::Display for EstimateProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAttempts => f.write_str("no past meet has an attempt"),
            Self::InvalidAttempt => f.write_str("every past attempt needs a weight above 0 kg"),
            Self::InvalidCurve { lift } => write!(f, "the {lift} make-rate curve is not valid"),
            Self::InvalidOpener { lift } => {
                write!(
                    f,
                    "the typical {lift} opener must be a share between 0 and 1"
                )
            }
            Self::InvalidSpread => write!(
                f,
                "day-form spread must be from 0 to {}",
                DayForm::MAX_SPREAD
            ),
        }
    }
}

impl std::error::Error for EstimateProblem {}

fn problems(input: &MaxEstimateInput) -> Vec<EstimateProblem> {
    let mut problems = Vec::new();
    let attempts = || {
        input
            .meets
            .iter()
            .flat_map(|meet| Lift::ALL.map(|lift| meet.attempts.get(lift)))
            .flatten()
    };
    if attempts().next().is_none() {
        problems.push(EstimateProblem::NoAttempts);
    }
    if attempts().any(|attempt| !(attempt.kg.is_finite() && attempt.kg > 0.0)) {
        problems.push(EstimateProblem::InvalidAttempt);
    }
    for lift in Lift::ALL {
        if !input.curves.get(lift).is_valid() {
            problems.push(EstimateProblem::InvalidCurve { lift });
        }
        let opener = *input.typical_opener.get(lift);
        if !(opener > 0.0 && opener <= 1.0) {
            problems.push(EstimateProblem::InvalidOpener { lift });
        }
    }
    if !(0.0..=DayForm::MAX_SPREAD).contains(&input.day_form_spread) {
        problems.push(EstimateProblem::InvalidSpread);
    }
    problems
}

/// Chance of exactly these makes and misses, averaged over day form.
fn chance_of_outcomes(
    curve: &MakeRateCurve,
    attempts: &[PastAttempt],
    level_kg: f64,
    spread: f64,
) -> f64 {
    normal_draws()
        .map(|(z, weight)| {
            let day_max_kg = level_kg * (1.0 + spread * z).max(LIGHTEST_DAY);
            let chance: f64 = attempts
                .iter()
                .map(|attempt| {
                    let make = curve.make_probability_kg(attempt.kg, day_max_kg);
                    if attempt.made { make } else { 1.0 - make }
                })
                .product();
            weight * chance
        })
        .sum()
}

/// Inverts a symmetric positive-definite matrix, `None` when it is not one.
fn invert<const N: usize>(mut matrix: [[f64; N]; N]) -> Option<[[f64; N]; N]> {
    let mut inverse: [[f64; N]; N] =
        std::array::from_fn(|i| std::array::from_fn(|j| f64::from(u8::from(i == j))));
    for column in 0..N {
        let pivot = matrix[column][column];
        if pivot.is_nan() || pivot <= 0.0 {
            return None;
        }
        for j in 0..N {
            matrix[column][j] /= pivot;
            inverse[column][j] /= pivot;
        }
        for row in (0..N).filter(|&row| row != column) {
            let factor = matrix[row][column];
            for j in 0..N {
                matrix[row][j] -= factor * matrix[column][j];
                inverse[row][j] -= factor * inverse[column][j];
            }
        }
    }
    Some(inverse)
}

/// Second derivatives of `f` at `x`, by central differences.
fn hessian<const N: usize>(f: impl Fn([f64; N]) -> f64, x: [f64; N]) -> [[f64; N]; N] {
    const STEP: f64 = 1e-3;
    let at = |moves: &[(usize, f64)]| {
        let mut point = x;
        for &(i, by) in moves {
            point[i] += by;
        }
        f(point)
    };
    std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            (at(&[(i, STEP), (j, STEP)])
                - at(&[(i, STEP), (j, -STEP)])
                - at(&[(i, -STEP), (j, STEP)])
                + at(&[(i, -STEP), (j, -STEP)]))
                / (4.0 * STEP * STEP)
        })
    })
}

/// How much stronger the lifter is on `on_day` than at each past meet, as a log ratio:
/// `fixed + own_gain * years`, where `own_gain` is how much faster than typical the lifter
/// gets stronger each year. Each meet day adds its step, and `on_day` too when it comes
/// after every meet.
struct GainsSince {
    /// For each of the input's meets, in its order.
    fixed: Vec<f64>,
    years: Vec<f64>,
    /// Including the meets before the input's.
    meets_done: usize,
}

impl GainsSince {
    /// `input` needs a meet with an attempt, as [`problems`] checks.
    fn new(progress: &Progress, input: &MaxEstimateInput) -> Self {
        let mut days: Vec<i32> = input
            .meets
            .iter()
            .filter(|meet| {
                Lift::ALL
                    .iter()
                    .any(|&lift| !meet.attempts.get(lift).is_empty())
            })
            .map(|meet| meet.day)
            .collect();
        days.sort_unstable();
        days.dedup();
        // The gain from the first meet day to each meet day.
        let (mut fixed, mut years) = (vec![0.0], vec![0.0]);
        for (done, pair) in days.windows(2).enumerate() {
            let between = years_between(pair[0], pair[1]);
            let meets_done = input.meets_before.saturating_add(done + 1);
            fixed.push(fixed[done] + progress.at_next_meet(meets_done, between, 0.0));
            years.push(years[done] + progress.counted_years(between));
        }
        // And to `on_day`, from the last meet day on or before it.
        let done = days.partition_point(|&day| day <= input.on_day);
        let last = done.saturating_sub(1);
        let between = years_between(days[last], input.on_day).max(0.0);
        let meets_done = input.meets_before.saturating_add(done);
        let past_every_meet = done == days.len() && between > 0.0;
        let gain_then = if past_every_meet {
            progress.at_next_meet(meets_done, between, 0.0)
        } else {
            progress.step(meets_done).per_year * progress.counted_years(between)
        };
        let (fixed_then, years_then) = (
            fixed[last] + gain_then,
            years[last] + progress.counted_years(between),
        );
        // A meet without attempts after the last one with attempts gets that one's gain,
        // which nothing reads.
        let day_index = |day: i32| {
            days.partition_point(|&meet_day| meet_day < day)
                .min(days.len() - 1)
        };
        let since = |gains: &[f64], then: f64| {
            input
                .meets
                .iter()
                .map(|meet| then - gains[day_index(meet.day)])
                .collect()
        };
        GainsSince {
            fixed: since(&fixed, fixed_then),
            years: since(&years, years_then),
            meets_done: input.meets_before.saturating_add(days.len()),
        }
    }

    /// Since the input's meet at `meet`.
    fn since(&self, meet: usize, own_gain: f64) -> f64 {
        self.fixed[meet] + own_gain * self.years[meet]
    }
}

/// The expected maxes on `on_day` that best explain the past meets, with how sure the
/// estimate is.
pub fn estimate_maxes(input: &MaxEstimateInput) -> Result<MaxEstimate, Vec<EstimateProblem>> {
    let problems = problems(input);
    if !problems.is_empty() {
        return Err(problems);
    }
    // Each lift's level on `on_day` is searched as a log ratio to its heaviest attempt, so
    // every parameter starts near 0 on the same scale.
    let heaviest = input
        .meets
        .iter()
        .fold(PerLift::from_fn(|_| 0.0), |heaviest, meet| {
            heaviest.map(|lift, &kg: &f64| {
                meet.attempts
                    .get(lift)
                    .iter()
                    .map(|a| a.kg)
                    .fold(kg, f64::max)
            })
        });
    let progress = &PROGRESS;
    let gains = GainsSince::new(progress, input);
    let log_typical_opener = input.typical_opener.map(|_, share| share.ln());
    let cost = |x: [f64; PARAMETERS]| {
        let own_gain = x[OWN_GAIN];
        let mut cost = (own_gain / progress.yearly_spread).powi(2) / 2.0;
        // A lift with no attempts only meets this, which keeps its parameter at 0.
        for lift in Lift::ALL
            .into_iter()
            .filter(|&lift| *heaviest.get(lift) == 0.0)
        {
            cost += x[lift as usize].powi(2) / 2.0;
        }
        let level_kg = heaviest.map(|lift, kg| kg * x[lift as usize].exp());
        for (index, meet) in input.meets.iter().enumerate() {
            let shrink = (-gains.since(index, own_gain)).exp();
            for lift in Lift::ALL {
                let attempts = meet.attempts.get(lift);
                let Some(opener) = attempts.first() else {
                    continue;
                };
                let level_then_kg = level_kg.get(lift) * shrink;
                let chance = chance_of_outcomes(
                    input.curves.get(lift),
                    attempts,
                    level_then_kg,
                    input.day_form_spread,
                );
                cost -= chance.max(f64::MIN_POSITIVE).ln();
                let opener_ratio = (opener.kg / level_then_kg).ln();
                cost +=
                    ((opener_ratio - log_typical_opener.get(lift)) / OPENER_SPREAD).powi(2) / 2.0;
            }
        }
        cost
    };
    let steps = [0.05; PARAMETERS];
    let best = minimize(cost, minimize(cost, [0.0; PARAMETERS], steps), steps);
    let covariance = invert(hessian(cost, best));
    let lifts = PerLift::from_fn(|lift| {
        let index = lift as usize;
        let lift_days = || {
            input
                .meets
                .iter()
                .filter(|meet| !meet.attempts.get(lift).is_empty())
                .map(|meet| meet.day)
        };
        let latest_day = lift_days().max()?;
        // Since the latest meet the level also drifts like a random walk, with the day-form
        // spread as its yearly spread, on top of the fit's own spread.
        let years_since = years_between(latest_day, input.on_day).max(0.0);
        let fit_spread = covariance.map_or(f64::NAN, |c| c[index][index].sqrt());
        Some(LiftEstimate {
            expected_max_kg: heaviest.get(lift) * best[index].exp(),
            uncertainty: fit_spread.hypot(input.day_form_spread * years_since.sqrt()),
            meets: lift_days().count(),
        })
    });
    let step = progress.step(gains.meets_done);
    Ok(MaxEstimate {
        shared_uncertainty: shared_uncertainty(&lifts),
        lifts,
        yearly_progress: (step.per_year + best[OWN_GAIN]).exp_m1(),
    })
}

#[cfg(test)]
mod tests {
    use rand::{RngExt, SeedableRng};
    use rand_distr::StandardNormal;
    use rand_xoshiro::Xoshiro256PlusPlus;

    use super::*;

    const CURVE: MakeRateCurve = MakeRateCurve {
        ratio_at_half: 1.0,
        steepness: 60.0,
        technical_miss_chance: 0.0,
    };
    const SPREAD: f64 = 0.03;
    const OPENER: f64 = 0.9;

    fn input(meets: Vec<PastMeet>, on_day: i32) -> MaxEstimateInput {
        MaxEstimateInput {
            curves: PerLift::from_fn(|_| CURVE),
            typical_opener: PerLift::from_fn(|_| OPENER),
            day_form_spread: SPREAD,
            meets,
            on_day,
            meets_before: 0,
        }
    }

    /// How much stronger a lifter gets in `days` after `meets_done` meets, as a ratio, when
    /// they progress as lifters typically do plus `own_gain` a year, if the meet after
    /// comes then.
    fn growth(meets_done: usize, days: i32, own_gain: f64) -> f64 {
        let years = f64::from(days) / DAYS_PER_YEAR;
        PROGRESS.at_next_meet(meets_done, years, own_gain).exp()
    }

    /// A lifter at `level_kg` at their first meet, with a meet every 60 days, progressing
    /// as lifters typically do plus `own_gain` a year, who opens at the typical share and
    /// jumps 5% and then 4% after a make. Also the levels at the last meet.
    fn simulated_meets(
        level_kg: PerLift<f64>,
        own_gain: f64,
        count: usize,
    ) -> (Vec<PastMeet>, PerLift<f64>) {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
        let mut level_kg = level_kg;
        let meets = (0..count)
            .map(|done| {
                if done > 0 {
                    let growth = growth(done, 60, own_gain);
                    level_kg = level_kg.map(|_, kg| kg * growth);
                }
                let attempts = level_kg.map(|_, &level_kg| {
                    let form: f64 = rng.sample(StandardNormal);
                    let day_max_kg = level_kg * (1.0 + SPREAD * form);
                    let mut kg = (level_kg * OPENER / 2.5).round() * 2.5;
                    let mut attempts = Vec::new();
                    for jump in [0.05, 0.04, 0.0] {
                        let made = rng.random::<f64>()
                            < CURVE.strength_make_probability(crate::testing::kg(kg), day_max_kg);
                        attempts.push(PastAttempt { kg, made });
                        if made {
                            kg = ((kg * (1.0 + jump)) / 2.5).round() * 2.5;
                        }
                    }
                    attempts
                });
                PastMeet {
                    day: done as i32 * 60,
                    attempts,
                }
            })
            .collect();
        (meets, level_kg)
    }

    fn levels() -> PerLift<f64> {
        PerLift {
            squat: 200.0,
            bench: 130.0,
            deadlift: 240.0,
        }
    }

    #[test]
    fn finds_a_typical_lifters_maxes_at_their_next_meet() {
        let (meets, last_kg) = simulated_meets(levels(), 0.0, 12);
        let last_day = meets.last().unwrap().day;
        let estimate = estimate_maxes(&input(meets, last_day + 40)).unwrap();
        for (lift, last_kg) in last_kg.iter() {
            let found = estimate.lifts.get(lift).unwrap();
            assert_eq!(found.meets, 12);
            let expected_kg = last_kg * growth(12, 40, 0.0);
            assert!(
                (found.expected_max_kg / expected_kg - 1.0).abs() < 0.025,
                "{lift}: {found:?}"
            );
            assert!(
                found.uncertainty > 0.0 && found.uncertainty < 0.03,
                "{found:?}"
            );
        }
        let typical = PROGRESS.step(12).per_year.exp_m1();
        assert!(
            (estimate.yearly_progress - typical).abs() < 0.04,
            "{estimate:?}"
        );
    }

    #[test]
    fn follows_a_lifter_getting_stronger_than_typical() {
        let (meets, last_kg) = simulated_meets(levels(), 0.1, 12);
        let last_day = meets.last().unwrap().day;
        let estimate = estimate_maxes(&input(meets, last_day)).unwrap();
        let expected = (PROGRESS.step(12).per_year + 0.1).exp_m1();
        assert!(
            (estimate.yearly_progress - expected).abs() < 0.05,
            "{estimate:?}"
        );
        let squat = estimate.lifts.squat.unwrap().expected_max_kg;
        assert!((squat / last_kg.squat - 1.0).abs() < 0.03, "{squat}");
    }

    #[test]
    fn carries_the_yearly_gain_only_so_far_past_the_last_meet() {
        let (meets, _) = simulated_meets(levels(), 0.1, 12);
        let last_day = meets.last().unwrap().day;
        let at = |on_day| estimate_maxes(&input(meets.clone(), on_day)).unwrap();
        let most_days = (PROGRESS.most_years * DAYS_PER_YEAR).ceil() as i32;
        let capped = at(last_day + most_days);
        let stale = at(last_day + 2 * most_days);
        let squat = |estimate: &MaxEstimate| estimate.lifts.squat.unwrap().expected_max_kg;
        assert_eq!(squat(&stale), squat(&capped));
        assert!(squat(&capped) > squat(&at(last_day + 1)), "{capped:?}");
    }

    #[test]
    fn a_new_lifter_is_expected_to_progress_as_new_lifters_do() {
        let (meets, _) = simulated_meets(levels(), 0.0, 1);
        let next_meet = estimate_maxes(&input(meets.clone(), 120)).unwrap();
        let on_the_day = estimate_maxes(&input(meets, 0)).unwrap();
        let squat = |estimate: &MaxEstimate| estimate.lifts.squat.unwrap().expected_max_kg;
        let grown = squat(&next_meet) / squat(&on_the_day);
        assert!((grown / growth(1, 120, 0.0) - 1.0).abs() < 1e-3, "{grown}");
        assert!(
            (next_meet.yearly_progress - PROGRESS.step(1).per_year.exp_m1()).abs() < 1e-3,
            "{next_meet:?}"
        );
        let uncertainty = on_the_day.lifts.squat.unwrap().uncertainty;
        assert!(uncertainty > 0.0 && uncertainty < 0.1, "{uncertainty}");
        assert!(next_meet.lifts.squat.unwrap().uncertainty > uncertainty);
    }

    #[test]
    fn adds_each_meets_step_and_the_yearly_gain_between_meet_days() {
        let meet = |day| PastMeet {
            day,
            attempts: PerLift {
                squat: vec![PastAttempt {
                    kg: 100.0,
                    made: true,
                }],
                bench: vec![],
                deadlift: vec![],
            },
        };
        let gains = |on_day| {
            let input = input(vec![meet(100), meet(0), meet(100)], on_day);
            GainsSince::new(&PROGRESS, &input)
        };
        let [first, second] = [PROGRESS.step(1), PROGRESS.step(2)];
        let years = |days: i32| f64::from(days) / DAYS_PER_YEAR;
        let own_gain = 0.02;
        let after = gains(500);
        let to_next = second.at_next_meet
            + (second.per_year + own_gain) * years(400).min(PROGRESS.most_years);
        assert!((after.since(0, own_gain) - to_next).abs() < 1e-12);
        assert!((after.since(2, own_gain) - to_next).abs() < 1e-12);
        let first_to_second = first.at_next_meet + (first.per_year + own_gain) * years(100);
        assert!((after.since(1, own_gain) - first_to_second - to_next).abs() < 1e-12);
        let between = gains(50);
        let halfway = (first.per_year + own_gain) * years(50);
        assert!((between.since(1, own_gain) - halfway).abs() < 1e-12);
        assert!((between.since(0, own_gain) - (halfway - first_to_second)).abs() < 1e-12);
    }

    #[test]
    fn skips_a_meet_without_attempts_after_the_others() {
        let (mut meets, _) = simulated_meets(levels(), 0.0, 2);
        let last_day = meets.last().unwrap().day;
        let with_empty = |meets: Vec<PastMeet>| estimate_maxes(&input(meets, last_day + 100));
        let without = with_empty(meets.clone()).unwrap();
        meets.push(PastMeet {
            day: last_day + 50,
            attempts: PerLift::from_fn(|_| Vec::new()),
        });
        assert_eq!(with_empty(meets).unwrap(), without);
    }

    #[test]
    fn counts_the_meets_before_the_ones_given() {
        let (meets, _) = simulated_meets(levels(), 0.0, 2);
        let last_day = meets.last().unwrap().day;
        let estimate = |meets_before| {
            let input = MaxEstimateInput {
                meets_before,
                ..input(meets.clone(), last_day + 200)
            };
            let gains = GainsSince::new(&PROGRESS, &input);
            (gains, estimate_maxes(&input).unwrap())
        };
        let ((new_gains, new), (veteran_gains, veteran)) = (estimate(0), estimate(8));
        let years = |days: i32| f64::from(days) / DAYS_PER_YEAR;
        let new_to_last = PROGRESS.at_next_meet(1, years(60), 0.0);
        let new_since_last = PROGRESS.at_next_meet(2, years(200), 0.0);
        assert!((new_gains.since(0, 0.0) - new_to_last - new_since_last).abs() < 1e-12);
        let veteran_to_last = PROGRESS.at_next_meet(9, years(60), 0.0);
        let veteran_since_last = PROGRESS.at_next_meet(10, years(200), 0.0);
        let veteran_since = veteran_to_last + veteran_since_last;
        assert!((veteran_gains.since(0, 0.0) - veteran_since).abs() < 1e-12);
        let squat = |estimate: &MaxEstimate| estimate.lifts.squat.unwrap().expected_max_kg;
        assert!(squat(&veteran) < squat(&new), "{veteran:?} {new:?}");
    }

    #[test]
    fn the_lifts_share_their_average_uncertainty_and_at_least_half_a_percent() {
        let estimate = estimate_maxes(&input(simulated_meets(levels(), 0.0, 3).0, 700)).unwrap();
        let mean = Lift::ALL
            .iter()
            .map(|&lift| estimate.lifts.get(lift).unwrap().uncertainty)
            .sum::<f64>()
            / 3.0;
        assert!((estimate.shared_uncertainty - mean.max(0.005)).abs() < 1e-12);
        let sure = estimate_maxes(&input(simulated_meets(levels(), 0.0, 40).0, 0)).unwrap();
        assert!(sure.shared_uncertainty >= 0.005, "{sure:?}");
    }

    #[test]
    fn an_opener_moves_the_estimate_by_how_sure_each_is() {
        let estimate = LiftEstimate {
            expected_max_kg: 100.0,
            uncertainty: OPENER_SPREAD,
            meets: 2,
        };
        let informed = estimate.with_opener(99.0, 0.9);
        let midpoint = ((100f64.ln() + (99.0f64 / 0.9).ln()) / 2.0).exp();
        assert!(
            (informed.expected_max_kg - midpoint).abs() < 1e-9,
            "{informed:?}"
        );
        assert!((informed.uncertainty - OPENER_SPREAD / 2f64.sqrt()).abs() < 1e-12);
        let sure = LiftEstimate {
            uncertainty: 0.001,
            ..estimate
        };
        assert!((sure.with_opener(99.0, 0.9).expected_max_kg - 100.0).abs() < 0.1);
    }

    #[test]
    fn an_older_history_is_less_sure() {
        let (meets, _) = simulated_meets(levels(), 0.0, 12);
        let last_day = meets.last().unwrap().day;
        let uncertainty = |on_day| {
            let estimate = estimate_maxes(&input(meets.clone(), on_day)).unwrap();
            estimate.lifts.bench.unwrap().uncertainty
        };
        let fresh = uncertainty(last_day);
        let stale = uncertainty(last_day + 730);
        assert!(stale > fresh + 0.02, "{fresh} {stale}");
    }

    #[test]
    fn more_meets_make_the_estimate_surer() {
        let uncertainty = |count| {
            let estimate = estimate_maxes(&input(simulated_meets(levels(), 0.0, count).0, 700));
            estimate.unwrap().lifts.bench.unwrap().uncertainty
        };
        assert!(uncertainty(2) > uncertainty(12));
    }

    #[test]
    fn a_lifter_who_never_misses_still_gets_a_finite_max() {
        let made = |kg| PastAttempt { kg, made: true };
        let meet = PastMeet {
            day: 0,
            attempts: PerLift {
                squat: vec![made(180.0), made(190.0), made(197.5)],
                bench: vec![],
                deadlift: vec![],
            },
        };
        let estimate = estimate_maxes(&input(vec![meet], 0)).unwrap();
        let squat = estimate.lifts.squat.unwrap().expected_max_kg;
        assert!(squat > 197.5 && squat < 215.0, "{squat}");
        assert_eq!(estimate.lifts.bench, None);
    }

    #[test]
    fn reports_every_problem() {
        let mut bad = input(vec![], 0);
        bad.day_form_spread = -1.0;
        bad.typical_opener.bench = 1.5;
        assert_eq!(
            estimate_maxes(&bad).unwrap_err(),
            [
                EstimateProblem::NoAttempts,
                EstimateProblem::InvalidOpener { lift: Lift::Bench },
                EstimateProblem::InvalidSpread
            ]
        );
        let meet = PastMeet {
            day: 0,
            attempts: PerLift {
                squat: vec![PastAttempt {
                    kg: -5.0,
                    made: true,
                }],
                bench: vec![],
                deadlift: vec![],
            },
        };
        assert_eq!(
            estimate_maxes(&input(vec![meet], 0)).unwrap_err(),
            [EstimateProblem::InvalidAttempt]
        );
    }

    #[test]
    fn inverts_a_symmetric_matrix() {
        let inverse = invert([[4.0, 2.0], [2.0, 3.0]]).unwrap();
        let expected = [[0.375, -0.25], [-0.25, 0.5]];
        for (row, expected_row) in inverse.iter().zip(expected) {
            for (value, expected) in row.iter().zip(expected_row) {
                assert!((value - expected).abs() < 1e-12);
            }
        }
        assert_eq!(invert([[0.0, 1.0], [1.0, 0.0]]), None);
    }
}
