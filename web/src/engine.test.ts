/// <reference types="node" />
import { readFileSync } from "node:fs";
import { beforeAll, expect, it } from "vitest";
import { initSync } from "../pkg/attempt_sim_wasm.js";
import { answer, failed } from "./engine.ts";
import { EXAMPLE } from "./example.ts";
import { buildRequest, buildTradeoffRequest } from "./inputs.ts";

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
