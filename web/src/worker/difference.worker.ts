/// <reference lib="webworker" />

interface DifferenceRequest {
  before: Blob;
  after: Blob;
  maxSide: number;
}

self.onmessage = async (event: MessageEvent<DifferenceRequest>) => {
  try {
    const [source, compressed] = await Promise.all([createImageBitmap(event.data.before), createImageBitmap(event.data.after)]);
    const scale = Math.min(1, event.data.maxSide / Math.max(source.width, source.height));
    const width = Math.max(1, Math.round(source.width * scale));
    const height = Math.max(1, Math.round(source.height * scale));
    const canvas = new OffscreenCanvas(width, height);
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) throw new Error("Brak kontekstu obrazu w Workerze.");
    context.drawImage(source, 0, 0, width, height);
    const before = context.getImageData(0, 0, width, height);
    context.clearRect(0, 0, width, height);
    context.drawImage(compressed, 0, 0, width, height);
    const after = context.getImageData(0, 0, width, height);
    const result = context.createImageData(width, height);
    let changedPixels = 0;
    let maxDelta = 0;
    for (let index = 0; index < result.data.length; index += 4) {
      const delta = Math.max(
        Math.abs(before.data[index]! - after.data[index]!),
        Math.abs(before.data[index + 1]! - after.data[index + 1]!),
        Math.abs(before.data[index + 2]! - after.data[index + 2]!),
      );
      maxDelta = Math.max(maxDelta, delta);
      // A small threshold eliminates encoder noise. Transparent pixels keep the
      // comparison image visible, while yellow-to-red marks visible changes.
      if (delta < 3) continue;
      changedPixels += 1;
      result.data[index] = 255;
      result.data[index + 1] = Math.max(48, 238 - delta * 2);
      result.data[index + 2] = 0;
      result.data[index + 3] = Math.min(220, 45 + delta * 7);
    }
    context.putImageData(result, 0, 0);
    const bitmap = canvas.transferToImageBitmap();
    source.close();
    compressed.close();
    self.postMessage({ ok: true, bitmap, changedPixels, totalPixels: width * height, maxDelta }, { transfer: [bitmap] });
  } catch (error) {
    self.postMessage({ ok: false, message: error instanceof Error ? error.message : String(error) });
  }
};

export {};
