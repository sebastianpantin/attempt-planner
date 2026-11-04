import { expect, it } from "vitest";
import {
  finePercent,
  headlines,
  kg,
  listOf,
  percent,
  simulationError,
} from "./format.ts";
import { PRECISE_SIMULATIONS, QUICK_SIMULATIONS } from "./inputs.ts";

it("shows kg with exactly one decimal", () => {
  expect(kg(200)).toBe("200.0 kg");
  expect(kg(202.5)).toBe("202.5 kg");
  expect(kg(512.3456)).toBe("512.3 kg");
});

it("shows probabilities with exactly one decimal", () => {
  expect(percent(0.0512)).toBe("5.1%");
  expect(percent(0.98)).toBe("98.0%");
  expect(percent(0)).toBe("0.0%");
  expect(percent(1)).toBe("100.0%");
  expect(percent(0.0014)).toBe("0.1%");
});

it("says a chance too small for one decimal is under 0.1%", () => {
  expect(percent(0.0002)).toBe("under 0.1%");
});

it("drops trailing zeros for axis ticks", () => {
  expect(finePercent(0.005)).toBe("0.5%");
  expect(finePercent(0.01)).toBe("1%");
  expect(finePercent(0.00125)).toBe("0.13%");
});

it("joins a list with commas and a final and", () => {
  expect(listOf(["a", "b", "c"])).toBe("a, b, and c");
  expect(listOf(["a", "b"])).toBe("a and b");
});

it("lists the headline figures, the goal chance only with a goal", () => {
  const figures = { expectedTotalKg: 553.21, bombOutProbability: 0.0484 };
  expect(headlines(figures)).toEqual([
    ["total", "553.2 kg"],
    ["bombOut", "4.8%"],
  ]);
  expect(headlines({ ...figures, goalProbability: 0.7812 })[1]).toEqual([
    "goal",
    "78.1%",
  ]);
});

it("sums up the simulation error in one line", () => {
  expect(
    simulationError(
      {
        expectedTotalKg: 0.83,
        goalProbability: 0.0032,
        bombOutProbability: 0.0001,
      },
      QUICK_SIMULATIONS,
    ),
  ).toBe(
    "Simulation error: about ±0.8 kg on the total, ±0.3% on the goal chance, and ±0.1% on the bomb-out chance. Tick Precise to narrow it.",
  );
  expect(
    simulationError(
      { expectedTotalKg: 0.1, bombOutProbability: 0.002 },
      PRECISE_SIMULATIONS,
    ),
  ).toBe(
    "Simulation error: about ±0.1 kg on the total and ±0.2% on the bomb-out chance.",
  );
});
