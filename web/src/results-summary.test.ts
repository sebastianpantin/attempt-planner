import { expect, it } from "vitest";
import { resultsSummary } from "./results-summary.ts";

const evaluation = {
  expectedTotalKg: 553.21,
  goalProbability: 0.7812 as number | undefined,
  bombOutProbability: 0.0484,
};

it("sums up the average total, goal chance and bomb-out chance", () => {
  expect(resultsSummary(evaluation)).toEqual([
    "Average 553.2 kg",
    "Goal 78.1%",
    "Bomb-out 4.8%",
  ]);
});

it("leaves out the goal chance without a goal", () => {
  expect(resultsSummary({ ...evaluation, goalProbability: undefined })).toEqual(
    ["Average 553.2 kg", "Bomb-out 4.8%"],
  );
});
