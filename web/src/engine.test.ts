/// <reference types="node" />
import { readFileSync } from "node:fs";
import { beforeAll, expect, it } from "vitest";
import { initSync } from "../pkg/attempt_sim_wasm.js";
import { answer, failed, search } from "./engine.ts";
import { CALIBRATIONS, EXAMPLE } from "./example.ts";
import {
  buildRequest,
  buildSearchRequest,
  buildTradeoffRequest,
  type Swept,
} from "./inputs.ts";
import { replanRequest, startMeet, take } from "./meet-day.ts";
import { buildEstimateRequest, parsePastMeets } from "./past-meets.ts";

beforeAll(() => {
  const wasm = new URL("../pkg/attempt_sim_wasm_bg.wasm", import.meta.url);
  initSync({ module: readFileSync(wasm) });
});

it("evaluates the example lifter", () => {
  const reply = answer({
    kind: "evaluate",
    request: buildRequest(EXAMPLE, false),
  });
  if (reply.kind !== "evaluated") throw new Error(JSON.stringify(reply));
  expect(reply.evaluation.goalProbability).toBeGreaterThan(0);
  expect(reply.evaluation.expectedTotalKg).toBeGreaterThan(400);
});

it("passes on the engine's problems with an invalid plan", () => {
  const reply = answer({
    kind: "evaluate",
    request: buildRequest({ ...EXAMPLE, "squat-openerKg": 181 }, false),
  });
  expect(reply).toEqual({
    kind: "invalid",
    problems: [
      {
        lift: "squat",
        field: "opener",
        code: "notLoadable",
        message: "attempt 1 is not a multiple of 2.5 kg",
      },
    ],
  });
});

it("fails a request the engine cannot read", () => {
  const reply = answer({
    kind: "evaluate",
    request: { ...buildRequest(EXAMPLE, false), seed: -1 },
  });
  expect(reply.kind).toBe("failed");
});

it("sweeps the plan family for the example lifter", () => {
  const reply = answer({
    kind: "tradeoff",
    request: buildTradeoffRequest(EXAMPLE),
  });
  if (reply.kind !== "swept") throw new Error(JSON.stringify(reply));
  expect(reply.points.some((point) => point.onFrontier)).toBe(true);
  expect(reply.bestForGoal).toBeDefined();
});

it("keeps the message of whatever went wrong", () => {
  expect(failed(new Error("no wasm"))).toEqual({
    kind: "failed",
    message: "no wasm",
  });
  expect(failed("no wasm")).toEqual({ kind: "failed", message: "no wasm" });
});

it("searches step by step until done", () => {
  const replies = [...search(buildSearchRequest(EXAMPLE))];
  const last = replies.at(-1);
  if (last?.kind !== "progress") throw new Error(JSON.stringify(last));
  expect(last.progress.done).toBe(true);
  expect(
    replies
      .slice(0, -1)
      .every((r) => r.kind === "progress" && !r.progress.done),
  ).toBe(true);
  expect(last.progress.bombOutProbability).toBeLessThanOrEqual(0.05);
});

it("searches to the same plans from the swept best plans", () => {
  const request = buildTradeoffRequest(EXAMPLE);
  const sweep = answer({ kind: "tradeoff", request });
  if (sweep.kind !== "swept") throw new Error(JSON.stringify(sweep));
  const last = (swept?: Swept) =>
    [...search(buildSearchRequest(EXAMPLE, swept))].at(-1);
  expect(buildSearchRequest(EXAMPLE, { sweep, request })).toHaveProperty(
    "familyBest",
  );
  expect(last({ sweep, request })).toEqual(last());
});

it("fails a search whose input has a problem", () => {
  const replies = [
    ...search({ ...buildSearchRequest(EXAMPLE), bombOutCap: 3 }),
  ];
  expect(replies).toEqual([
    {
      kind: "failed",
      message: "bomb-out cap must be a chance between 0 and 1",
    },
  ]);
});

it("estimates maxes from past meets", () => {
  const { meets } = parsePastMeets(
    [
      "2025-03-15 180 190 -197.5 120 125 -130 220 235 245",
      "2025-06-14 182.5 192.5 200 120 127.5 -132.5 222.5 237.5 -250",
    ].join("\n"),
  );
  const reply = answer({
    kind: "estimate",
    request: buildEstimateRequest(
      EXAMPLE,
      meets,
      20_300,
      CALIBRATIONS.any.typicalOpener,
    ),
  });
  if (reply.kind !== "estimated") throw new Error(JSON.stringify(reply));
  const { squat, bench } = reply.estimate.lifts;
  expect(squat?.meets).toBe(2);
  expect(squat?.expectedMaxKg).toBeGreaterThan(190);
  expect(squat?.expectedMaxKg).toBeLessThan(215);
  expect(bench?.uncertainty).toBeGreaterThan(0);
});

it("expects less progress from a lifter with meets before the pasted ones", () => {
  const { meets } = parsePastMeets(
    "2025-03-15 180 190 -197.5 120 125 -130 220 235 245",
  );
  const squatMax = (meetsBefore: number | undefined) => {
    const request = buildEstimateRequest(
      EXAMPLE,
      meets,
      20_400,
      CALIBRATIONS.any.typicalOpener,
    );
    if (meetsBefore === undefined) delete request.meetsBefore;
    else request.meetsBefore = meetsBefore;
    const reply = answer({ kind: "estimate", request });
    if (reply.kind !== "estimated") throw new Error(JSON.stringify(reply));
    return reply.estimate.lifts.squat?.expectedMaxKg ?? Number.NaN;
  };
  expect(squatMax(undefined)).toBe(squatMax(0));
  expect(squatMax(9)).toBeLessThan(squatMax(0) - 5);
});

it("fails an estimate without attempts", () => {
  const reply = answer({
    kind: "estimate",
    request: buildEstimateRequest(
      EXAMPLE,
      [],
      20_300,
      CALIBRATIONS.any.typicalOpener,
    ),
  });
  expect(reply).toEqual({
    kind: "failed",
    message: "no past meet has an attempt",
  });
});

it("lists one-step changes to the current plans, best first", () => {
  const reply = answer({
    kind: "changes",
    request: buildSearchRequest(EXAMPLE),
  });
  if (reply.kind !== "changed") throw new Error(JSON.stringify(reply));
  expect(reply.current.plans).toEqual(buildRequest(EXAMPLE, false).plans);
  expect(reply.changes.length).toBeGreaterThan(20);
  const [best] = reply.changes;
  expect(best?.weightKg).toBeGreaterThanOrEqual(0);
});

it("reports the spread of totals and each lift's attempt paths", () => {
  const reply = answer({
    kind: "evaluate",
    request: buildRequest(EXAMPLE, false),
  });
  if (reply.kind !== "evaluated") throw new Error(JSON.stringify(reply));
  const { totals, lifts } = reply.evaluation;
  const share = totals.reduce((sum, total) => sum + total.share, 0);
  expect(share).toBeCloseTo(1, 9);
  expect(lifts.squat.paths).toHaveLength(8);
  expect(lifts.squat.attemptMakeProbabilities[0]).toBeGreaterThan(0.9);
});

it("replans the example meet after the squat opener", () => {
  const meet = startMeet(buildSearchRequest(EXAMPLE));
  const goalChance = (taken: { weightKg: number; made: boolean }[]) => {
    const reply = answer({
      kind: "replan",
      request: replanRequest({ ...meet, taken }),
    });
    if (reply.kind !== "replanned") throw new Error(JSON.stringify(reply));
    return { chance: reply.result.goalProbability ?? 0, next: reply.next };
  };
  const opener = meet.setup.plans.squat.openerKg;
  const before = goalChance([]);
  const made = goalChance(take(meet, { weightKg: opener, made: true }).taken);
  const missed = goalChance([{ weightKg: opener, made: false }]);
  expect(before.next).toEqual({ lift: "squat", attempt: 1, weightKg: opener });
  expect(made.chance).toBeGreaterThan(before.chance);
  expect(missed.chance).toBeLessThan(before.chance);
  expect(made.next?.attempt).toBe(2);
  expect(made.next?.weightKg).toBeGreaterThan(opener);
});
