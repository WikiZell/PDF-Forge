import { renderPage, type PageSizePt } from "./api";

export const PRINT_MAX_PAGES = 300;
const PRINT_DPI = 150;

/**
 * Printing hands the rendered pages (PDFium bitmaps at 150 dpi) to the WebView's print
 * dialog via window.print(). Chosen over a native print job for simplicity and portability;
 * capped at PRINT_MAX_PAGES to bound memory.
 */
export async function printDocument(docId: number, sizes: PageSizePt[], pages?: number[]): Promise<void> {
  const list = pages ?? sizes.map((_, i) => i);
  if (list.length > PRINT_MAX_PAGES) {
    throw new Error(`Printing is limited to ${PRINT_MAX_PAGES} pages at a time. Print a smaller range.`);
  }
  const root = document.createElement("div");
  root.id = "print-root";
  const urls: string[] = [];
  try {
    for (const p of list) {
      const blob = await renderPage(docId, p, PRINT_DPI / 72);
      const url = URL.createObjectURL(blob);
      urls.push(url);
      const img = document.createElement("img");
      img.src = url;
      img.alt = "";
      img.style.width = `${sizes[p].width}pt`;
      img.style.height = `${sizes[p].height}pt`;
      root.appendChild(img);
    }
    document.body.appendChild(root);
    await Promise.all(Array.from(root.querySelectorAll("img")).map((i) => i.decode().catch(() => {})));
    await new Promise<void>((resolve) => {
      window.addEventListener("afterprint", () => resolve(), { once: true });
      window.print();
      // Some webviews never fire afterprint; don't leak the bitmaps forever.
      setTimeout(resolve, 10 * 60 * 1000);
    });
  } finally {
    root.remove();
    urls.forEach((u) => URL.revokeObjectURL(u));
  }
}
