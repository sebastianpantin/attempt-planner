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
use std::ops::AddAssign;
use std::sync::atomic::{AtomicUsize, Ordering};

use attempt_sim::lift::{Lift, PerLift};
use attempt_sim::minimize::minimize_to;
use attempt_sim::model::{DayForm, MakeRateCurve};
use attempt_sim::normal::normal_draws;
use attempt_sim::past_meets::{
    DAYS_PER_YEAR, LiftEstimate, MAX_TREND_DAYS, MaxEstimate, MaxEstimateInput, PastAttempt,
    PastMeet, chance_of_outcomes, estimate_maxes,
};

use crate::data::Entry;
use crate::fitted::{Fitted, Model};
use crate::stats::is_complete;

/// One lifter in this many is held out for scoring.
const HELD_OUT_EVERY: u64 = 10;
/// The past meets used to estimate the maxes, the latest ones.
const MAX_PAST_MEETS: usize = 6;
/// Keeps the log loss finite for a chance rounded to 0 or 1.
const SUREST_CHANCE: f64 = 1e-6;
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
impl AddAssign<&Score> for Score {
    fn add_assign(&mut self, other: &Score) {
        self.attempts += other.attempts;
        self.made += other.made;
        self.predicted += other.predicted;
        self.log_loss += other.log_loss;
        self.brier += other.brier;
    }
}

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
        let pairs = |a: &mut [Score], b: &[Score]| a.iter_mut().zip(b).for_each(|(a, b)| *a += b);
        self.all += &other.all;
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
    /// Named in [`SCENARIOS`].
    pub scenarios: [Scores; SCENARIOS.len()],
    /// The fitted load choice of each model: every lifter alike, then with prone lifters.
    pub load_choices: [LoadChoice; 2],
}

impl Default for LoadChoice {
    fn default() -> Self {
        LoadChoice {
            log_shares: [0.0; 4],
            spread: 1.0,
        }
    }
}

/// Each way of predicting the chances: which model, and where the expected maxes come
/// from. "Past meets and the opener" also takes the opener the lifter chose as evidence.
/// A lifter's own kind is the share of prone lifters updated by their past meets.
/// "Loads chosen on the day" also takes each 2nd and 3rd attempt the lifter chose as
/// evidence of their day max, through a fitted [`LoadChoice`].
pub const SCENARIOS: [&str; 8] = [
    "alike, from past meets",
    "alike, from past meets and the opener",
    "prone share, from past meets",
    "prone share, from past meets and the opener",
    "own kind, from past meets",
    "own kind, from past meets and the opener",
    "alike, opener and loads chosen on the day",
    "prone share, opener and loads chosen on the day",
];

/// How lifters choose their 2nd and 3rd attempts knowing how the day is going: at a
/// typical share of their day max, give or take a spread. Taken as evidence, a heavy
/// chosen attempt says the day is good.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadChoice {
    /// The log of the typical share of the day max: 2nd attempt after a made and after a
    /// missed opener, then 3rd attempt after a made and after a missed 2nd.
    pub log_shares: [f64; 4],
    /// Standard deviation of the log of the chosen share around the typical one.
    pub spread: f64,
}

impl AddAssign<&Backtest> for Backtest {
    fn add_assign(&mut self, other: &Backtest) {
        self.meets += other.meets;
        self.skipped += other.skipped;
        self.baseline += &other.baseline;
        for (scores, other) in self.scenarios.iter_mut().zip(&other.scenarios) {
            scores.merge(other);
        }
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
            attempts: self
                .attempts
                .map(|_, attempts| Self::past_attempts(attempts)),
        }
    }

    fn past_attempts(attempts: &[f64; 3]) -> Vec<PastAttempt> {
        attempts
            .iter()
            .map(|&kg| PastAttempt {
                kg: kg.abs(),
                made: kg > 0.0,
            })
            .collect()
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

/// Each attempt's index in [`Scores::attempts`]: the opener, then whether the attempt
/// before was made.
fn slots(attempts: &[f64; 3]) -> [usize; 3] {
    let made = attempts.map(|kg| kg > 0.0);
    [0, if made[0] { 1 } else { 2 }, if made[1] { 3 } else { 4 }]
}

/// The chance of making each attempt of a meet, in meet order, given every attempt before
/// it. The day maxes are drawn as the planner draws them: a part shared by the lifts and a
/// part of their own, each averaged over a grid of normal draws. Each lift's kind of
/// lifter, as shares and curves, holds for all its attempts.
fn sequential_chances(
    kinds: &PerLift<[(f64, MakeRateCurve); 2]>,
    expected_kg: &PerLift<f64>,
    day_form: &DayForm,
    attempts: &PerLift<[f64; 3]>,
    choice: Option<&LoadChoice>,
) -> PerLift<[f64; 3]> {
    let draws: Vec<(f64, f64)> = normal_draws().collect();
    let weights = day_form.draw_weights();
    let n = draws.len();
    // For each lift, kind and shared draw: the chance of the lift's attempts so far for
    // each own draw. And for each lift and shared draw, that averaged over the own draws
    // and the kinds.
    let mut chances = PerLift::from_fn(|_| vec![1.0; 2 * n * n]);
    let mut averaged = PerLift::from_fn(|_| vec![1.0; n]);
    let mut so_far = 1.0;
    let mut result = PerLift::from_fn(|_| [0.0; 3]);
    let after = |before: f64, made_part: f64, made: bool| {
        if made { made_part } else { before - made_part }
    };
    for lift in Lift::ALL {
        let expected = *expected_kg.get(lift);
        let slots = slots(attempts.get(lift));
        for (attempt, &kg) in attempts.get(lift).iter().enumerate() {
            let (load, made) = (kg.abs(), kg > 0.0);
            if let Some(choice) = choice.filter(|_| attempt > 0) {
                let log_share = choice.log_shares[slots[attempt] - 1];
                so_far = 0.0;
                for (i, &(shared_z, shared_weight)) in draws.iter().enumerate() {
                    let mut average = 0.0;
                    for (j, &(own_z, own_weight)) in draws.iter().enumerate() {
                        let day = weights.day_max(expected, shared_z, own_z);
                        let gap = ((load / day).ln() - log_share) / choice.spread;
                        let likelihood = (-gap * gap / 2.0).exp();
                        for (kind, (share, _)) in kinds.get(lift).iter().enumerate() {
                            let chance = &mut chances.get_mut(lift)[(kind * n + i) * n + j];
                            *chance *= likelihood;
                            average += share * own_weight * *chance;
                        }
                    }
                    averaged.get_mut(lift)[i] = average;
                    let all: f64 = Lift::ALL.iter().map(|&l| averaged.get(l)[i]).product();
                    so_far += shared_weight * all;
                }
            }
            let mut chance_made = 0.0;
            for (i, &(shared_z, shared_weight)) in draws.iter().enumerate() {
                let mut average_made = 0.0;
                for (kind, (share, curve)) in kinds.get(lift).iter().enumerate() {
                    let start = (kind * n + i) * n;
                    let row = &mut chances.get_mut(lift)[start..start + n];
                    for (chance, &(own_z, own_weight)) in row.iter_mut().zip(&draws) {
                        let day = weights.day_max(expected, shared_z, own_z);
                        let make = curve.make_probability_kg(load, day);
                        average_made += share * own_weight * *chance * make;
                        *chance *= if made { make } else { 1.0 - make };
                    }
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

/// Each lift's kinds of lifter with the prone share updated by how likely the lifter's
/// past attempts are for each kind, given the estimated maxes.
fn own_kinds(
    model: &Model,
    estimate: &MaxEstimate,
    past: &[Meet],
    on_day: i32,
) -> PerLift<[(f64, MakeRateCurve); 2]> {
    let latest_day = past.iter().map(|meet| meet.day).max().unwrap_or(on_day);
    let level_day = on_day.min(latest_day.saturating_add(MAX_TREND_DAYS));
    let growth = (1.0 + estimate.yearly_progress).ln();
    model.kinds.map(|lift, kinds| {
        let Some(lift_estimate) = estimate.lifts.get(lift) else {
            return *kinds;
        };
        let likelihood = |curve: &MakeRateCurve| -> f64 {
            past.iter()
                .map(|meet| {
                    let years = f64::from(level_day - meet.day) / DAYS_PER_YEAR;
                    let level_kg = lift_estimate.expected_max_kg * (-growth * years).exp();
                    let attempts = Meet::past_attempts(meet.attempts.get(lift));
                    chance_of_outcomes(curve, &attempts, level_kg, model.day_form.spread).ln()
                })
                .sum()
        };
        let [(alike_share, alike), (prone_share, prone)] = *kinds;
        let log_odds = (prone_share / alike_share).ln() + likelihood(&prone) - likelihood(&alike);
        let prone_share = 1.0 / (1.0 + (-log_odds).exp());
        [(1.0 - prone_share, alike), (prone_share, prone)]
    })
}

/// Adds the scores of one meet's attempts, given each attempt's predicted chance.
fn add_meet(
    scores: &mut Scores,
    attempts: &PerLift<[f64; 3]>,
    chances: &PerLift<[f64; 3]>,
    past_misses: &PerLift<usize>,
) {
    for lift in Lift::ALL {
        let attempts = attempts.get(lift);
        let outcomes = attempts.iter().zip(chances.get(lift)).zip(slots(attempts));
        for ((&kg, &chance), slot) in outcomes {
            let made = kg > 0.0;
            scores.all.add(chance, made);
            scores.attempts.get_mut(lift)[slot].add(chance, made);
            scores.past_misses.get_mut(lift)[*past_misses.get(lift)].add(chance, made);
            let bin = BIN_EDGES[1..]
                .iter()
                .position(|&edge| chance < edge)
                .unwrap_or(BIN_EDGES.len() - 2);
            scores.bins[bin].add(chance, made);
        }
    }
}

/// Each lift's estimate, `None` unless every lift has one with a finite uncertainty.
fn every_lift(estimate: &MaxEstimate) -> Option<PerLift<LiftEstimate>> {
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

/// The estimate of each lift's expected max from `past`, `None` unless every lift gets
/// one with a finite uncertainty.
fn estimate(model: &Model, past: &[Meet], on_day: i32) -> Option<MaxEstimate> {
    let input = MaxEstimateInput {
        curves: model.mean_curves(),
        typical_opener: model.typical_opener,
        day_form_spread: model.day_form.spread,
        meets: past.iter().map(Meet::past).collect(),
        on_day,
    };
    estimate_maxes(&input)
        .ok()
        .filter(|estimate| every_lift(estimate).is_some())
}

/// The expected maxes and day form the planner would use with this estimate.
fn planned(model: &Model, estimate: &MaxEstimate) -> (PerLift<f64>, DayForm) {
    let lifts = every_lift(estimate).expect("every lift is estimated");
    let day_form = DayForm {
        max_uncertainty: estimate.shared_uncertainty,
        ..model.day_form
    };
    (lifts.map(|_, lift| lift.expected_max_kg), day_form)
}

/// Lifters whose meets the load choice is fitted to, at most.
const CHOICE_LIFTERS: usize = 2_000;

/// The [`LoadChoice`] under which `model`'s chances of the latest meets of some of
/// `entries`' lifters, with maxes from past meets and the opener, have the lowest log loss.
pub fn fit_load_choice(model: &Model, entries: &[Entry]) -> LoadChoice {
    let lifters = lifters_meets(entries);
    let step = lifters.len().div_ceil(CHOICE_LIFTERS).max(1);
    let meets: Vec<_> = lifters
        .iter()
        .step_by(step)
        .filter_map(|meets| {
            let (latest, past) = meets.split_last()?;
            let past = &past[past.len().saturating_sub(MAX_PAST_MEETS)..];
            let openers = latest.attempts.map(|_, attempts| attempts[0].abs());
            let estimate =
                estimate(model, past, latest.day)?.with_openers(&openers, &model.typical_opener);
            every_lift(&estimate)?;
            let (expected, day_form) = planned(model, &estimate);
            Some((latest.attempts, expected, day_form))
        })
        .collect();
    let choice =
        |[second_make, second_miss, third_make, third_miss, log_spread]: [f64; 5]| LoadChoice {
            log_shares: [second_make, second_miss, third_make, third_miss],
            spread: log_spread.exp(),
        };
    let log_loss = |point: [f64; 5]| {
        let choice = choice(point);
        in_parallel(&meets, |(attempts, expected, day_form)| {
            let chances =
                sequential_chances(&model.kinds, expected, day_form, attempts, Some(&choice));
            let mut score = Score::default();
            for lift in Lift::ALL {
                for (&kg, &chance) in attempts.get(lift).iter().zip(chances.get(lift)) {
                    score.add(chance, kg > 0.0);
                }
            }
            score
        })
        .log_loss()
    };
    let start = [
        0.95f64.ln(),
        0.9f64.ln(),
        0.99f64.ln(),
        0.95f64.ln(),
        0.04f64.ln(),
    ];
    choice(minimize_to(
        log_loss,
        start,
        [0.02, 0.02, 0.02, 0.02, 0.3],
        1e-6,
        400,
    ))
}

/// Folds `work` over `items` on every core, each thread taking the next item when free.
fn in_parallel<T: Sync, R: Default + Send + for<'a> AddAssign<&'a R>>(
    items: &[T],
    work: impl Fn(&T) -> R + Sync,
) -> R {
    let threads = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut total = R::default();
                    while let Some(item) = items.get(next.fetch_add(1, Ordering::Relaxed)) {
                        total += &work(item);
                    }
                    total
                })
            })
            .collect();
        workers.into_iter().fold(R::default(), |mut total, worker| {
            total += &worker.join().expect("a worker thread panicked");
            total
        })
    })
}

/// Scores one lifter's latest meet from the meets before it, in each of [`SCENARIOS`].
fn score_lifter(
    fitted: &Fitted,
    models: &[(Model, LoadChoice); 2],
    meets: &[Meet],
    backtest: &mut Backtest,
) {
    let (latest, past) = meets.split_last().expect("two or more meets");
    let past = &past[past.len().saturating_sub(MAX_PAST_MEETS)..];
    let [(alike, alike_choice), (prone, prone_choice)] = models;
    let (Some(alike_estimate), Some(prone_estimate)) = (
        estimate(alike, past, latest.day),
        estimate(prone, past, latest.day),
    ) else {
        backtest.skipped += 1;
        return;
    };
    backtest.meets += 1;

    let past_misses = PerLift::from_fn(|lift| {
        past.iter()
            .flat_map(|meet| meet.attempts.get(lift))
            .filter(|&&kg| kg < 0.0)
            .count()
            .min(PAST_MISS_GROUPS - 1)
    });
    let openers = latest.attempts.map(|_, attempts| attempts[0].abs());
    let own = own_kinds(prone, &prone_estimate, past, latest.day);
    let with_opener = |model: &Model, estimate: &MaxEstimate| {
        estimate.with_openers(&openers, &model.typical_opener)
    };
    let ways = [
        (alike, alike.kinds, alike_estimate, None),
        (
            alike,
            alike.kinds,
            with_opener(alike, &alike_estimate),
            None,
        ),
        (prone, prone.kinds, prone_estimate, None),
        (
            prone,
            prone.kinds,
            with_opener(prone, &prone_estimate),
            None,
        ),
        (prone, own, prone_estimate, None),
        (prone, own, with_opener(prone, &prone_estimate), None),
        (
            alike,
            alike.kinds,
            with_opener(alike, &alike_estimate),
            Some(alike_choice),
        ),
        (
            prone,
            prone.kinds,
            with_opener(prone, &prone_estimate),
            Some(prone_choice),
        ),
    ];
    for ((model, kinds, estimate, choice), scores) in ways.iter().zip(&mut backtest.scenarios) {
        let (expected, day_form) = planned(model, estimate);
        let chances = sequential_chances(kinds, &expected, &day_form, &latest.attempts, *choice);
        add_meet(scores, &latest.attempts, &chances, &past_misses);
    }

    for lift in Lift::ALL {
        let slot_rates = fitted.stats.get(lift).make_rates.all();
        let attempts = latest.attempts.get(lift);
        for (&kg, slot) in attempts.iter().zip(slots(attempts)) {
            backtest.baseline.add(slot_rates[slot], kg > 0.0);
        }
    }
}

/// Scores the models fitted to `fitted` on the held-out lifters among `held_out`, with
/// the load choice fitted to some of the lifters in `fitted_to`.
pub fn backtest(fitted: &Fitted, fitted_to: &[Entry], held_out: &[Entry]) -> Backtest {
    let models = [fitted.model(), fitted.kinds_model()]
        .map(|model| (model, fit_load_choice(&model, fitted_to)));
    let lifters = lifters_meets(held_out);
    let mut backtest = in_parallel(&lifters, |meets| {
        let mut backtest = Backtest::default();
        score_lifter(fitted, &models, meets, &mut backtest);
        backtest
    });
    backtest.load_choices = models.map(|(_, choice)| choice);
    backtest
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
        assert_eq!(slots(&[100.0, 105.0, -110.0]), [0, 1, 3]);
        assert_eq!(slots(&[-100.0, 100.0, 105.0]), [0, 2, 3]);
        assert_eq!(slots(&[100.0, -105.0, -105.0]), [0, 1, 4]);
    }

    #[test]
    fn a_known_day_max_gives_each_attempt_its_own_chance() {
        let curves = PerLift::from_fn(|_| [(1.0, CURVE), (0.0, CURVE)]);
        let expected = PerLift::from_fn(|_| 100.0);
        let attempts = PerLift::from_fn(|_| [90.0, -100.0, 102.5]);
        let day_form = DayForm {
            spread: 0.0,
            max_uncertainty: 0.0,
            correlation: 0.4,
        };
        let chances = sequential_chances(&curves, &expected, &day_form, &attempts, None);
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
        let curves = PerLift::from_fn(|_| [(1.0, CURVE), (0.0, CURVE)]);
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
            sequential_chances(&curves, &expected, &day_form, &attempts, None).bench[0]
        };
        let (made, missed) = ([90.0, 95.0, 100.0], [90.0, -95.0, -95.0]);
        assert!(bench_opener(missed, 0.5) < bench_opener(made, 0.5) - 0.05);
        assert!(close(bench_opener(missed, 0.0), bench_opener(made, 0.0)));
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
