import type {
  LiftPlanKg,
  PlanShape,
  TradeoffPoint,
} from "../pkg/attempt_sim_wasm.js";
import { attemptTree } from "./attempt-tree.ts";
import {
  finePercent,
  LIFT_NAMES,
  ROUND_CHANCES,
  roundChanceUp,
  weight,
} from "./format.ts";
import { LIFTS } from "./inputs.ts";

export const shapeLabel = (shape: PlanShape) =>
  `${shape.openerPercent}% / ${shape.secondPercent}% / ${shape.thirdPercent}%`;

export interface HighlightedPoint {
  point: TradeoffPoint;
  bestForGoal: boolean;
}

/** The frontier plans plus the best plan for the goal, from safest to riskiest. */
export function highlightedPoints(
  points: TradeoffPoint[],
  bestForGoal: number | undefined,
): HighlightedPoint[] {
  return points
    .map((point, index) => ({ point, bestForGoal: index === bestForGoal }))
    .filter(({ point, bestForGoal }) => point.onFrontier || bestForGoal)
    .sort(
      (a, b) =>
        a.point.bombOutProbability - b.point.bombOutProbability ||
        b.point.expectedTotalKg - a.point.expectedTotalKg,
    );
}

/** The bomb-out bands the comparison table picks a plan from, by their upper ends. */
export const RISK_LEVELS = [0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1];

/** The riskiest band always shown; riskier ones only fill a table with fewer rows. */
const USUAL_RISK = 0.1;
const FEWEST_ROWS = 3;

export interface ComparisonRow {
  point: TradeoffPoint;
  /** The plan's risk band, such as "1% to 2% bomb-out", or why else it is listed. */
  label: string;
  bestForGoal: boolean;
}

/** Ranks plans by goal chance when there is a goal, then by average total. */
const better = (a: TradeoffPoint, b: TradeoffPoint) =>
  (a.goalProbability ?? 0) - (b.goalProbability ?? 0) ||
  a.expectedTotalKg - b.expectedTotalKg;

function bandLabel(below: number | undefined, level: number) {
  if (below === undefined) return `Up to ${finePercent(level)} bomb-out`;
  if (level === 1) return `Over ${finePercent(below)} bomb-out`;
  return `${finePercent(below)} to ${finePercent(level)} bomb-out`;
}

/**
 * Plans that span the risk levels, from safest to riskiest: the best plan in each band of
 * bomb-out chance up to 10%, and in riskier bands while fewer than `FEWEST_ROWS` are picked
 * (by goal chance when there is a goal, else by average total), and the best plan for the
 * goal, marked.
 */
export function comparisonRows(
  points: TradeoffPoint[],
  bestForGoal: number | undefined,
): ComparisonRow[] {
  const best = bestForGoal === undefined ? undefined : points[bestForGoal];
  const rows: ComparisonRow[] = [];
  RISK_LEVELS.forEach((level, index) => {
    if (level > USUAL_RISK && rows.length >= FEWEST_ROWS) return;
    const below = RISK_LEVELS[index - 1];
    const inBand = points.filter(
      (point) =>
        point.bombOutProbability <= level + 1e-9 &&
        (below === undefined || point.bombOutProbability > below + 1e-9),
    );
    const pick = inBand.reduce<TradeoffPoint | undefined>(
      (top, point) => (!top || better(point, top) > 0 ? point : top),
      undefined,
    );
    if (pick) {
      rows.push({
        point: pick,
        label: bandLabel(below, level),
        bestForGoal: pick === best,
      });
    }
  });
  if (best && !rows.some((row) => row.point === best)) {
    rows.push({ point: best, label: "Best for the goal", bestForGoal: true });
  }
  return rows.sort(
    (a, b) =>
      a.point.bombOutProbability - b.point.bombOutProbability ||
      b.point.expectedTotalKg - a.point.expectedTotalKg,
  );
}

/** The attempts after makes, "182.5 / 195 / 202.5". */
export function plannedAttempts(plan: LiftPlanKg): string {
  const opener = attemptTree(plan);
  const second = opener?.afterMake;
  const third = second?.afterMake;
  return opener && second && third
    ? [opener, second, third].map((node) => weight(node.weightKg)).join(" / ")
    : "";
}

/** Each lift's attempts after makes, one line per lift. */
export const planLines = (plans: TradeoffPoint["plans"]) =>
  LIFTS.map(
    (lift) => `${LIFT_NAMES[lift].short} ${plannedAttempts(plans[lift])}`,
  );

/** What the best plan for the goal is best at, naming the bomb-out cap when there is one. */
export const bestForGoalLabel = (bombOutCap: number | undefined) =>
  bombOutCap === undefined
    ? "best for the goal"
    : `best for the goal within ${finePercent(bombOutCap)} bomb-out`;

/** Why no plan is marked best for the goal although there is a goal, if that is so. */
export function noGoalPickNote(
  goalKg: number | undefined,
  bombOutCap: number | undefined,
  bestForGoal: number | undefined,
): string {
  if (
    goalKg === undefined ||
    bombOutCap === undefined ||
    bestForGoal !== undefined
  ) {
    return "";
  }
  return `No plan in the family keeps the bomb-out chance within ${finePercent(bombOutCap)}, so none is marked best for the goal. Raise the highest bomb-out chance in step 1 to see one.`;
}

/** The smallest gap between risk ticks, as a share of the axis length. */
const MIN_TICK_GAP = 0.08;

/**
 * The end of a square-root risk axis, the first round tick at or above the riskiest plan
 * and at least 1%, and the round ticks along it, the larger ticks winning where two would
 * sit closer than `MIN_TICK_GAP`.
 */
export function riskAxis(maxRisk: number): { max: number; ticks: number[] } {
  const max = roundChanceUp(Math.max(maxRisk, 0.01));
  const position = (tick: number) => Math.sqrt(tick / max);
  const kept = [0];
  for (const tick of ROUND_CHANCES.filter(
    (tick) => tick > 0 && tick <= max,
  ).reverse()) {
    if (
      kept.every(
        (other) => Math.abs(position(tick) - position(other)) >= MIN_TICK_GAP,
      )
    ) {
      kept.push(tick);
    }
  }
  return { max, ticks: kept.sort((a, b) => a - b) };
}
