//! Scores the planner's predictions on meets it has not seen.
//!
//! Lifters are split in two by name. The model is fitted to one part. For each lifter in
//! the other part with at least two complete meets, the expected maxes for their latest
//! meet are estimated from all the meets before it, as the page does from pasted past
//! meets.
//! Each attempt of the latest meet then gets the model's chance of being made, given the
//! attempts before it in the meet, and is scored against what happened.
//!
//! Lifters choose their attempts knowing how their training and their day are going,
//! which past meets don't show. Some scenarios stand in for that knowledge: the opener as
//! evidence of the expected max, and the 2nd and 3rd attempts as evidence of the day max.

use std::ops::AddAssign;

use attempt_sim::lift::{Lift, PerLift};
use attempt_sim::model::DayForm;

use crate::attempts::{
    BACKTEST_GRID, Estimates, Meet, every_lift, lifters_meets, sequential_chances, slots,
};
use crate::data::Entry;
use crate::fitted::Fitted;
use crate::parallel::in_parallel;

/// One lifter in this many is held out for scoring.
const HELD_OUT_EVERY: u64 = 10;
/// Keeps the log loss finite for a chance rounded to 0 or 1.
const SUREST_CHANCE: f64 = 1e-6;
/// Bounds of the bins of predicted chances, from 0 to 1.
pub const BIN_EDGES: [f64; 8] = [0.0, 0.5, 0.7, 0.8, 0.9, 0.95, 0.98, 1.0];
/// Lifters by how many attempts at a lift they missed in their past meets: 0, 1, 2 or more.
pub const PAST_MISS_GROUPS: usize = 3;

/// One way of predicting the chances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scenario {
    pub name: &'static str,
    /// Also taking the opener the lifter chose as evidence of their expected max.
    opener: bool,
    /// Also taking each 2nd and 3rd attempt the lifter chose as evidence of their day max,
    /// through the fitted load choices.
    chosen_loads: bool,
}

pub const SCENARIOS: [Scenario; 3] = [
    Scenario {
        name: "from past meets",
        opener: false,
        chosen_loads: false,
    },
    Scenario {
        name: "from past meets and the opener",
        opener: true,
        chosen_loads: false,
    },
    Scenario {
        name: "opener and loads chosen on the day",
        opener: true,
        chosen_loads: true,
    },
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
    pub fn add(&mut self, chance: f64, made: bool) {
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

/// Scores one lifter's latest meet from the meets before it, in each of [`SCENARIOS`].
fn score_lifter(fitted: &Fitted, meets: &[Meet]) -> Backtest {
    let mut backtest = Backtest::default();
    let model = fitted.model();
    let (latest, past) = meets.split_last().expect("two or more meets");
    let Some(estimates) = Estimates::of(&model, latest, past) else {
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
    for (scenario, scores) in SCENARIOS.iter().zip(&mut backtest.scenarios) {
        // With the opener the model's uncertainty of the expected max, measured from
        // consecutive meets, is the one the fit used; from past meets alone, the
        // estimate's own is the better guess.
        let (estimate, day_form) = if scenario.opener {
            (&estimates.with_opener, model.day_form)
        } else {
            let max_uncertainty = estimates.past_meets.shared_uncertainty;
            let day_form = DayForm {
                max_uncertainty,
                ..model.day_form
            };
            (&estimates.past_meets, day_form)
        };
        let expected = every_lift(estimate)
            .expect("every lift is estimated")
            .map(|_, lift| lift.expected_max_kg);
        let choices = scenario
            .chosen_loads
            .then_some(&fitted.attempts.load_choices);
        let chances = sequential_chances(
            &BACKTEST_GRID,
            &model.curves,
            &expected,
            &day_form,
            &latest.attempts,
            choices,
        );
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

/// Scores the model fitted to `fitted` on the lifters in `held_out`.
pub fn backtest(fitted: &Fitted, held_out: &[Entry]) -> Backtest {
    let lifters = lifters_meets(held_out);
    in_parallel(&lifters, |meets| score_lifter(fitted, meets))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn holds_out_about_one_lifter_in_ten_the_same_way_every_time() {
        let names: Vec<String> = (0..10_000).map(|i| format!("Lifter {i}")).collect();
        let held_out = names.iter().filter(|name| is_held_out(name)).count();
        assert!((800..1_200).contains(&held_out), "{held_out}");
        assert_eq!(is_held_out("Lifter 7"), is_held_out("Lifter 7"));
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
