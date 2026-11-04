import init, { type EvaluateRequest } from "../pkg/attempt_sim_wasm.js";
import { answer, failed } from "./engine.ts";

const ready = init();

addEventListener("message", (event: MessageEvent<EvaluateRequest>) => {
  ready
    .then(() => answer(event.data), failed)
    .then((reply) => postMessage(reply));
});
