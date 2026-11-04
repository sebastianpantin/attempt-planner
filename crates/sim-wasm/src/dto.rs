use attempt_sim::evaluate::{
    self, EvaluateInput, Evaluation, InputField, InputProblem, LiftModel, Lifter, ProblemCode,
};
use attempt_sim::lift::{Lift, PerLift};
use attempt_sim::model::{DayForm, MakeRateCurve};
use attempt_sim::plan::{Jumps, LiftPlan};
use attempt_sim::rules::Weight;
use attempt_sim::search::{self, Search, SearchInput};
use attempt_sim::tradeoff::{self, PlanShape, TradeoffInput};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct LiftModelInput {
    pub expected_max_kg: f64,
    pub curve: MakeRateCurve,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct LifterInput {
    pub lifts: PerLift<LiftModelInput>,
    pub day_form: DayForm,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct JumpsKg {
    pub after_make_kg: f64,
    pub after_miss_kg: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct LiftPlanKg {
    pub opener_kg: f64,
    pub second: JumpsKg,
    pub third: JumpsKg,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateRequest {
    pub lifter: LifterInput,
    pub plans: PerLift<LiftPlanKg>,
    #[serde(default)]
    #[tsify(optional)]
    pub goal_kg: Option<f64>,
    pub simulations: u32,
    /// 32 bits rather than the engine's 64, because a JavaScript number cannot hold every
    /// 64-bit integer exactly.
    pub seed: u32,
}

impl From<Jumps> for JumpsKg {
    fn from(jumps: Jumps) -> Self {
        Self {
            after_make_kg: jumps.after_make.kg(),
            after_miss_kg: jumps.after_miss.kg(),
        }
    }
}

impl From<LiftPlan> for LiftPlanKg {
    fn from(plan: LiftPlan) -> Self {
        Self {
            opener_kg: plan.opener.kg(),
            second: plan.second.into(),
            third: plan.third.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct TradeoffRequest {
    pub lifter: LifterInput,
    #[serde(default)]
    #[tsify(optional)]
    pub goal_kg: Option<f64>,
    pub simulations: u32,
    pub seed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct TradeoffPoint {
    pub shape: PlanShape,
    pub plans: PerLift<LiftPlanKg>,
    pub bomb_out_probability: f64,
    pub expected_total_kg: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub goal_probability: Option<f64>,
    /// No other plan in the family has a lower or equal bomb-out chance and a higher
    /// expected total.
    pub on_frontier: bool,
}

impl From<tradeoff::TradeoffPoint> for TradeoffPoint {
    fn from(point: tradeoff::TradeoffPoint) -> Self {
        Self {
            shape: point.shape,
            plans: point.plans.map(|_, &plan| plan.into()),
            bomb_out_probability: point.bomb_out_probability,
            expected_total_kg: point.expected_total_kg,
            goal_probability: point.goal_probability,
            on_frontier: point.on_frontier,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub lift: Option<Lift>,
    pub field: InputField,
    pub code: ProblemCode,
    pub message: String,
}

impl From<InputProblem> for Problem {
    fn from(problem: InputProblem) -> Self {
        Self {
            lift: problem.lift(),
            field: problem.field(),
            code: problem.code(),
            message: problem.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Tsify)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EvaluateResponse {
    Evaluated { evaluation: Evaluation },
    Invalid { problems: Vec<Problem> },
}

/// Converts the request, runs the simulation and returns the evaluation or every problem.
pub fn evaluate(request: &EvaluateRequest) -> EvaluateResponse {
    match to_input(request).and_then(|input| evaluate::evaluate(&input)) {
        Ok(evaluation) => EvaluateResponse::Evaluated { evaluation },
        Err(problems) => EvaluateResponse::Invalid {
            problems: problems.into_iter().map(Problem::from).collect(),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Tsify)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TradeoffResponse {
    Swept {
        points: Vec<TradeoffPoint>,
        /// Index into `points` of the plan with the best chance of reaching the goal.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[tsify(optional)]
        best_for_goal: Option<usize>,
    },
    Invalid {
        problems: Vec<Problem>,
    },
}

/// Converts the request, sweeps the plan family and returns its points or every problem.
pub fn tradeoff(request: &TradeoffRequest) -> TradeoffResponse {
    match to_tradeoff_input(request).and_then(|input| tradeoff::tradeoff(&input)) {
        Ok(result) => TradeoffResponse::Swept {
            points: result.points.into_iter().map(TradeoffPoint::from).collect(),
            best_for_goal: result.best_for_goal,
        },
        Err(problems) => TradeoffResponse::Invalid {
            problems: problems.into_iter().map(Problem::from).collect(),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub lifter: LifterInput,
    /// Plans to start from as well as the tradeoff family. Ignored when invalid.
    pub plans: PerLift<LiftPlanKg>,
    #[serde(default)]
    #[tsify(optional)]
    pub goal_kg: Option<f64>,
    /// Highest acceptable chance of bombing out, from 0 to 1.
    #[serde(default)]
    #[tsify(optional)]
    pub bomb_out_cap: Option<f64>,
    pub simulations: u32,
    pub seed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct SearchProgress {
    /// The round in progress, from 1.
    pub round: u32,
    pub done: bool,
    /// The best plans so far and how they did on the search's simulated meets.
    pub plans: PerLift<LiftPlanKg>,
    pub bomb_out_probability: f64,
    pub expected_total_kg: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub goal_probability: Option<f64>,
}

impl From<search::SearchProgress> for SearchProgress {
    fn from(progress: search::SearchProgress) -> Self {
        let best = progress.best;
        Self {
            round: progress.round,
            done: progress.done,
            plans: best.plans.map(|_, &plan| plan.into()),
            bomb_out_probability: best.outcome.bomb_out_probability,
            expected_total_kg: best.outcome.expected_total_kg,
            goal_probability: best.outcome.goal_probability,
        }
    }
}

/// Converts the request and sets up a search, or returns every problem with the input.
pub fn start_search(request: &SearchRequest) -> Result<Search, Vec<Problem>> {
    let mut problems = Vec::new();
    let lifter = read_lifter(&mut problems, &request.lifter);
    let goal = read_goal(&mut problems, request.goal_kg);
    let mut ignored = Vec::new();
    let start = read_plans(&mut ignored, &request.plans);
    let input = SearchInput {
        lifter,
        start: ignored.is_empty().then_some(start),
        goal,
        bomb_out_cap: request.bomb_out_cap,
        simulations: request.simulations,
        seed: u64::from(request.seed),
    };
    let search = if problems.is_empty() {
        Search::new(&input)
    } else {
        Err(problems)
    };
    search.map_err(|problems| problems.into_iter().map(Problem::from).collect())
}

fn to_tradeoff_input(request: &TradeoffRequest) -> Result<TradeoffInput, Vec<InputProblem>> {
    let mut problems = Vec::new();
    let lifter = read_lifter(&mut problems, &request.lifter);
    let goal = read_goal(&mut problems, request.goal_kg);
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(TradeoffInput {
        lifter,
        goal,
        simulations: request.simulations,
        seed: u64::from(request.seed),
    })
}

fn read_plans(problems: &mut Vec<InputProblem>, plans: &PerLift<LiftPlanKg>) -> PerLift<LiftPlan> {
    plans.map(|lift, plan| {
        let opener = read_weight(problems, plan.opener_kg, Some(lift), InputField::Opener);
        let mut jumps = |input: JumpsKg, field| Jumps {
            after_make: read_weight(problems, input.after_make_kg, Some(lift), field),
            after_miss: read_weight(problems, input.after_miss_kg, Some(lift), field),
        };
        LiftPlan {
            opener,
            second: jumps(plan.second, InputField::Second),
            third: jumps(plan.third, InputField::Third),
        }
    })
}

fn read_lifter(problems: &mut Vec<InputProblem>, lifter: &LifterInput) -> Lifter {
    let lifts = lifter.lifts.map(|lift, model| LiftModel {
        expected_max: read_weight(
            problems,
            model.expected_max_kg,
            Some(lift),
            InputField::ExpectedMax,
        ),
        curve: model.curve,
    });
    Lifter {
        lifts,
        day_form: lifter.day_form,
    }
}

fn read_goal(problems: &mut Vec<InputProblem>, goal_kg: Option<f64>) -> Option<Weight> {
    goal_kg.map(|kg| read_weight(problems, kg, None, InputField::Goal))
}

fn to_input(request: &EvaluateRequest) -> Result<EvaluateInput, Vec<InputProblem>> {
    let mut problems = Vec::new();
    let lifter = read_lifter(&mut problems, &request.lifter);
    let plans = read_plans(&mut problems, &request.plans);
    let goal = read_goal(&mut problems, request.goal_kg);
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(EvaluateInput {
        lifter,
        plans,
        goal,
        simulations: request.simulations,
        seed: u64::from(request.seed),
    })
}

fn read_weight(
    problems: &mut Vec<InputProblem>,
    kg: f64,
    lift: Option<Lift>,
    field: InputField,
) -> Weight {
    Weight::from_kg(kg).unwrap_or_else(|| {
        problems.push(InputProblem::InvalidWeight { lift, field });
        Weight::ZERO
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jumps(after_make_kg: f64, after_miss_kg: f64) -> JumpsKg {
        JumpsKg {
            after_make_kg,
            after_miss_kg,
        }
    }

    fn request() -> EvaluateRequest {
        let model = |expected_max_kg| LiftModelInput {
            expected_max_kg,
            curve: MakeRateCurve {
                ratio_at_half: 1.0,
                steepness: 1_000.0,
            },
        };
        let plan = |opener_kg, second, third| LiftPlanKg {
            opener_kg,
            second: jumps(second, 0.0),
            third: jumps(third, 0.0),
        };
        EvaluateRequest {
            lifter: LifterInput {
                lifts: PerLift {
                    squat: model(200.0),
                    bench: model(130.0),
                    deadlift: model(240.0),
                },
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
            goal_kg: Some(550.0),
            simulations: 500,
            seed: 7,
        }
    }

    fn evaluation(request: &EvaluateRequest) -> Evaluation {
        match evaluate(request) {
            EvaluateResponse::Evaluated { evaluation } => evaluation,
            EvaluateResponse::Invalid { problems } => panic!("unexpected problems: {problems:?}"),
        }
    }

    fn problems(request: &EvaluateRequest) -> Vec<Problem> {
        match evaluate(request) {
            EvaluateResponse::Invalid { problems } => problems,
            EvaluateResponse::Evaluated { .. } => panic!("expected problems"),
        }
    }

    #[test]
    fn evaluates_a_valid_request() {
        let evaluation = evaluation(&request());
        assert_eq!(evaluation.expected_total_kg, 550.0);
        assert_eq!(evaluation.goal_probability, Some(1.0));
        assert_eq!(evaluation.lifts.bench.median_best_kg, Some(125.0));
        assert_eq!(evaluation.total_percentiles_kg.p50, 550.0);
    }

    #[test]
    fn matches_the_engine_for_the_same_seed() {
        let mut request = request();
        request.lifter.day_form.spread = 0.04;
        request.lifter.lifts.squat.curve.steepness = 25.0;
        let engine = evaluate::evaluate(&to_input(&request).unwrap()).unwrap();
        assert_eq!(evaluation(&request), engine);
    }

    #[test]
    fn engine_problems_name_their_lift_and_field() {
        let mut request = request();
        request.plans.squat.opener_kg = 181.0;
        request.plans.bench.third = jumps(0.0, 0.0);
        request.lifter.lifts.deadlift.expected_max_kg = 15.0;
        request.lifter.lifts.bench.curve.steepness = 0.0;
        request.lifter.day_form.correlation = 2.0;
        request.simulations = 0;
        let found: Vec<_> = problems(&request)
            .into_iter()
            .map(|p| (p.lift, p.field, p.code))
            .collect();
        assert_eq!(
            found,
            [
                (
                    Some(Lift::Bench),
                    InputField::Curve,
                    ProblemCode::InvalidCurve
                ),
                (
                    Some(Lift::Deadlift),
                    InputField::ExpectedMax,
                    ProblemCode::ExpectedMaxBelowBar
                ),
                (None, InputField::DayForm, ProblemCode::InvalidDayForm),
                (
                    None,
                    InputField::Simulations,
                    ProblemCode::SimulationsOutOfRange
                ),
                (
                    Some(Lift::Squat),
                    InputField::Opener,
                    ProblemCode::NotLoadable
                ),
                (
                    Some(Lift::Bench),
                    InputField::Third,
                    ProblemCode::NoIncreaseAfterMake
                ),
            ]
        );
    }

    #[test]
    fn problems_carry_the_engine_message() {
        let mut request = request();
        request.plans.squat.opener_kg = 17.5;
        assert_eq!(
            problems(&request)[0].message,
            "attempt 1 is lighter than the 20 kg bar"
        );
    }

    #[test]
    fn unreadable_weights_are_reported_per_field() {
        let mut request = request();
        request.lifter.lifts.squat.expected_max_kg = f64::NAN;
        request.plans.bench.second = jumps(-2.5, 0.0);
        request.goal_kg = Some(-1.0);
        let found: Vec<_> = problems(&request)
            .into_iter()
            .map(|p| (p.lift, p.field, p.code, p.message))
            .collect();
        let invalid = |lift, field| {
            (
                lift,
                field,
                ProblemCode::InvalidWeight,
                "not a valid weight in kg".to_string(),
            )
        };
        assert_eq!(
            found,
            [
                invalid(Some(Lift::Squat), InputField::ExpectedMax),
                invalid(Some(Lift::Bench), InputField::Second),
                invalid(None, InputField::Goal),
            ]
        );
    }

    fn tradeoff_request(goal_kg: Option<f64>) -> TradeoffRequest {
        let mut lifter = request().lifter;
        lifter.day_form.spread = 0.03;
        for lift in Lift::ALL {
            lifter.lifts.get_mut(lift).curve.steepness = 30.0;
        }
        TradeoffRequest {
            lifter,
            goal_kg,
            simulations: 500,
            seed: 7,
        }
    }

    #[test]
    fn sweeps_the_plan_family_in_kg() {
        let request = tradeoff_request(Some(550.0));
        let TradeoffResponse::Swept {
            points,
            best_for_goal,
        } = tradeoff(&request)
        else {
            panic!("expected points");
        };
        let point = points
            .iter()
            .find(|p| {
                p.shape
                    == PlanShape {
                        opener_percent: 90,
                        second_percent: 97,
                        third_percent: 102,
                    }
            })
            .unwrap();
        assert_eq!(
            point.plans.squat,
            LiftPlanKg {
                opener_kg: 180.0,
                second: jumps(12.5, 0.0),
                third: jumps(10.0, 0.0),
            }
        );
        let best = &points[best_for_goal.unwrap()];
        assert!(
            points
                .iter()
                .all(|p| p.goal_probability <= best.goal_probability)
        );
        assert!(points.iter().any(|p| p.on_frontier));
    }

    #[test]
    fn tradeoff_reports_lifter_and_goal_problems() {
        let problems = |request: &TradeoffRequest| match tradeoff(request) {
            TradeoffResponse::Invalid { problems } => problems
                .into_iter()
                .map(|p| (p.lift, p.field))
                .collect::<Vec<_>>(),
            TradeoffResponse::Swept { .. } => panic!("expected problems"),
        };
        assert_eq!(
            problems(&tradeoff_request(Some(-5.0))),
            [(None, InputField::Goal)]
        );
        let mut request = tradeoff_request(None);
        request.lifter.lifts.bench.expected_max_kg = 10.0;
        assert_eq!(
            problems(&request),
            [(Some(Lift::Bench), InputField::ExpectedMax)]
        );
    }

    fn search_request() -> SearchRequest {
        let request = tradeoff_request(Some(550.0));
        SearchRequest {
            lifter: request.lifter,
            plans: super::tests::request().plans,
            goal_kg: request.goal_kg,
            bomb_out_cap: Some(0.05),
            simulations: 500,
            seed: 7,
        }
    }

    fn run(mut search: Search) -> SearchProgress {
        loop {
            let progress = SearchProgress::from(search.step());
            if progress.done {
                return progress;
            }
        }
    }

    #[test]
    fn searches_from_the_requested_plans_and_reports_kg() {
        let found = run(start_search(&search_request()).unwrap());
        assert!(found.bomb_out_probability <= 0.05);
        assert!(found.goal_probability.is_some());
        let opener = Weight::from_kg(found.plans.squat.opener_kg).unwrap();
        assert!(opener.is_loadable());
    }

    #[test]
    fn an_unreadable_start_is_ignored() {
        let mut request = search_request();
        request.plans.bench.opener_kg = f64::NAN;
        assert!(run(start_search(&request).unwrap()).done);
    }

    #[test]
    fn search_reports_lifter_goal_and_cap_problems() {
        let mut request = search_request();
        request.goal_kg = Some(-1.0);
        let fields = |request: &SearchRequest| -> Vec<_> {
            start_search(request)
                .unwrap_err()
                .into_iter()
                .map(|p| p.field)
                .collect()
        };
        assert_eq!(fields(&request), [InputField::Goal]);
        request.goal_kg = None;
        request.bomb_out_cap = Some(2.0);
        assert_eq!(fields(&request), [InputField::BombOutCap]);
    }

    #[test]
    fn no_goal_means_no_goal_probability() {
        let mut request = request();
        request.goal_kg = None;
        let evaluation = evaluation(&request);
        assert_eq!(evaluation.goal_probability, None);
        assert_eq!(evaluation.standard_errors.goal_probability, None);
    }
}
