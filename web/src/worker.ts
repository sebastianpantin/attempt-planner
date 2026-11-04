import init from "../pkg/attempt_sim_wasm.js";
import { answer, failed, type Job, search } from "./engine.ts";

const ready = init();

addEventListener("message", (event: MessageEvent<Job>) => {
  const job = event.data;
  ready.then(
    () => {
      if (job.kind === "search") {
        for (const reply of search(job.request)) postMessage(reply);
      } else {
        postMessage(answer(job));
      }
    },
    (error) => postMessage(failed(error)),
  );
});
