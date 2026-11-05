//! Scores the planner's predictions on meets it has not seen.
//!
//! Lifters are split in two by name. The model is fitted to one part. For each lifter in
//! the other part with at least two complete meets, the expected maxes for their latest
//! meet are estimated from the meets before it, as the page does from pasted past meets.
//! Each attempt of the latest meet then gets the model's chance of being made, given the
//! attempts before it in the meet, and is scored against what happened.
//!
//! Lifters choose their attempts knowing how their training went, which past meets don't
//! show. A second set of scores stands in for that knowledge by also taking the opener
//! they chose as evidence of their expected max, as the estimate does for past openers.

use std::num::NonZeroUsize;

use attempt_sim::lift::{Lift, PerLift};
use attempt_sim::model::MakeRateCurve;
use attempt_sim::normal::normal_draws;
use attempt_sim::past_meets::{
    MaxEstimateInput, OPENER_SPREAD, PastAttempt, PastMeet, estimate_maxes,
};

use crate::data::Entry;
use crate::fitted::Fitted;
use crate::stats::is_complete;

/// One lifter in this many is held out for scoring.
const HELD_OUT_EVERY: u64 = 10;
/// The past meets used to estimate the maxes, the latest ones.
const MAX_PAST_MEETS: usize = 6;
/// The page never sets the uncertainty of the expected max below this.
const LEAST_UNCERTAINTY: f64 = 0.005;
/// Keeps the log loss finite for a chance rounded to 0 or 1.
const SUREST_CHANCE: f64 = 1e-6;
const LIGHTEST_DAY: f64 = 0.01;
/// Bounds of the bins of predicted chances, from 0 to 1.
pub const BIN_EDGES: [f64; 8] = [0.0, 0.5, 0.7, 0.8, 0.9, 0.95, 0.98, 1.0];
/// Lifters by how many attempts at a lift they missed in their past meets: 0, 1, 2 or more.
pub const PAST_MISS_GROUPS: usize = 3;

/// Whether a lifter is held out for scoring, by a hash of their name that stays the same
/// from run to run.
pub fn is_held_out(lifter: &str) -> bool {
    let hash = lifter
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    hash % HELD_OUT_EVERY == 0
}

/// How well predicted chances match what happened.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Score {
    pub attempts: usize,
    pub made: usize,
    predicted: f64,
    log_loss: f64,
    brier: f64,
}

impl Score {
    fn add(&mut self, chance: f64, made: bool) {
        let chance = chance.clamp(SUREST_CHANCE, 1.0 - SUREST_CHANCE);
        let happened = if made { chance } else { 1.0 - chance };
        self.attempts += 1;
        self.made += usize::from(made);
        self.predicted += chance;
        self.log_loss -= happened.ln();
        self.brier += (1.0 - happened).powi(2);
    }

    fn merge(&mut self, other: &Score) {
        self.attempts += other.attempts;
        self.made += other.made;
        self.predicted += other.predicted;
        self.log_loss += other.log_loss;
        self.brier += other.brier;
    }

    fn mean(&self, sum: f64) -> f64 {
        sum / self.attempts as f64
    }

    pub fn observed(&self) -> f64 {
        self.mean(self.made as f64)
    }

    pub fn predicted(&self) -> f64 {
        self.mean(self.predicted)
    }

    /// Mean negative log chance of what happened, in nats: lower is better.
    pub fn log_loss(&self) -> f64 {
        self.mean(self.log_loss)
    }

    /// Mean squared error of the chance: lower is better.
    pub fn brier(&self) -> f64 {
        self.mean(self.brier)
    }
}

/// Scores of one way of estimating the expected maxes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scores {
    pub all: Score,
    /// Per lift: the opener, 2nd after a make and after a miss, 3rd after a make and after
    /// a miss.
    pub attempts: PerLift<[Score; 5]>,
    /// By predicted chance, in [`BIN_EDGES`].
    pub bins: [Score; BIN_EDGES.len() - 1],
    /// Per lift, by how many attempts at the lift the lifter missed in their past meets.
    pub past_misses: PerLift<[Score; PAST_MISS_GROUPS]>,
}

impl Scores {
    fn merge(&mut self, other: &Scores) {
        let pairs =
            |a: &mut [Score], b: &[Score]| a.iter_mut().zip(b).for_each(|(a, b)| a.merge(b));
        self.all.merge(&other.all);
        for lift in Lift::ALL {
            pairs(self.attempts.get_mut(lift), other.attempts.get(lift));
            pairs(self.past_misses.get_mut(lift), other.past_misses.get(lift));
        }
        pairs(&mut self.bins, &other.bins);
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Backtest {
    pub meets: usize,
    /// Held-out meets skipped because the past meets gave no usable estimate.
    pub skipped: usize,
    /// Each attempt's chance as the share of that attempt made in the fitted data, after a
    /// make or a miss, whatever the load: what the model must beat.
    pub baseline: Score,
    /// The expected maxes estimated from past meets alone.
    pub past_meets: Scores,
    /// The expected maxes estimated from past meets and the opener the lifter chose.
    pub with_opener: Scores,
}

impl Backtest {
    fn merge(&mut self, other: &Backtest) {
        self.meets += other.meets;
        self.skipped += other.skipped;
        self.baseline.merge(&other.baseline);
        self.past_meets.merge(&other.past_meets);
        self.with_opener.merge(&other.with_opener);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Meet {
    day: i32,
    attempts: PerLift<[f64; 3]>,
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

/// Each lifter's complete meets in date order, one entry per day, for lifters with two or
/// more.
fn lifters_meets(entries: &[Entry]) -> Vec<Vec<Meet>> {
    let mut entries: Vec<(&str, Meet)> = entries
        .iter()
        .filter(|entry| {
            Lift::ALL
                .iter()
                .all(|&lift| is_complete(*entry.attempts.get(lift)))
        })
        .filter_map(|entry| {
            let meet = Meet {
                day: entry.day?,
                attempts: entry.attempts,
            };
            Some((entry.lifter.as_str(), meet))
        })
        .collect();
    entries.sort_by(|a, b| (a.0, a.1.day).cmp(&(b.0, b.1.day)));
    entries.dedup_by(|a, b| a.0 == b.0 && a.1.day == b.1.day);
    entries
        .chunk_by(|a, b| a.0 == b.0)
        .filter(|meets| meets.len() >= 2)
        .map(|meets| meets.iter().map(|(_, meet)| *meet).collect())
        .collect()
}

/// The slot of attempt `attempt` in [`Backtest::attempts`], given whether the one before
/// was made.
fn slot(attempt: usize, made_before: bool) -> usize {
    match (attempt, made_before) {
        (0, _) => 0,
        (1, true) => 1,
        (1, false) => 2,
        (2, true) => 3,
        _ => 4,
    }
}

fn make_chance(curve: &MakeRateCurve, load_kg: f64, day_max_kg: f64) -> f64 {
    (1.0 - curve.technical_miss_chance)
        / (1.0 + (curve.steepness * (load_kg / day_max_kg - curve.ratio_at_half)).exp())
}

/// The chance of making each attempt of a meet, in meet order, given every attempt before
/// it. The day maxes are drawn as the planner draws them: a part shared by the lifts and a
/// part of their own, each averaged over a grid of normal draws.
fn sequential_chances(
    curves: &PerLift<MakeRateCurve>,
    expected_kg: &PerLift<f64>,
    spread: f64,
    correlation: f64,
    attempts: &PerLift<[f64; 3]>,
) -> PerLift<[f64; 3]> {
    let draws: Vec<(f64, f64)> = normal_draws().collect();
    let (shared, own) = (
        spread * correlation.sqrt(),
        spread * (1.0 - correlation).sqrt(),
    );
    let n = draws.len();
    // For each shared draw and each lift: the chance of the lift's attempts so far for
    // each own draw, and averaged over the own draws.
    let mut chances = PerLift::from_fn(|_| vec![1.0; n * n]);
    let mut averaged = PerLift::from_fn(|_| vec![1.0; n]);
    let mut so_far = 1.0;
    let mut result = PerLift::from_fn(|_| [0.0; 3]);
    for lift in Lift::ALL {
        let (curve, expected) = (curves.get(lift), *expected_kg.get(lift));
        for (attempt, &kg) in attempts.get(lift).iter().enumerate() {
            let (load, made) = (kg.abs(), kg > 0.0);
            let mut chance_made = 0.0;
            for (i, &(shared_z, shared_weight)) in draws.iter().enumerate() {
                let row = &mut chances.get_mut(lift)[i * n..(i + 1) * n];
                let mut average_made = 0.0;
                for (chance, &(own_z, own_weight)) in row.iter_mut().zip(&draws) {
                    let day = expected * (1.0 + shared * shared_z + own * own_z).max(LIGHTEST_DAY);
                    let make = make_chance(curve, load, day);
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
                *average = if made {
                    average_made
                } else {
                    *average - average_made
                };
            }
            result.get_mut(lift)[attempt] = chance_made / so_far;
            so_far = if made {
                chance_made
            } else {
                so_far - chance_made
            };
        }
    }
    result
}

/// The expected max and its uncertainty, in log kg, after also taking `opener_kg` as
/// evidence: an opener that strays from the typical share of the max as much as openers do.
fn with_opener(log_kg: f64, uncertainty: f64, opener_kg: f64, typical_opener: f64) -> (f64, f64) {
    let from_opener = (opener_kg / typical_opener).ln();
    let [past_weight, opener_weight] = [uncertainty, OPENER_SPREAD].map(|sd| sd.powi(-2));
    let weight = past_weight + opener_weight;
    (
        (past_weight * log_kg + opener_weight * from_opener) / weight,
        weight.powf(-0.5),
    )
}

/// Adds the scores of one meet's attempts, given each attempt's predicted chance.
fn add_meet(
    scores: &mut Scores,
    attempts: &PerLift<[f64; 3]>,
    chances: &PerLift<[f64; 3]>,
    past_misses: &PerLift<usize>,
) {
    for lift in Lift::ALL {
        let mut made_before = true;
        let outcomes = attempts.get(lift).iter().zip(chances.get(lift));
        for (attempt, (&kg, &chance)) in outcomes.enumerate() {
            let made = kg > 0.0;
            scores.all.add(chance, made);
            scores.attempts.get_mut(lift)[slot(attempt, made_before)].add(chance, made);
            scores.past_misses.get_mut(lift)[*past_misses.get(lift)].add(chance, made);
            let bin = BIN_EDGES[1..]
                .iter()
                .position(|&edge| chance < edge)
                .unwrap_or(BIN_EDGES.len() - 2);
            scores.bins[bin].add(chance, made);
            made_before = made;
        }
    }
}

/// Scores one lifter's latest meet from the meets before it.
fn score_lifter(fitted: &Fitted, meets: &[Meet], backtest: &mut Backtest) {
    let (latest, past) = meets.split_last().expect("two or more meets");
    let past = &past[past.len().saturating_sub(MAX_PAST_MEETS)..];
    let curves = fitted.curves();
    let typical_opener = fitted.typical_opener();
    let day_form = fitted.day_form();
    let input = MaxEstimateInput {
        curves,
        typical_opener,
        day_form_spread: day_form.spread,
        meets: past.iter().map(Meet::past).collect(),
        on_day: latest.day,
    };
    let estimates = estimate_maxes(&input).ok().and_then(|estimate| {
        let lifts = estimate.lifts;
        let lifts = [lifts.squat?, lifts.bench?, lifts.deadlift?];
        lifts
            .iter()
            .all(|lift| lift.uncertainty.is_finite())
            .then_some(lifts)
    });
    let Some(estimates) = estimates else {
        backtest.skipped += 1;
        return;
    };
    let estimates = PerLift::from_fn(|lift| estimates[lift as usize]);
    backtest.meets += 1;

    let past_misses = PerLift::from_fn(|lift| {
        past.iter()
            .flat_map(|meet| meet.attempts.get(lift))
            .filter(|&&kg| kg < 0.0)
            .count()
            .min(PAST_MISS_GROUPS - 1)
    });
    // As the page does: one uncertainty for every lift, the average, and at least 0.5%.
    let mean =
        |values: PerLift<f64>| Lift::ALL.iter().map(|&lift| values.get(lift)).sum::<f64>() / 3.0;
    let score = |scores: &mut Scores, log_kg: PerLift<f64>, uncertainty: PerLift<f64>| {
        let spread = day_form
            .spread
            .hypot(mean(uncertainty).max(LEAST_UNCERTAINTY));
        let expected = log_kg.map(|_, log_kg| log_kg.exp());
        let chances = sequential_chances(
            &curves,
            &expected,
            spread,
            day_form.correlation,
            &latest.attempts,
        );
        add_meet(scores, &latest.attempts, &chances, &past_misses);
    };
    score(
        &mut backtest.past_meets,
        estimates.map(|_, lift| lift.expected_max_kg.ln()),
        estimates.map(|_, lift| lift.uncertainty),
    );
    let informed = estimates.map(|lift, estimate| {
        let opener_kg = latest.attempts.get(lift)[0].abs();
        let typical = *typical_opener.get(lift);
        with_opener(
            estimate.expected_max_kg.ln(),
            estimate.uncertainty,
            opener_kg,
            typical,
        )
    });
    score(
        &mut backtest.with_opener,
        informed.map(|_, (log_kg, _)| *log_kg),
        informed.map(|_, (_, uncertainty)| *uncertainty),
    );

    for lift in Lift::ALL {
        let slot_rates = fitted.stats.get(lift).make_rates.all();
        let mut made_before = true;
        for (attempt, &kg) in latest.attempts.get(lift).iter().enumerate() {
            let made = kg > 0.0;
            backtest
                .baseline
                .add(slot_rates[slot(attempt, made_before)], made);
            made_before = made;
        }
    }
}

/// Scores the model fitted to `fitted` on the held-out lifters among `entries`.
pub fn backtest(fitted: &Fitted, entries: &[Entry]) -> Backtest {
    let lifters = lifters_meets(entries);
    let threads = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
    let chunk = lifters.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let workers: Vec<_> = lifters
            .chunks(chunk)
            .map(|lifters| {
                scope.spawn(move || {
                    let mut backtest = Backtest::default();
                    for meets in lifters {
                        score_lifter(fitted, meets, &mut backtest);
                    }
                    backtest
                })
            })
            .collect();
        workers
            .into_iter()
            .fold(Backtest::default(), |mut total, worker| {
                total.merge(&worker.join().expect("a backtest thread panicked"));
                total
            })
    })
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
    fn holds_out_about_one_lifter_in_ten_the_same_way_every_time() {
        let names: Vec<String> = (0..10_000).map(|i| format!("Lifter {i}")).collect();
        let held_out = names.iter().filter(|name| is_held_out(name)).count();
        assert!((800..1_200).contains(&held_out), "{held_out}");
        assert_eq!(is_held_out("Lifter 7"), is_held_out("Lifter 7"));
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
    fn names_each_attempt_by_whether_the_one_before_was_made() {
        assert_eq!(slot(0, false), 0);
        assert_eq!([slot(1, true), slot(1, false)], [1, 2]);
        assert_eq!([slot(2, true), slot(2, false)], [3, 4]);
    }

    #[test]
    fn a_known_day_max_gives_each_attempt_its_own_chance() {
        let curves = PerLift::from_fn(|_| CURVE);
        let expected = PerLift::from_fn(|_| 100.0);
        let attempts = PerLift::from_fn(|_| [90.0, -100.0, 102.5]);
        let chances = sequential_chances(&curves, &expected, 0.0, 0.4, &attempts);
        for lift in Lift::ALL {
            for (chance, kg) in chances.get(lift).iter().zip(attempts.get(lift)) {
                assert!(
                    close(*chance, make_chance(&CURVE, kg.abs(), 100.0)),
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
            sequential_chances(&curves, &expected, 0.05, correlation, &attempts).bench[0]
        };
        let (made, missed) = ([90.0, 95.0, 100.0], [90.0, -95.0, -95.0]);
        assert!(bench_opener(missed, 0.5) < bench_opener(made, 0.5) - 0.05);
        assert!(close(bench_opener(missed, 0.0), bench_opener(made, 0.0)));
    }

    #[test]
    fn an_opener_moves_the_estimate_by_how_sure_each_is() {
        let typical = 0.9;
        let past = 100f64.ln();
        let (log_kg, uncertainty) = with_opener(past, OPENER_SPREAD, 99.0, typical);
        assert!(close(log_kg, (past + (99.0 / typical).ln()) / 2.0));
        assert!(close(uncertainty, OPENER_SPREAD / 2f64.sqrt()));
        let (sure, _) = with_opener(past, 0.001, 99.0, typical);
        assert!((sure - past).abs() < 0.001);
    }

    #[test]
    fn scores_a_coin_flip_by_its_log_loss_and_brier_score() {
        let mut score = Score::default();
        score.add(0.5, true);
        score.add(0.5, false);
        assert!(close(score.log_loss(), 2f64.ln()));
        assert!(close(score.brier(), 0.25));
        assert!(close(score.observed(), 0.5));
        assert!(close(score.predicted(), 0.5));
    }
}
