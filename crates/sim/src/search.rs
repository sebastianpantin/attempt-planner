use crate::evaluate::{InputProblem, Lifter, MeetDays};
use crate::lift::{Lift, PerLift};
pub use crate::outcome::Outcome;
use crate::outcome::OutcomeTally;
use crate::plan::{Jumps, LiftPlan};
use crate::rules::{ATTEMPTS_PER_LIFT, BAR, PLATE_STEP, Weight, best_attempt};
use crate::tradeoff::{self, TradeoffInput};

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
}

/// The best plans found so far, with their outcome on the search's simulated meets.
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
#[derive(Debug, Clone, Copy)]
enum Setting {
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

    fn with(self, plan: LiftPlan, weight: Weight) -> LiftPlan {
        let mut plan = plan;
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
    make_thresholds_kg: Vec<PerLift<[f64; ATTEMPTS_PER_LIFT]>>,
    lifter: Lifter,
    goal: Option<Weight>,
    bomb_out_cap: Option<f64>,
    best: Found,
    next: usize,
    unchanged: usize,
    round: u32,
}

impl Search {
    pub fn new(input: &SearchInput) -> Result<Self, Vec<InputProblem>> {
        let cap_is_valid = input
            .bomb_out_cap
            .is_none_or(|cap| (0.0..=1.0).contains(&cap));
        let days = MeetDays::new(input.lifter, input.simulations, input.seed);
        let days = match (days, cap_is_valid) {
            (Ok(days), true) => days,
            (days, _) => {
                let mut problems = days.err().unwrap_or_default();
                if !cap_is_valid {
                    problems.push(InputProblem::InvalidBombOutCap);
                }
                return Err(problems);
            }
        };
        let family = tradeoff::tradeoff(&TradeoffInput {
            lifter: input.lifter,
            goal: input.goal,
            simulations: input.simulations,
            seed: input.seed,
        })?;

        let make_thresholds_kg: Vec<_> = days.iter().map(|day| day.make_thresholds_kg).collect();
        let higher = |a: Found, b: Found| {
            if rank(input.bomb_out_cap, &b.outcome) > rank(input.bomb_out_cap, &a.outcome) {
                b
            } else {
                a
            }
        };
        let found = |plans| Found {
            plans,
            outcome: score(&make_thresholds_kg, &plans, input.goal),
        };
        let family = family.points.iter().map(|point| Found {
            plans: point.plans,
            outcome: Outcome {
                bomb_out_probability: point.bomb_out_probability,
                expected_total_kg: point.expected_total_kg,
                goal_probability: point.goal_probability,
            },
        });
        let start = input
            .start
            .filter(|plans| plans.iter().all(|(_, plan)| plan.problems().is_empty()))
            .map(found);
        let best = family
            .chain(start)
            .reduce(higher)
            .unwrap_or_else(|| found(PerLift::from_fn(|_| fallback_plan())));
        Ok(Self {
            make_thresholds_kg,
            lifter: input.lifter,
            goal: input.goal,
            bomb_out_cap: input.bomb_out_cap,
            best,
            next: 0,
            unchanged: 0,
            round: 1,
        })
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
            let others = self.other_lifts_totals(lift);
            let current = *self.best.plans.get(lift);
            let expected_max = self.lifter.lifts.get(lift).expected_max;
            let mut improved = false;
            for weight in setting.values(expected_max) {
                let candidate = setting.with(current, weight);
                if candidate == current || !candidate.problems().is_empty() {
                    continue;
                }
                let outcome = self.score_lift(lift, &candidate, &others);
                if self.ranks_higher(&outcome, &self.best.outcome) {
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

    fn ranks_higher(&self, a: &Outcome, b: &Outcome) -> bool {
        rank(self.bomb_out_cap, a) > rank(self.bomb_out_cap, b)
    }

    /// Each day's total over the lifts other than `lift`, `None` when one of them bombs out.
    fn other_lifts_totals(&self, lift: Lift) -> Vec<Option<Weight>> {
        self.make_thresholds_kg
            .iter()
            .map(|thresholds| {
                self.best
                    .plans
                    .iter()
                    .filter(|(other, _)| *other != lift)
                    .map(|(other, plan)| best_attempt(&plan.attempts(*thresholds.get(other))))
                    .sum()
            })
            .collect()
    }

    fn score_lift(&self, lift: Lift, plan: &LiftPlan, others: &[Option<Weight>]) -> Outcome {
        let mut tally = OutcomeTally::default();
        for (thresholds, other) in self.make_thresholds_kg.iter().zip(others) {
            let best = best_attempt(&plan.attempts(*thresholds.get(lift)));
            tally.record(other.zip(best).map(|(other, best)| other + best), self.goal);
        }
        tally.outcome(self.goal)
    }
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

fn score(
    make_thresholds_kg: &[PerLift<[f64; ATTEMPTS_PER_LIFT]>],
    plans: &PerLift<LiftPlan>,
    goal: Option<Weight>,
) -> Outcome {
    let mut tally = OutcomeTally::default();
    for thresholds in make_thresholds_kg {
        let total = plans
            .iter()
            .map(|(lift, plan)| best_attempt(&plan.attempts(*thresholds.get(lift))))
            .sum();
        tally.record(total, goal);
    }
    tally.outcome(goal)
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

/// Runs a search to the end and returns the best plans found.
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
    use crate::evaluate::{EvaluateInput, InputProblem, LiftModel, Lifter, evaluate};
    use crate::lift::{Lift, PerLift};
    use crate::model::{DayForm, MakeRateCurve};
    use crate::plan::{Jumps, LiftPlan};
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
                },
            }),
            day_form: DayForm {
                spread,
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
        let family = crate::tradeoff::tradeoff(&crate::tradeoff::TradeoffInput {
            lifter: input.lifter,
            goal: input.goal,
            simulations: input.simulations,
            seed: input.seed,
        })
        .unwrap();
        for point in family.points {
            assert!(point.goal_probability <= found.outcome.goal_probability);
        }
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
}
