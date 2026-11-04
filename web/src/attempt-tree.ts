import type { LiftPlanKg } from "../pkg/attempt_sim_wasm.js";

export interface AttemptNode {
  weightKg: number;
  afterMake?: AttemptNode;
  afterMiss?: AttemptNode;
}

/** Every attempt the plan can lead to, or `undefined` while a weight is missing. */
export function attemptTree(plan: LiftPlanKg): AttemptNode | undefined {
  const { second, third } = plan;
  const weights = [plan.openerKg, second.afterMakeKg, second.afterMissKg];
  if (
    ![...weights, third.afterMakeKg, third.afterMissKg].every(Number.isFinite)
  ) {
    return undefined;
  }
  const thirds = (secondKg: number): AttemptNode => ({
    weightKg: secondKg,
    afterMake: { weightKg: secondKg + third.afterMakeKg },
    afterMiss: { weightKg: secondKg + third.afterMissKg },
  });
  return {
    weightKg: plan.openerKg,
    afterMake: thirds(plan.openerKg + second.afterMakeKg),
    afterMiss: thirds(plan.openerKg + second.afterMissKg),
  };
}
