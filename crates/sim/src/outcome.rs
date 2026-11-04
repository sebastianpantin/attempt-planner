use crate::rules::Weight;

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

/// Counts meet totals into an [`Outcome`] without keeping every total.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OutcomeTally {
    meets: u32,
    bomb_outs: u32,
    total_grams: u64,
    goals_met: u32,
}

impl OutcomeTally {
    /// Records one meet's total, `None` for a bomb-out.
    pub(crate) fn record(&mut self, total: Option<Weight>, goal: Option<Weight>) {
        self.meets += 1;
        match total {
            None => self.bomb_outs += 1,
            Some(total) => {
                self.total_grams += u64::from(total.grams());
                self.goals_met += u32::from(goal.is_some_and(|goal| total >= goal));
            }
        }
    }

    pub(crate) fn outcome(&self, goal: Option<Weight>) -> Outcome {
        let meets = f64::from(self.meets);
        Outcome {
            bomb_out_probability: f64::from(self.bomb_outs) / meets,
            expected_total_kg: self.total_grams as f64 / 1000.0 / meets,
            goal_probability: goal.map(|_| f64::from(self.goals_met) / meets),
        }
    }
}
