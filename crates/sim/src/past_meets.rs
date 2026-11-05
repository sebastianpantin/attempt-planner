//! Estimates a lifter's expected maxes from the attempts they took at past meets.
//!
//! Each meet's day max is the lifter's level on that date moved by day form, and the
//! level grows by a steady yearly rate. The estimate is the level, today, and rate that
//! best explain which attempts were made and missed, given the make-rate curves. Openers
//! also count as evidence: lifters tend to open at a typical share of their max, which
//! keeps the estimate finite for a lifter who never misses. The rate is only fitted when
//! the meets fall on at least two days, and only carried a short way past the last meet.

use std::fmt;

use crate::lift::{Lift, PerLift};
use crate::minimize::minimize;
use crate::model::{DayForm, MakeRateCurve};
use crate::normal::normal_draws;

/// How far a lifter's opener usually strays from the typical share of their max.
const OPENER_SPREAD: f64 = 0.05;
/// The page never sets the uncertainty of the expected max below this.
const LEAST_SHARED_UNCERTAINTY: f64 = 0.005;
/// How fast a lifter is assumed to get stronger before the meets say otherwise: no change
/// on average, give or take 10% a year.
const YEARLY_PROGRESS_SPREAD: f64 = 0.1;
const DAYS_PER_YEAR: f64 = 365.25;
/// How far past the latest meet the yearly rate is carried, about 6 months; after that the
/// level is held flat. A trend fitted over a few meets says little about the years after
/// them, and lifters plateau, detrain or get hurt.
const MAX_TREND_DAYS: i32 = 183;
const LIGHTEST_DAY: f64 = 0.01;
const PARAMETERS: usize = 4;
const PROGRESS: usize = 3;

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
    /// How much stronger the lifter gets in a year, as a fraction, shared by all lifts. 0
    /// when the meets fall on fewer than two days, which cannot show a trend.
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

/// The expected maxes on `on_day` that best explain the past meets, with how sure the
/// estimate is.
pub fn estimate_maxes(input: &MaxEstimateInput) -> Result<MaxEstimate, Vec<EstimateProblem>> {
    let problems = problems(input);
    if !problems.is_empty() {
        return Err(problems);
    }
    // Each lift's level is searched as a log ratio to its heaviest attempt, so every
    // parameter starts near 0 on the same scale.
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
    let meet_days = || {
        input
            .meets
            .iter()
            .filter(|meet| {
                Lift::ALL
                    .iter()
                    .any(|&lift| !meet.attempts.get(lift).is_empty())
            })
            .map(|meet| meet.day)
    };
    let latest_day = meet_days().max().unwrap_or(input.on_day);
    let fits_trend = meet_days().any(|day| day != latest_day);
    // The level is fitted on the day the trend stops and held flat from there.
    let level_day = input.on_day.min(latest_day.saturating_add(MAX_TREND_DAYS));
    let years_before = |meet: &PastMeet| f64::from(level_day - meet.day) / DAYS_PER_YEAR;
    let cost = |x: [f64; PARAMETERS]| {
        // Without a trend the progress parameter only meets its prior, which keeps it at 0
        // and its second derivative positive, so the covariance can still be found.
        let mut cost = (x[PROGRESS] / YEARLY_PROGRESS_SPREAD).powi(2) / 2.0;
        let progress = if fits_trend { x[PROGRESS] } else { 0.0 };
        for lift in Lift::ALL {
            let index = lift as usize;
            if *heaviest.get(lift) == 0.0 {
                cost += x[index].powi(2) / 2.0;
                continue;
            }
            let level_kg = heaviest.get(lift) * x[index].exp();
            for meet in &input.meets {
                let attempts = meet.attempts.get(lift);
                let Some(opener) = attempts.first() else {
                    continue;
                };
                let level_then_kg = level_kg * (-progress * years_before(meet)).exp();
                let chance = chance_of_outcomes(
                    input.curves.get(lift),
                    attempts,
                    level_then_kg,
                    input.day_form_spread,
                );
                cost -= chance.max(f64::MIN_POSITIVE).ln();
                let opener_ratio = (opener.kg / level_then_kg).ln();
                let typical = input.typical_opener.get(lift).ln();
                cost += ((opener_ratio - typical) / OPENER_SPREAD).powi(2) / 2.0;
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
        // Since the latest meet the level drifts like a random walk, with the day-form
        // spread as its yearly spread, on top of the fit's own spread.
        let years_since = f64::from(input.on_day - latest_day).max(0.0) / DAYS_PER_YEAR;
        let fit_spread = covariance.map_or(f64::NAN, |c| c[index][index].sqrt());
        Some(LiftEstimate {
            expected_max_kg: heaviest.get(lift) * best[index].exp(),
            uncertainty: fit_spread.hypot(input.day_form_spread * years_since.sqrt()),
            meets: lift_days().count(),
        })
    });
    Ok(MaxEstimate {
        shared_uncertainty: shared_uncertainty(&lifts),
        lifts,
        yearly_progress: if fits_trend {
            best[PROGRESS].exp_m1()
        } else {
            0.0
        },
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
        }
    }

    /// A lifter at `level_kg` on day 0, getting `yearly_growth` stronger each year, who
    /// opens at the typical share and jumps 5% and then 4% after a make.
    fn simulated_meets(level_kg: PerLift<f64>, yearly_growth: f64, count: i32) -> Vec<PastMeet> {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
        (0..count)
            .map(|i| {
                let day = i * 60;
                let growth = (1.0 + yearly_growth).powf(f64::from(day) / DAYS_PER_YEAR);
                let attempts = level_kg.map(|_, &level_kg| {
                    let level_kg = level_kg * growth;
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
                PastMeet { day, attempts }
            })
            .collect()
    }

    fn levels() -> PerLift<f64> {
        PerLift {
            squat: 200.0,
            bench: 130.0,
            deadlift: 240.0,
        }
    }

    #[test]
    fn finds_a_steady_lifters_maxes() {
        let meets = simulated_meets(levels(), 0.0, 12);
        let estimate = estimate_maxes(&input(meets, 700)).unwrap();
        for (lift, level) in levels().iter() {
            let found = estimate.lifts.get(lift).unwrap();
            assert_eq!(found.meets, 12);
            assert!(
                (found.expected_max_kg / level - 1.0).abs() < 0.025,
                "{lift}: {found:?}"
            );
            assert!(
                found.uncertainty > 0.0 && found.uncertainty < 0.03,
                "{found:?}"
            );
        }
        assert!(estimate.yearly_progress.abs() < 0.04, "{estimate:?}");
    }

    #[test]
    fn follows_a_lifter_getting_stronger() {
        let meets = simulated_meets(levels(), 0.1, 12);
        let last_day = meets.last().unwrap().day;
        let estimate = estimate_maxes(&input(meets, last_day)).unwrap();
        assert!(
            (estimate.yearly_progress - 0.1).abs() < 0.05,
            "{estimate:?}"
        );
        let growth = 1.1f64.powf(f64::from(last_day) / DAYS_PER_YEAR);
        let squat = estimate.lifts.squat.unwrap().expected_max_kg;
        assert!((squat / (200.0 * growth) - 1.0).abs() < 0.03, "{squat}");
    }

    #[test]
    fn carries_the_trend_at_most_about_six_months_past_the_last_meet() {
        let meets = simulated_meets(levels(), 0.1, 12);
        let last_day = meets.last().unwrap().day;
        let at = |on_day| estimate_maxes(&input(meets.clone(), on_day)).unwrap();
        let capped = at(last_day + MAX_TREND_DAYS);
        let stale = at(last_day + 600);
        let squat = |estimate: &MaxEstimate| estimate.lifts.squat.unwrap().expected_max_kg;
        assert_eq!(squat(&stale), squat(&capped));
        assert!(squat(&capped) > squat(&at(last_day)), "{capped:?}");
        let projected = 200.0 * 1.1f64.powf(f64::from(last_day + 600) / DAYS_PER_YEAR);
        assert!(squat(&stale) < projected * 0.97, "{stale:?}");
    }

    #[test]
    fn a_single_meet_day_shows_no_trend() {
        let meets = simulated_meets(levels(), 0.0, 1);
        let estimate = estimate_maxes(&input(meets.clone(), 400)).unwrap();
        assert_eq!(estimate.yearly_progress, 0.0);
        let on_the_day = estimate_maxes(&input(meets, 0)).unwrap();
        assert_eq!(
            estimate.lifts.squat.unwrap().expected_max_kg,
            on_the_day.lifts.squat.unwrap().expected_max_kg
        );
        let uncertainty = on_the_day.lifts.squat.unwrap().uncertainty;
        assert!(uncertainty > 0.0 && uncertainty < 0.1, "{uncertainty}");
    }

    #[test]
    fn the_lifts_share_their_average_uncertainty_and_at_least_half_a_percent() {
        let estimate = estimate_maxes(&input(simulated_meets(levels(), 0.0, 3), 700)).unwrap();
        let mean = Lift::ALL
            .iter()
            .map(|&lift| estimate.lifts.get(lift).unwrap().uncertainty)
            .sum::<f64>()
            / 3.0;
        assert!((estimate.shared_uncertainty - mean.max(0.005)).abs() < 1e-12);
        let sure = estimate_maxes(&input(simulated_meets(levels(), 0.0, 40), 0)).unwrap();
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
        let meets = simulated_meets(levels(), 0.0, 12);
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
            let estimate = estimate_maxes(&input(simulated_meets(levels(), 0.0, count), 700));
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
