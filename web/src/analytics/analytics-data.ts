import type {
  Change,
  Evaluation,
  Lift,
  LiftPlanKg,
  LiftSummary,
  PerLift,
  PlanOutcome,
  Setting,
  TotalShare,
} from "../../pkg/attempt_sim_wasm.js";
import { kg, percent } from "../format.ts";
import { LIFTS } from "../inputs.ts";

export const LIFT_NAMES: Record<Lift, string> = {
  squat: "Squat",
  bench: "Bench",
  deadlift: "Deadlift",
};

const BIN_WIDTHS = [5, 10, 20, 50];
const MAX_BINS = 30;

export interface Bin {
  fromKg: number;
  toKg: number;
  share: number;
  reachesGoal: boolean;
}

export interface Histogram {
  bombOutShare: number;
  bins: Bin[];
}

/**
 * The totals in bins of 5 kg or wider, bomb-outs apart. Bin edges line up with the goal,
 * so every bin is wholly at or above it or wholly short of it.
 */
export function histogram(totals: TotalShare[], goalKg?: number): Histogram {
  const made = totals.filter((total) => total.totalKg > 0);
  const bombOutShare = totals
    .filter((total) => total.totalKg === 0)
    .reduce((sum, total) => sum + total.share, 0);
  if (made.length === 0) return { bombOutShare, bins: [] };
  const lowest = made[0]!.totalKg;
  const highest = made[made.length - 1]!.totalKg;
  const width =
    BIN_WIDTHS.find((width) => (highest - lowest) / width < MAX_BINS) ?? 100;
  const origin = goalKg ?? 0;
  const binStart = (totalKg: number) =>
    origin + Math.floor((totalKg - origin) / width) * width;
  const bins: Bin[] = [];
  for (let fromKg = binStart(lowest); fromKg <= highest; fromKg += width) {
    bins.push({
      fromKg,
      toKg: fromKg + width,
      share: 0,
      reachesGoal: goalKg !== undefined && fromKg >= goalKg,
    });
  }
  for (const total of made) {
    bins[
      Math.round((binStart(total.totalKg) - bins[0]!.fromKg) / width)
    ]!.share += total.share;
  }
  return { bombOutShare, bins };
}

export interface ChancePoint {
  totalKg: number;
  chance: number;
}

/** Chance of a total at or above `totalKg`. */
export const chanceOfAtLeast = (totals: TotalShare[], totalKg: number) =>
  totals
    .filter((total) => total.totalKg >= totalKg)
    .reduce((sum, total) => sum + total.share, 0);

/** The chance of a total at least as high as each total that came up, bomb-outs left out. */
export function reachCurve(totals: TotalShare[]): ChancePoint[] {
  let chance = totals.reduce((sum, total) => sum + total.share, 0);
  const points: ChancePoint[] = [];
  for (const total of totals) {
    if (total.totalKg > 0) points.push({ totalKg: total.totalKg, chance });
    chance -= total.share;
  }
  return points;
}

export interface LiftRisk {
  lift: Lift;
  total: number;
  technical: number;
  tooHeavy: number;
}

export interface BombOutRisk {
  lifts: LiftRisk[];
  /** The end of a shared scale for the bars, a round chance at or above every lift's. */
  scaleMax: number;
  hint: string;
}

const RISK_SCALES = [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1];

/** Each lift's bomb-out chance split by cause, and a line naming the riskiest lift. */
export function bombOutRisk(lifts: PerLift<LiftSummary>): BombOutRisk {
  const risks = LIFTS.map((lift): LiftRisk => {
    const total = 1 - lifts[lift].makeProbability;
    const technical = Math.min(lifts[lift].technicalBombOutProbability, total);
    return { lift, total, technical, tooHeavy: total - technical };
  });
  const summed = risks.reduce((sum, risk) => sum + risk.total, 0);
  const riskiest = risks.reduce((a, b) => (b.total > a.total ? b : a));
  return {
    lifts: risks,
    scaleMax: RISK_SCALES.find((scale) => scale >= riskiest.total - 1e-9) ?? 1,
    hint:
      summed === 0
        ? "No lift bombed out in the simulated meets."
        : `${LIFT_NAMES[riskiest.lift]} carries ${percent(riskiest.total / summed)} of the bomb-out risk.`,
  };
}

export interface PathRow {
  made: boolean[];
  bestKg?: number;
  share: number;
}

export interface AttemptView {
  makeChances: number[];
  /** The paths after a made opener that came up, most likely first. */
  rows: PathRow[];
  /** Share of meets with a missed opener, all paths together. */
  missedOpenerShare: number;
}

export function attemptView(summary: LiftSummary): AttemptView {
  const happened = summary.paths.filter((path) => path.share > 0);
  return {
    makeChances: summary.attemptMakeProbabilities,
    rows: happened.filter((path) => path.made[0]),
    missedOpenerShare: happened
      .filter((path) => !path.made[0])
      .reduce((sum, path) => sum + path.share, 0),
  };
}

export interface DayView {
  badKg: number;
  normalKg: number;
  goodKg: number;
  bestPossibleKg: number;
}

export function dayView(evaluation: Evaluation): DayView {
  const { p10, p50, p90 } = evaluation.totalPercentilesKg;
  const bestPossibleKg = LIFTS.reduce(
    (sum, lift) =>
      sum +
      Math.max(0, ...evaluation.lifts[lift].paths.map((p) => p.bestKg ?? 0)),
    0,
  );
  return { badKg: p10, normalKg: p50, goodKg: p90, bestPossibleKg };
}

const ATTEMPT_NAMES: Record<Setting, [string, string]> = {
  opener: ["opener", ""],
  secondAfterMake: ["2nd", " after a make"],
  secondAfterMiss: ["2nd", " after a miss"],
  thirdAfterMake: ["3rd", " after a make"],
  thirdAfterMiss: ["3rd", " after a miss"],
};

const settingKg = (plan: LiftPlanKg, setting: Setting) =>
  ({
    opener: plan.openerKg,
    secondAfterMake: plan.second.afterMakeKg,
    secondAfterMiss: plan.second.afterMissKg,
    thirdAfterMake: plan.third.afterMakeKg,
    thirdAfterMiss: plan.third.afterMissKg,
  })[setting];

/** A change in words, such as "Bench 3rd: +5.0 kg instead of +7.5 kg after a make". */
export function describeChange(change: Change, current: PlanOutcome): string {
  const [attempt, after] = ATTEMPT_NAMES[change.setting];
  const sign = change.setting === "opener" ? "" : "+";
  const before = settingKg(current.plans[change.lift], change.setting);
  return `${LIFT_NAMES[change.lift]} ${attempt}: ${sign}${kg(change.weightKg)} instead of ${sign}${kg(before)}${after}`;
}

/** Changes in chance smaller than one shown decimal read as no change. */
const SMALLEST_DELTA = 0.0005;

export type Direction = "better" | "worse" | "same";

export interface Delta {
  text: string;
  direction: Direction;
}

/** A signed change in a chance, "better" when it moves the way `better` points. */
export function delta(label: string, change: number, better: 1 | -1): Delta {
  if (Math.abs(change) < SMALLEST_DELTA) {
    return { text: `${label} same`, direction: "same" };
  }
  const sign = change > 0 ? "+" : "−";
  return {
    text: `${label} ${sign}${percent(Math.abs(change))}`,
    direction: Math.sign(change) === better ? "better" : "worse",
  };
}

/** Same order as the engine's search: within the cap first, then goal chance, then average total. */
function rank(outcome: PlanOutcome, bombOutCap?: number): number[] {
  const feasible =
    bombOutCap === undefined || outcome.bombOutProbability <= bombOutCap;
  if (!feasible)
    return [0, -outcome.bombOutProbability, outcome.expectedTotalKg];
  return [
    1,
    outcome.goalProbability ?? outcome.expectedTotalKg,
    outcome.expectedTotalKg,
  ];
}

const ranksHigher = (a: number[], b: number[]) => {
  const index = a.findIndex((value, i) => value !== b[i]);
  return index >= 0 && a[index]! > b[index]!;
};

export interface WhatIf {
  text: string;
  goal?: Delta;
  bombOut: Delta;
  plans: PerLift<LiftPlanKg>;
}

const WHAT_IFS = 3;

/**
 * Up to three changes: those ranking above the current plan, best first, then those
 * cutting the bomb-out chance most.
 */
export function whatIfs(
  current: PlanOutcome,
  changes: Change[],
  bombOutCap?: number,
): WhatIf[] {
  const currentRank = rank(current, bombOutCap);
  const improving = changes.filter((change) =>
    ranksHigher(rank(change.result, bombOutCap), currentRank),
  );
  const safer = changes
    .filter(
      (change) =>
        !improving.includes(change) &&
        change.result.bombOutProbability < current.bombOutProbability,
    )
    .sort((a, b) => a.result.bombOutProbability - b.result.bombOutProbability);
  return [...improving, ...safer].slice(0, WHAT_IFS).map((change) => ({
    text: describeChange(change, current),
    ...(change.result.goalProbability === undefined ||
    current.goalProbability === undefined
      ? {}
      : {
          goal: delta(
            "goal",
            change.result.goalProbability - current.goalProbability,
            1,
          ),
        }),
    bombOut: delta(
      "bomb-out",
      change.result.bombOutProbability - current.bombOutProbability,
      -1,
    ),
    plans: change.result.plans,
  }));
}
