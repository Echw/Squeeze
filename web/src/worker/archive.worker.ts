/// <reference lib="webworker" />

import { zipSync } from "fflate";

interface ArchiveRequest {
  entries: Record<string, ArrayBuffer>;
}

self.onmessage = (event: MessageEvent<ArchiveRequest>) => {
  const entries = Object.fromEntries(
    Object.entries(event.data.entries).map(([name, buffer]) => [name, new Uint8Array(buffer)]),
  );
  try {
    const archive = zipSync(entries, { level: 6 });
    const buffer = archive.buffer.slice(archive.byteOffset, archive.byteOffset + archive.byteLength) as ArrayBuffer;
    self.postMessage({ ok: true, buffer }, { transfer: [buffer] });
  } catch (error) {
    self.postMessage({ ok: false, message: error instanceof Error ? error.message : String(error) });
  }
};

export {};
