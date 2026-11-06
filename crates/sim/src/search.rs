use std::sync::Arc;

use crate::days::{StepPlan, Steps, goal_steps, totals};
use crate::evaluate::{InputProblem, Lifter, MeetDays, plan_problems};
use crate::lift::{Lift, PerLift};
use crate::meet_day::Taken;
pub use crate::outcome::Outcome;
use crate::outcome::{OutcomeTally, is_valid_bomb_out_cap};
use crate::plan::{Jumps, LiftPlan};
use crate::rules::{BAR, PLATE_STEP, Weight};
use crate::tradeoff;

/// A search stops after this many rounds even if it is still improving.
pub const MAX_ROUNDS: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchInput {
    pub lifter: Lifter,
    /// Plans to start from. Ignored when they break the attempt rules.
    pub start: Option<PerLift<LiftPlan>>,
    pub goal: Option<Weight>,
    /// Highest acceptable chance of bombing out, if any.
    pub bomb_out_cap: Option<f64>,
    pub simulations: u32,
    pub seed: u64,
    /// The plans at `best_for_search` of a tradeoff already swept with the same lifter, goal,
    /// cap, simulations and seed, so the search need not sweep again. Swept when absent or
    /// invalid.
    pub family_best: Option<PerLift<LiftPlan>>,
}

/// Where a search starts from and what it ranks plans by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SearchStart {
    pub(crate) plans: Option<PerLift<LiftPlan>>,
    pub(crate) family_best: Option<PerLift<LiftPlan>>,
    pub(crate) goal: Option<Weight>,
    pub(crate) bomb_out_cap: Option<f64>,
}

/// Plans with their outcome on the simulated meets they were scored on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Found {
    pub plans: PerLift<LiftPlan>,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchProgress {
    /// The round in progress, from 1. A round tries every setting of every lift once.
    pub round: u32,
    pub done: bool,
    pub best: Found,
}

/// One number in a lift plan that the search varies while the rest stay fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum Setting {
    Opener,
    SecondAfterMake,
    SecondAfterMiss,
    ThirdAfterMake,
    ThirdAfterMiss,
}

const SETTINGS: [Setting; 5] = [
    Setting::Opener,
    Setting::SecondAfterMake,
    Setting::SecondAfterMiss,
    Setting::ThirdAfterMake,
    Setting::ThirdAfterMiss,
];

const COORDINATES: usize = Lift::ALL.len() * SETTINGS.len();

impl Setting {
    /// The weights tried for this setting, as percentages of the expected max rounded
    /// down to the plate step.
    fn values(self, expected_max: Weight) -> impl Iterator<Item = Weight> {
        let (from_percent, to_percent, lightest) = match self {
            Self::Opener => (70, 105, BAR),
            Self::SecondAfterMake | Self::ThirdAfterMake => (0, 15, PLATE_STEP),
            Self::SecondAfterMiss | Self::ThirdAfterMiss => (0, 5, Weight::ZERO),
        };
        let step = u64::from(PLATE_STEP.grams());
        let at = |percent: u64| u64::from(expected_max.grams()) * percent / 100 / step * step;
        let lightest = u64::from(lightest.grams());
        (at(from_percent).max(lightest)..=at(to_percent))
            .step_by(PLATE_STEP.grams() as usize)
            .filter_map(|grams| u32::try_from(grams).ok().map(Weight::from_grams))
    }

    fn value(self, plan: &LiftPlan) -> Weight {
        match self {
            Self::Opener => plan.opener,
            Self::SecondAfterMake => plan.second.after_make,
            Self::SecondAfterMiss => plan.second.after_miss,
            Self::ThirdAfterMake => plan.third.after_make,
            Self::ThirdAfterMiss => plan.third.after_miss,
        }
    }

    fn with(self, mut plan: LiftPlan, weight: Weight) -> LiftPlan {
        match self {
            Self::Opener => plan.opener = weight,
            Self::SecondAfterMake => plan.second.after_make = weight,
            Self::SecondAfterMiss => plan.second.after_miss = weight,
            Self::ThirdAfterMake => plan.third.after_make = weight,
            Self::ThirdAfterMiss => plan.third.after_miss = weight,
        }
        plan
    }
}

/// Looks for the plans with the best chance of reaching the goal (or, without a goal,
/// the highest average total) whose bomb-out chance stays under the cap.
///
/// Starts from the best of the tradeoff family and the given plans, then changes one
/// setting of one lift at a time, keeping any change that ranks higher, until a whole
/// round changes nothing. Every plan is scored on the same simulated meets.
#[derive(Debug, Clone)]
pub struct Search {
    days: Arc<MeetDays>,
    /// The attempts already taken, whose settings stay as they were.
    taken: Taken,
    goal: Option<Weight>,
    bomb_out_cap: Option<f64>,
    best: Found,
    /// Each day's total of the lifts other than the one being changed. They stay the same
    /// while one lift's settings are tried, so they are worked out when the lift changes.
    others: Vec<Steps>,
    next: usize,
    unchanged: usize,
    round: u32,
}

impl Search {
    pub fn new(input: &SearchInput) -> Result<Self, Vec<InputProblem>> {
        let days = MeetDays::shared_with_cap(
            input.lifter,
            input.simulations,
            input.seed,
            input.bomb_out_cap,
        )?;
        Ok(Self::on_days(
            days,
            Taken::default(),
            &SearchStart {
                plans: input.start,
                family_best: input.family_best,
                goal: input.goal,
                bomb_out_cap: input.bomb_out_cap,
            },
        ))
    }

    /// A search on `days` for plans that take the attempts in `taken`, which `days` must
    /// agree with. Only the settings the meet has not decided yet change.
    pub(crate) fn on_days(days: Arc<MeetDays>, taken: Taken, start: &SearchStart) -> Self {
        let higher = |a: Found, b: Found| {
            if ranks_higher(start.bomb_out_cap, &b.outcome, &a.outcome) {
                b
            } else {
                a
            }
        };
        let found = |plans: PerLift<LiftPlan>| {
            let plans = taken.pin_all(&plans);
            Found {
                plans,
                outcome: score(&days, &plans, start.goal),
            }
        };
        let valid =
            |plans: &PerLift<LiftPlan>| plans.iter().all(|(_, plan)| plan.problems().is_empty());
        let family_best = match start.family_best.filter(valid) {
            Some(plans) => Some(found(plans)),
            None => {
                let swept = tradeoff::sweep(&days, start.goal, start.bomb_out_cap, &taken);
                swept.best_for_search.map(|index| {
                    let point = &swept.points[index];
                    Found {
                        plans: point.plans,
                        outcome: point.outcome(),
                    }
                })
            }
        };
        let best = family_best
            .into_iter()
            .chain(start.plans.filter(valid).map(found))
            .reduce(higher)
            .unwrap_or_else(|| found(PerLift::from_fn(|_| fallback_plan())));
        Self {
            days,
            taken,
            goal: start.goal,
            bomb_out_cap: start.bomb_out_cap,
            best,
            others: Vec::new(),
            next: 0,
            unchanged: 0,
            round: 1,
        }
    }

    pub fn is_done(&self) -> bool {
        self.unchanged >= COORDINATES || self.round > MAX_ROUNDS
    }

    pub fn best(&self) -> Found {
        self.best
    }

    /// Tries every value of the next setting and keeps the best. Does nothing once done.
    pub fn step(&mut self) -> SearchProgress {
        if !self.is_done() {
            let lift = Lift::ALL[self.next / SETTINGS.len()];
            let setting = SETTINGS[self.next % SETTINGS.len()];
            if self.next.is_multiple_of(SETTINGS.len()) {
                self.others = other_lifts_totals(&self.days, &self.best.plans, lift);
            }
            let current = *self.best.plans.get(lift);
            let expected_max = self.days.lifter().lifts.get(lift).expected_max;
            let mut improved = false;
            for weight in setting.values(expected_max) {
                let candidate = self.taken.pin(lift, setting.with(current, weight));
                if candidate == current || !candidate.problems().is_empty() {
                    continue;
                }
                let outcome = score_lift(&self.days, lift, &candidate, &self.others, self.goal);
                if ranks_higher(self.bomb_out_cap, &outcome, &self.best.outcome) {
                    *self.best.plans.get_mut(lift) = candidate;
                    self.best.outcome = outcome;
                    improved = true;
                }
            }
            self.unchanged = if improved { 0 } else { self.unchanged + 1 };
            self.next = (self.next + 1) % COORDINATES;
            if self.next == 0 {
                self.round += 1;
            }
        }
        SearchProgress {
            round: self.round.min(MAX_ROUNDS),
            done: self.is_done(),
            best: self.best,
        }
    }
}

/// Each day's total over the lifts other than `lift`, 0 when one of them bombs out.
fn other_lifts_totals(days: &MeetDays, plans: &PerLift<LiftPlan>, lift: Lift) -> Vec<Steps> {
    totals(
        lift.others()
            .map(|other| (days.lifts().get(other), StepPlan::new(plans.get(other)))),
    )
}

/// The outcome with `plan` for `lift`, given the other lifts' totals on the same days.
fn score_lift(
    days: &MeetDays,
    lift: Lift,
    plan: &LiftPlan,
    others: &[Steps],
    goal: Option<Weight>,
) -> Outcome {
    let mut tally = OutcomeTally::default();
    let plan = StepPlan::new(plan);
    days.lifts()
        .get(lift)
        .record(&mut tally, &plan, others, goal_steps(goal));
    tally.outcome(goal)
}

/// The plans to change, and what to rank the changes by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChangeInput {
    pub lifter: Lifter,
    pub plans: PerLift<LiftPlan>,
    pub goal: Option<Weight>,
    pub bomb_out_cap: Option<f64>,
    pub simulations: u32,
    pub seed: u64,
}

/// One setting of one lift moved by one or two plate steps, and the plans it gives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    pub lift: Lift,
    pub setting: Setting,
    pub weight: Weight,
    pub found: Found,
    /// Whether the change ranks above the plans as given, as the search ranks plans.
    pub improves: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Changes {
    /// The outcome of the plans as given, on the same simulated meets as the changes.
    pub current: Outcome,
    /// Every valid change that moves the outcome, ranked as the search ranks plans, best
    /// first.
    pub changes: Vec<Change>,
}

/// How many plate steps a single change moves a setting by.
const CHANGE_STEPS: [u32; 2] = [1, 2];

/// Moves each setting of each lift one and two plate steps up and down, keeping the rest
/// of the plans, and scores every change on the same simulated meets as the plans as
/// given. Changes that leave the outcome exactly as it was are left out.
pub fn single_changes(input: &ChangeInput) -> Result<Changes, Vec<InputProblem>> {
    let days = MeetDays::shared(input.lifter, input.simulations, input.seed);
    let mut problems = days.as_ref().err().cloned().unwrap_or_default();
    problems.extend(plan_problems(&input.plans));
    if !is_valid_bomb_out_cap(input.bomb_out_cap) {
        problems.push(InputProblem::InvalidBombOutCap);
    }
    let days = match days {
        Ok(days) if problems.is_empty() => days,
        _ => return Err(problems),
    };
    let current = score(&days, &input.plans, input.goal);
    let current_rank = rank(input.bomb_out_cap, &current);
    let mut changes = Vec::new();
    for lift in Lift::ALL {
        let others = other_lifts_totals(&days, &input.plans, lift);
        let plan = *input.plans.get(lift);
        for setting in SETTINGS {
            let grams = setting.value(&plan).grams();
            let steps = CHANGE_STEPS.into_iter().flat_map(|plates| {
                let by = plates * PLATE_STEP.grams();
                [grams.checked_sub(by), grams.checked_add(by)]
            });
            for weight in steps.flatten().map(Weight::from_grams) {
                let candidate = setting.with(plan, weight);
                if !candidate.problems().is_empty() {
                    continue;
                }
                let mut plans = input.plans;
                *plans.get_mut(lift) = candidate;
                let outcome = score_lift(&days, lift, &candidate, &others, input.goal);
                if outcome == current {
                    continue;
                }
                changes.push(Change {
                    lift,
                    setting,
                    weight,
                    found: Found { plans, outcome },
                    improves: rank(input.bomb_out_cap, &outcome) > current_rank,
                });
            }
        }
    }
    let rank_of = |change: &Change| rank(input.bomb_out_cap, &change.found.outcome);
    changes.sort_by(|a, b| {
        rank_of(b)
            .partial_cmp(&rank_of(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(Changes { current, changes })
}

pub(crate) fn ranks_higher(bomb_out_cap: Option<f64>, a: &Outcome, b: &Outcome) -> bool {
    rank(bomb_out_cap, a) > rank(bomb_out_cap, b)
}

/// Feasible outcomes rank above those over the cap. Feasible ones rank by goal chance and
/// then average total; the rest by lower bomb-out chance and then average total.
fn rank(bomb_out_cap: Option<f64>, outcome: &Outcome) -> [f64; 3] {
    let feasible = bomb_out_cap.is_none_or(|cap| outcome.bomb_out_probability <= cap);
    match (feasible, outcome.goal_probability) {
        (true, Some(goal)) => [1.0, goal, outcome.expected_total_kg],
        (true, None) => [1.0, outcome.expected_total_kg, 0.0],
        (false, _) => [
            0.0,
            -outcome.bomb_out_probability,
            outcome.expected_total_kg,
        ],
    }
}

fn score(days: &MeetDays, plans: &PerLift<LiftPlan>, goal: Option<Weight>) -> Outcome {
    let others = other_lifts_totals(days, plans, Lift::Squat);
    score_lift(days, Lift::Squat, &plans.squat, &others, goal)
}

/// The lightest valid plan, a start for maxes too light for the tradeoff family.
fn fallback_plan() -> LiftPlan {
    let jumps = Jumps {
        after_make: PLATE_STEP,
        after_miss: Weight::ZERO,
    };
    LiftPlan {
        opener: BAR,
        second: jumps,
        third: jumps,
    }
}

pub fn search(input: &SearchInput) -> Result<Found, Vec<InputProblem>> {
    let mut search = Search::new(input)?;
    while !search.is_done() {
        search.step();
    }
    Ok(search.best())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluate::{EvaluateInput, LiftModel, evaluate};
    use crate::model::{DayForm, MakeRateCurve};
    use crate::testing::kg;

    fn lifter(spread: f64, steepness: f64) -> Lifter {
        let maxes = PerLift {
            squat: 200.0,
            bench: 130.0,
            deadlift: 240.0,
        };
        Lifter {
            lifts: maxes.map(|_, &max| LiftModel {
                expected_max: kg(max),
                curve: MakeRateCurve {
                    ratio_at_half: 1.0,
                    steepness,
                    technical_miss_chance: 0.0,
                },
            }),
            day_form: DayForm {
                spread,
                max_uncertainty: 0.0,
                correlation: 0.5,
            },
        }
    }

    fn plan(opener: f64, second: f64, third: f64) -> LiftPlan {
        LiftPlan {
            opener: kg(opener),
            second: Jumps {
                after_make: kg(second),
                after_miss: kg(0.0),
            },
            third: Jumps {
                after_make: kg(third),
                after_miss: kg(0.0),
            },
        }
    }

    fn timid_plans() -> PerLift<LiftPlan> {
        PerLift {
            squat: plan(150.0, 10.0, 10.0),
            bench: plan(100.0, 5.0, 5.0),
            deadlift: plan(180.0, 10.0, 10.0),
        }
    }

    fn input(goal: Option<f64>, cap: Option<f64>) -> SearchInput {
        SearchInput {
            lifter: lifter(0.03, 30.0),
            start: Some(timid_plans()),
            goal: goal.map(kg),
            bomb_out_cap: cap,
            simulations: 2_000,
            seed: 5,
            family_best: None,
        }
    }

    fn outcome_of(input: &SearchInput, plans: PerLift<LiftPlan>) -> Outcome {
        let evaluation = evaluate(&EvaluateInput {
            lifter: input.lifter,
            plans,
            goal: input.goal,
            simulations: input.simulations,
            seed: input.seed,
        })
        .unwrap();
        Outcome {
            bomb_out_probability: evaluation.bomb_out_probability,
            expected_total_kg: evaluation.expected_total_kg,
            goal_probability: evaluation.goal_probability,
        }
    }

    #[test]
    fn finds_the_certain_best_when_nothing_is_uncertain() {
        let mut lifter = lifter(0.0, 10_000.0);
        for lift in Lift::ALL {
            lifter.lifts.get_mut(lift).curve.ratio_at_half = 1.005;
        }
        let input = SearchInput {
            lifter,
            ..input(None, None)
        };
        let found = search(&input).unwrap();
        assert_eq!(found.outcome.bomb_out_probability, 0.0);
        assert_eq!(found.outcome.expected_total_kg, 200.0 + 130.0 + 240.0);
    }

    #[test]
    fn reports_the_outcome_of_the_plans_it_returns() {
        let input = input(Some(550.0), None);
        let found = search(&input).unwrap();
        let outcome = outcome_of(&input, found.plans);
        assert!((outcome.expected_total_kg - found.outcome.expected_total_kg).abs() < 1e-9);
        assert_eq!(outcome.goal_probability, found.outcome.goal_probability);
        assert_eq!(
            outcome.bomb_out_probability,
            found.outcome.bomb_out_probability
        );
    }

    #[test]
    fn never_does_worse_than_the_start_or_the_family() {
        let input = input(Some(550.0), None);
        let found = search(&input).unwrap();
        let start = outcome_of(&input, timid_plans());
        assert!(found.outcome.goal_probability >= start.goal_probability);
        let family = tradeoff::tradeoff(&tradeoff::TradeoffInput {
            lifter: input.lifter,
            goal: input.goal,
            bomb_out_cap: None,
            simulations: input.simulations,
            seed: input.seed,
        })
        .unwrap();
        for point in family.points {
            assert!(point.goal_probability <= found.outcome.goal_probability);
        }
    }

    #[test]
    fn the_best_plans_of_the_same_sweep_give_the_same_search() {
        let input = input(Some(540.0), Some(0.05));
        let swept = tradeoff::tradeoff(&tradeoff::TradeoffInput {
            lifter: input.lifter,
            goal: input.goal,
            bomb_out_cap: input.bomb_out_cap,
            simulations: input.simulations,
            seed: input.seed,
        })
        .unwrap();
        let given = SearchInput {
            family_best: Some(swept.points[swept.best_for_search.unwrap()].plans),
            ..input
        };
        assert_eq!(search(&given).unwrap(), search(&input).unwrap());
    }

    #[test]
    fn given_family_plans_are_used_instead_of_sweeping() {
        let input = SearchInput {
            start: None,
            ..input(Some(540.0), Some(0.05))
        };
        let given = SearchInput {
            family_best: Some(timid_plans()),
            ..input
        };
        assert_eq!(Search::new(&given).unwrap().best().plans, timid_plans());
        assert_ne!(Search::new(&input).unwrap().best().plans, timid_plans());
    }

    #[test]
    fn invalid_family_plans_are_swept_again() {
        let input = input(Some(540.0), Some(0.05));
        let mut plans = timid_plans();
        plans.squat.opener = kg(10.0);
        let given = SearchInput {
            family_best: Some(plans),
            ..input
        };
        assert_eq!(
            Search::new(&given).unwrap().best(),
            Search::new(&input).unwrap().best()
        );
    }

    #[test]
    fn keeps_the_bomb_out_chance_under_the_cap() {
        let cap = 0.005;
        let found = search(&input(Some(560.0), Some(cap))).unwrap();
        assert!(found.outcome.bomb_out_probability <= cap);
        let uncapped = search(&input(Some(560.0), None)).unwrap();
        assert!(uncapped.outcome.bomb_out_probability > cap);
    }

    #[test]
    fn without_a_goal_maximises_the_average_total() {
        let found = search(&input(None, None)).unwrap();
        let start = outcome_of(&input(None, None), timid_plans());
        assert!(found.outcome.expected_total_kg > start.expected_total_kg);
        assert_eq!(found.outcome.goal_probability, None);
    }

    #[test]
    fn found_plans_follow_the_rules() {
        let found = search(&input(Some(550.0), Some(0.02))).unwrap();
        for (_, plan) in found.plans.iter() {
            assert_eq!(plan.problems(), []);
        }
    }

    #[test]
    fn steps_until_done_and_reports_each_round() {
        let mut search = Search::new(&input(Some(550.0), None)).unwrap();
        let mut steps = 0;
        let mut rounds = Vec::new();
        while !search.is_done() {
            let progress = search.step();
            rounds.push(progress.round);
            steps += 1;
            assert!(steps < 10_000);
        }
        assert!(rounds.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(search.step().done);
    }

    #[test]
    fn an_invalid_start_falls_back_to_the_family() {
        let mut input = input(Some(550.0), None);
        input.start.as_mut().unwrap().squat.opener = kg(181.0);
        let found = search(&input).unwrap();
        assert_eq!(found.plans.squat.problems(), []);
    }

    #[test]
    fn the_same_seed_finds_the_same_plans() {
        let input = input(Some(550.0), None);
        assert_eq!(search(&input).unwrap(), search(&input).unwrap());
    }

    #[test]
    fn refuses_an_invalid_lifter_or_cap() {
        let mut input = input(None, Some(1.5));
        input.lifter.lifts.get_mut(Lift::Bench).expected_max = kg(10.0);
        assert_eq!(
            Search::new(&input).unwrap_err(),
            [
                InputProblem::ExpectedMaxBelowBar { lift: Lift::Bench },
                InputProblem::InvalidBombOutCap
            ]
        );
    }

    fn change_input(goal: Option<f64>, cap: Option<f64>) -> ChangeInput {
        let search = input(goal, cap);
        ChangeInput {
            lifter: search.lifter,
            plans: timid_plans(),
            goal: search.goal,
            bomb_out_cap: cap,
            simulations: search.simulations,
            seed: search.seed,
        }
    }

    #[test]
    fn each_change_moves_one_setting_by_one_or_two_plate_steps() {
        let result = single_changes(&change_input(Some(560.0), None)).unwrap();
        let mut moves = std::collections::BTreeSet::new();
        for change in &result.changes {
            let before = timid_plans();
            let old = change.setting.value(before.get(change.lift)).grams();
            let moved = old.abs_diff(change.weight.grams());
            assert!(moved == PLATE_STEP.grams() || moved == 2 * PLATE_STEP.grams());
            moves.insert(moved);
            let mut expected = before;
            *expected.get_mut(change.lift) =
                change.setting.with(*before.get(change.lift), change.weight);
            assert_eq!(change.found.plans, expected);
        }
        assert_eq!(moves.len(), 2);
    }

    #[test]
    fn leaves_out_changes_that_do_not_move_the_outcome() {
        let result = single_changes(&change_input(Some(560.0), None)).unwrap();
        assert!(
            result
                .changes
                .iter()
                .all(|change| change.found.outcome != result.current)
        );
        let mut certain = change_input(Some(560.0), None);
        certain.lifter = lifter(0.0, 1e6);
        let result = single_changes(&certain).unwrap();
        let after_miss = |change: &&Change| {
            matches!(
                change.setting,
                Setting::SecondAfterMiss | Setting::ThirdAfterMiss
            )
        };
        assert_eq!(result.changes.iter().filter(after_miss).count(), 0);
    }

    #[test]
    fn changes_are_scored_on_the_same_meets_and_ranked_best_first() {
        let changes = change_input(Some(560.0), Some(0.05));
        let result = single_changes(&changes).unwrap();
        let search = input(Some(560.0), Some(0.05));
        assert_eq!(result.current, outcome_of(&search, timid_plans()));
        for change in &result.changes {
            assert_eq!(
                change.found.outcome,
                outcome_of(&search, change.found.plans)
            );
        }
        let ranks: Vec<_> = result
            .changes
            .iter()
            .map(|change| rank(changes.bomb_out_cap, &change.found.outcome))
            .collect();
        assert!(ranks.windows(2).all(|pair| pair[0] >= pair[1]));
    }

    #[test]
    fn a_change_improves_when_it_ranks_above_the_plans_as_given() {
        let changes = change_input(Some(560.0), Some(0.05));
        let result = single_changes(&changes).unwrap();
        let current = rank(changes.bomb_out_cap, &result.current);
        for change in &result.changes {
            assert_eq!(
                change.improves,
                rank(changes.bomb_out_cap, &change.found.outcome) > current
            );
        }
        assert!(result.changes.iter().any(|change| change.improves));
        assert!(result.changes.iter().any(|change| !change.improves));
    }

    #[test]
    fn changes_report_every_problem_with_the_input() {
        let mut input = change_input(None, Some(2.0));
        input.plans.bench.opener = kg(101.0);
        input.simulations = 0;
        assert_eq!(
            single_changes(&input).unwrap_err().len(),
            3,
            "simulations, the bench opener and the cap"
        );
    }
}
