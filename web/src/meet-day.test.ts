import { describe, expect, it } from "vitest";
import type { TakenAttempt } from "../pkg/attempt_sim_wasm.js";
import { EXAMPLE } from "./example.ts";
import { buildSearchRequest } from "./inputs.ts";
import {
  afterNext,
  basis,
  bombedOut,
  describeSetup,
  describeTaken,
  lightestNextKg,
  loadMeet,
  loadProblem,
  replanRequest,
  saveMeet,
  slotOf,
  startMeet,
  STORAGE_KEY,
  take,
  undo,
} from "./meet-day.ts";

const made = (weightKg: number): TakenAttempt => ({ weightKg, made: true });
const missed = (weightKg: number): TakenAttempt => ({ weightKg, made: false });

const squats = [made(180), missed(190), made(190)];

function memoryStorage() {
  const items = new Map<string, string>();
  return {
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
    removeItem: (key: string) => void items.delete(key),
  };
}

describe("a meet", () => {
  const meet = startMeet(buildSearchRequest(EXAMPLE));

  it("starts from the plan, goal and cap, with nothing taken", () => {
    const search = buildSearchRequest(EXAMPLE);
    expect(meet.taken).toEqual([]);
    expect(meet.setup.plans).toEqual(search.plans);
    expect(meet.setup.goalKg).toBe(search.goalKg);
    expect(meet.setup.bombOutCap).toBe(search.bombOutCap);
    expect(meet.setup).not.toHaveProperty("familyBest");
  });

  it("reads back the goal, limit and openers it starts from", () => {
    const { plans } = meet.setup;
    expect(
      describeSetup({ ...meet.setup, goalKg: 550, bombOutCap: 0.05 }),
    ).toBe(
      `Goal 550 kg, bomb-out risk up to 5%. Openers: squat ${plans.squat.openerKg}, bench ${plans.bench.openerKg}, and deadlift ${plans.deadlift.openerKg} kg.`,
    );
    const { goalKg: _, bombOutCap: __, ...open } = meet.setup;
    expect(describeSetup(open)).toMatch(
      /^No goal, so the highest average total, any bomb-out risk\./,
    );
  });

  it("asks to replan with every attempt taken, in order", () => {
    const request = replanRequest(take(take(meet, made(180)), missed(190)));
    expect(request.taken).toEqual([made(180), missed(190)]);
    expect(request.lifter).toEqual(meet.setup.lifter);
  });

  it("undoes the last attempt only", () => {
    const after = undo(take(take(meet, made(180)), missed(190)));
    expect(after.taken).toEqual([made(180)]);
    expect(undo(meet).taken).toEqual([]);
  });

  it("survives a reload", () => {
    const storage = memoryStorage();
    const progress = take(meet, made(180));
    saveMeet(storage, progress);
    expect(loadMeet(storage)).toEqual(progress);
    saveMeet(storage, undefined);
    expect(loadMeet(storage)).toBeUndefined();
  });

  it("ignores a saved meet it cannot read", () => {
    const storage = memoryStorage();
    for (const saved of [
      "{",
      "3",
      '{"setup":{}}',
      '{"setup":{},"taken":[{"weightKg":"180","made":true}]}',
    ]) {
      storage.setItem(STORAGE_KEY, saved);
      expect(loadMeet(storage)).toBeUndefined();
    }
  });
});

describe("attempts in meet order", () => {
  it("go squat, bench, deadlift, three each", () => {
    expect(slotOf(0)).toEqual({ lift: "squat", attempt: 1 });
    expect(slotOf(5)).toEqual({ lift: "bench", attempt: 3 });
    expect(slotOf(6)).toEqual({ lift: "deadlift", attempt: 1 });
    expect(slotOf(9)).toBeUndefined();
    expect(describeTaken(4, missed(125))).toBe(
      "Bench press attempt 2: 125 kg, missed",
    );
  });

  it("never get lighter, and go up after a make", () => {
    expect(lightestNextKg([])).toBe(20);
    expect(lightestNextKg([made(180)])).toBe(182.5);
    expect(lightestNextKg([missed(180)])).toBe(180);
    expect(lightestNextKg(squats)).toBe(20);
  });

  it("explain a load that breaks the rules", () => {
    expect(loadProblem(182.5, [made(180)])).toBeUndefined();
    expect(loadProblem(180, [missed(180)])).toBeUndefined();
    expect(loadProblem(180, [made(180)])).toBe(
      "At least 182.5 kg: heavier than the attempt you made.",
    );
    expect(loadProblem(177.5, [missed(180)])).toBe(
      "At least 180 kg: no lighter than the attempt before.",
    );
    expect(loadProblem(181, [])).toBe("Loads go up in 2.5 kg steps.");
    expect(loadProblem(15, [])).toBe("At least the 20 kg bar.");
    expect(loadProblem(Number.NaN, [])).toBe("Enter the load in kg.");
    expect(loadProblem(1002.5, [])).toBe("At most 1,000 kg.");
  });
});

describe("the meet so far", () => {
  it("notices a bomb-out", () => {
    const taken = [...squats, missed(120), missed(120), missed(120)];
    expect(bombedOut(taken)).toEqual(["bench"]);
    expect(bombedOut(squats)).toEqual([]);
  });
});

describe("the next attempt", () => {
  const plans = buildSearchRequest(EXAMPLE).plans;

  it("shows where a make or a miss leads", () => {
    const { second } = plans.squat;
    expect(afterNext(plans, { lift: "squat", attempt: 1, weightKg: 180 })).toBe(
      `If made, then ${180 + second.afterMakeKg} kg. If missed, then ${180 + second.afterMissKg} kg.`,
    );
  });

  it("after a lift's last attempt, names the next lift's opener", () => {
    expect(afterNext(plans, { lift: "squat", attempt: 3, weightKg: 200 })).toBe(
      `Then the bench press opens at ${plans.bench.openerKg} kg.`,
    );
    expect(
      afterNext(plans, { lift: "deadlift", attempt: 3, weightKg: 250 }),
    ).toBe("Your last attempt.");
  });

  it("says where its figures come from", () => {
    const replanned = {
      kind: "replanned" as const,
      result: { plans, bombOutProbability: 0, expectedTotalKg: 0 },
      meets: 18000,
      redrawn: false,
    };
    expect(basis(replanned, 0)).toBe("From 18,000 simulated meets.");
    expect(basis(replanned, 1)).toBe(
      "From the 18,000 simulated meets that went like yours so far.",
    );
    expect(basis({ ...replanned, meets: 20000, redrawn: true }, 2)).toBe(
      "Few simulated meets went like yours, so 20,000 were drawn to match it.",
    );
  });
});
