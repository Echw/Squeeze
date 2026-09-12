import type { CompressionJob, OptimizationReport } from "../types";

export function outputName(job: CompressionJob): string {
  const extension = extensionFor(job.report?.outputFormat);
  const basename = job.file.name.replace(/\.[^.]+$/u, "");
  return `${basename}.squeezed.${extension}`;
}

export function downloadJob(job: CompressionJob): void {
  if (!job.output) return;
  download(new Blob([job.output as BlobPart], { type: mimeFor(job.report?.outputFormat) }), outputName(job));
}

function extensionFor(format: OptimizationReport["outputFormat"] | undefined): string {
  return ({ jpeg: "jpg", png: "png", webp: "webp", avif: "avif" } as const)[format ?? "jpeg"];
}

function mimeFor(format: OptimizationReport["outputFormat"] | undefined): string {
  return ({ jpeg: "image/jpeg", png: "image/png", webp: "image/webp", avif: "image/avif" } as const)[format ?? "jpeg"];
}

export function downloadReport(job: CompressionJob): void {
  if (!job.report) return;
  const report = JSON.stringify({ file: job.file.name, profile: job.profile, ...job.report }, null, 2);
  download(new Blob([report], { type: "application/json" }), `${outputName(job)}.json`);
}

export async function downloadZip(jobs: readonly CompressionJob[]): Promise<void> {
  const entries: Record<string, ArrayBuffer> = {};
  for (const job of jobs) {
    if (job.output) {
      entries[uniqueName(entries, outputName(job))] = job.output.buffer.slice(
        job.output.byteOffset,
        job.output.byteOffset + job.output.byteLength,
      ) as ArrayBuffer;
    }
  }
  if (Object.keys(entries).length === 0) return;
  const worker = new Worker(new URL("../worker/archive.worker.ts", import.meta.url), { type: "module", name: "squeeze-archive" });
  try {
    const archive = await new Promise<ArrayBuffer>((resolve, reject) => {
      worker.onmessage = (event: MessageEvent<{ ok: boolean; buffer?: ArrayBuffer; message?: string }>) => {
        if (event.data.ok && event.data.buffer) resolve(event.data.buffer);
        else reject(new Error(event.data.message ?? "Nie udało się utworzyć ZIP."));
      };
      worker.onerror = () => reject(new Error("Worker archiwum uległ awarii."));
      const transfers = Object.values(entries);
      worker.postMessage({ entries }, { transfer: transfers });
    });
    download(new Blob([archive], { type: "application/zip" }), "squeeze-images.zip");
  } finally {
    worker.terminate();
  }
}

function uniqueName(entries: Record<string, unknown>, proposed: string): string {
  if (!(proposed in entries)) return proposed;
  const dot = proposed.lastIndexOf(".");
  const base = dot > 0 ? proposed.slice(0, dot) : proposed;
  const extension = dot > 0 ? proposed.slice(dot) : "";
  let index = 2;
  while (`${base}-${index}${extension}` in entries) index += 1;
  return `${base}-${index}${extension}`;
}

function download(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}
