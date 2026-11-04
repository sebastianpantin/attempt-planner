import { expect, it } from "vitest";
import {
  finePercent,
  headlines,
  jump,
  kg,
  listOf,
  percent,
  roundChanceUp,
  simulationError,
  wholePercent,
} from "./format.ts";
import { PRECISE_SIMULATIONS, QUICK_SIMULATIONS } from "./inputs.ts";

it("shows kg with at most one decimal, without a needless .0", () => {
  expect(kg(200)).toBe("200 kg");
  expect(kg(195)).toBe("195 kg");
  expect(kg(202.5)).toBe("202.5 kg");
  expect(kg(512.3456)).toBe("512.3 kg");
  expect(kg(1002.5)).toBe("1,002.5 kg");
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

it("signs a jump, and reads a jump of 0 after a miss as a repeat", () => {
  expect(jump(12.5, false)).toBe("+12.5");
  expect(jump(5, true)).toBe("+5");
  expect(jump(0, true)).toBe("repeat");
  expect(jump(0, false)).toBe("0");
});

it("rounds a whole percent", () => {
  expect(wholePercent(0.914)).toBe("91%");
});

it("rounds a chance up to the next round one", () => {
  expect(roundChanceUp(0)).toBe(0);
  expect(roundChanceUp(0.03)).toBe(0.05);
  expect(roundChanceUp(0.1 + 1e-12)).toBe(0.1);
  expect(roundChanceUp(0.6)).toBe(1);
});
