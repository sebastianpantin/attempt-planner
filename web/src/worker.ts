import init from "../pkg/attempt_sim_wasm.js";
import { answer, failed, type Job } from "./engine.ts";

const ready = init();

addEventListener("message", (event: MessageEvent<Job>) => {
  ready
    .then(() => answer(event.data), failed)
    .then((reply) => postMessage(reply));
});
