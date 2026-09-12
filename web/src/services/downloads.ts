import { zipSync } from "fflate";
import type { CompressionJob } from "../types";

export function outputName(job: CompressionJob): string {
  return job.file.name.replace(/(\.[^.]+)$/u, ".squeezed$1");
}

export function downloadJob(job: CompressionJob): void {
  if (!job.output) return;
  download(new Blob([job.output as BlobPart], { type: job.file.type }), outputName(job));
}

export function downloadReport(job: CompressionJob): void {
  if (!job.report) return;
  const report = JSON.stringify({ file: job.file.name, profile: job.profile, ...job.report }, null, 2);
  download(new Blob([report], { type: "application/json" }), `${outputName(job)}.json`);
}

export function downloadZip(jobs: readonly CompressionJob[]): void {
  const entries: Record<string, Uint8Array> = {};
  for (const job of jobs) {
    if (job.output) entries[uniqueName(entries, outputName(job))] = job.output;
  }
  if (Object.keys(entries).length === 0) return;
  const archive = zipSync(entries, { level: 6 });
  download(new Blob([archive as BlobPart], { type: "application/zip" }), "squeeze-images.zip");
}

function uniqueName(entries: Record<string, Uint8Array>, proposed: string): string {
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
