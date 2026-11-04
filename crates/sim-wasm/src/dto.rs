use attempt_sim::evaluate::{
    self, EvaluateInput, Evaluation, InputField, InputProblem, LiftModel, Lifter, ProblemCode,
};
use attempt_sim::lift::{Lift, PerLift};
use attempt_sim::model::{DayForm, MakeRateCurve};
use attempt_sim::plan::{Jumps, LiftPlan};
use attempt_sim::rules::Weight;
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

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct JumpsInput {
    pub after_make_kg: f64,
    pub after_miss_kg: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct LiftPlanInput {
    pub opener_kg: f64,
    pub second: JumpsInput,
    pub third: JumpsInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateRequest {
    pub lifter: LifterInput,
    pub plans: PerLift<LiftPlanInput>,
    #[serde(default)]
    #[tsify(optional)]
    pub goal_kg: Option<f64>,
    pub simulations: u32,
    /// 32 bits rather than the engine's 64, because a JavaScript number cannot hold every
    /// 64-bit integer exactly.
    pub seed: u32,
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

fn to_input(request: &EvaluateRequest) -> Result<EvaluateInput, Vec<InputProblem>> {
    let mut problems = Vec::new();
    let lifts = request.lifter.lifts.map(|lift, model| LiftModel {
        expected_max: read_weight(
            &mut problems,
            model.expected_max_kg,
            Some(lift),
            InputField::ExpectedMax,
        ),
        curve: model.curve,
    });
    let plans = request.plans.map(|lift, plan| {
        let opener = read_weight(
            &mut problems,
            plan.opener_kg,
            Some(lift),
            InputField::Opener,
        );
        let mut jumps = |input: JumpsInput, field| Jumps {
            after_make: read_weight(&mut problems, input.after_make_kg, Some(lift), field),
            after_miss: read_weight(&mut problems, input.after_miss_kg, Some(lift), field),
        };
        LiftPlan {
            opener,
            second: jumps(plan.second, InputField::Second),
            third: jumps(plan.third, InputField::Third),
        }
    });
    let goal = request
        .goal_kg
        .map(|kg| read_weight(&mut problems, kg, None, InputField::Goal));

    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(EvaluateInput {
        lifter: Lifter {
            lifts,
            day_form: request.lifter.day_form,
        },
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

    fn jumps(after_make_kg: f64, after_miss_kg: f64) -> JumpsInput {
        JumpsInput {
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
        let plan = |opener_kg, second, third| LiftPlanInput {
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
        assert_eq!(evaluation.lifts.bench.expected_best_kg, Some(125.0));
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

    #[test]
    fn no_goal_means_no_goal_probability() {
        let mut request = request();
        request.goal_kg = None;
        let evaluation = evaluation(&request);
        assert_eq!(evaluation.goal_probability, None);
        assert_eq!(evaluation.standard_errors.goal_probability, None);
    }
}
