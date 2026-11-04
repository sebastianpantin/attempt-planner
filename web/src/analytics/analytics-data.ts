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
import { attemptTree } from "../attempt-tree.ts";
import {
  shortKg as kg,
  LIFT_NAMES,
  percent,
  roundChanceUp,
  weight,
} from "../format.ts";
import { LIFTS } from "../inputs.ts";
import type { KnownGoal } from "../outcome.ts";

/** At least 10 kg, so every bin spans four 2.5 kg totals and the bars do not alternate. */
const BIN_WIDTHS = [10, 20, 25, 50];
const MAX_BINS = 24;

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
 * The totals in bins of 10 kg or wider, bomb-outs apart. Bin edges line up with the goal,
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

/** The bin most totals land in. */
export const tallestBin = (bins: Bin[]) =>
  bins.reduce<Bin | undefined>(
    (tallest, bin) => (!tallest || bin.share > tallest.share ? bin : tallest),
    undefined,
  );

/** Label every so many bins, keeping about nine labels at most. */
export const labelEvery = (bins: number) => Math.max(1, Math.ceil(bins / 9));

/** The histogram in words, for its accessible name. */
export function histogramLabel(
  { bombOutShare, bins }: Histogram,
  goal?: KnownGoal,
): string {
  const tallest = tallestBin(bins);
  return [
    `Histogram of simulated totals. Bomb-out ${percent(bombOutShare)}.`,
    tallest &&
      `Totals from ${kg(bins[0]!.fromKg)} to ${kg(bins.at(-1)!.toKg)}, most often ${kg(tallest.fromKg)} to ${kg(tallest.toKg)}.`,
    goal && `${percent(goal.chance)} at or above the goal of ${kg(goal.kg)}.`,
  ]
    .filter(Boolean)
    .join(" ");
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

/** A total and the chance of reaching it, "560 kg: 48.2%". */
export const totalCheck = (totals: TotalShare[], totalKg: number) =>
  `${kg(totalKg)}: ${percent(chanceOfAtLeast(totals, totalKg))}`;

/** The reach curve in words, for its accessible name. */
export function reachLabel(points: ChancePoint[], goal?: KnownGoal): string {
  const first = points[0];
  const last = points.at(-1);
  return [
    "Chance of reaching each total.",
    first && last
      ? `${percent(first.chance)} reach ${kg(first.totalKg)}, falling to ${percent(last.chance)} at ${kg(last.totalKg)}.`
      : "Every simulated meet bombs out.",
    goal && `${percent(goal.chance)} reach the goal of ${kg(goal.kg)}.`,
  ]
    .filter(Boolean)
    .join(" ");
}

const TOTAL_STEP = 2.5;

/**
 * The range of the slider that checks any total: from the lowest to the highest total
 * that came up, starting at `wantedKg` moved onto a 2.5 kg step within it.
 */
export function totalCheckRange(points: ChancePoint[], wantedKg: number) {
  const min = points[0]!.totalKg;
  const max = points.at(-1)!.totalKg;
  const stepped = Math.round(wantedKg / TOTAL_STEP) * TOTAL_STEP;
  return { min, max, value: Math.min(Math.max(stepped, min), max) };
}

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
  /** Why the lifts' chances add up to more than the meet's, when they do. */
  overlap: string;
  hint: string;
}

/** The share of the meet's bomb-out chance from which one lift carries most of it. */
const MOST_OF_THE_RISK = 0.5;
/** A meet bomb-out chance below this is rare enough to need no lift named or fixed. */
const WORTH_FIXING = 0.01;

function riskAdvice({ lift, technical, tooHeavy }: LiftRisk): string {
  return tooHeavy >= technical
    ? `A lighter ${LIFT_NAMES[lift].full.toLowerCase()} opener is the cheapest fix.`
    : "Most of it is technical misses, so cleaner technique helps more than lighter weights.";
}

/**
 * Each lift's own chance of missing all three attempts, split by cause; why they can add
 * up to more than the meet's bomb-out chance; and, unless bomb-outs are rare, a line naming
 * the riskiest lift, with advice when that lift makes up most of the meet's chance.
 */
export function bombOutRisk(
  lifts: PerLift<LiftSummary>,
  bombOutProbability: number,
): BombOutRisk {
  const risks = LIFTS.map((lift): LiftRisk => {
    const total = 1 - lifts[lift].makeProbability;
    const technical = Math.min(lifts[lift].technicalBombOutProbability, total);
    return { lift, total, technical, tooHeavy: total - technical };
  });
  const summed = risks.reduce((sum, risk) => sum + risk.total, 0);
  const riskiest = risks.reduce((a, b) => (b.total > a.total ? b : a));
  const name = LIFT_NAMES[riskiest.lift].full;
  return {
    lifts: risks,
    scaleMax: roundChanceUp(Math.max(riskiest.total, 0.01)),
    overlap:
      percent(summed) === percent(bombOutProbability)
        ? ""
        : `One meet can bomb out on two lifts, so these add up to ${percent(summed)}, more than your ${percent(bombOutProbability)} chance of bombing out.`,
    hint:
      summed === 0
        ? "No lift bombed out in the simulated meets."
        : bombOutProbability < WORTH_FIXING
          ? `Bomb-outs are rare with this plan: ${percent(bombOutProbability)} of meets.`
          : [
              `${name} is the riskiest lift: it bombs out in ${percent(riskiest.total)} of meets, against ${percent(bombOutProbability)} for the meet as a whole.`,
              ...(riskiest.total >= MOST_OF_THE_RISK * bombOutProbability
                ? [riskAdvice(riskiest)]
                : []),
            ].join(" "),
  };
}

export interface PathRow {
  made: boolean[];
  bestKg?: number;
  share: number;
}

/** The weights a path's attempts were taken at, empty while the plan is unknown. */
export function pathWeights(plan: LiftPlanKg | undefined, made: boolean[]) {
  const weights: number[] = [];
  let node = plan && attemptTree(plan);
  for (const [index, wasMade] of made.entries()) {
    if (!node) return [];
    weights[index] = node.weightKg;
    node = wasMade ? node.afterMake : node.afterMiss;
  }
  return weights;
}

/** A path's weights, "182.5 → 195 → 202.5 kg", or its best lift while the plan is unknown. */
export function pathText(row: PathRow, plan?: LiftPlanKg): string {
  const weights = pathWeights(plan, row.made);
  if (weights.length > 0) {
    return `${weights.map(weight).join(" → ")} kg`;
  }
  return row.bestKg === undefined ? "no lift" : `best ${kg(row.bestKg)}`;
}

/** The labels of the make-chance cards: a later attempt counts at whatever weight it is. */
export const MAKE_CHANCE_LABELS = [
  "1st attempt made",
  "2nd attempt made (any weight)",
  "3rd attempt made (any weight)",
];

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

/** "Applied: …", then "Simulating…" until the results with the change are in. */
export const appliedLine = (change: string, done: boolean) =>
  `Applied: ${change}. ${done ? "Results updated." : "Simulating…"}`;

/** Each setting's attempt, when it applies and its weight in a plan. */
const SETTINGS: Record<
  Setting,
  { attempt: string; after: string; kg: (plan: LiftPlanKg) => number }
> = {
  opener: { attempt: "opener", after: "", kg: (plan) => plan.openerKg },
  secondAfterMake: {
    attempt: "2nd",
    after: " after a make",
    kg: (plan) => plan.second.afterMakeKg,
  },
  secondAfterMiss: {
    attempt: "2nd",
    after: " after a miss",
    kg: (plan) => plan.second.afterMissKg,
  },
  thirdAfterMake: {
    attempt: "3rd",
    after: " after a make",
    kg: (plan) => plan.third.afterMakeKg,
  },
  thirdAfterMiss: {
    attempt: "3rd",
    after: " after a miss",
    kg: (plan) => plan.third.afterMissKg,
  },
};

/** A jump in words, "+5 kg", or "repeat the weight" for none. */
const jumpText = (jumpKg: number) =>
  jumpKg === 0 ? "repeat the weight" : `${jumpKg > 0 ? "+" : ""}${kg(jumpKg)}`;

/**
 * A change in words, such as "Bench press 3rd after a make: +5 kg instead of +7.5 kg". A
 * 2nd or 3rd attempt is a jump from the attempt before.
 */
export function describeChange(change: Change, current: PlanOutcome): string {
  const { attempt, after, kg: settingKg } = SETTINGS[change.setting];
  const show = change.setting === "opener" ? kg : jumpText;
  const before = settingKg(current.plans[change.lift]);
  return `${LIFT_NAMES[change.lift].full} ${attempt}${after}: ${show(change.weightKg)} instead of ${show(before)}`;
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

export interface WhatIf {
  text: string;
  goal?: Delta;
  bombOut: Delta;
  plans: PerLift<LiftPlanKg>;
}

const WHAT_IFS = 3;

/**
 * Up to three changes: those the engine ranks above the current plan, best first, then
 * those cutting the bomb-out chance most.
 */
export function whatIfs(current: PlanOutcome, changes: Change[]): WhatIf[] {
  const improving = changes.filter((change) => change.improves);
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
