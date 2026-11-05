/**
 * Files dragged in from Finder or File Explorer. The windows are made with Tauri's own drop handling off
 * (disable_drag_drop_handler in src-tauri/src/lib.rs, so the browser's drag and drop works inside the page on Windows:
 * the Sessions tree, the tabs), so a dropped file reaches the page as the browser's drop event, with its contents but
 * not its path. The browser's own answer to a file dropped where nothing takes it is to open it, which replaced the
 * whole window with the file (an XML file's tree, say), taking every tab with it: `stopFileDropsOpening` stops that.
 * A drop zone (DropZone.svelte) takes the file itself.
 */

/** Whether a drag carries files from the system (not text, nor a tab or a session dragged within the window) */
export function carriesFiles(event: DragEvent): boolean {
  return Array.from(event.dataTransfer?.types ?? []).includes("Files");
}

/**
 * Never let a dropped file open in the window: a file dragged over it isn't offered to the browser to open, and one
 * dropped where nothing takes it does nothing. Drags within the window (tabs, sessions) aren't touched.
 * @param target The window to guard (the main one; the pages' own guard is in public/web/host/webview.js)
 */
export function stopFileDropsOpening(target: Window): void {
  for (const type of ["dragover", "drop"]) {
    target.addEventListener(type, (event) => {
      if (carriesFiles(event as DragEvent)) event.preventDefault();
    });
  }
}

/**
 * A dropped file's contents as base64, to hand to the core (which saves it where an import can read it: the browser
 * gives no path)
 */
export async function base64Of(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  // in pieces: String.fromCharCode with a whole large file's bytes as arguments overflows the stack
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}
