import {
  evaluate,
  type EvaluateRequest,
  type EvaluateResponse,
  tradeoff,
  type TradeoffRequest,
  type TradeoffResponse,
} from "../pkg/attempt_sim_wasm.js";

export type Failed = { kind: "failed"; message: string };

/** The request and response of each kind of job the worker runs. */
export interface Jobs {
  evaluate: { request: EvaluateRequest; response: EvaluateResponse };
  tradeoff: { request: TradeoffRequest; response: TradeoffResponse };
}

export type Reply<K extends keyof Jobs> = Jobs[K]["response"] | Failed;

export type Job = {
  [K in keyof Jobs]: { kind: K; request: Jobs[K]["request"] };
}[keyof Jobs];

export const failed = (error: unknown): Failed => ({
  kind: "failed",
  message: error instanceof Error ? error.message : String(error),
});

export function answer(job: Job): Reply<keyof Jobs> {
  try {
    return job.kind === "evaluate"
      ? evaluate(job.request)
      : tradeoff(job.request);
  } catch (error) {
    return failed(error);
  }
}
