import {
  type ChangesResponse,
  estimateMaxes,
  type MaxEstimate,
  type MaxEstimateInput,
  PlanSearch,
  replan,
  type ReplanRequest,
  type ReplanResponse,
  type SearchProgress,
  type SearchRequest,
  singleChanges,
  evaluate,
  type EvaluateRequest,
  type EvaluateResponse,
  tradeoff,
  type TradeoffRequest,
  type TradeoffResponse,
} from "../pkg/attempt_sim_wasm.js";

export type Failed = { kind: "failed"; message: string };

export interface Jobs {
  evaluate: { request: EvaluateRequest; response: EvaluateResponse };
  tradeoff: { request: TradeoffRequest; response: TradeoffResponse };
  changes: { request: SearchRequest; response: ChangesResponse };
  replan: { request: ReplanRequest; response: ReplanResponse };
  estimate: {
    request: MaxEstimateInput;
    response: { kind: "estimated"; estimate: MaxEstimate };
  };
}

export type Reply<K extends keyof Jobs> = Jobs[K]["response"] | Failed;

export type Job =
  | {
      [K in keyof Jobs]: { kind: K; request: Jobs[K]["request"] };
    }[keyof Jobs]
  | { kind: "search"; request: SearchRequest };

export type SearchReply =
  { kind: "progress"; progress: SearchProgress } | Failed;

export const failed = (error: unknown): Failed => ({
  kind: "failed",
  message: error instanceof Error ? error.message : String(error),
});

export function answer(
  job: Extract<Job, { kind: keyof Jobs }>,
): Reply<keyof Jobs> {
  try {
    switch (job.kind) {
      case "evaluate":
        return evaluate(job.request);
      case "tradeoff":
        return tradeoff(job.request);
      case "changes":
        return singleChanges(job.request);
      case "replan":
        return replan(job.request);
      case "estimate":
        return { kind: "estimated", estimate: estimateMaxes(job.request) };
    }
  } catch (error) {
    return failed(error);
  }
}

/** Runs a search one step at a time, yielding its progress after every step. */
export function* search(request: SearchRequest): Generator<SearchReply> {
  let planSearch: PlanSearch;
  try {
    planSearch = new PlanSearch(request);
  } catch (error) {
    yield failed(error);
    return;
  }
  try {
    for (;;) {
      const progress = planSearch.step();
      yield { kind: "progress", progress };
      if (progress.done) return;
    }
  } catch (error) {
    yield failed(error);
  } finally {
    planSearch.free();
  }
}
