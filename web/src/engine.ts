import {
  evaluate,
  type EvaluateRequest,
  type EvaluateResponse,
} from "../pkg/attempt_sim_wasm.js";

export type Reply = EvaluateResponse | { kind: "failed"; message: string };

export const failed = (error: unknown): Reply => ({
  kind: "failed",
  message: error instanceof Error ? error.message : String(error),
});

export function answer(request: EvaluateRequest): Reply {
  try {
    return evaluate(request);
  } catch (error) {
    return failed(error);
  }
}
