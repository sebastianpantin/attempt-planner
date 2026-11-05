//! Scores the planner's predictions on meets it has not seen.
//!
//! Lifters are split in two by name. The model is fitted to one part. For each lifter in
//! the other part with at least two complete meets, the expected maxes for their latest
//! meet are estimated from the meets before it, as the page does from pasted past meets.
//! Each attempt of the latest meet then gets the model's chance of being made, given the
//! attempts before it in the meet, and is scored against what happened.
//!
//! Lifters choose their attempts knowing how their training and their day are going,
//! which past meets don't show. Some scenarios stand in for that knowledge: the opener as
//! evidence of the expected max, and the 2nd and 3rd attempts as evidence of the day max.

use std::ops::AddAssign;
use std::sync::LazyLock;

use attempt_sim::lift::{Lift, PerLift};
use attempt_sim::minimize::minimize_to;
use attempt_sim::model::{DayForm, MakeRateCurve};
use attempt_sim::normal::normal_grid;
use attempt_sim::past_meets::{
    LiftEstimate, MaxEstimate, MaxEstimateInput, PastAttempt, PastMeet, estimate_maxes,
    log_chance_of_past_meets,
};

use crate::data::{Entry, by_lifter_and_day};
use crate::fitted::{Fitted, Kinds, Model};
use crate::parallel::in_parallel;
use crate::stats::is_complete;

/// The grid of normal draws the chances are averaged over. Coarser than the planner's,
/// which is plenty for curves as soft as the fitted ones and four times faster.
static GRID: LazyLock<Vec<(f64, f64)>> = LazyLock::new(|| normal_grid(GRID_POINTS, 6.0));
const GRID_POINTS: usize = 61;

/// One lifter in this many is held out for scoring.
const HELD_OUT_EVERY: u64 = 10;
/// The past meets used to estimate the maxes, the latest ones.
const MAX_PAST_MEETS: usize = 6;
/// Keeps the log loss finite for a chance rounded to 0 or 1.
const SUREST_CHANCE: f64 = 1e-6;
/// Lifters whose meets the load choice is fitted to, at most.
const CHOICE_LIFTERS: usize = 2_000;
/// Bounds of the bins of predicted chances, from 0 to 1.
pub const BIN_EDGES: [f64; 8] = [0.0, 0.5, 0.7, 0.8, 0.9, 0.95, 0.98, 1.0];
/// Lifters by how many attempts at a lift they missed in their past meets: 0, 1, 2 or more.
pub const PAST_MISS_GROUPS: usize = 3;

/// One way of predicting the chances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scenario {
    pub name: &'static str,
    /// With a share of lifters prone to misses whatever the load, not every lifter alike.
    prone: bool,
    /// With that share updated by how likely the lifter's past meets are for each kind.
    own_kind: bool,
    /// Also taking the opener the lifter chose as evidence of their expected max.
    opener: bool,
    /// Also taking each 2nd and 3rd attempt the lifter chose as evidence of their day max,
    /// through a fitted [`LoadChoice`].
    chosen_loads: bool,
}

const fn scenario(
    name: &'static str,
    prone: bool,
    own_kind: bool,
    opener: bool,
    chosen_loads: bool,
) -> Scenario {
    Scenario {
        name,
        prone,
        own_kind,
        opener,
        chosen_loads,
    }
}

pub const SCENARIOS: [Scenario; 8] = [
    scenario("alike, from past meets", false, false, false, false),
    scenario(
        "alike, from past meets and the opener",
        false,
        false,
        true,
        false,
    ),
    scenario("prone share, from past meets", true, false, false, false),
    scenario(
        "prone share, from past meets and the opener",
        true,
        false,
        true,
        false,
    ),
    scenario("own kind, from past meets", true, true, false, false),
    scenario(
        "own kind, from past meets and the opener",
        true,
        true,
        true,
        false,
    ),
    scenario(
        "alike, opener and loads chosen on the day",
        false,
        false,
        true,
        true,
    ),
    scenario(
        "prone share, opener and loads chosen on the day",
        true,
        false,
        true,
        true,
    ),
];

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

impl AddAssign<&Score> for Score {
    fn add_assign(&mut self, other: &Score) {
        self.attempts += other.attempts;
        self.made += other.made;
        self.predicted += other.predicted;
        self.log_loss += other.log_loss;
        self.brier += other.brier;
    }
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

/// Scores of one scenario.
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

impl AddAssign<&Scores> for Scores {
    fn add_assign(&mut self, other: &Scores) {
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
    /// One per [`SCENARIOS`].
    pub scenarios: [Scores; SCENARIOS.len()],
}

impl AddAssign<&Backtest> for Backtest {
    fn add_assign(&mut self, other: &Backtest) {
        self.meets += other.meets;
        self.skipped += other.skipped;
        self.baseline += &other.baseline;
        for (scores, other) in self.scenarios.iter_mut().zip(&other.scenarios) {
            *scores += other;
        }
    }
}

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

/// Each lifter's complete meets in date order, one per day, for lifters with two or more.
fn lifters_meets(entries: &[Entry]) -> Vec<Vec<Meet>> {
    let complete: Vec<(&str, Meet)> = by_lifter_and_day(entries)
        .into_iter()
        .filter(|(_, _, entry)| {
            Lift::ALL
                .iter()
                .all(|&lift| is_complete(*entry.attempts.get(lift)))
        })
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

/// A lifter's latest meet and the past meets the estimate uses.
fn latest_and_past(meets: &[Meet]) -> Option<(&Meet, &[Meet])> {
    let (latest, past) = meets.split_last()?;
    Some((latest, &past[past.len().saturating_sub(MAX_PAST_MEETS)..]))
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
/// lifter holds for all its attempts. With a `choice`, each 2nd and 3rd attempt is also
/// evidence of the day max before its chance is worked out.
fn sequential_chances(
    kinds: &PerLift<Kinds>,
    expected_kg: &PerLift<f64>,
    day_form: &DayForm,
    attempts: &PerLift<[f64; 3]>,
    choice: Option<&LoadChoice>,
) -> PerLift<[f64; 3]> {
    let draws = &*GRID;
    let weights = day_form.draw_weights();
    let n = draws.len();
    let at = |kind: usize, shared: usize, own: usize| (kind * n + shared) * n + own;
    // For each lift, kind and pair of shared and own draws: the chance of the lift's
    // attempts so far. And for each lift and shared draw, that averaged over the own draws
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
        let days: Vec<f64> = draws
            .iter()
            .flat_map(|&(shared_z, _)| {
                draws
                    .iter()
                    .map(move |&(own_z, _)| weights.day_max(expected, shared_z, own_z))
            })
            .collect();
        let log_days: Vec<f64> = if choice.is_some() {
            days.iter().map(|day| day.ln()).collect()
        } else {
            Vec::new()
        };
        let kinds = kinds.get(lift);
        let strength = MakeRateCurve {
            technical_miss_chance: 0.0,
            ..kinds.curve
        };
        let shares = kinds.shares();
        let present = || {
            shares
                .iter()
                .enumerate()
                .filter(|(_, (share, _))| *share > 0.0)
        };
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
                        let likelihood = (-gap * gap / 2.0).exp();
                        for (kind, &(share, _)) in present() {
                            let chance = &mut chances.get_mut(lift)[at(kind, i, j)];
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
            for (i, &(_, shared_weight)) in draws.iter().enumerate() {
                let mut average_made = 0.0;
                for (j, &(_, own_weight)) in draws.iter().enumerate() {
                    let strong = strength.make_probability_kg(load, days[i * n + j]);
                    for (kind, &(share, no_technical_miss)) in present() {
                        let chance = &mut chances.get_mut(lift)[at(kind, i, j)];
                        let make = no_technical_miss * strong;
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
/// past meets are for each kind, given the estimated maxes.
fn own_kinds(model: &Model, input: &MaxEstimateInput, estimate: &MaxEstimate) -> PerLift<Kinds> {
    model.kinds.map(|lift, kinds| {
        let log_chance =
            |curve: &MakeRateCurve| log_chance_of_past_meets(input, estimate, lift, curve);
        let (Some(alike), Some(prone)) =
            (log_chance(&kinds.curve), log_chance(&kinds.prone_curve()))
        else {
            return *kinds;
        };
        let log_odds = (kinds.prone_share / (1.0 - kinds.prone_share)).ln() + prone - alike;
        Kinds {
            prone_share: 1.0 / (1.0 + (-log_odds).exp()),
            ..*kinds
        }
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

/// A model's estimates of a lifter's expected maxes for their latest meet.
struct Estimates {
    input: MaxEstimateInput,
    past_meets: MaxEstimate,
    with_opener: MaxEstimate,
}

impl Estimates {
    /// `None` unless every lift gets an estimate with a finite uncertainty.
    fn of(model: &Model, latest: &Meet, past: &[Meet]) -> Option<Self> {
        let input = MaxEstimateInput {
            curves: model.mean_curves(),
            typical_opener: model.typical_opener,
            day_form_spread: model.day_form.spread,
            meets: past.iter().map(Meet::past).collect(),
            on_day: latest.day,
        };
        let past_meets = estimate_maxes(&input).ok()?;
        let openers = latest.attempts.map(|_, attempts| attempts[0].abs());
        let with_opener = past_meets.with_openers(&openers, &model.typical_opener);
        every_lift(&past_meets)?;
        every_lift(&with_opener)?;
        Some(Estimates {
            input,
            past_meets,
            with_opener,
        })
    }
}

/// The expected maxes and day form the planner would use with this estimate, which has
/// every lift.
fn planned(model: &Model, estimate: &MaxEstimate) -> (PerLift<f64>, DayForm) {
    let lifts = every_lift(estimate).expect("every lift is estimated");
    let day_form = DayForm {
        max_uncertainty: estimate.shared_uncertainty,
        ..model.day_form
    };
    (lifts.map(|_, lift| lift.expected_max_kg), day_form)
}

/// The [`LoadChoice`] under which `model`'s chances of the latest meets of some of
/// `entries`' lifters, with maxes from past meets and the opener, have the lowest log loss.
/// The search starts from `start`, or from typical jumps without one.
pub fn fit_load_choice(model: &Model, entries: &[Entry], start: Option<&LoadChoice>) -> LoadChoice {
    let lifters = lifters_meets(entries);
    let step = lifters.len().div_ceil(CHOICE_LIFTERS).max(1);
    let meets: Vec<_> = lifters
        .iter()
        .step_by(step)
        .filter_map(|meets| {
            let (latest, past) = latest_and_past(meets)?;
            let estimates = Estimates::of(model, latest, past)?;
            let (expected, day_form) = planned(model, &estimates.with_opener);
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
    let start = start.map_or([0.95, 0.9, 0.99, 0.95, 0.04].map(f64::ln), |choice| {
        let [a, b, c, d] = choice.log_shares;
        [a, b, c, d, choice.spread.ln()]
    });
    choice(minimize_to(
        log_loss,
        start,
        [0.02, 0.02, 0.02, 0.02, 0.3],
        1e-6,
        400,
    ))
}

/// Each model with its fitted load choice: every lifter alike, then with prone lifters.
type Models = [(Model, LoadChoice); 2];

/// Scores one lifter's latest meet from the meets before it, in each of [`SCENARIOS`].
fn score_lifter(fitted: &Fitted, models: &Models, meets: &[Meet]) -> Backtest {
    let mut backtest = Backtest::default();
    let (latest, past) = latest_and_past(meets).expect("two or more meets");
    let estimates = models
        .iter()
        .map(|(model, _)| Estimates::of(model, latest, past))
        .collect::<Option<Vec<_>>>();
    let Some(estimates) = estimates else {
        backtest.skipped += 1;
        return backtest;
    };
    backtest.meets += 1;

    let past_misses = PerLift::from_fn(|lift| {
        past.iter()
            .flat_map(|meet| meet.attempts.get(lift))
            .filter(|&&kg| kg < 0.0)
            .count()
            .min(PAST_MISS_GROUPS - 1)
    });
    let (prone, prone_estimates) = (&models[1].0, &estimates[1]);
    let own = own_kinds(prone, &prone_estimates.input, &prone_estimates.past_meets);
    for (scenario, scores) in SCENARIOS.iter().zip(&mut backtest.scenarios) {
        let which = usize::from(scenario.prone);
        let ((model, choice), estimates) = (&models[which], &estimates[which]);
        let estimate = if scenario.opener {
            &estimates.with_opener
        } else {
            &estimates.past_meets
        };
        let kinds = if scenario.own_kind { own } else { model.kinds };
        let choice = scenario.chosen_loads.then_some(choice);
        let (expected, day_form) = planned(model, estimate);
        let chances = sequential_chances(&kinds, &expected, &day_form, &latest.attempts, choice);
        add_meet(scores, &latest.attempts, &chances, &past_misses);
    }

    for lift in Lift::ALL {
        let slot_rates = fitted.stats.get(lift).make_rates.all();
        let attempts = latest.attempts.get(lift);
        for (&kg, slot) in attempts.iter().zip(slots(attempts)) {
            backtest.baseline.add(slot_rates[slot], kg > 0.0);
        }
    }
    backtest
}

/// Scores the models fitted to `fitted` on the lifters in `held_out`, with each model's
/// load choice fitted to some of the lifters in `fitted_to`. Also gives the load choices:
/// every lifter alike, then with prone lifters.
pub fn backtest(
    fitted: &Fitted,
    fitted_to: &[Entry],
    held_out: &[Entry],
) -> (Backtest, [LoadChoice; 2]) {
    let alike = fitted.model();
    let alike_choice = fit_load_choice(&alike, fitted_to, None);
    let prone = fitted.kinds_model();
    let prone_choice = fit_load_choice(&prone, fitted_to, Some(&alike_choice));
    let models = [(alike, alike_choice), (prone, prone_choice)];
    let lifters = lifters_meets(held_out);
    let backtest = in_parallel(&lifters, |meets| score_lifter(fitted, &models, meets));
    (backtest, [alike_choice, prone_choice])
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURVE: MakeRateCurve = MakeRateCurve {
        ratio_at_half: 1.0,
        steepness: 50.0,
        technical_miss_chance: 0.02,
    };

    const ALIKE: Kinds = Kinds {
        curve: CURVE,
        prone_share: 0.0,
        prone_technical_miss_chance: 0.0,
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
        let curves = PerLift::from_fn(|_| ALIKE);
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
        let curves = PerLift::from_fn(|_| ALIKE);
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
    fn a_heavier_chosen_attempt_says_the_day_is_better() {
        let curves = PerLift::from_fn(|_| ALIKE);
        let expected = PerLift::from_fn(|_| 100.0);
        let day_form = DayForm {
            spread: 0.04,
            max_uncertainty: 0.0,
            correlation: 0.4,
        };
        let choice = LoadChoice {
            log_shares: [0.97f64.ln(); 4],
            spread: 0.02,
        };
        let second = |kg: f64| {
            let attempts = PerLift::from_fn(|_| [90.0, kg, kg + 2.5]);
            sequential_chances(&curves, &expected, &day_form, &attempts, Some(&choice)).squat[1]
        };
        let without = |kg: f64| {
            let attempts = PerLift::from_fn(|_| [90.0, kg, kg + 2.5]);
            sequential_chances(&curves, &expected, &day_form, &attempts, None).squat[1]
        };
        assert!(second(102.5) > without(102.5) + 0.1, "{}", second(102.5));
        assert!(
            second(102.5) > second(95.0) - 0.2,
            "{} {}",
            second(102.5),
            second(95.0)
        );
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
