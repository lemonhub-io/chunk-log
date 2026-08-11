import init, { benchmark_trial, verify_batch_semantics } from "./pkg/chunklog_opfs_benchmark.js";

const initialized = init();
initialized.then(() => self.postMessage({ progress: true, stage: "wasm-init-complete" }));

self.onmessage = async (event) => {
  await initialized;
  const { scenarios, runId } = event.data;
  const root = await navigator.storage.getDirectory();
  const samples = [];

  try {
    self.postMessage({ progress: true, stage: "opfs-root-open" });
    for await (const [name] of root.entries()) {
      if (name.startsWith("chunklog-opfs-")) await root.removeEntry(name);
    }
    const semanticsFile = `chunklog-opfs-${runId}-semantics.log`;
    await verify_batch_semantics(semanticsFile);
    await root.removeEntry(semanticsFile);
    self.postMessage({ progress: true, stage: "batch-semantics-verified" });
    for (const scenario of scenarios) {
      for (let iteration = -1; iteration < scenario.iterations; iteration += 1) {
        const warmup = iteration < 0;
        const suffix = warmup ? "warmup" : String(iteration);
        const fileName = `chunklog-opfs-${runId}-${scenario.name}-${suffix}.log`;
        self.postMessage({ progress: true, stage: "trial-start", scenario: scenario.name, iteration });
        const json = await benchmark_trial(
          fileName,
          scenario.objectCount,
          scenario.payloadSize,
          scenario.batched,
        );
        self.postMessage({ progress: true, stage: "trial-finished", scenario: scenario.name, iteration });
        const result = JSON.parse(json);
        const handle = await root.getFileHandle(fileName);
        result.physical_bytes = (await handle.getFile()).size;
        result.scenario = scenario.name;
        result.iteration = iteration;
        result.warmup = warmup;
        await root.removeEntry(fileName);
        self.postMessage({ progress: true, sample: result });
        if (!warmup) samples.push(result);
      }
    }
    self.postMessage({ ok: true, samples });
  } catch (error) {
    self.postMessage({
      ok: false,
      error: error instanceof Error ? `${error.name}: ${error.message}\n${error.stack ?? ""}` : String(error),
    });
  }
};
