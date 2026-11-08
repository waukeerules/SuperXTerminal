/**
 * Columns a user can resize by dragging a heading's right edge (a double click puts it back), their widths kept in
 * the window's storage under a prefix and `key`: the file transfer tabs' (the transfer list, and each side's files),
 * and File > Edit Sessions as Table…'s. Begun for the file transfer tabs, whose prefix is the default so their widths
 * already kept stay.
 */
export class ResizableColumns {
  widths = $state<number[]>([]);

  /**
   * @param key Which columns, under the prefix
   * @param defaults Each column's width to start with, and after a double click
   * @param minimum How narrow a column can be dragged
   * @param prefix Whose columns: the file transfer tabs' by default
   */
  constructor(
    private readonly key: string,
    private readonly defaults: number[],
    private readonly minimum = 30,
    private readonly prefix = "sxt.transfer.",
  ) {
    this.widths = this.saved();
  }

  /** All the widths together, for the table */
  get total(): number {
    return this.widths.reduce((a, b) => a + b, 0);
  }

  /** The widths kept for `key`, or the defaults when none are kept (or they don't fit these columns) */
  private saved(): number[] {
    try {
      const saved = JSON.parse(localStorage.getItem(this.prefix + this.key) ?? "null");
      if (Array.isArray(saved) && saved.length === this.defaults.length && saved.every((w) => typeof w === "number")) return saved;
    } catch {
      // nothing saved: the defaults
    }
    return [...this.defaults];
  }

  /** Keep the widths, for the next time the columns are shown */
  private save(): void {
    try {
      localStorage.setItem(this.prefix + this.key, JSON.stringify(this.widths));
    } catch {
      // kept while they're shown only
    }
  }

  /** Drag a heading's right edge; `current` is the column's width as it shows now (a column that fills the rest) */
  startResize(event: PointerEvent, i: number, current = this.widths[i]): void {
    event.preventDefault();
    event.stopPropagation();
    const grip = event.currentTarget as HTMLElement;
    grip.setPointerCapture(event.pointerId);
    const startX = event.clientX;
    const move = (e: PointerEvent) => (this.widths[i] = Math.max(this.minimum, Math.round(current + e.clientX - startX)));
    const done = () => {
      grip.removeEventListener("pointermove", move);
      grip.removeEventListener("pointerup", done);
      grip.removeEventListener("pointercancel", done);
      this.save();
    };
    grip.addEventListener("pointermove", move);
    grip.addEventListener("pointerup", done);
    grip.addEventListener("pointercancel", done);
  }

  /** A double-click on a heading's edge: back to the column's default width */
  reset(i: number): void {
    this.widths[i] = this.defaults[i];
    this.save();
  }
}
