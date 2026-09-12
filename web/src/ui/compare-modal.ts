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
        <div><p class="eyebrow">Porównanie 1:1</p><h2></h2></div>
        <button class="icon-button" type="button" data-close aria-label="Zamknij porównanie">${closeIcon()}</button>
      </header>
      <div class="compare-canvas" data-compare aria-label="Porównanie obrazu przed i po kompresji">
        <div class="compare-content" data-content>
          <img data-before alt="Oryginalny obraz" draggable="false" />
          <div class="compare-after" data-after><img alt="Obraz po kompresji" draggable="false" /></div>
          <div class="compare-divider" data-divider role="slider" tabindex="0" aria-label="Pozycja porównania" aria-valuemin="0" aria-valuemax="100" aria-valuenow="50"><span aria-hidden="true">↔</span></div>
          <span class="compare-label compare-label--before">Oryginał</span><span class="compare-label compare-label--after">Po kompresji</span>
          <canvas class="difference-canvas" data-difference hidden></canvas>
        </div>
      </div>
      <div class="compare-controls"><label class="zoom-control"><span>Powiększenie</span><input data-zoom type="range" min="100" max="400" value="100" /><output data-zoom-output>100%</output></label></div>
      <footer class="compare-footer"><p data-diff-summary>Przeciągnij obraz przy powiększeniu. Różnice pojawiają się jako nakładka na właściwy obraz.</p><button class="button button--secondary" type="button" data-diff>Pokaż różnice</button></footer>
    </div>`;
  dialog.querySelector("h2")!.textContent = job.file.name;
  const before = dialog.querySelector<HTMLImageElement>("[data-before]")!;
  const after = dialog.querySelector<HTMLImageElement>("[data-after] img")!;
  const canvas = dialog.querySelector<HTMLCanvasElement>("[data-difference]")!;
  const compare = dialog.querySelector<HTMLElement>("[data-compare]")!;
  const content = dialog.querySelector<HTMLElement>("[data-content]")!;
  const divider = dialog.querySelector<HTMLElement>("[data-divider]")!;
  const differenceSummary = dialog.querySelector<HTMLElement>("[data-diff-summary]")!;
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
  dialog.addEventListener("click", (event) => { if (event.target === dialog) dialog.close(); });
  const setComparePosition = (clientX: number) => {
    const bounds = compare.getBoundingClientRect();
    const value = Math.round(Math.max(0, Math.min(100, ((clientX - bounds.left) / bounds.width) * 100)));
    dialog.style.setProperty("--compare", `${value}%`);
    divider.setAttribute("aria-valuenow", String(value));
  };
  divider.addEventListener("pointerdown", (event) => {
    event.preventDefault();
    event.stopPropagation();
    divider.setPointerCapture(event.pointerId);
    setComparePosition(event.clientX);
  });
  divider.addEventListener("pointermove", (event) => {
    if (divider.hasPointerCapture(event.pointerId)) setComparePosition(event.clientX);
  });
  divider.addEventListener("keydown", (event) => {
    const current = Number(divider.getAttribute("aria-valuenow") ?? 50);
    const delta = event.key === "ArrowLeft" ? -2 : event.key === "ArrowRight" ? 2 : 0;
    if (!delta) return;
    event.preventDefault();
    const bounds = compare.getBoundingClientRect();
    setComparePosition(bounds.left + (Math.max(0, Math.min(100, current + delta)) / 100) * bounds.width);
  });
  const zoom = dialog.querySelector<HTMLInputElement>("[data-zoom]")!;
  const zoomOutput = dialog.querySelector<HTMLOutputElement>("[data-zoom-output]")!;
  let panX = 0;
  let panY = 0;
  const setTransform = () => {
    content.style.setProperty("--zoom", `${Number(zoom.value) / 100}`);
    content.style.setProperty("--pan-x", `${panX}px`);
    content.style.setProperty("--pan-y", `${panY}px`);
    zoomOutput.value = `${zoom.value}%`;
    zoomOutput.textContent = zoomOutput.value;
  };
  zoom.addEventListener("input", setTransform);
  compare.addEventListener("wheel", (event) => {
    if (!event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
    zoom.value = String(Math.max(100, Math.min(400, Number(zoom.value) + (event.deltaY < 0 ? 10 : -10))));
    setTransform();
  }, { passive: false });
  let drag: { x: number; y: number; panX: number; panY: number } | undefined;
  compare.addEventListener("pointerdown", (event) => {
    if (Number(zoom.value) === 100) return;
    drag = { x: event.clientX, y: event.clientY, panX, panY };
    compare.setPointerCapture(event.pointerId);
  });
  compare.addEventListener("pointermove", (event) => {
    if (!drag) return;
    panX = drag.panX + event.clientX - drag.x;
    panY = drag.panY + event.clientY - drag.y;
    setTransform();
  });
  compare.addEventListener("pointerup", () => { drag = undefined; });
  dialog.querySelector<HTMLButtonElement>("[data-diff]")!.addEventListener("click", async (event) => {
    const button = event.currentTarget as HTMLButtonElement;
    if (!canvas.hidden) {
      canvas.hidden = true;
      canvas.width = 1;
      canvas.height = 1;
      differenceSummary.textContent = "Przeciągnij obraz przy powiększeniu. Różnice pojawiają się jako nakładka na właściwy obraz.";
      button.textContent = "Pokaż różnice";
      return;
    }
    button.disabled = true;
    button.textContent = "Generowanie…";
    try {
      const difference = await renderDifference(job.file, new Blob([job.output as BlobPart], { type: job.file.type }));
      canvas.width = difference.bitmap.width;
      canvas.height = difference.bitmap.height;
      canvas.getContext("2d")!.drawImage(difference.bitmap, 0, 0);
      difference.bitmap.close();
      canvas.hidden = false;
      const changedPercent = difference.totalPixels ? (difference.changedPixels / difference.totalPixels) * 100 : 0;
      differenceSummary.textContent = difference.changedPixels === 0
        ? "Brak różnic pikseli — wynik jest bezstratny."
        : `Żółty i czerwony zaznaczają zmienione piksele: ${changedPercent.toFixed(changedPercent < 1 ? 2 : 1)}%. Najsilniejsza zmiana kanału: ${difference.maxDelta}/255.`;
      button.textContent = "Ukryj różnice";
    } finally { button.disabled = false; }
  });
  setTransform();
  dialog.showModal();
}

interface DifferenceResult {
  bitmap: ImageBitmap;
  changedPixels: number;
  totalPixels: number;
  maxDelta: number;
}

async function renderDifference(before: Blob, after: Blob): Promise<DifferenceResult> {
  const worker = new Worker(new URL("../worker/difference.worker.ts", import.meta.url), { type: "module", name: "squeeze-difference" });
  try {
    return await new Promise<DifferenceResult>((resolve, reject) => {
      worker.onmessage = (event: MessageEvent<{ ok: boolean; bitmap?: ImageBitmap; changedPixels?: number; totalPixels?: number; maxDelta?: number; message?: string }>) => {
        if (event.data.ok && event.data.bitmap) resolve({ bitmap: event.data.bitmap, changedPixels: event.data.changedPixels ?? 0, totalPixels: event.data.totalPixels ?? 0, maxDelta: event.data.maxDelta ?? 0 });
        else reject(new Error(event.data.message ?? "Nie udało się utworzyć mapy różnic."));
      };
      worker.onerror = () => reject(new Error("Worker porównania uległ awarii."));
      worker.postMessage({ before, after, maxSide: 1600 });
    });
  } finally { worker.terminate(); }
}

function closeIcon(): string {
  return `<svg aria-hidden="true" viewBox="0 0 24 24" width="20" height="20"><path d="m6 6 12 12M18 6 6 18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>`;
}
