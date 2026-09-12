import type { CompressionJob } from "../types";

export function openComparison(job: CompressionJob, returnFocus?: HTMLElement): void {
  if (!job.output || !job.report) return;
  const template = document.querySelector<HTMLTemplateElement>("#compare-template")!;
  const dialog = template.content.firstElementChild!.cloneNode(true) as HTMLDialogElement;
  const originalUrl = URL.createObjectURL(job.file);
  const resultUrl = URL.createObjectURL(new Blob([job.output as BlobPart], { type: mimeFor(job.report.outputFormat) }));
  const before = document.createElement("img");
  const after = document.createElement("img");
  before.alt = "Oryginał";
  after.alt = "Po kompresji";
  before.draggable = false;
  after.draggable = false;
  dialog.querySelector("[data-before]")!.replaceWith(before);
  dialog.querySelector("[data-after]")!.append(after);
  const stage = dialog.querySelector<HTMLElement>("[data-stage]")!;
  const content = dialog.querySelector<HTMLElement>("[data-content]")!;
  const divider = dialog.querySelector<HTMLElement>("[data-divider]")!;
  const zoom = dialog.querySelector<HTMLInputElement>("[data-zoom]")!;
  const output = dialog.querySelector<HTMLOutputElement>("output")!;
  dialog.querySelector("h2")!.textContent = job.file.name;
  dialog.querySelector("header p")!.textContent = `${format(job.file.size)} → ${format(job.report.optimizedSize)}`;
  before.src = originalUrl;
  after.src = resultUrl;

  const setPosition = (value: number) => {
    const bounded = Math.max(0, Math.min(100, Math.round(value)));
    dialog.style.setProperty("--compare", `${bounded}%`);
    divider.setAttribute("aria-valuenow", String(bounded));
  };
  const fromPointer = (clientX: number) => {
    const bounds = stage.getBoundingClientRect();
    setPosition(((clientX - bounds.left) / bounds.width) * 100);
  };
  const setZoom = (value: number) => {
    dialog.style.setProperty("--zoom", String(value / 100));
    output.value = `${value}%`;
  };

  divider.addEventListener("pointerdown", (event) => { divider.setPointerCapture(event.pointerId); fromPointer(event.clientX); });
  divider.addEventListener("pointermove", (event) => { if (divider.hasPointerCapture(event.pointerId)) fromPointer(event.clientX); });
  divider.addEventListener("keydown", (event) => {
    const delta = event.key === "ArrowLeft" ? -2 : event.key === "ArrowRight" ? 2 : 0;
    if (!delta) return;
    event.preventDefault();
    setPosition(Number(divider.getAttribute("aria-valuenow")) + delta);
  });
  zoom.addEventListener("input", () => setZoom(Number(zoom.value)));
  dialog.querySelector("[data-actual]")!.addEventListener("click", () => { zoom.value = "100"; setZoom(100); });
  dialog.querySelector("[data-close]")!.addEventListener("click", () => dialog.close());
  dialog.addEventListener("click", (event) => { if (event.target === dialog) dialog.close(); });
  dialog.addEventListener("close", () => {
    URL.revokeObjectURL(originalUrl);
    URL.revokeObjectURL(resultUrl);
    dialog.remove();
    returnFocus?.focus();
  }, { once: true });

  content.addEventListener("pointerdown", (event) => {
    if (Number(zoom.value) === 100 || (event.target as Element).closest("[data-divider]")) return;
    const startX = event.clientX;
    const startY = event.clientY;
    const panX = Number(dialog.dataset.panX ?? 0);
    const panY = Number(dialog.dataset.panY ?? 0);
    content.setPointerCapture(event.pointerId);
    const move = (next: PointerEvent) => {
      const x = panX + next.clientX - startX;
      const y = panY + next.clientY - startY;
      dialog.dataset.panX = String(x);
      dialog.dataset.panY = String(y);
      dialog.style.setProperty("--pan-x", `${x}px`);
      dialog.style.setProperty("--pan-y", `${y}px`);
    };
    content.addEventListener("pointermove", move);
    const finish = () => {
      content.removeEventListener("pointermove", move);
      content.removeEventListener("pointerup", finish);
      content.removeEventListener("pointercancel", finish);
      content.removeEventListener("lostpointercapture", finish);
    };
    content.addEventListener("pointerup", finish);
    content.addEventListener("pointercancel", finish);
    content.addEventListener("lostpointercapture", finish);
  });

  document.body.append(dialog);
  dialog.showModal();
}

function mimeFor(format: string): string { return `image/${format === "jpeg" ? "jpeg" : format}`; }
function format(value: number): string { return value < 1024 * 1024 ? `${(value / 1024).toFixed(1)} KB` : `${(value / 1024 / 1024).toFixed(2)} MB`; }
