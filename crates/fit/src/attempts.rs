//! The chance of each attempt of a lifter's meet, given the attempts before it, with the
//! expected maxes estimated from the lifter's past meets: the model that the fit to real
//! attempts and the backtest share.
//!
//! Lifters choose their attempts knowing how their training and their day are going,
//! which past meets don't show. The opener stands in for that as evidence of the expected
//! max, and the 2nd and 3rd attempts as evidence of the day max.

use std::borrow::Cow;
use std::sync::LazyLock;

use attempt_sim::lift::{Lift, PerLift};
use attempt_sim::model::{DayForm, MakeRateCurve};
use attempt_sim::normal::normal_grid;
use attempt_sim::past_meets::{
    LiftEstimate, MaxEstimate, MaxEstimateInput, PastAttempt, PastMeet, estimate_maxes,
};

use crate::data::{Entry, by_lifter_and_day};
use crate::fitted::Model;
use crate::stats::is_complete;

/// Grids of normal draws the chances are averaged over, coarser than the planner's. The
/// backtest's is plenty for curves as soft as the fitted ones. The fit, which works out
/// the chances thousands of times, uses a coarser one still: its log loss differs from a
/// 241-point grid's by under 0.0001.
pub static BACKTEST_GRID: LazyLock<Vec<(f64, f64)>> = LazyLock::new(|| normal_grid(61, 6.0));
pub static FIT_GRID: LazyLock<Vec<(f64, f64)>> = LazyLock::new(|| normal_grid(31, 6.0));

/// Which grid of day draws to average over: the fit's or the backtest's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grid {
    Fit,
    Backtest,
}

impl Grid {
    /// The grid's draws of each part of the day max, from a Student-t with `dof` degrees of
    /// freedom scaled to unit variance, or normal for infinite `dof`. A t's tails reach
    /// further, so its grid does too, keeping the spacing of the normal one.
    pub fn draws(self, dof: f64) -> Cow<'static, [(f64, f64)]> {
        if dof.is_infinite() {
            return Cow::Borrowed(match self {
                Grid::Fit => FIT_GRID.as_slice(),
                Grid::Backtest => BACKTEST_GRID.as_slice(),
            });
        }
        let count = match self {
            Grid::Fit => 51,
            Grid::Backtest => 101,
        };
        Cow::Owned(student_t_grid(count, 10.0, dof))
    }
}

/// `count` evenly spaced points weighted by a Student-t density with `dof` degrees of
/// freedom, scaled so that the grid's own variance is 1: the cut-off tails would otherwise
/// shrink the spread.
pub fn student_t_grid(count: usize, range: f64, dof: f64) -> Vec<(f64, f64)> {
    let grid: Vec<(f64, f64)> = normal_grid(count, range)
        .into_iter()
        .map(|(z, _)| (z, student_t_density(z, dof)))
        .collect();
    let total: f64 = grid.iter().map(|(_, density)| density).sum();
    let variance: f64 = grid.iter().map(|(z, density)| density * z * z).sum::<f64>() / total;
    grid.into_iter()
        .map(|(z, density)| (z / variance.sqrt(), density / total))
        .collect()
}

/// The Student-t density at `z`, up to a constant, normal for infinite `dof`.
pub fn student_t_density(z: f64, dof: f64) -> f64 {
    if dof.is_infinite() {
        (-z * z / 2.0).exp()
    } else {
        (1.0 + z * z / dof).powf(-(dof + 1.0) / 2.0)
    }
}

/// Heavier tails than the planner's model, each neutral at its default: research into
/// why hard attempts are made more often than the model allows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tails {
    /// Degrees of freedom of the Student-t that each part of the day max is drawn from,
    /// scaled to the same spread. Infinite for the planner's normal.
    pub day_form_dof: f64,
    /// Each lift's chance of making a load however far it is above the day max, short of a
    /// technical miss.
    pub floors: PerLift<f64>,
    /// Degrees of freedom of the Student-t that a chosen share of the day max is drawn
    /// from. Infinite for normal.
    pub choice_dof: f64,
}

impl Tails {
    pub const NORMAL: Tails = Tails {
        day_form_dof: f64::INFINITY,
        floors: PerLift {
            squat: 0.0,
            bench: 0.0,
            deadlift: 0.0,
        },
        choice_dof: f64::INFINITY,
    };
}

/// The chance of making `load` on a day max of `day_max`, with a `floor` under the part
/// that depends on the load.
fn make_chance(curve: &MakeRateCurve, floor: f64, load: f64, day_max: f64) -> f64 {
    let strength = curve.make_probability_kg(load, day_max);
    floor * (1.0 - curve.technical_miss_chance) + (1.0 - floor) * strength
}

/// How lifters choose their 2nd and 3rd attempts at a lift knowing how the day is going:
/// at a typical share of their day max, give or take a spread. Taken as evidence, a heavy
/// chosen attempt says the day is good.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadChoice {
    /// The log of the typical share of the day max: 2nd attempt after a made and after a
    /// missed opener, then 3rd attempt after a made and after a missed 2nd.
    pub log_shares: [f64; 4],
    /// Standard deviation of the log of the chosen share around the typical one.
    pub spread: f64,
}

/// One complete meet: each lift's attempts in kg, negative when missed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meet {
    pub day: i32,
    pub attempts: PerLift<[f64; 3]>,
}

impl Meet {
    fn past(&self) -> PastMeet {
        PastMeet {
            day: self.day,
            attempts: self.attempts.map(|_, attempts| {
                attempts
                    .iter()
                    .map(|&kg| PastAttempt {
                        kg: kg.abs(),
                        made: kg > 0.0,
                    })
                    .collect()
            }),
        }
    }
}

/// Each lifter's complete meets in date order, one per day, for lifters with two or more.
pub fn lifters_meets(entries: &[Entry]) -> Vec<Vec<Meet>> {
    let complete = entries.iter().filter(|entry| {
        Lift::ALL
            .iter()
            .all(|&lift| is_complete(*entry.attempts.get(lift)))
    });
    let complete: Vec<(&str, Meet)> = by_lifter_and_day(complete)
        .into_iter()
        .map(|(lifter, day, entry)| {
            let attempts = entry.attempts;
            (lifter, Meet { day, attempts })
        })
        .collect();
    complete
        .chunk_by(|a, b| a.0 == b.0)
        .filter(|meets| meets.len() >= 2)
        .map(|meets| meets.iter().map(|(_, meet)| *meet).collect())
        .collect()
}

/// Each attempt's slot: 0 for the opener, then 1 or 2 for the 2nd after a make or a miss,
/// and 3 or 4 for the 3rd after a make or a miss.
pub fn slots(attempts: &[f64; 3]) -> [usize; 3] {
    let made = attempts.map(|kg| kg > 0.0);
    [0, if made[0] { 1 } else { 2 }, if made[1] { 3 } else { 4 }]
}

/// The chance of making each attempt of a meet, in meet order, given every attempt before
/// it. The day maxes are drawn as the planner draws them: a part shared by the lifts and a
/// part of their own, each averaged over the grid of `draws`, which [`Grid::draws`] gives
/// for `tails`. With `choices`, each 2nd and 3rd attempt is also evidence of the day max
/// before its chance is worked out.
pub fn sequential_chances(
    draws: &[(f64, f64)],
    curves: &PerLift<MakeRateCurve>,
    expected_kg: &PerLift<f64>,
    day_form: &DayForm,
    attempts: &PerLift<[f64; 3]>,
    choices: Option<&PerLift<LoadChoice>>,
    tails: &Tails,
) -> PerLift<[f64; 3]> {
    let weights = day_form.draw_weights();
    let n = draws.len();
    // For each lift and pair of shared and own draws: the chance of the lift's attempts so
    // far. And for each lift and shared draw, that averaged over the own draws.
    let mut chances = PerLift::from_fn(|_| vec![1.0; n * n]);
    let mut averaged = PerLift::from_fn(|_| vec![1.0; n]);
    let mut so_far = 1.0;
    let mut result = PerLift::from_fn(|_| [0.0; 3]);
    let after = |before: f64, made_part: f64, made: bool| {
        if made { made_part } else { before - made_part }
    };
    for lift in Lift::ALL {
        let expected = *expected_kg.get(lift);
        let days: Vec<f64> = draws
            .iter()
            .flat_map(|&(shared_z, _)| {
                draws
                    .iter()
                    .map(move |&(own_z, _)| weights.day_max(expected, shared_z, own_z))
            })
            .collect();
        let choice = choices.map(|choices| choices.get(lift));
        let log_days: Vec<f64> = if choice.is_some() {
            days.iter().map(|day| day.ln()).collect()
        } else {
            Vec::new()
        };
        let (curve, floor) = (curves.get(lift), *tails.floors.get(lift));
        let slots = slots(attempts.get(lift));
        for (attempt, &kg) in attempts.get(lift).iter().enumerate() {
            let (load, made) = (kg.abs(), kg > 0.0);
            if let Some(choice) = choice.filter(|_| attempt > 0) {
                let log_share = load.ln() - choice.log_shares[slots[attempt] - 1];
                so_far = 0.0;
                for (i, &(_, shared_weight)) in draws.iter().enumerate() {
                    let mut average = 0.0;
                    for (j, &(_, own_weight)) in draws.iter().enumerate() {
                        let gap = (log_share - log_days[i * n + j]) / choice.spread;
                        let chance = &mut chances.get_mut(lift)[i * n + j];
                        *chance *= student_t_density(gap, tails.choice_dof);
                        average += own_weight * *chance;
                    }
                    averaged.get_mut(lift)[i] = average;
                    let all: f64 = Lift::ALL.iter().map(|&l| averaged.get(l)[i]).product();
                    so_far += shared_weight * all;
                }
            }
            let mut chance_made = 0.0;
            for (i, &(_, shared_weight)) in draws.iter().enumerate() {
                let mut average_made = 0.0;
                for (j, &(_, own_weight)) in draws.iter().enumerate() {
                    let make = make_chance(curve, floor, load, days[i * n + j]);
                    let chance = &mut chances.get_mut(lift)[i * n + j];
                    average_made += own_weight * *chance * make;
                    *chance *= if made { make } else { 1.0 - make };
                }
                let others: f64 = lift
                    .others()
                    .iter()
                    .map(|&other| averaged.get(other)[i])
                    .product();
                chance_made += shared_weight * others * average_made;
                let average = &mut averaged.get_mut(lift)[i];
                *average = after(*average, average_made, made);
            }
            // After an attempt the model calls impossible, every later chance is 0 / 0.
            result.get_mut(lift)[attempt] = if so_far > 0.0 {
                chance_made / so_far
            } else {
                0.0
            };
            so_far = after(so_far, chance_made, made);
        }
    }
    result
}

/// Each lift's estimate, `None` unless every lift has one with a finite uncertainty.
pub fn every_lift(estimate: &MaxEstimate) -> Option<PerLift<LiftEstimate>> {
    let lifts = estimate.lifts;
    let lifts = PerLift {
        squat: lifts.squat?,
        bench: lifts.bench?,
        deadlift: lifts.deadlift?,
    };
    Lift::ALL
        .iter()
        .all(|&lift| lifts.get(lift).uncertainty.is_finite())
        .then_some(lifts)
}

/// A model's estimates of a lifter's expected maxes for their latest meet.
pub struct Estimates {
    pub past_meets: MaxEstimate,
    pub with_opener: MaxEstimate,
}

impl Estimates {
    /// `None` unless every lift gets an estimate with a finite uncertainty.
    pub fn of(model: &Model, latest: &Meet, past: &[Meet]) -> Option<Self> {
        let input = MaxEstimateInput {
            curves: model.curves,
            typical_opener: model.typical_opener,
            day_form_spread: model.day_form.spread,
            meets: past.iter().map(Meet::past).collect(),
            on_day: latest.day,
            meets_before: 0,
        };
        let past_meets = estimate_maxes(&input).ok()?;
        let openers = latest.attempts.map(|_, attempts| attempts[0].abs());
        let with_opener = past_meets.with_openers(&openers, &model.typical_opener);
        every_lift(&past_meets)?;
        every_lift(&with_opener)?;
        Some(Estimates {
            past_meets,
            with_opener,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURVE: MakeRateCurve = MakeRateCurve {
        ratio_at_half: 1.0,
        steepness: 50.0,
        technical_miss_chance: 0.02,
    };

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn entry(lifter: &str, day: i32, squat_opener: f64) -> Entry {
        Entry {
            lifter: lifter.to_owned(),
            day: Some(day),
            attempts: PerLift {
                squat: [squat_opener, 105.0, -110.0],
                bench: [60.0, 62.5, 65.0],
                deadlift: [120.0, 130.0, 135.0],
            },
        }
    }

    #[test]
    fn keeps_lifters_with_two_complete_meets_on_different_days_in_date_order() {
        let entries = [
            entry("A", 200, 100.0),
            entry("A", 100, 95.0),
            entry("A", 100, 97.5),
            entry("B", 100, 100.0),
            entry("C", 100, 100.0),
            entry("C", 150, f64::NAN),
        ];
        let lifters = lifters_meets(&entries);
        assert_eq!(lifters.len(), 1);
        let days: Vec<i32> = lifters[0].iter().map(|meet| meet.day).collect();
        assert_eq!(days, [100, 200]);
    }

    #[test]
    fn a_student_t_grid_keeps_the_spread_and_puts_more_weight_far_out() {
        let moment = |grid: &[(f64, f64)], power: i32| {
            grid.iter().map(|(z, w)| w * z.powi(power)).sum::<f64>()
        };
        let beyond = |grid: &[(f64, f64)], far: f64| {
            grid.iter()
                .filter(|(z, _)| z.abs() > far)
                .map(|(_, w)| w)
                .sum::<f64>()
        };
        let t = Grid::Backtest.draws(4.0);
        assert!(close(moment(&t, 0), 1.0));
        assert!(moment(&t, 1).abs() < 1e-9);
        assert!(close(moment(&t, 2), 1.0));
        assert!(beyond(&t, 3.0) > 2.0 * beyond(&BACKTEST_GRID, 3.0));
        assert_eq!(*Grid::Fit.draws(f64::INFINITY), *FIT_GRID);
    }

    #[test]
    fn a_floor_keeps_a_chance_of_making_a_load_far_above_the_day_max() {
        let curves = PerLift::from_fn(|_| CURVE);
        let expected = PerLift::from_fn(|_| 100.0);
        let attempts = PerLift::from_fn(|_| [90.0, 100.0, 150.0]);
        let day_form = DayForm {
            spread: 0.0,
            max_uncertainty: 0.0,
            correlation: 0.4,
        };
        let tails = Tails {
            floors: PerLift::from_fn(|_| 0.1),
            ..Tails::NORMAL
        };
        let chances = sequential_chances(
            &BACKTEST_GRID,
            &curves,
            &expected,
            &day_form,
            &attempts,
            None,
            &tails,
        );
        let at_the_day_max = 0.1 * 0.98 + 0.9 * CURVE.make_probability_kg(100.0, 100.0);
        assert!(close(chances.squat[1], at_the_day_max));
        assert!((chances.squat[2] - 0.1 * 0.98).abs() < 1e-6);
    }

    #[test]
    fn names_each_attempt_by_whether_the_one_before_was_made() {
        assert_eq!(slots(&[100.0, 105.0, -110.0]), [0, 1, 3]);
        assert_eq!(slots(&[-100.0, 100.0, 105.0]), [0, 2, 3]);
        assert_eq!(slots(&[100.0, -105.0, -105.0]), [0, 1, 4]);
    }

    #[test]
    fn a_known_day_max_gives_each_attempt_its_own_chance() {
        let curves = PerLift::from_fn(|_| CURVE);
        let expected = PerLift::from_fn(|_| 100.0);
        let attempts = PerLift::from_fn(|_| [90.0, -100.0, 102.5]);
        let day_form = DayForm {
            spread: 0.0,
            max_uncertainty: 0.0,
            correlation: 0.4,
        };
        let chances = sequential_chances(
            &BACKTEST_GRID,
            &curves,
            &expected,
            &day_form,
            &attempts,
            None,
            &Tails::NORMAL,
        );
        for lift in Lift::ALL {
            for (chance, kg) in chances.get(lift).iter().zip(attempts.get(lift)) {
                assert!(
                    close(*chance, CURVE.make_probability_kg(kg.abs(), 100.0)),
                    "{chances:?}"
                );
            }
        }
    }

    #[test]
    fn a_miss_lowers_the_chances_of_lifts_that_share_the_day() {
        let curves = PerLift::from_fn(|_| CURVE);
        let expected = PerLift::from_fn(|_| 100.0);
        let bench_opener = |squat: [f64; 3], correlation| {
            let attempts = PerLift {
                squat,
                bench: [95.0, 97.5, 100.0],
                deadlift: [95.0, 97.5, 100.0],
            };
            let day_form = DayForm {
                spread: 0.05,
                max_uncertainty: 0.0,
                correlation,
            };
            sequential_chances(
                &BACKTEST_GRID,
                &curves,
                &expected,
                &day_form,
                &attempts,
                None,
                &Tails::NORMAL,
            )
            .bench[0]
        };
        let (made, missed) = ([90.0, 95.0, 100.0], [90.0, -95.0, -95.0]);
        assert!(bench_opener(missed, 0.5) < bench_opener(made, 0.5) - 0.05);
        assert!(close(bench_opener(missed, 0.0), bench_opener(made, 0.0)));
    }

    #[test]
    fn a_heavier_chosen_attempt_says_the_day_is_better() {
        let curves = PerLift::from_fn(|_| CURVE);
        let expected = PerLift::from_fn(|_| 100.0);
        let day_form = DayForm {
            spread: 0.04,
            max_uncertainty: 0.0,
            correlation: 0.4,
        };
        let choices = PerLift::from_fn(|_| LoadChoice {
            log_shares: [0.97f64.ln(); 4],
            spread: 0.02,
        });
        let second = |kg: f64, choices| {
            let attempts = PerLift::from_fn(|_| [90.0, kg, kg + 2.5]);
            sequential_chances(
                &BACKTEST_GRID,
                &curves,
                &expected,
                &day_form,
                &attempts,
                choices,
                &Tails::NORMAL,
            )
            .squat[1]
        };
        let chosen = |kg| second(kg, Some(&choices));
        assert!(
            chosen(102.5) > second(102.5, None) + 0.1,
            "{}",
            chosen(102.5)
        );
        let third = |second_kg: f64| {
            let attempts = PerLift::from_fn(|_| [90.0, second_kg, 105.0]);
            let chances = sequential_chances(
                &BACKTEST_GRID,
                &curves,
                &expected,
                &day_form,
                &attempts,
                Some(&choices),
                &Tails::NORMAL,
            );
            chances.squat[2]
        };
        assert!(
            third(100.0) > third(95.0) + 0.05,
            "{} {}",
            third(100.0),
            third(95.0)
        );
    }
}
