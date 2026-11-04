use std::fmt;

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rand_distr::{Distribution, StandardNormal};

use crate::lift::{Lift, PerLift};
use crate::model::{DayForm, MakeRateCurve};
use crate::plan::{LiftPlan, PlanStep};
use crate::rules::{ATTEMPTS_PER_LIFT, BAR, RuleViolation, Weight, best_attempt};

pub const MAX_SIMULATIONS: u32 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiftModel {
    /// The max the lifter expects to hit on a normal day at this meet.
    pub expected_max: Weight,
    pub curve: MakeRateCurve,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lifter {
    pub lifts: PerLift<LiftModel>,
    pub day_form: DayForm,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvaluateInput {
    pub lifter: Lifter,
    pub plans: PerLift<LiftPlan>,
    pub goal: Option<Weight>,
    pub simulations: u32,
    pub seed: u64,
}

/// The input a problem belongs to, so a UI can mark it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum InputField {
    ExpectedMax,
    Curve,
    Opener,
    Second,
    Third,
    DayForm,
    Simulations,
    Goal,
}

impl From<PlanStep> for InputField {
    fn from(step: PlanStep) -> Self {
        match step {
            PlanStep::Opener => Self::Opener,
            PlanStep::Second => Self::Second,
            PlanStep::Third => Self::Third,
        }
    }
}

/// A stable name for each kind of problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum ProblemCode {
    InvalidWeight,
    BelowBar,
    TooHeavy,
    NotLoadable,
    Lighter,
    NoIncreaseAfterMake,
    TooManyAttempts,
    ExpectedMaxBelowBar,
    InvalidCurve,
    InvalidDayForm,
    SimulationsOutOfRange,
}

impl From<RuleViolation> for ProblemCode {
    fn from(violation: RuleViolation) -> Self {
        match violation {
            RuleViolation::TooManyAttempts => Self::TooManyAttempts,
            RuleViolation::BelowBar { .. } => Self::BelowBar,
            RuleViolation::TooHeavy { .. } => Self::TooHeavy,
            RuleViolation::NotLoadable { .. } => Self::NotLoadable,
            RuleViolation::Lighter { .. } => Self::Lighter,
            RuleViolation::NoIncreaseAfterMake { .. } => Self::NoIncreaseAfterMake,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputProblem {
    /// A weight given in kg that is negative or not a number.
    InvalidWeight {
        lift: Option<Lift>,
        field: InputField,
    },
    Plan {
        lift: Lift,
        step: PlanStep,
        violation: RuleViolation,
    },
    ExpectedMaxBelowBar {
        lift: Lift,
    },
    InvalidCurve {
        lift: Lift,
    },
    InvalidDayForm,
    SimulationsOutOfRange,
}

impl InputProblem {
    pub fn lift(&self) -> Option<Lift> {
        match *self {
            Self::InvalidWeight { lift, .. } => lift,
            Self::Plan { lift, .. }
            | Self::ExpectedMaxBelowBar { lift }
            | Self::InvalidCurve { lift } => Some(lift),
            Self::InvalidDayForm | Self::SimulationsOutOfRange => None,
        }
    }

    pub fn field(&self) -> InputField {
        match *self {
            Self::InvalidWeight { field, .. } => field,
            Self::Plan { step, .. } => step.into(),
            Self::ExpectedMaxBelowBar { .. } => InputField::ExpectedMax,
            Self::InvalidCurve { .. } => InputField::Curve,
            Self::InvalidDayForm => InputField::DayForm,
            Self::SimulationsOutOfRange => InputField::Simulations,
        }
    }

    pub fn code(&self) -> ProblemCode {
        match *self {
            Self::InvalidWeight { .. } => ProblemCode::InvalidWeight,
            Self::Plan { violation, .. } => violation.into(),
            Self::ExpectedMaxBelowBar { .. } => ProblemCode::ExpectedMaxBelowBar,
            Self::InvalidCurve { .. } => ProblemCode::InvalidCurve,
            Self::InvalidDayForm => ProblemCode::InvalidDayForm,
            Self::SimulationsOutOfRange => ProblemCode::SimulationsOutOfRange,
        }
    }
}

impl fmt::Display for InputProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWeight { .. } => f.write_str("not a valid weight in kg"),
            Self::Plan { violation, .. } => violation.fmt(f),
            Self::ExpectedMaxBelowBar { .. } => {
                write!(f, "expected max is lighter than the {BAR} bar")
            }
            Self::InvalidCurve { .. } => {
                f.write_str("make-rate curve needs a positive, finite midpoint and steepness")
            }
            Self::InvalidDayForm => write!(
                f,
                "day-form spread must be between 0 and {} and correlation between 0 and 1",
                DayForm::MAX_SPREAD
            ),
            Self::SimulationsOutOfRange => {
                write!(f, "simulations must be between 1 and {MAX_SIMULATIONS}")
            }
        }
    }
}

impl std::error::Error for InputProblem {}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct LiftSummary {
    /// Chance of at least one made attempt.
    pub make_probability: f64,
    /// Average best attempt over the meets where at least one attempt was made.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(feature = "tsify", tsify(optional))]
    pub expected_best_kg: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct Percentiles {
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
}

/// Monte Carlo standard errors of the matching estimates in [`Evaluation`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct StandardErrors {
    pub bomb_out_probability: f64,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(feature = "tsify", tsify(optional))]
    pub goal_probability: Option<f64>,
    pub expected_total_kg: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct Evaluation {
    pub bomb_out_probability: f64,
    /// Chance of a total at or above the goal. Absent when no goal was given.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(feature = "tsify", tsify(optional))]
    pub goal_probability: Option<f64>,
    /// Average total, counting a bomb-out as 0.
    pub expected_total_kg: f64,
    /// Average total over the meets without a bomb-out.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(feature = "tsify", tsify(optional))]
    pub expected_total_without_bomb_out_kg: Option<f64>,
    /// Percentiles of the total, counting a bomb-out as 0.
    pub total_percentiles_kg: Percentiles,
    pub standard_errors: StandardErrors,
    pub lifts: PerLift<LiftSummary>,
}

/// One simulated meet day: each lift's day max and the rolls that decide its attempts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeetDay {
    pub day_max_kg: PerLift<f64>,
    pub rolls: PerLift<[f64; ATTEMPTS_PER_LIFT]>,
}

/// A seeded set of simulated meet days for one lifter. Every plan evaluated against the
/// same `MeetDays` sees exactly the same days, so differences between plans are not noise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeetDays {
    lifter: Lifter,
    simulations: u32,
    seed: u64,
}

impl MeetDays {
    pub fn new(lifter: Lifter, simulations: u32, seed: u64) -> Result<Self, Vec<InputProblem>> {
        let problems = setup_problems(&lifter, simulations);
        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(Self {
            lifter,
            simulations,
            seed,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = MeetDay> + '_ {
        let mut rng = ChaCha8Rng::seed_from_u64(self.seed);
        let expected_kg = self.lifter.lifts.map(|_, lift| lift.expected_max.kg());
        (0..self.simulations).map(move |_| {
            let shared: f64 = StandardNormal.sample(&mut rng);
            let own = PerLift::from_fn(|_| StandardNormal.sample(&mut rng));
            MeetDay {
                day_max_kg: self.lifter.day_form.day_maxes(&expected_kg, shared, &own),
                rolls: PerLift::from_fn(|_| rng.random()),
            }
        })
    }

    pub fn evaluate(
        &self,
        plans: &PerLift<LiftPlan>,
        goal: Option<Weight>,
    ) -> Result<Evaluation, Vec<InputProblem>> {
        let problems = plan_problems(plans);
        if !problems.is_empty() {
            return Err(problems);
        }
        let mut tally = Tally::new(self.simulations);
        for day in self.iter() {
            tally.record(plans.map(|lift, plan| {
                let curve = &self.lifter.lifts.get(lift).curve;
                best_attempt(&plan.attempts(curve, *day.day_max_kg.get(lift), *day.rolls.get(lift)))
            }));
        }
        Ok(tally.finish(goal))
    }
}

/// Simulates the meet `input.simulations` times and summarises the outcomes.
pub fn evaluate(input: &EvaluateInput) -> Result<Evaluation, Vec<InputProblem>> {
    let mut problems = setup_problems(&input.lifter, input.simulations);
    problems.extend(plan_problems(&input.plans));
    if !problems.is_empty() {
        return Err(problems);
    }
    MeetDays::new(input.lifter, input.simulations, input.seed)?.evaluate(&input.plans, input.goal)
}

fn setup_problems(lifter: &Lifter, simulations: u32) -> Vec<InputProblem> {
    let mut problems = Vec::new();
    for (lift, model) in lifter.lifts.iter() {
        if model.expected_max < BAR {
            problems.push(InputProblem::ExpectedMaxBelowBar { lift });
        }
        if !model.curve.is_valid() {
            problems.push(InputProblem::InvalidCurve { lift });
        }
    }
    if !lifter.day_form.is_valid() {
        problems.push(InputProblem::InvalidDayForm);
    }
    if !(1..=MAX_SIMULATIONS).contains(&simulations) {
        problems.push(InputProblem::SimulationsOutOfRange);
    }
    problems
}

fn plan_problems(plans: &PerLift<LiftPlan>) -> Vec<InputProblem> {
    plans
        .iter()
        .flat_map(|(lift, plan)| {
            plan.problems()
                .into_iter()
                .map(move |(step, violation)| InputProblem::Plan {
                    lift,
                    step,
                    violation,
                })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Default)]
struct LiftTally {
    makes: u32,
    best_grams: u64,
}

/// Running counts over simulated meets, with a bomb-out recorded as a zero total.
struct Tally {
    totals: Vec<Weight>,
    lifts: PerLift<LiftTally>,
}

impl Tally {
    fn new(simulations: u32) -> Self {
        Self {
            totals: Vec::with_capacity(simulations as usize),
            lifts: PerLift::from_fn(|_| LiftTally::default()),
        }
    }

    fn record(&mut self, bests: PerLift<Option<Weight>>) {
        for (lift, best) in bests.iter() {
            if let Some(best) = best {
                let tally = self.lifts.get_mut(lift);
                tally.makes += 1;
                tally.best_grams += u64::from(best.grams());
            }
        }
        let total = bests.iter().map(|(_, best)| *best).sum::<Option<Weight>>();
        self.totals.push(total.unwrap_or(Weight::ZERO));
    }

    fn finish(mut self, goal: Option<Weight>) -> Evaluation {
        self.totals.sort_unstable();
        let totals = &self.totals;
        let runs = totals.len() as f64;
        let bomb_outs = totals.partition_point(|&t| t == Weight::ZERO);
        let completed = &totals[bomb_outs..];
        let total_kg: f64 = totals.iter().map(|t| t.kg()).sum();
        let total_kg_squared: f64 = totals.iter().map(|t| t.kg() * t.kg()).sum();
        let expected_total_kg = total_kg / runs;
        let variance = (total_kg_squared / runs - expected_total_kg * expected_total_kg).max(0.0);

        let bomb_out_probability = bomb_outs as f64 / runs;
        let goal_probability = goal.map(|goal| {
            let met = completed.len() - completed.partition_point(|&t| t < goal);
            met as f64 / runs
        });
        let proportion_error = |p: f64| (p * (1.0 - p) / runs).sqrt();
        let percentile = |quantile: f64| {
            let rank = (quantile * runs).ceil() as usize;
            totals[rank.clamp(1, totals.len()) - 1].kg()
        };

        Evaluation {
            bomb_out_probability,
            goal_probability,
            expected_total_kg,
            expected_total_without_bomb_out_kg: (!completed.is_empty())
                .then(|| total_kg / completed.len() as f64),
            total_percentiles_kg: Percentiles {
                p10: percentile(0.1),
                p50: percentile(0.5),
                p90: percentile(0.9),
            },
            standard_errors: StandardErrors {
                bomb_out_probability: proportion_error(bomb_out_probability),
                goal_probability: goal_probability.map(proportion_error),
                expected_total_kg: (variance / runs).sqrt(),
            },
            lifts: self.lifts.map(|_, tally| LiftSummary {
                make_probability: f64::from(tally.makes) / runs,
                expected_best_kg: (tally.makes > 0)
                    .then(|| tally.best_grams as f64 / 1000.0 / f64::from(tally.makes)),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Jumps;
    use crate::rules::{PLATE_STEP, check_progression};
    use crate::testing::kg;
    use proptest::prelude::*;

    fn curve(steepness: f64) -> MakeRateCurve {
        MakeRateCurve {
            ratio_at_half: 1.0,
            steepness,
        }
    }

    const SHARP: f64 = 1_000.0;

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

    fn certain_meet() -> EvaluateInput {
        let maxes = PerLift {
            squat: 200.0,
            bench: 130.0,
            deadlift: 240.0,
        };
        EvaluateInput {
            lifter: Lifter {
                lifts: maxes.map(|_, &max| LiftModel {
                    expected_max: kg(max),
                    curve: curve(SHARP),
                }),
                day_form: DayForm {
                    spread: 0.0,
                    correlation: 0.5,
                },
            },
            plans: PerLift {
                squat: plan(180.0, 10.0, 15.0),
                bench: plan(117.5, 7.5, 7.5),
                deadlift: plan(215.0, 15.0, 5.0),
            },
            goal: None,
            simulations: 2_000,
            seed: 7,
        }
    }

    fn uncertain_meet(steepness: f64, spread: f64) -> EvaluateInput {
        let mut meet = certain_meet();
        meet.lifter.day_form.spread = spread;
        for lift in Lift::ALL {
            meet.lifter.lifts.get_mut(lift).curve = curve(steepness);
        }
        meet
    }

    #[test]
    fn certain_outcome_without_day_form_spread() {
        let result = evaluate(&certain_meet()).unwrap();
        assert_eq!(result.bomb_out_probability, 0.0);
        assert_eq!(result.lifts.squat.expected_best_kg, Some(190.0));
        assert_eq!(result.lifts.bench.expected_best_kg, Some(125.0));
        assert_eq!(result.lifts.deadlift.expected_best_kg, Some(235.0));
        assert_eq!(result.expected_total_kg, 550.0);
        assert_eq!(result.expected_total_without_bomb_out_kg, Some(550.0));
        let p = result.total_percentiles_kg;
        assert_eq!((p.p10, p.p50, p.p90), (550.0, 550.0, 550.0));
        assert_eq!(result.standard_errors.expected_total_kg, 0.0);
        assert_eq!(result.standard_errors.bomb_out_probability, 0.0);
    }

    #[test]
    fn goal_probability_compares_the_total() {
        let mut meet = certain_meet();
        assert_eq!(evaluate(&meet).unwrap().goal_probability, None);
        meet.goal = Some(kg(550.0));
        assert_eq!(evaluate(&meet).unwrap().goal_probability, Some(1.0));
        meet.goal = Some(kg(552.5));
        assert_eq!(evaluate(&meet).unwrap().goal_probability, Some(0.0));
    }

    #[test]
    fn opening_above_the_max_bombs_out() {
        let mut meet = certain_meet();
        meet.plans.bench = plan(140.0, 5.0, 5.0);
        meet.goal = Some(kg(100.0));
        let result = evaluate(&meet).unwrap();
        assert_eq!(result.bomb_out_probability, 1.0);
        assert_eq!(result.goal_probability, Some(0.0));
        assert_eq!(result.expected_total_kg, 0.0);
        assert_eq!(result.expected_total_without_bomb_out_kg, None);
        assert_eq!(result.lifts.bench.make_probability, 0.0);
        assert_eq!(result.lifts.bench.expected_best_kg, None);
        assert_eq!(result.lifts.squat.make_probability, 1.0);
    }

    #[test]
    fn repeated_attempts_match_the_closed_form() {
        let mut meet = certain_meet();
        meet.lifter.lifts.squat.curve = curve(20.0);
        meet.plans.squat = plan(200.0, 2.5, 2.5);
        meet.simulations = 200_000;
        let expected = 1.0 - 0.5_f64.powi(3);
        let simulated = evaluate(&meet).unwrap().lifts.squat.make_probability;
        assert!(
            (simulated - expected).abs() < 0.005,
            "{simulated} vs {expected}"
        );
    }

    #[test]
    fn standard_errors_follow_the_binomial_formula() {
        let mut meet = uncertain_meet(25.0, 0.04);
        meet.goal = Some(kg(550.0));
        let result = evaluate(&meet).unwrap();
        let p = result.goal_probability.unwrap();
        let expected = (p * (1.0 - p) / f64::from(meet.simulations)).sqrt();
        assert!(p > 0.0 && p < 1.0);
        assert_eq!(result.standard_errors.goal_probability, Some(expected));
        assert!(result.standard_errors.expected_total_kg > 0.0);
    }

    #[test]
    fn more_simulations_shrink_the_standard_error() {
        let mut meet = uncertain_meet(25.0, 0.04);
        let few = evaluate(&meet).unwrap().standard_errors.expected_total_kg;
        meet.simulations *= 16;
        let many = evaluate(&meet).unwrap().standard_errors.expected_total_kg;
        assert!(many < few / 2.0, "{many} vs {few}");
    }

    #[test]
    fn same_seed_same_result_and_new_seed_new_result() {
        let mut meet = uncertain_meet(25.0, 0.04);
        let first = evaluate(&meet).unwrap();
        assert_eq!(evaluate(&meet).unwrap(), first);
        meet.seed = 8;
        assert_ne!(evaluate(&meet).unwrap(), first);
    }

    #[test]
    fn every_plan_sees_the_same_meet_days() {
        let meet = uncertain_meet(25.0, 0.04);
        let days = MeetDays::new(meet.lifter, meet.simulations, meet.seed).unwrap();
        let mut bolder = meet.plans;
        bolder.squat = plan(190.0, 7.5, 5.0);
        let first = days.evaluate(&meet.plans, None).unwrap();
        let second = days.evaluate(&bolder, None).unwrap();
        assert_ne!(first.lifts.squat, second.lifts.squat);
        assert_eq!(first.lifts.bench, second.lifts.bench);
        assert_eq!(first.lifts.deadlift, second.lifts.deadlift);
        assert_eq!(days.evaluate(&meet.plans, None), evaluate(&meet));
    }

    #[test]
    fn correlated_lifts_spread_the_total_wider() {
        let mut meet = uncertain_meet(30.0, 0.05);
        meet.simulations = 20_000;
        let mut width = |correlation| {
            meet.lifter.day_form.correlation = correlation;
            let p = evaluate(&meet).unwrap().total_percentiles_kg;
            p.p90 - p.p10
        };
        assert!(width(1.0) > width(0.0));
    }

    #[test]
    fn invalid_input_lists_every_problem() {
        let mut meet = certain_meet();
        meet.plans.squat.opener = kg(181.0);
        meet.lifter.lifts.bench.expected_max = kg(15.0);
        meet.lifter.lifts.deadlift.curve.steepness = 0.0;
        meet.lifter.day_form.correlation = 2.0;
        meet.simulations = 0;
        assert_eq!(
            evaluate(&meet).unwrap_err(),
            [
                InputProblem::ExpectedMaxBelowBar { lift: Lift::Bench },
                InputProblem::InvalidCurve {
                    lift: Lift::Deadlift
                },
                InputProblem::InvalidDayForm,
                InputProblem::SimulationsOutOfRange,
                InputProblem::Plan {
                    lift: Lift::Squat,
                    step: PlanStep::Opener,
                    violation: RuleViolation::NotLoadable { attempt: 1 }
                },
            ]
        );
    }

    #[test]
    fn problems_name_their_lift_field_and_code() {
        let problems = [
            InputProblem::InvalidWeight {
                lift: None,
                field: InputField::Goal,
            },
            InputProblem::Plan {
                lift: Lift::Bench,
                step: PlanStep::Third,
                violation: RuleViolation::NoIncreaseAfterMake { attempt: 3 },
            },
            InputProblem::ExpectedMaxBelowBar {
                lift: Lift::Deadlift,
            },
            InputProblem::InvalidCurve { lift: Lift::Squat },
            InputProblem::InvalidDayForm,
            InputProblem::SimulationsOutOfRange,
        ];
        let described = problems.map(|p| (p.lift(), p.field(), p.code()));
        assert_eq!(
            described,
            [
                (None, InputField::Goal, ProblemCode::InvalidWeight),
                (
                    Some(Lift::Bench),
                    InputField::Third,
                    ProblemCode::NoIncreaseAfterMake
                ),
                (
                    Some(Lift::Deadlift),
                    InputField::ExpectedMax,
                    ProblemCode::ExpectedMaxBelowBar
                ),
                (
                    Some(Lift::Squat),
                    InputField::Curve,
                    ProblemCode::InvalidCurve
                ),
                (None, InputField::DayForm, ProblemCode::InvalidDayForm),
                (
                    None,
                    InputField::Simulations,
                    ProblemCode::SimulationsOutOfRange
                ),
            ]
        );
    }

    #[test]
    fn problems_read_as_sentences() {
        let messages = [
            InputProblem::InvalidWeight {
                lift: Some(Lift::Squat),
                field: InputField::Opener,
            },
            InputProblem::Plan {
                lift: Lift::Squat,
                step: PlanStep::Opener,
                violation: RuleViolation::BelowBar { attempt: 1 },
            },
            InputProblem::ExpectedMaxBelowBar { lift: Lift::Bench },
            InputProblem::InvalidCurve { lift: Lift::Bench },
            InputProblem::InvalidDayForm,
            InputProblem::SimulationsOutOfRange,
        ]
        .map(|problem| problem.to_string());
        assert_eq!(
            messages,
            [
                "not a valid weight in kg",
                "attempt 1 is lighter than the 20 kg bar",
                "expected max is lighter than the 20 kg bar",
                "make-rate curve needs a positive, finite midpoint and steepness",
                "day-form spread must be between 0 and 0.5 and correlation between 0 and 1",
                "simulations must be between 1 and 1000000",
            ]
        );
    }

    #[test]
    fn too_many_simulations_are_refused() {
        let mut meet = certain_meet();
        meet.simulations = MAX_SIMULATIONS + 1;
        assert_eq!(
            evaluate(&meet).unwrap_err(),
            [InputProblem::SimulationsOutOfRange]
        );
    }

    fn steps(range: std::ops::RangeInclusive<u32>) -> impl Strategy<Value = Weight> {
        range.prop_map(|n| Weight::from_grams(n * PLATE_STEP.grams()))
    }

    fn valid_plan() -> impl Strategy<Value = LiftPlan> {
        (
            steps(8..=120),
            steps(1..=10),
            steps(0..=4),
            steps(1..=10),
            steps(0..=4),
        )
            .prop_map(|(opener, make2, miss2, make3, miss3)| LiftPlan {
                opener,
                second: Jumps {
                    after_make: make2,
                    after_miss: miss2,
                },
                third: Jumps {
                    after_make: make3,
                    after_miss: miss3,
                },
            })
    }

    proptest! {
        #[test]
        fn simulated_attempts_follow_the_rules(
            plan in valid_plan(),
            day_max_kg in 20.0..400.0f64,
            rolls in prop::array::uniform3(0.0..1.0f64),
        ) {
            prop_assert!(plan.problems().is_empty());
            let attempts = plan.attempts(&curve(SHARP), day_max_kg, rolls);
            prop_assert_eq!(check_progression(&attempts), Ok(()));
        }

        #[test]
        fn probabilities_stay_between_zero_and_one(
            squat in valid_plan(),
            bench in valid_plan(),
            deadlift in valid_plan(),
            spread in 0.0..0.2f64,
            correlation in 0.0..=1.0f64,
            seed in any::<u64>(),
        ) {
            let mut meet = uncertain_meet(20.0, spread);
            meet.lifter.day_form.correlation = correlation;
            meet.plans = PerLift { squat, bench, deadlift };
            meet.goal = Some(kg(500.0));
            meet.simulations = 200;
            meet.seed = seed;
            let result = evaluate(&meet).unwrap();
            for p in [result.bomb_out_probability, result.goal_probability.unwrap()] {
                prop_assert!((0.0..=1.0).contains(&p));
            }
            let p = result.total_percentiles_kg;
            prop_assert!(p.p10 <= p.p50 && p.p50 <= p.p90);
        }
    }
}
