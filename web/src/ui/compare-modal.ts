import type { CompressionJob } from "../types";

export async function openComparison(job: CompressionJob): Promise<void> {
  if (!job.output) return;
  const originalUrl = URL.createObjectURL(job.file);
  const optimizedUrl = URL.createObjectURL(new Blob([job.output as BlobPart], { type: job.file.type }));
  const dialog = document.createElement("dialog");
  dialog.className = "compare-dialog";
  dialog.innerHTML = `
    <div class="compare-shell">
      <header class="compare-header">
        <div>
          <p class="eyebrow">Porównanie</p>
          <h2></h2>
        </div>
        <button class="icon-button" type="button" data-close aria-label="Zamknij porównanie">${closeIcon()}</button>
      </header>
      <div class="compare-canvas" data-compare>
        <img data-before alt="Oryginalny obraz" draggable="false" />
        <div class="compare-after" data-after><img alt="Obraz po kompresji" draggable="false" /></div>
        <div class="compare-divider" data-divider><span aria-hidden="true">↔</span></div>
        <span class="compare-label compare-label--before">Oryginał</span>
        <span class="compare-label compare-label--after">Po kompresji</span>
        <canvas class="difference-canvas" data-difference hidden></canvas>
      </div>
      <label class="compare-range-label">
        <span class="sr-only">Pozycja suwaka porównania</span>
        <input data-range type="range" min="0" max="100" value="50" />
      </label>
      <footer class="compare-footer">
        <p>Przeciągnij suwak, aby sprawdzić detale. Mapa różnic działa na podglądzie.</p>
        <button class="button button--secondary" type="button" data-diff>Pokaż mapę różnic</button>
      </footer>
    </div>
  `;
  dialog.querySelector("h2")!.textContent = job.file.name;
  const before = dialog.querySelector<HTMLImageElement>("[data-before]")!;
  const after = dialog.querySelector<HTMLImageElement>("[data-after] img")!;
  before.src = originalUrl;
  after.src = optimizedUrl;
  document.body.append(dialog);

  const cleanup = () => {
    URL.revokeObjectURL(originalUrl);
    URL.revokeObjectURL(optimizedUrl);
    dialog.remove();
  };
  dialog.addEventListener("close", cleanup, { once: true });
  dialog.querySelector("[data-close]")!.addEventListener("click", () => dialog.close());
  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) dialog.close();
  });
  dialog.querySelector<HTMLInputElement>("[data-range]")!.addEventListener("input", (event) => {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    dialog.style.setProperty("--compare", `${value}%`);
  });
  dialog.querySelector<HTMLButtonElement>("[data-diff]")!.addEventListener("click", async (event) => {
    const button = event.currentTarget as HTMLButtonElement;
    const canvas = dialog.querySelector<HTMLCanvasElement>("[data-difference]")!;
    if (!canvas.hidden) {
      canvas.hidden = true;
      canvas.width = 1;
      canvas.height = 1;
      button.textContent = "Pokaż mapę różnic";
      return;
    }
    button.disabled = true;
    button.textContent = "Generowanie…";
    try {
      await renderDifference(job.file, new Blob([job.output as BlobPart], { type: job.file.type }), canvas);
      canvas.hidden = false;
      button.textContent = "Ukryj mapę różnic";
    } finally {
      button.disabled = false;
    }
  });
  dialog.showModal();
}

async function renderDifference(before: Blob, after: Blob, canvas: HTMLCanvasElement): Promise<void> {
  const [source, compressed] = await Promise.all([createImageBitmap(before), createImageBitmap(after)]);
  const scale = Math.min(1, 1600 / Math.max(source.width, source.height));
  const width = Math.max(1, Math.round(source.width * scale));
  const height = Math.max(1, Math.round(source.height * scale));
  const scratch = document.createElement("canvas");
  scratch.width = width;
  scratch.height = height;
  const context = scratch.getContext("2d", { willReadFrequently: true })!;
  context.drawImage(source, 0, 0, width, height);
  const a = context.getImageData(0, 0, width, height);
  context.clearRect(0, 0, width, height);
  context.drawImage(compressed, 0, 0, width, height);
  const b = context.getImageData(0, 0, width, height);
  const output = context.createImageData(width, height);
  for (let index = 0; index < output.data.length; index += 4) {
    const difference = Math.min(
      255,
      (Math.abs(a.data[index]! - b.data[index]!) +
        Math.abs(a.data[index + 1]! - b.data[index + 1]!) +
        Math.abs(a.data[index + 2]! - b.data[index + 2]!)) *
        3,
    );
    output.data[index] = difference;
    output.data[index + 1] = Math.max(0, difference - 80);
    output.data[index + 2] = 255 - difference;
    output.data[index + 3] = 255;
  }
  canvas.width = width;
  canvas.height = height;
  canvas.getContext("2d")!.putImageData(output, 0, 0);
  source.close();
  compressed.close();
}

function closeIcon(): string {
  return `<svg aria-hidden="true" viewBox="0 0 24 24" width="20" height="20"><path d="m6 6 12 12M18 6 6 18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>`;
}

