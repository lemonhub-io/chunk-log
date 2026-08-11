const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");

const playwrightPath = process.env.PLAYWRIGHT_CORE_PATH || "playwright-core";
const { chromium } = require(playwrightPath);

const root = __dirname;
const outputPath = path.resolve(process.argv[2] || path.join(root, "opfs-results.json"));
const progressPath = `${outputPath}.progress.log`;
const browserExecutable = process.env.OPFS_BROWSER_EXE;
if (!browserExecutable) throw new Error("OPFS_BROWSER_EXE is required");

const mime = new Map([
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".mjs", "text/javascript; charset=utf-8"],
  [".wasm", "application/wasm"],
]);

const server = http.createServer((request, response) => {
  const pathname = new URL(request.url, "http://127.0.0.1").pathname;
  const relative = pathname === "/" ? "index.html" : decodeURIComponent(pathname.slice(1));
  const filePath = path.resolve(root, relative);
  if (!filePath.startsWith(`${root}${path.sep}`) && filePath !== path.join(root, "index.html")) {
    response.writeHead(403).end();
    return;
  }
  fs.readFile(filePath, (error, bytes) => {
    if (error) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, {
      "content-type": mime.get(path.extname(filePath)) || "application/octet-stream",
      "cache-control": "no-store",
    });
    response.end(bytes);
  });
});

function quantile(sorted, fraction) {
  const position = (sorted.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  if (lower === upper) return sorted[lower];
  return sorted[lower] + (sorted[upper] - sorted[lower]) * (position - lower);
}

function summarize(samples) {
  const metrics = ["open_empty_ms", "import_ms", "reopen_ms", "read_all_ms", "physical_bytes"];
  const summaries = {};
  for (const scenario of [...new Set(samples.map((sample) => sample.scenario))]) {
    const selected = samples.filter((sample) => sample.scenario === scenario);
    summaries[scenario] = { samples: selected.length };
    for (const metric of metrics) {
      const values = selected.map((sample) => sample[metric]).sort((a, b) => a - b);
      summaries[scenario][metric] = {
        median: quantile(values, 0.5),
        p25: quantile(values, 0.25),
        p75: quantile(values, 0.75),
      };
    }
  }
  return summaries;
}

const scenarios = process.env.OPFS_SMOKE ? [
  { name: "smoke-n1", objectCount: 1, payloadSize: 256, batched: true, iterations: 1 },
] : [
  { name: "batched-n100", objectCount: 100, payloadSize: 256, batched: true, iterations: 9 },
  { name: "unbatched-n100", objectCount: 100, payloadSize: 256, batched: false, iterations: 9 },
  { name: "batched-n1000", objectCount: 1000, payloadSize: 256, batched: true, iterations: 7 },
  { name: "unbatched-n1000", objectCount: 1000, payloadSize: 256, batched: false, iterations: 7 },
  { name: "batched-n10000", objectCount: 10000, payloadSize: 256, batched: true, iterations: 5 },
];

(async () => {
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = server.address().port;
  const browser = await chromium.launch({ executablePath: browserExecutable, headless: true });
  try {
    const page = await browser.newPage();
    fs.rmSync(progressPath, { force: true });
    page.on("console", (message) => {
      const line = `${new Date().toISOString()} ${message.text()}\n`;
      fs.appendFileSync(progressPath, line);
      process.stderr.write(line);
    });
    await page.goto(`http://127.0.0.1:${port}/`, { waitUntil: "load" });
    const environment = await page.evaluate(async () => ({
      userAgent: navigator.userAgent,
      hardwareConcurrency: navigator.hardwareConcurrency,
      storageEstimate: await navigator.storage.estimate(),
    }));
    const runId = `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    const samples = await page.evaluate(
      ({ scenarios, runId }) => window.runOpfsBenchmark({ scenarios, runId }),
      { scenarios, runId },
    );
    const report = {
      schema: 1,
      implementation: "coalesced-write-log-cache",
      batch_semantics_verified: true,
      generated_at: new Date().toISOString(),
      environment,
      scenarios,
      summaries: summarize(samples),
      samples,
    };
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
    fs.rmSync(progressPath, { force: true });
    process.stdout.write(`${JSON.stringify(report.summaries, null, 2)}\n`);
  } finally {
    await browser.close();
    server.close();
  }
})().catch((error) => {
  server.close();
  console.error(error);
  process.exitCode = 1;
});
