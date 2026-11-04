use crate::days::Steps;
use crate::rules::{PLATE_STEP, Weight};

/// The figures a search ranks plans by, over a set of simulated meets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    pub bomb_out_probability: f64,
    /// Average total, counting a bomb-out as 0.
    pub expected_total_kg: f64,
    /// Absent when no goal was given.
    pub goal_probability: Option<f64>,
}

/// A bomb-out cap is a chance from 0 to 1, or none.
pub(crate) fn is_valid_bomb_out_cap(cap: Option<f64>) -> bool {
    cap.is_none_or(|cap| (0.0..=1.0).contains(&cap))
}

/// Counts meet totals into an [`Outcome`] without keeping every total. The counts fit in
/// `u32`s: a total is at most 1,200 plate steps, so even a million meets add up to well under
/// `u32::MAX`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OutcomeTally {
    meets: u32,
    bomb_outs: u32,
    total_steps: u32,
    goals_met: u32,
}

impl OutcomeTally {
    /// Records one meet's total, 0 for a bomb-out, against the goal from [`goal_steps`](crate::days::goal_steps).
    #[inline(always)]
    pub(crate) fn record(&mut self, total: Steps, goal: Steps) {
        self.meets += 1;
        self.bomb_outs += u32::from(total == 0);
        self.total_steps += u32::from(total);
        self.goals_met += u32::from(total >= goal);
    }

    pub(crate) fn outcome(&self, goal: Option<Weight>) -> Outcome {
        let meets = f64::from(self.meets);
        Outcome {
            bomb_out_probability: f64::from(self.bomb_outs) / meets,
            expected_total_kg: f64::from(self.total_steps) * PLATE_STEP.kg() / meets,
            goal_probability: goal.map(|_| f64::from(self.goals_met) / meets),
        }
    }
}
