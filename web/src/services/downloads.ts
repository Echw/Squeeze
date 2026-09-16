import { zipSync } from "fflate";

import type { CompressionJob, OptimizationReport } from "../types";

export function outputName(job: CompressionJob): string {
  const basename = job.file.name.replace(/\.[^.]+$/u, "");
  return `${basename}.squeezed.${extensionFor(job.report?.outputFormat)}`;
}

export function downloadJob(job: CompressionJob): void {
  if (!job.output) return;
  download(new Blob([job.output as BlobPart], { type: mimeFor(job.report?.outputFormat) }), outputName(job));
}

function extensionFor(format: OptimizationReport["outputFormat"] | undefined): string {
  return ({ jpeg: "jpg", png: "png" } as const)[format ?? "jpeg"];
}

function mimeFor(format: OptimizationReport["outputFormat"] | undefined): string {
  return ({ jpeg: "image/jpeg", png: "image/png" } as const)[format ?? "jpeg"];
}

export async function downloadZip(jobs: readonly CompressionJob[]): Promise<void> {
  const entries: Record<string, Uint8Array> = {};
  for (const job of jobs) {
    if (job.output) entries[uniqueName(entries, outputName(job))] = job.output.slice();
  }
  if (Object.keys(entries).length === 0) return;
  const archive = archiveEntries(entries);
  const bytes = archive.buffer.slice(archive.byteOffset, archive.byteOffset + archive.byteLength) as ArrayBuffer;
  download(new Blob([bytes], { type: "application/zip" }), "squeeze-images.zip");
}

// The ZIP is deliberately built only after the user asks for it. Keeping this
// small operation local avoids a second worker and makes downloads reliable.
export function archiveEntries(entries: Record<string, Uint8Array>): Uint8Array {
  return zipSync(entries, { level: 6 });
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
