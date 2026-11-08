/**
 * Session icons drawn as SVG instead of loaded from images, ported from SuperXPuTTY's IconTemplates (C# GDI+):
 * network-diagram style symbols (switch, router, firewall, ...) whose body takes any colour, so one template covers
 * "green switch = layer 2, blue switch = layer 3" and every other colour code. Each template draws into the unit square
 * (0..1 on both axes, y down), as the C# painters do once `Render` has scaled the graphics to the icon size.
 *
 * The drawing code mirrors the C#: a small `Canvas` stands in for `Graphics` and a `Path` for `GraphicsPath`, so each
 * painter reads line for line like its original.
 */

/** One template: its picker title, default colour (#RRGGBB), whether it's a general icon or a toolbar glyph, and its drawing */
export interface Template {
  title: string;
  color: string;
  general: boolean;
  glyph: boolean;
  /** SVG markup (elements only, no <svg> wrapper) for a 0..1 viewBox, in the given body colour (#RRGGBB). `uid` must prefix every id (gradients, clip paths) so many icons can sit in one page. */
  draw: (body: string, uid: string) => string;
}

// ---------------------------------------------------------------------------------------------------------------------
// Colours and numbers

/** The glyph colour drawn on coloured bodies */
const GLYPH = "#FFFFFF";

/** A colour's red, green and blue (0..255) from #RRGGBB */
function parseColor(color: string): [number, number, number] {
  const hex = color.replace(/^#/, "");
  return [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16) || 0) as [number, number, number];
}

/** #RRGGBB from red, green and blue (0..255) */
function formatColor(r: number, g: number, b: number): string {
  return "#" + [r, g, b].map((c) => c.toString(16).padStart(2, "0").toUpperCase()).join("");
}

/**
 * A colour made lighter (amount > 0) or darker (amount < 0), as IconTemplates.Shade: -1 is black, 1 is white. The C#
 * works in single precision and truncates, so this rounds through `Math.fround` the same way to land on the same bytes.
 */
export function shade(color: string, amount: number): string {
  const a = Math.fround(amount);
  const mix = (c: number) =>
    a >= 0 ? Math.trunc(Math.fround(c + Math.fround((255 - c) * a))) : Math.trunc(Math.fround(c * Math.fround(1 + a)));
  const [r, g, b] = parseColor(color);
  return formatColor(mix(r), mix(g), mix(b));
}

/** A number for SVG markup: at most 4 decimals (every source literal fits), no trailing zeros */
function num(x: number): string {
  const s = String(parseFloat(x.toFixed(4)));
  return s === "-0" ? "0" : s;
}

/** XML-escape text content or an attribute value */
function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/** A point */
type Pt = readonly [number, number];

/** A point as "x y" */
function pt(p: Pt): string {
  return `${num(p[0])} ${num(p[1])}`;
}

/** Points as "x,y x,y ..." for polygon and polyline */
function pts(points: readonly Pt[]): string {
  return points.map((p) => `${num(p[0])},${num(p[1])}`).join(" ");
}

// ---------------------------------------------------------------------------------------------------------------------
// Brushes, pens, paths and the canvas

/** A solid colour with an alpha (0..255), as Color.FromArgb(alpha, colour) */
interface Translucent {
  color: string;
  alpha: number;
}

/** A LinearGradientBrush between two points, from one colour to another */
interface Gradient {
  from: Pt;
  to: Pt;
  start: string;
  end: string;
}

/** What fills a shape: a colour, a translucent colour or a gradient */
type Brush = string | Translucent | Gradient;

/** A GDI+ Pen: colour, width, and the caps and join where they're not the defaults (flat caps, miter join) */
interface Pen {
  color: string | Translucent;
  width: number;
  cap?: "round";
  join?: "round";
  dash?: boolean;
}

/** Degrees to radians */
const rad = (deg: number) => (deg * Math.PI) / 180;

/**
 * GDI+ arc angles are true angles from the ellipse's centre (where the ray meets the ellipse), not the ellipse's
 * parametric angle; on a circle they're the same. This gives the parametric angle (radians) for a true angle (degrees),
 * kept in the same turn so sweeps stay monotonic.
 */
function unstretch(deg: number, rx: number, ry: number): number {
  const a = rad(deg);
  const t = Math.atan2(rx * Math.sin(a), ry * Math.cos(a));
  return t + 2 * Math.PI * Math.round((a - t) / (2 * Math.PI));
}

/**
 * A GraphicsPath: figures of lines, Béziers and arcs, appended as GDI+ does (each piece joins the open figure with a
 * line from its last point), plus every point GDI+ stores, for its bounds.
 */
class Path {
  private parts: string[] = [];
  private points: Pt[] = [];
  private last: Pt | null = null;
  private figures = 0;

  /** FillMode.Winding (SVG nonzero) rather than GDI+'s default Alternate (SVG evenodd) */
  winding: boolean;

  /** @param winding FillMode.Winding rather than Alternate */
  constructor(winding = false) {
    this.winding = winding;
  }

  /** Continue the open figure to a point, or start a figure there */
  private to(p: Pt): void {
    if (!this.last) {
      this.parts.push(`M${pt(p)}`);
      this.figures++;
    } else if (Math.abs(this.last[0] - p[0]) > 1e-9 || Math.abs(this.last[1] - p[1]) > 1e-9) {
      this.parts.push(`L${pt(p)}`);
    }
    this.points.push(p);
    this.last = p;
  }

  /** AddLine */
  line(x1: number, y1: number, x2: number, y2: number): this {
    this.to([x1, y1]);
    this.to([x2, y2]);
    return this;
  }

  /** AddBezier */
  bezier(x0: number, y0: number, x1: number, y1: number, x2: number, y2: number, x3: number, y3: number): this {
    this.to([x0, y0]);
    this.parts.push(`C${pt([x1, y1])} ${pt([x2, y2])} ${pt([x3, y3])}`);
    this.points.push([x1, y1], [x2, y2], [x3, y3]);
    this.last = [x3, y3];
    return this;
  }

  /**
   * AddArc: part of the ellipse in the box (x, y, w, h), from `start` degrees through `sweep` degrees, clockwise on
   * screen for a positive sweep. It becomes SVG arc commands: sweep-flag 1 for a positive sweep (angles grow clockwise
   * with y down, as SVG's do), large-arc for more than half the ellipse, and a full turn split in two.
   */
  arc(x: number, y: number, w: number, h: number, start: number, sweep: number): this {
    const rx = w / 2, ry = h / 2, cx = x + rx, cy = y + ry;
    sweep = Math.max(-360, Math.min(360, sweep));
    const at = (t: number): Pt => [cx + rx * Math.cos(t), cy + ry * Math.sin(t)];
    const t0 = unstretch(start, rx, ry);
    this.to(at(t0));
    const pieces = Math.abs(sweep) >= 360 ? [start + sweep / 2, start + sweep] : [start + sweep];
    let from = t0;
    for (const end of pieces) {
      const t1 = unstretch(end, rx, ry);
      const large = Math.abs(t1 - from) > Math.PI ? 1 : 0;
      this.parts.push(`A${num(rx)} ${num(ry)} 0 ${large} ${sweep > 0 ? 1 : 0} ${pt(at(t1))}`);
      from = t1;
    }
    // GDI+ stores an arc as Béziers of at most 90 degrees each; their control points count towards its bounds
    const step = sweep > 0 ? 90 : -90;
    for (let a = start; sweep > 0 ? a < start + sweep : a > start + sweep; a += step) {
      const b = sweep > 0 ? Math.min(a + step, start + sweep) : Math.max(a + step, start + sweep);
      const ta = unstretch(a, rx, ry), tb = unstretch(b, rx, ry);
      const k = (4 / 3) * Math.tan((tb - ta) / 4);
      const pa = at(ta), pb = at(tb);
      this.points.push(
        [pa[0] - k * rx * Math.sin(ta), pa[1] + k * ry * Math.cos(ta)],
        [pb[0] + k * rx * Math.sin(tb), pb[1] - k * ry * Math.cos(tb)],
        pb,
      );
    }
    this.last = at(unstretch(start + sweep, rx, ry));
    return this;
  }

  /** CloseFigure */
  close(): this {
    if (this.last) {
      this.parts.push("Z");
      this.last = null;
    }
    return this;
  }

  /** AddEllipse: a closed figure of its own, starting at angle 0 and going clockwise, as GDI+'s does */
  ellipse(x: number, y: number, w: number, h: number): this {
    this.last = null;
    return this.arc(x, y, w, h, 0, 360).close();
  }

  /** AddRectangle: a closed figure, clockwise from the top left */
  rect(x: number, y: number, w: number, h: number): this {
    return this.polygon([[x, y], [x + w, y], [x + w, y + h], [x, y + h]]);
  }

  /** AddPolygon: a closed figure */
  polygon(points: readonly Pt[]): this {
    this.last = null;
    for (const p of points) this.to(p);
    return this.close();
  }

  /** GetBounds: the box round every stored point, Bézier control points included */
  bounds(): { top: number; bottom: number } {
    const ys = this.points.map((p) => p[1]);
    return { top: Math.min(...ys), bottom: Math.max(...ys) };
  }

  /** The SVG fill-rule, where it differs from SVG's default; one figure fills the same either way */
  fillRule(): string {
    return !this.winding && this.figures > 1 ? ` fill-rule="evenodd"` : "";
  }

  /** The SVG path data */
  data(): string {
    return this.parts.join("");
  }
}

/** A Graphics: collects SVG elements in drawing order, so later ones paint over earlier ones */
class Canvas {
  private out: string[] = [];
  private ids = 0;
  private readonly uid: string;

  /** @param uid prefixes every id this canvas makes */
  constructor(uid: string) {
    this.uid = uid;
  }

  /** `fill="..."` (and opacity) for a brush, adding a gradient definition first when it is one */
  private paint(attr: "fill" | "stroke", brush: Brush): string {
    if (typeof brush === "string") return `${attr}="${brush}"`;
    if ("alpha" in brush) return `${attr}="${brush.color}" ${attr}-opacity="${num(brush.alpha / 255)}"`;
    const id = `${this.uid}-g${++this.ids}`;
    this.out.push(
      `<defs><linearGradient id="${esc(id)}" gradientUnits="userSpaceOnUse" x1="${num(brush.from[0])}" y1="${num(brush.from[1])}" x2="${num(brush.to[0])}" y2="${num(brush.to[1])}">` +
        `<stop offset="0" stop-color="${brush.start}"/><stop offset="1" stop-color="${brush.end}"/></linearGradient></defs>`,
    );
    return `${attr}="url(#${esc(id)})"`;
  }

  /** Stroke attributes for a pen; `corners` adds GDI+'s miter limit (10, where SVG's default is 4) on miter joins */
  private pen(pen: Pen, corners: boolean): string {
    let s = `fill="none" ${this.paint("stroke", pen.color)} stroke-width="${num(pen.width)}"`;
    if (pen.cap) s += ` stroke-linecap="${pen.cap}"`;
    if (pen.join) s += ` stroke-linejoin="${pen.join}"`;
    else if (corners) s += ` stroke-miterlimit="10"`;
    // DashStyle.Dash: dashes three pen widths long with one-width gaps
    if (pen.dash) s += ` stroke-dasharray="${num(pen.width * 3)} ${num(pen.width)}"`;
    return s;
  }

  /** FillPath */
  fill(path: Path, brush: Brush): void {
    const fill = this.paint("fill", brush);
    this.out.push(`<path d="${path.data()}" ${fill}${path.fillRule()}/>`);
  }

  /** DrawPath */
  stroke(path: Path, pen: Pen): void {
    this.out.push(`<path d="${path.data()}" ${this.pen(pen, true)}/>`);
  }

  /** FillRectangle */
  fillRect(brush: Brush, x: number, y: number, w: number, h: number): void {
    const fill = this.paint("fill", brush);
    this.out.push(`<rect x="${num(x)}" y="${num(y)}" width="${num(w)}" height="${num(h)}" ${fill}/>`);
  }

  /** DrawRectangle */
  strokeRect(pen: Pen, x: number, y: number, w: number, h: number): void {
    this.out.push(`<rect x="${num(x)}" y="${num(y)}" width="${num(w)}" height="${num(h)}" ${this.pen(pen, false)}/>`);
  }

  /** An ellipse element for the box (x, y, w, h), a circle when it's square */
  private ellipseTag(x: number, y: number, w: number, h: number, attrs: string): string {
    const c = `cx="${num(x + w / 2)}" cy="${num(y + h / 2)}"`;
    return w === h
      ? `<circle ${c} r="${num(w / 2)}" ${attrs}/>`
      : `<ellipse ${c} rx="${num(w / 2)}" ry="${num(h / 2)}" ${attrs}/>`;
  }

  /** FillEllipse */
  fillEllipse(brush: Brush, x: number, y: number, w: number, h: number): void {
    const fill = this.paint("fill", brush);
    this.out.push(this.ellipseTag(x, y, w, h, fill));
  }

  /** DrawEllipse */
  strokeEllipse(pen: Pen, x: number, y: number, w: number, h: number): void {
    this.out.push(this.ellipseTag(x, y, w, h, this.pen(pen, false)));
  }

  /** DrawArc */
  strokeArc(pen: Pen, x: number, y: number, w: number, h: number, start: number, sweep: number): void {
    this.out.push(`<path d="${new Path().arc(x, y, w, h, start, sweep).data()}" ${this.pen(pen, false)}/>`);
  }

  /** DrawLine */
  line(pen: Pen, x1: number, y1: number, x2: number, y2: number): void {
    this.out.push(`<line x1="${num(x1)}" y1="${num(y1)}" x2="${num(x2)}" y2="${num(y2)}" ${this.pen(pen, false)}/>`);
  }

  /** DrawLines */
  lines(pen: Pen, points: readonly Pt[]): void {
    this.out.push(`<polyline points="${pts(points)}" ${this.pen(pen, true)}/>`);
  }

  /** DrawBezier */
  strokeBezier(pen: Pen, x0: number, y0: number, x1: number, y1: number, x2: number, y2: number, x3: number, y3: number): void {
    this.out.push(`<path d="${new Path().bezier(x0, y0, x1, y1, x2, y2, x3, y3).data()}" ${this.pen(pen, false)}/>`);
  }

  /** FillPolygon (GDI+'s default Alternate fill; a single figure fills the same under SVG's nonzero) */
  fillPolygon(brush: Brush, points: readonly Pt[]): void {
    const fill = this.paint("fill", brush);
    this.out.push(`<polygon points="${pts(points)}" ${fill}/>`);
  }

  /** DrawPolygon */
  strokePolygon(pen: Pen, points: readonly Pt[]): void {
    this.out.push(`<polygon points="${pts(points)}" ${this.pen(pen, true)}/>`);
  }

  /** Text centred on a point, `size` being the C# AddString em size */
  text(text: string, x: number, y: number, size: number, color: string, bold: boolean): void {
    this.out.push(
      `<text x="${num(x)}" y="${num(y)}" font-size="${num(size)}"${bold ? ` font-weight="bold"` : ""} ` +
        `font-family="-apple-system, 'Segoe UI', sans-serif" text-anchor="middle" dominant-baseline="central" fill="${color}">${esc(text)}</text>`,
    );
  }

  /** Draw under a transform (`g.Save()`, transform, draw, `g.Restore()`) */
  transformed(transform: string, draw: () => void): void {
    this.out.push(`<g transform="${transform}">`);
    draw();
    this.out.push(`</g>`);
  }

  /** The markup drawn so far */
  markup(): string {
    return this.out.join("");
  }
}

// ---------------------------------------------------------------------------------------------------------------------
// Drawing helpers

/** A rectangle with rounded corners (IconTemplates.RoundRect) */
function roundRect(x: number, y: number, w: number, h: number, radius: number): Path {
  const d = radius * 2;
  return new Path()
    .arc(x, y, d, d, 180, 90)
    .arc(x + w - d, y, d, d, 270, 90)
    .arc(x + w - d, y + h - d, d, d, 0, 90)
    .arc(x, y + h - d, d, d, 90, 90)
    .close();
}

/**
 * Fill a shape as a coloured body: a top-to-bottom gradient around the colour and a darker edge, which keeps it
 * readable on light and dark backgrounds
 */
function fillBody(c: Canvas, path: Path, body: string): void {
  const { top, bottom } = path.bounds();
  c.fill(path, { from: [0, top - 0.01], to: [0, bottom + 0.01], start: shade(body, 0.18), end: shade(body, -0.12) });
  c.stroke(path, { color: shade(body, -0.35), width: 0.045 });
}

/** A filled arrow from one point to another, with its head at the end; `head` is the head's length and half-width */
function arrow(c: Canvas, brush: Brush, from: Pt, to: Pt, shaft: number, head: number): void {
  const dx = to[0] - from[0], dy = to[1] - from[1];
  const length = Math.sqrt(dx * dx + dy * dy);
  const ux = dx / length, uy = dy / length;
  const nx = -uy, ny = ux;
  const neck: Pt = [to[0] - ux * head, to[1] - uy * head];
  const s = shaft / 2;
  c.fillPolygon(brush, [
    [from[0] + nx * s, from[1] + ny * s],
    [neck[0] + nx * s, neck[1] + ny * s],
    [neck[0] + nx * head, neck[1] + ny * head],
    to,
    [neck[0] - nx * head, neck[1] - ny * head],
    [neck[0] - nx * s, neck[1] - ny * s],
    [from[0] - nx * s, from[1] - ny * s],
  ]);
}

/** Four arrows pointing out from a centre, as on router symbols; `reach` is how far the tips reach */
function outwardCross(c: Canvas, cx: number, cy: number, reach: number): void {
  const gap = reach * 0.18;
  arrow(c, GLYPH, [cx - gap, cy], [cx - reach, cy], 0.075, 0.11);
  arrow(c, GLYPH, [cx + gap, cy], [cx + reach, cy], 0.075, 0.11);
  arrow(c, GLYPH, [cx, cy - gap], [cx, cy - reach], 0.075, 0.11);
  arrow(c, GLYPH, [cx, cy + gap], [cx, cy + reach], 0.075, 0.11);
}

/**
 * A box seen from above-front: a lighter top, a darker side and the front (x, y, w, h) filled as a body; the back edge
 * sits `depth` right of and `lift` above the front
 */
function isoBox(c: Canvas, body: string, x: number, y: number, w: number, h: number, depth: number, lift: number): void {
  const right = x + w, bottom = y + h;
  const top: Pt[] = [[x, y], [x + depth, y - lift], [right + depth, y - lift], [right, y]];
  const side: Pt[] = [[right, y], [right + depth, y - lift], [right + depth, bottom - lift], [right, bottom]];
  const edge: Pen = { color: shade(body, -0.4), width: 0.04, join: "round" };
  c.fillPolygon(shade(body, 0.28), top);
  c.fillPolygon(shade(body, -0.25), side);
  fillBody(c, new Path().rect(x, y, w, h), body);
  c.strokePolygon(edge, top);
  c.strokePolygon(edge, side);
}

/** The wireless symbol: three arcs over a dot at (cx, baseY), the outer arc's radius `reach`, `width` thick */
function wirelessArcs(c: Canvas, color: string, cx: number, baseY: number, reach: number, width: number): void {
  const pen: Pen = { color, width, cap: "round" };
  for (let i = 1; i <= 3; i++) {
    const r = (reach * i) / 3;
    c.strokeArc(pen, cx - r, baseY - r, r * 2, r * 2, 225, 90);
  }
  c.fillEllipse(color, cx - width * 0.8, baseY - width * 0.8, width * 1.6, width * 1.6);
}

/** Text centred in a box (x, y, w, h), bold unless told otherwise, as IconTemplates.CentredText */
function centredText(c: Canvas, text: string, x: number, y: number, w: number, h: number, height: number, color: string, bold = true): void {
  c.text(text, x + w / 2, y + h / 2, height, color, bold);
}

/**
 * A telephone handset on the diagonal, as on call buttons, as one smooth outline: the earpiece at the top left, the
 * mouthpiece at the bottom right, and the grip curving between them; (x, y, w, h) is the box it fills
 */
function handset(x: number, y: number, w: number, h: number): Path {
  const X = (u: number) => x + w * u, Y = (v: number) => y + h * v;
  const b = (x0: number, y0: number, x1: number, y1: number, x2: number, y2: number, x3: number, y3: number) =>
    path.bezier(X(x0), Y(y0), X(x1), Y(y1), X(x2), Y(y2), X(x3), Y(y3));
  const l = (x0: number, y0: number, x1: number, y1: number) => path.line(X(x0), Y(y0), X(x1), Y(y1));
  const path = new Path();
  // the outer edge, from the earpiece's tip round to the mouthpiece's
  b(0.08, 0.03, 0.02, 0.07, -0.02, 0.22, 0.04, 0.40);
  b(0.04, 0.40, 0.12, 0.62, 0.36, 0.86, 0.62, 0.97);
  b(0.62, 0.97, 0.72, 1.01, 0.86, 1.00, 0.92, 0.94);
  l(0.92, 0.94, 0.98, 0.88);
  b(0.98, 0.88, 1.01, 0.85, 1.00, 0.79, 0.96, 0.76);
  // the mouthpiece's end, the inner edge of the grip, and the earpiece's end
  l(0.96, 0.76, 0.80, 0.65);
  b(0.80, 0.65, 0.76, 0.62, 0.71, 0.63, 0.68, 0.66);
  l(0.68, 0.66, 0.62, 0.72);
  b(0.62, 0.72, 0.50, 0.66, 0.36, 0.52, 0.29, 0.39);
  l(0.29, 0.39, 0.35, 0.33);
  b(0.35, 0.33, 0.38, 0.30, 0.38, 0.25, 0.36, 0.21);
  l(0.36, 0.21, 0.25, 0.05);
  b(0.25, 0.05, 0.22, 0.01, 0.13, 0.00, 0.08, 0.03);
  return path.close();
}

/** A coloured disc, the base of most toolbar glyphs */
function disc(c: Canvas, body: string): void {
  fillBody(c, new Path().ellipse(0.06, 0.06, 0.88, 0.88), body);
}

/** A small disc with a white plus in the bottom-right corner, for "new ..." buttons */
function plusBadge(c: Canvas, body: string): void {
  fillBody(c, new Path().ellipse(0.5, 0.5, 0.48, 0.48), body);
  const plus: Pen = { color: GLYPH, width: 0.08, cap: "round" };
  c.line(plus, 0.74, 0.62, 0.74, 0.86);
  c.line(plus, 0.62, 0.74, 0.86, 0.74);
}

// ---------------------------------------------------------------------------------------------------------------------
// Templates

/** Switch: a rounded square with two rows of opposing arrows */
function paintSwitch(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.06, 0.06, 0.88, 0.88, 0.14), body);
  arrow(c, GLYPH, [0.24, 0.34], [0.80, 0.34], 0.075, 0.12);
  arrow(c, GLYPH, [0.76, 0.66], [0.20, 0.66], 0.075, 0.12);
}

/** Layer 3 switch: a rounded square with arrows pointing out on all four sides */
function paintL3Switch(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.06, 0.06, 0.88, 0.88, 0.14), body);
  outwardCross(c, 0.5, 0.5, 0.34);
}

/** Stack switch: two switch boxes, one on the other, with traffic each way */
function paintStackSwitch(c: Canvas, body: string): void {
  isoBox(c, shade(body, -0.1), 0.06, 0.66, 0.66, 0.28, 0.2, 0.23);
  arrow(c, GLYPH, [0.14, 0.80], [0.64, 0.80], 0.065, 0.1);
  isoBox(c, body, 0.06, 0.32, 0.66, 0.28, 0.2, 0.23);
  arrow(c, GLYPH, [0.64, 0.46], [0.14, 0.46], 0.065, 0.1);
}

/** Core switch: the layer 3 switch's arrows going out from a hub, as the backbone everything crosses */
function paintCoreSwitch(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.06, 0.06, 0.88, 0.88, 0.14), body);
  outwardCross(c, 0.5, 0.5, 0.36);
  c.fillEllipse(GLYPH, 0.34, 0.34, 0.32, 0.32);
  c.fillEllipse(body, 0.42, 0.42, 0.16, 0.16);
}

/** Distribution switch: one uplink fanning out to three arrows down to the access layer */
function paintDistributionSwitch(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.06, 0.06, 0.88, 0.88, 0.14), body);
  const line: Pen = { color: GLYPH, width: 0.07, cap: "round" };
  c.line(line, 0.5, 0.16, 0.5, 0.44);
  c.line(line, 0.22, 0.44, 0.78, 0.44);
  for (const x of [0.22, 0.5, 0.78]) {
    arrow(c, GLYPH, [x, 0.44], [x, 0.84], 0.07, 0.11);
  }
}

/** Router: a disc with arrows pointing out on all four sides */
function paintRouter(c: Canvas, body: string): void {
  fillBody(c, new Path().ellipse(0.05, 0.05, 0.9, 0.9), body);
  outwardCross(c, 0.5, 0.5, 0.34);
}

/** Voice router: a disc with a call symbol, a handset on the diagonal with arrows going each way */
function paintVoiceRouter(c: Canvas, body: string): void {
  fillBody(c, new Path().ellipse(0.05, 0.05, 0.9, 0.9), body);
  c.fill(handset(0.22, 0.31, 0.52, 0.49), GLYPH);
  const line: Pen = { color: GLYPH, width: 0.05, cap: "round", join: "round" };
  c.line(line, 0.45, 0.25, 0.66, 0.25);
  c.lines(line, [[0.61, 0.20], [0.66, 0.25], [0.61, 0.30]]);
  c.line(line, 0.70, 0.38, 0.49, 0.38);
  c.lines(line, [[0.54, 0.33], [0.49, 0.38], [0.54, 0.43]]);
}

/** Data centre switch: a box seen from above-front, arrows on the front */
function paintNexus(c: Canvas, body: string): void {
  const front: Pt[] = [[0.06, 0.36], [0.72, 0.36], [0.72, 0.94], [0.06, 0.94]];
  const top: Pt[] = [[0.06, 0.36], [0.28, 0.10], [0.94, 0.10], [0.72, 0.36]];
  const side: Pt[] = [[0.72, 0.36], [0.94, 0.10], [0.94, 0.68], [0.72, 0.94]];
  const edge: Pen = { color: shade(body, -0.4), width: 0.04, join: "round" };
  c.fillPolygon(shade(body, 0.28), top);
  c.fillPolygon(shade(body, -0.25), side);
  fillBody(c, new Path().polygon(front), body);
  c.strokePolygon(edge, top);
  c.strokePolygon(edge, side);
  arrow(c, GLYPH, [0.16, 0.53], [0.62, 0.53], 0.07, 0.1);
  arrow(c, GLYPH, [0.62, 0.77], [0.16, 0.77], 0.07, 0.1);
}

/** Firewall: a brick wall */
function paintFirewall(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.06, 0.12, 0.88, 0.76, 0.06), body);
  const mortar: Pen = { color: shade(body, 0.75), width: 0.05 };
  for (const y of [0.31, 0.50, 0.69]) {
    c.line(mortar, 0.08, y, 0.92, y);
  }
  // joints alternate per course, like bricks
  const tops = [0.14, 0.31, 0.50, 0.69];
  const bottoms = [0.31, 0.50, 0.69, 0.86];
  for (let row = 0; row < 4; row++) {
    const xs = row % 2 === 0 ? [0.36, 0.64] : [0.22, 0.5, 0.78];
    for (const x of xs) {
      c.line(mortar, x, tops[row], x, bottoms[row]);
    }
  }
}

/** VPN concentrator: a box with two tunnels through it, traffic going each way, their mouths on the side */
function paintVpn(c: Canvas, body: string): void {
  // a little lower than the C#, whose box's top face runs past the top of the square and is cut off there
  c.transformed("translate(0 0.04)", () => {
    isoBox(c, body, 0.04, 0.30, 0.62, 0.64, 0.28, 0.32);
    const tube: Pen = { color: GLYPH, width: 0.045 };
    const tops = [0.38, 0.66];
    for (let i = 0; i < 2; i++) {
      c.stroke(roundRect(0.10, tops[i], 0.56, 0.20, 0.10), tube);
      const y = tops[i] + 0.10;
      if (i === 0) {
        arrow(c, GLYPH, [0.18, y], [0.60, y], 0.04, 0.08);
      } else {
        arrow(c, GLYPH, [0.58, y], [0.16, y], 0.04, 0.08);
      }
      // the side face rises as it goes back, so each tunnel's mouth sits higher than the tunnel
      c.strokeEllipse(tube, 0.75, tops[i] - 0.13, 0.10, 0.20);
    }
  });
}

/** Wireless: an access point with signal arcs above it */
function paintWireless(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.12, 0.66, 0.76, 0.26, 0.08), body);
  const arc: Pen = { color: body, width: 0.085, cap: "round" };
  c.strokeArc(arc, 0.34, 0.40, 0.32, 0.32, 225, 90);
  c.strokeArc(arc, 0.20, 0.24, 0.60, 0.60, 225, 90);
  c.strokeArc(arc, 0.06, 0.08, 0.88, 0.88, 225, 90);
  c.fillEllipse(GLYPH, 0.22, 0.75, 0.08, 0.08);
  c.fillEllipse(GLYPH, 0.34, 0.75, 0.08, 0.08);
}

/** Access point: a flat puck with the signal rising from it */
function paintAccessPoint(c: Canvas, body: string): void {
  const side = new Path()
    .arc(0.08, 0.60, 0.84, 0.24, 180, -180)
    .line(0.92, 0.72, 0.92, 0.80)
    .arc(0.08, 0.68, 0.84, 0.24, 0, 180)
    .close();
  fillBody(c, side, shade(body, -0.15));
  c.fillEllipse(shade(body, 0.2), 0.08, 0.60, 0.84, 0.24);
  c.strokeEllipse({ color: shade(body, -0.35), width: 0.04 }, 0.08, 0.60, 0.84, 0.24);
  wirelessArcs(c, body, 0.5, 0.62, 0.5, 0.08);
}

/**
 * Wireless controller, after the network-diagram symbol: a low box with arrows converging on its top and a braided
 * wave across its front
 */
function paintWirelessController(c: Canvas, body: string): void {
  const fx = 0.02, fy = 0.54, fw = 0.80, fh = 0.40;
  isoBox(c, body, fx, fy, fw, fh, 0.16, 0.36);
  // drawn flat in a unit square (across, then front to back) mapped onto the top face, so the arrows lie on it in
  // perspective. GDI+'s Matrix(m11, m12, m21, m22, dx, dy) is SVG's matrix(a, b, c, d, e, f) in the same order, and
  // MultiplyTransform prepends it, which is what a nested <g transform> does
  c.transformed("matrix(0.8 0 0.16 -0.36 0.02 0.54)", () => {
    arrow(c, GLYPH, [0.10, 0.10], [0.36, 0.44], 0.07, 0.11);
    arrow(c, GLYPH, [0.50, 0.06], [0.50, 0.44], 0.07, 0.11);
    arrow(c, GLYPH, [0.90, 0.10], [0.64, 0.44], 0.07, 0.11);
    arrow(c, GLYPH, [0.50, 0.52], [0.50, 0.94], 0.13, 0.17);
  });
  const mid = fy + fh / 2, amplitude = fh * 0.28;
  const left = fx + 0.06, right = fx + fw - 0.06;
  const steps = 60;
  const up: Pt[] = [], down: Pt[] = [];
  for (let i = 0; i <= steps; i++) {
    const x = left + ((right - left) * i) / steps;
    const y = amplitude * Math.sin((i * Math.PI * 4) / steps);
    up.push([x, mid + y]);
    down.push([x, mid - y]);
  }
  // each strand gets a dark outline, so the second one drawn appears to pass over the first
  const outline: Pen = { color: shade(body, -0.45), width: 0.11, cap: "round" };
  const strand: Pen = { color: GLYPH, width: 0.065, cap: "round" };
  c.lines(outline, down);
  c.lines(strand, down);
  c.lines(outline, up);
  c.lines(strand, up);
}

/** Load balancer: a rounded square with one arrow splitting into three */
function paintLoadBalancer(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.06, 0.06, 0.88, 0.88, 0.14), body);
  c.line({ color: GLYPH, width: 0.075 }, 0.18, 0.5, 0.42, 0.5);
  arrow(c, GLYPH, [0.42, 0.5], [0.82, 0.22], 0.07, 0.11);
  arrow(c, GLYPH, [0.42, 0.5], [0.84, 0.5], 0.07, 0.11);
  arrow(c, GLYPH, [0.42, 0.5], [0.82, 0.78], 0.07, 0.11);
}

/** Console server: a unit with a prompt above a row of serial ports */
function paintConsoleServer(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.05, 0.14, 0.9, 0.72, 0.1), body);
  const prompt: Pen = { color: GLYPH, width: 0.075, cap: "round", join: "round" };
  c.lines(prompt, [[0.18, 0.28], [0.32, 0.39], [0.18, 0.50]]);
  c.line(prompt, 0.40, 0.52, 0.60, 0.52);
  const port = shade(body, 0.6);
  for (let i = 0; i < 4; i++) {
    c.fillRect(port, 0.15 + i * 0.19, 0.64, 0.13, 0.1);
  }
}

/** UPS: a battery with a lightning bolt */
function paintUps(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.16, 0.14, 0.68, 0.82, 0.09), body);
  c.fillRect(shade(body, -0.3), 0.36, 0.04, 0.28, 0.1);
  c.fillPolygon(GLYPH, [[0.56, 0.24], [0.30, 0.60], [0.48, 0.60], [0.42, 0.86], [0.70, 0.48], [0.52, 0.48]]);
}

/** Server: a tower case with drive bays and a light */
function paintServer(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.22, 0.04, 0.56, 0.92, 0.08), body);
  const bay = shade(body, 0.6);
  c.fillRect(bay, 0.32, 0.16, 0.36, 0.08);
  c.fillRect(bay, 0.32, 0.30, 0.36, 0.08);
  c.fillRect(bay, 0.32, 0.44, 0.36, 0.08);
  c.fillEllipse("#7CFC5C", 0.58, 0.78, 0.09, 0.09);
}

/** Workstation: a monitor on a stand */
function paintWorkstation(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.06, 0.1, 0.88, 0.6, 0.07), body);
  const stand = shade(body, -0.2);
  c.fillRect("#D6ECFA", 0.13, 0.17, 0.74, 0.45);
  c.fillRect(stand, 0.43, 0.70, 0.14, 0.14);
  c.fill(roundRect(0.26, 0.83, 0.48, 0.09, 0.04), stand);
}

/** Terminal: a window with a title bar and a prompt */
function paintTerminal(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.05, 0.1, 0.9, 0.8, 0.08), body);
  c.fillRect(shade(body, 0.35), 0.08, 0.13, 0.84, 0.12);
  const prompt: Pen = { color: GLYPH, width: 0.075, cap: "round", join: "round" };
  c.lines(prompt, [[0.2, 0.4], [0.36, 0.53], [0.2, 0.66]]);
  c.line(prompt, 0.44, 0.7, 0.66, 0.7);
}

/** Windows: four panes */
function paintWindows(c: Canvas, body: string): void {
  const gap = 0.06, size = (0.88 - gap) / 2;
  for (let i = 0; i < 4; i++) {
    const x = 0.06 + (i % 2) * (size + gap), y = 0.06 + Math.floor(i / 2) * (size + gap);
    fillBody(c, roundRect(x, y, size, size, 0.03), body);
  }
}

/** Database: a stacked cylinder */
function paintDatabase(c: Canvas, body: string): void {
  const path = new Path()
    .arc(0.14, 0.06, 0.72, 0.24, 180, 180)
    .line(0.86, 0.18, 0.86, 0.82)
    .arc(0.14, 0.70, 0.72, 0.24, 0, 180)
    .close();
  fillBody(c, path, body);
  const ring: Pen = { color: shade(body, 0.55), width: 0.045 };
  c.fillEllipse(shade(body, 0.3), 0.14, 0.06, 0.72, 0.24);
  c.strokeArc(ring, 0.14, 0.30, 0.72, 0.24, 0, 180);
  c.strokeArc(ring, 0.14, 0.50, 0.72, 0.24, 0, 180);
}

/** Storage: stacked disk shelves */
function paintStorage(c: Canvas, body: string): void {
  for (let i = 0; i < 3; i++) {
    fillBody(c, roundRect(0.06, 0.08 + i * 0.29, 0.88, 0.24, 0.05), body);
    c.fillRect(shade(body, 0.55), 0.16, 0.17 + i * 0.29, 0.46, 0.06);
    c.fillEllipse("#7CFC5C", 0.74, 0.16 + i * 0.29, 0.08, 0.08);
  }
}

/** Cloud */
function paintCloud(c: Canvas, body: string): void {
  // winding fill, so where the circles overlap stays filled (every figure runs clockwise, as GDI+ adds them)
  const path = new Path(true)
    .ellipse(0.06, 0.42, 0.36, 0.36)
    .ellipse(0.24, 0.2, 0.46, 0.46)
    .ellipse(0.56, 0.38, 0.38, 0.38)
    .rect(0.24, 0.52, 0.52, 0.26);
  // filled as one region, no edge
  c.fill(path, { from: [0, 0.18], to: [0, 0.8], start: shade(body, 0.22), end: shade(body, -0.1) });
}

/** IP phone: a desk phone with a handset and keypad */
function paintPhone(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.08, 0.3, 0.84, 0.62, 0.1), body);
  fillBody(c, roundRect(0.08, 0.1, 0.84, 0.16, 0.08), body);
  const key = shade(body, 0.65);
  for (let row = 0; row < 3; row++) {
    for (let col = 0; col < 3; col++) {
      c.fillRect(key, 0.26 + col * 0.18, 0.42 + row * 0.15, 0.1, 0.08);
    }
  }
}

/** Camera: a security camera on a mount */
function paintCamera(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.08, 0.22, 0.68, 0.36, 0.08), body);
  const mount = shade(body, -0.2);
  c.fillPolygon(mount, [[0.76, 0.3], [0.94, 0.2], [0.94, 0.6], [0.76, 0.5]]);
  c.fillEllipse("#D6ECFA", 0.14, 0.3, 0.2, 0.2);
  c.fillRect(mount, 0.4, 0.58, 0.1, 0.24);
  c.fillRect(mount, 0.26, 0.8, 0.38, 0.08);
}

/** Printer */
function paintPrinter(c: Canvas, body: string): void {
  const paperEdge: Pen = { color: "#909090", width: 0.03 };
  c.fillRect("#FFFFFF", 0.26, 0.06, 0.48, 0.3);
  c.strokeRect(paperEdge, 0.26, 0.06, 0.48, 0.3);
  fillBody(c, roundRect(0.06, 0.32, 0.88, 0.42, 0.08), body);
  c.fillRect("#FFFFFF", 0.26, 0.62, 0.48, 0.3);
  c.strokeRect(paperEdge, 0.26, 0.62, 0.48, 0.3);
}

/** Folder */
function paintFolder(c: Canvas, body: string): void {
  const back = new Path().polygon([[0.06, 0.16], [0.38, 0.16], [0.46, 0.26], [0.94, 0.26], [0.94, 0.84], [0.06, 0.84]]);
  c.fill(back, shade(body, -0.15));
  fillBody(c, roundRect(0.06, 0.34, 0.88, 0.52, 0.05), body);
}

/** Flag: a pennant on a pole, for colour-coding anything */
function paintFlag(c: Canvas, body: string): void {
  c.fillRect("#787878", 0.14, 0.06, 0.08, 0.9);
  const path = new Path()
    .bezier(0.22, 0.1, 0.42, 0.02, 0.62, 0.2, 0.88, 0.1)
    .line(0.88, 0.1, 0.88, 0.56)
    .bezier(0.88, 0.56, 0.62, 0.66, 0.42, 0.48, 0.22, 0.56)
    .close();
  fillBody(c, path, body);
}

// ---------------------------------------------------------------------------------------------------------------------
// General icons

/** Home: a house with a roof, door and window */
function paintHouse(c: Canvas, body: string): void {
  const walls = roundRect(0.18, 0.44, 0.64, 0.5, 0.04);
  c.fill(walls, "#ECEFF1");
  c.stroke(walls, { color: "#90A4AE", width: 0.04 });
  fillBody(c, new Path().polygon([[0.5, 0.06], [0.96, 0.5], [0.04, 0.5]]), body);
  c.fillRect(shade(body, -0.25), 0.43, 0.64, 0.16, 0.3);
}

/**
 * Globe: a sphere seen side on, as the usual globe sign: the rim, the meridians curving round it (the centre one
 * straight), the equator, and two latitudes as straight lines across, as they look from level with the equator. The
 * latitudes stop short of the rim, so the grid reads at a tab's 16 pixels
 */
function paintGlobe(c: Canvas, body: string): void {
  fillBody(c, new Path().ellipse(0.06, 0.06, 0.88, 0.88), body);
  const line: Pen = { color: { color: GLYPH, alpha: 225 }, width: 0.05 };
  c.strokeEllipse(line, 0.31, 0.08, 0.38, 0.84);
  c.line(line, 0.5, 0.08, 0.5, 0.92);
  c.line(line, 0.08, 0.5, 0.92, 0.5);
  // a latitude 0.2 above and below the equator: the chord of the 0.42 circle there
  const half = Math.sqrt(0.42 * 0.42 - 0.2 * 0.2);
  c.line(line, 0.5 - half, 0.3, 0.5 + half, 0.3);
  c.line(line, 0.5 - half, 0.7, 0.5 + half, 0.7);
}

/** Map: a folded map with a route and a pin */
function paintMap(c: Canvas, body: string): void {
  const panels: Pt[][] = [
    [[0.06, 0.16], [0.36, 0.08], [0.36, 0.84], [0.06, 0.92]],
    [[0.36, 0.08], [0.64, 0.16], [0.64, 0.92], [0.36, 0.84]],
    [[0.64, 0.16], [0.94, 0.08], [0.94, 0.84], [0.64, 0.92]],
  ];
  panels.forEach((panel, i) => fillBody(c, new Path().polygon(panel), i === 1 ? shade(body, -0.12) : body));
  c.strokeBezier({ color: "#FFFFFF", width: 0.05, dash: true }, 0.12, 0.78, 0.36, 0.5, 0.6, 0.75, 0.84, 0.32);
  c.fillEllipse("#E53935", 0.74, 0.2, 0.18, 0.18);
}

/** Truck: a lorry with a cab and wheels */
function paintTruck(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.04, 0.2, 0.58, 0.5, 0.04), body);
  fillBody(c, new Path().polygon([[0.64, 0.34], [0.82, 0.34], [0.96, 0.52], [0.96, 0.7], [0.64, 0.7]]), shade(body, -0.15));
  c.fillPolygon("#D6ECFA", [[0.7, 0.39], [0.8, 0.39], [0.89, 0.51], [0.7, 0.51]]);
  for (const x of [0.14, 0.7]) {
    c.fillEllipse("#37474F", x, 0.62, 0.2, 0.2);
    c.fillEllipse("#CFD8DC", x + 0.06, 0.68, 0.08, 0.08);
  }
}

/** Book: a closed book with pages showing */
function paintBook(c: Canvas, body: string): void {
  c.fillRect("#FAFAFA", 0.22, 0.12, 0.64, 0.82);
  c.strokeRect({ color: "#B0B0B0", width: 0.03 }, 0.22, 0.12, 0.64, 0.82);
  fillBody(c, roundRect(0.14, 0.06, 0.66, 0.82, 0.05), body);
  c.fillRect(shade(body, -0.3), 0.14, 0.08, 0.09, 0.78);
  c.fillRect(shade(body, 0.6), 0.34, 0.24, 0.36, 0.12);
}

/** Document: a page with a folded corner and lines of text */
function paintDocument(c: Canvas, body: string): void {
  const outline: Pt[] = [[0.16, 0.04], [0.62, 0.04], [0.86, 0.28], [0.86, 0.96], [0.16, 0.96]];
  c.fillPolygon("#FAFAFA", outline);
  c.fillPolygon("#D0D0D0", [[0.62, 0.04], [0.86, 0.28], [0.62, 0.28]]);
  c.strokePolygon({ color: "#909090", width: 0.04, join: "round" }, outline);
  const text: Pen = { color: body, width: 0.06, cap: "round" };
  c.line(text, 0.28, 0.42, 0.74, 0.42);
  c.line(text, 0.28, 0.56, 0.74, 0.56);
  c.line(text, 0.28, 0.70, 0.74, 0.70);
  c.line(text, 0.28, 0.84, 0.56, 0.84);
}

/** Archive: a box with a lid and a zip down the front */
function paintArchive(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.1, 0.3, 0.8, 0.62, 0.05), body);
  fillBody(c, roundRect(0.04, 0.1, 0.92, 0.22, 0.05), shade(body, 0.12));
  for (let i = 0; i < 4; i++) {
    c.fillRect("#ECEFF1", i % 2 === 0 ? 0.44 : 0.5, 0.36 + i * 0.1, 0.06, 0.06);
  }
  c.fillRect("#ECEFF1", 0.42, 0.76, 0.16, 0.1);
}

/** Email: an envelope */
function paintEmail(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.06, 0.18, 0.88, 0.64, 0.06), body);
  c.lines({ color: GLYPH, width: 0.06, join: "round" }, [[0.12, 0.26], [0.5, 0.56], [0.88, 0.26]]);
}

/** Chat: a speech bubble with dots */
function paintChat(c: Canvas, body: string): void {
  // FillMode.Winding as in the C#, with the tail running the same way round as the bubble, so where they overlap
  // (just above the bubble's bottom edge) stays filled. The C#'s tail runs the other way, which winds to zero there
  // and leaves a hole the background shows through
  const bubble = roundRect(0.06, 0.1, 0.88, 0.6, 0.16).polygon([[0.48, 0.66], [0.2, 0.92], [0.24, 0.62]]);
  bubble.winding = true;
  c.fill(bubble, { from: [0, 0.08], to: [0, 0.94], start: shade(body, 0.18), end: shade(body, -0.12) });
  for (const dx of [0.26, 0.44, 0.62]) {
    c.fillEllipse(GLYPH, dx, 0.34, 0.12, 0.12);
  }
}

/** Feed: the RSS symbol on a rounded square */
function paintRss(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.06, 0.06, 0.88, 0.88, 0.16), body);
  // the dot and both arcs round the same centre, the outer arc ending well inside the square's edge
  const [x, y] = [0.28, 0.72];
  const arc: Pen = { color: GLYPH, width: 0.1 };
  c.fillEllipse(GLYPH, x - 0.075, y - 0.075, 0.15, 0.15);
  for (const r of [0.24, 0.43]) c.strokeArc(arc, x - r, y - r, r * 2, r * 2, 270, 90);
}

/** Clock: a face with hands inside a coloured rim */
function paintClock(c: Canvas, body: string): void {
  fillBody(c, new Path().ellipse(0.05, 0.05, 0.9, 0.9), body);
  const hand: Pen = { color: "#37474F", width: 0.07, cap: "round" };
  c.fillEllipse("#FAFAFA", 0.16, 0.16, 0.68, 0.68);
  c.line(hand, 0.5, 0.5, 0.5, 0.27);
  c.line(hand, 0.5, 0.5, 0.68, 0.58);
}

/** Help: a question mark on a disc */
function paintHelp(c: Canvas, body: string): void {
  disc(c, body);
  centredText(c, "?", 0.1, 0.08, 0.8, 0.86, 0.7, GLYPH);
}

/** Information: an i on a disc */
function paintInfo(c: Canvas, body: string): void {
  disc(c, body);
  centredText(c, "i", 0.1, 0.08, 0.8, 0.86, 0.7, GLYPH);
}

/** Light bulb: a glowing bulb on a screw base */
function paintLightbulb(c: Canvas, body: string): void {
  const bulb = new Path()
    .arc(0.18, 0.04, 0.64, 0.64, 140, 260)
    .line(0.66, 0.62, 0.62, 0.74)
    .line(0.38, 0.74, 0.34, 0.62)
    .close();
  fillBody(c, bulb, body);
  const thread: Pen = { color: "#B0BEC5", width: 0.035 };
  c.fillEllipse({ color: "#FFFFFF", alpha: 150 }, 0.3, 0.14, 0.16, 0.16);
  c.fillRect("#78909C", 0.36, 0.76, 0.28, 0.16);
  c.line(thread, 0.36, 0.82, 0.64, 0.82);
  c.line(thread, 0.36, 0.88, 0.64, 0.88);
}

/** Lightning: a bolt */
function paintBolt(c: Canvas, body: string): void {
  const bolt = new Path().polygon([[0.58, 0.04], [0.18, 0.56], [0.46, 0.56], [0.36, 0.96], [0.82, 0.4], [0.54, 0.4]]);
  fillBody(c, bolt, body);
}

/** Television: a screen on legs with an aerial */
function paintTv(c: Canvas, body: string): void {
  const aerial: Pen = { color: shade(body, -0.2), width: 0.05, cap: "round" };
  c.line(aerial, 0.5, 0.24, 0.32, 0.06);
  c.line(aerial, 0.5, 0.24, 0.7, 0.06);
  c.line(aerial, 0.26, 0.84, 0.2, 0.94);
  c.line(aerial, 0.74, 0.84, 0.8, 0.94);
  fillBody(c, roundRect(0.06, 0.22, 0.88, 0.64, 0.08), body);
  c.fillRect("#81D4FA", 0.14, 0.3, 0.6, 0.48);
  const knob = shade(body, 0.6);
  c.fillEllipse(knob, 0.79, 0.36, 0.08, 0.08);
  c.fillEllipse(knob, 0.79, 0.52, 0.08, 0.08);
}

/** Mobile device: a phone or handheld with a screen and button */
function paintMobile(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.24, 0.04, 0.52, 0.92, 0.1), body);
  c.fillRect("#D6ECFA", 0.31, 0.13, 0.38, 0.6);
  c.fillEllipse(shade(body, 0.55), 0.44, 0.79, 0.12, 0.1);
}

/** Star: a five-pointed star, the Sessions tree's Favorites */
function paintStar(c: Canvas, body: string): void {
  const points: Pt[] = [];
  for (let i = 0; i < 10; i++) {
    const a = -Math.PI / 2 + (i * Math.PI) / 5;
    const r = i % 2 === 0 ? 0.47 : 0.2;
    points.push([0.5 + Math.cos(a) * r, 0.53 + Math.sin(a) * r]);
  }
  fillBody(c, new Path().polygon(points), body);
}

/** Padlock: a shackle over a coloured body with a keyhole, the Sessions tree's private sessions */
function paintLock(c: Canvas, body: string): void {
  const shackle: Pen = { color: shade(body, -0.2), width: 0.1 };
  c.strokeArc(shackle, 0.29, 0.1, 0.42, 0.5, 180, 180);
  c.line(shackle, 0.29, 0.35, 0.29, 0.48);
  c.line(shackle, 0.71, 0.35, 0.71, 0.48);
  fillBody(c, roundRect(0.17, 0.44, 0.66, 0.5, 0.08), body);
  c.fillEllipse(GLYPH, 0.43, 0.56, 0.14, 0.14);
  c.fillRect(GLYPH, 0.47, 0.64, 0.06, 0.16);
}

/** Building: an office block with rows of windows and a door, for a site */
function paintBuilding(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.18, 0.08, 0.64, 0.86, 0.04), body);
  const window: Translucent = { color: GLYPH, alpha: 225 };
  for (const y of [0.18, 0.34, 0.5]) {
    for (const x of [0.28, 0.44, 0.6]) c.fillRect(window, x, y, 0.12, 0.1);
  }
  c.fillRect(shade(body, -0.4), 0.42, 0.7, 0.16, 0.24);
}

/** Person: a head and shoulders, for a user's own session */
function paintPerson(c: Canvas, body: string): void {
  fillBody(c, new Path().ellipse(0.32, 0.06, 0.36, 0.36), body);
  fillBody(c, new Path().arc(0.12, 0.48, 0.76, 0.6, 180, 180).line(0.88, 0.78, 0.88, 0.94).line(0.88, 0.94, 0.12, 0.94).close(), body);
}

/** Key: a key on the slant, its bow a ring the background shows through */
function paintKey(c: Canvas, body: string): void {
  c.transformed("rotate(-40 0.5 0.5)", () => {
    fillBody(c, new Path().polygon([[0.4, 0.45], [0.96, 0.45], [0.96, 0.66], [0.88, 0.66], [0.88, 0.56], [0.8, 0.56],
      [0.8, 0.66], [0.72, 0.66], [0.72, 0.56], [0.4, 0.56]]), body);
    // GDI+'s Alternate fill: the inner circle is the bow's hole
    fillBody(c, new Path().ellipse(0.04, 0.3, 0.42, 0.42).ellipse(0.15, 0.41, 0.2, 0.2), body);
  });
}

/** Shield: a shield with a tick, for something secured */
function paintShield(c: Canvas, body: string): void {
  fillBody(c, new Path().line(0.5, 0.05, 0.88, 0.17).bezier(0.88, 0.17, 0.9, 0.6, 0.72, 0.82, 0.5, 0.95)
    .bezier(0.5, 0.95, 0.28, 0.82, 0.1, 0.6, 0.12, 0.17).close(), body);
  c.lines({ color: GLYPH, width: 0.09, cap: "round", join: "round" }, [[0.31, 0.5], [0.45, 0.64], [0.7, 0.36]]);
}

/** Settings: a gear of eight teeth, its middle a hole the background shows through */
function paintGear(c: Canvas, body: string): void {
  const points: Pt[] = [];
  const step = (Math.PI * 2) / 8;
  const at = (angle: number, r: number): Pt => [0.5 + Math.cos(angle) * r, 0.5 + Math.sin(angle) * r];
  for (let i = 0; i < 8; i++) {
    const a = i * step;
    points.push(at(a - step * 0.32, 0.34), at(a - step * 0.17, 0.46), at(a + step * 0.17, 0.46), at(a + step * 0.32, 0.34));
  }
  // GDI+'s Alternate fill: the circle is the hole
  fillBody(c, new Path().polygon(points).ellipse(0.36, 0.36, 0.28, 0.28), body);
}

/** Wrench: an open-ended spanner on the slant, for maintenance */
function paintWrench(c: Canvas, body: string): void {
  // the head: a disc with a square notch out of its end
  const [cx, cy, r] = [0.5, 0.25, 0.2];
  const edge = Math.asin(0.07 / r);
  const head: Pt[] = [];
  for (let i = 0; i <= 24; i++) {
    const a = -Math.PI / 2 + edge + ((Math.PI * 2 - edge * 2) * i) / 24;
    head.push([cx + Math.cos(a) * r, cy + Math.sin(a) * r]);
  }
  head.push([0.43, 0.22], [0.57, 0.22]);
  c.transformed("rotate(45 0.5 0.5)", () => {
    fillBody(c, roundRect(0.43, 0.36, 0.14, 0.58, 0.07), body);
    fillBody(c, new Path().polygon(head), body);
  });
}

/** Warning: a triangle with an exclamation mark, dark on the body so it reads on yellow */
function paintWarning(c: Canvas, body: string): void {
  fillBody(c, new Path().polygon([[0.5, 0.07], [0.96, 0.9], [0.04, 0.9]]), body);
  const mark = shade(body, -0.65);
  c.fill(roundRect(0.455, 0.33, 0.09, 0.33, 0.04), mark);
  c.fillEllipse(mark, 0.45, 0.71, 0.1, 0.1);
}

/** Bell: a bell with its clapper, for alerts */
function paintBell(c: Canvas, body: string): void {
  c.fillEllipse(shade(body, -0.35), 0.4, 0.78, 0.2, 0.17);
  fillBody(c, new Path().bezier(0.2, 0.74, 0.27, 0.6, 0.18, 0.15, 0.5, 0.15).bezier(0.5, 0.15, 0.82, 0.15, 0.73, 0.6, 0.8, 0.74)
    .line(0.8, 0.74, 0.88, 0.82).line(0.88, 0.82, 0.12, 0.82).close(), body);
  c.fillEllipse(shade(body, -0.35), 0.45, 0.06, 0.1, 0.1);
}

/** Calendar: a page with a coloured top, its rings and its days, for something on a schedule */
function paintCalendar(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.08, 0.12, 0.84, 0.82, 0.08), body);
  c.fillRect("#FAFAFA", 0.14, 0.34, 0.72, 0.54);
  for (const y of [0.42, 0.57, 0.72]) {
    for (const x of [0.21, 0.37, 0.53, 0.69]) c.fillRect(shade(body, -0.1), x, y, 0.1, 0.09);
  }
  const ring = shade(body, -0.4);
  c.fill(roundRect(0.26, 0.05, 0.08, 0.16, 0.04), ring);
  c.fill(roundRect(0.66, 0.05, 0.08, 0.16, 0.04), ring);
}

/** Lab: a flask of the body's colour, for a lab or test system */
function paintFlask(c: Canvas, body: string): void {
  const flask: Pt[] = [[0.38, 0.06], [0.62, 0.06], [0.62, 0.36], [0.9, 0.88], [0.1, 0.88], [0.38, 0.36]];
  c.fillPolygon({ color: "#ECEFF1", alpha: 200 }, flask);
  // the liquid: the flask below 0.58, where its sides are at x 0.268 and 0.732
  c.fill(new Path().polygon([[0.262, 0.58], [0.738, 0.58], [0.9, 0.88], [0.1, 0.88]]),
    { from: [0, 0.58], to: [0, 0.89], start: shade(body, 0.18), end: shade(body, -0.12) });
  c.strokePolygon({ color: shade(body, -0.35), width: 0.045, join: "round" }, flask);
  c.fillEllipse({ color: GLYPH, alpha: 200 }, 0.4, 0.66, 0.08, 0.08);
  c.fillEllipse({ color: GLYPH, alpha: 200 }, 0.54, 0.74, 0.06, 0.06);
}

/** Bug: a beetle with its legs and feelers, for testing or a known problem */
function paintBug(c: Canvas, body: string): void {
  const leg: Pen = { color: shade(body, -0.35), width: 0.06, cap: "round" };
  for (const [y, dy] of [[0.48, -0.08], [0.62, 0], [0.76, 0.08]]) {
    c.line(leg, 0.3, y, 0.1, y + dy);
    c.line(leg, 0.7, y, 0.9, y + dy);
  }
  c.line(leg, 0.43, 0.2, 0.33, 0.06);
  c.line(leg, 0.57, 0.2, 0.67, 0.06);
  fillBody(c, new Path().ellipse(0.37, 0.14, 0.26, 0.22), shade(body, -0.2));
  fillBody(c, new Path().ellipse(0.26, 0.3, 0.48, 0.64), body);
  c.line({ color: { color: GLYPH, alpha: 200 }, width: 0.04 }, 0.5, 0.34, 0.5, 0.9);
}

/** Code: </> on a rounded square, for development */
function paintCode(c: Canvas, body: string): void {
  fillBody(c, roundRect(0.06, 0.06, 0.88, 0.88, 0.16), body);
  const pen: Pen = { color: GLYPH, width: 0.08, cap: "round", join: "round" };
  c.lines(pen, [[0.34, 0.32], [0.18, 0.5], [0.34, 0.68]]);
  c.lines(pen, [[0.66, 0.32], [0.82, 0.5], [0.66, 0.68]]);
  c.line(pen, 0.56, 0.27, 0.44, 0.73);
}

/** Heart: for a favourite */
function paintHeart(c: Canvas, body: string): void {
  fillBody(c, new Path().bezier(0.5, 0.9, 0.2, 0.68, 0.04, 0.52, 0.06, 0.32).bezier(0.06, 0.32, 0.08, 0.1, 0.4, 0.04, 0.5, 0.26)
    .bezier(0.5, 0.26, 0.6, 0.04, 0.92, 0.1, 0.94, 0.32).bezier(0.94, 0.32, 0.96, 0.52, 0.8, 0.68, 0.5, 0.9).close(), body);
}

/** Tag: a price tag on the slant, its hole one the background shows through, for a label */
function paintTag(c: Canvas, body: string): void {
  c.transformed("rotate(-45 0.5 0.5)", () => {
    // GDI+'s Alternate fill: the circle is the hole
    fillBody(c, new Path().polygon([[0.1, 0.5], [0.28, 0.31], [0.86, 0.31], [0.86, 0.69], [0.28, 0.69]])
      .ellipse(0.23, 0.445, 0.11, 0.11), body);
  });
}

// ---------------------------------------------------------------------------------------------------------------------
// Toolbar glyphs

/** Connect / send: a white tick on a disc */
function paintCheck(c: Canvas, body: string): void {
  disc(c, body);
  c.lines({ color: GLYPH, width: 0.12, cap: "round", join: "round" }, [[0.28, 0.52], [0.44, 0.67], [0.73, 0.35]]);
}

/** Clear: a white cross on a disc */
function paintClear(c: Canvas, body: string): void {
  disc(c, body);
  const cross: Pen = { color: GLYPH, width: 0.11, cap: "round" };
  c.line(cross, 0.34, 0.34, 0.66, 0.66);
  c.line(cross, 0.66, 0.34, 0.34, 0.66);
}

/** Mask text: a white asterisk on a disc */
function paintMask(c: Canvas, body: string): void {
  disc(c, body);
  const spoke: Pen = { color: GLYPH, width: 0.09, cap: "round" };
  for (let i = 0; i < 3; i++) {
    const a = Math.PI / 2 + (i * Math.PI) / 3;
    const dx = Math.cos(a) * 0.22, dy = Math.sin(a) * 0.22;
    c.line(spoke, 0.5 - dx, 0.5 - dy, 0.5 + dx, 0.5 + dy);
  }
}

/** Text shows (not masked): a white eye on a disc */
function paintEye(c: Canvas, body: string): void {
  disc(c, body);
  const eye = new Path().bezier(0.2, 0.5, 0.32, 0.3, 0.68, 0.3, 0.8, 0.5).bezier(0.8, 0.5, 0.68, 0.7, 0.32, 0.7, 0.2, 0.5);
  c.fill(eye, GLYPH);
  c.fillEllipse(body, 0.42, 0.42, 0.16, 0.16);
}

/** Stop / no filter: a white bar on a disc */
function paintStop(c: Canvas, body: string): void {
  disc(c, body);
  c.fillRect(GLYPH, 0.26, 0.44, 0.48, 0.12);
}

/** Search: a white magnifying glass on a disc */
function paintSearch(c: Canvas, body: string): void {
  disc(c, body);
  c.strokeEllipse({ color: GLYPH, width: 0.08 }, 0.28, 0.26, 0.3, 0.3);
  c.line({ color: GLYPH, width: 0.11, cap: "round" }, 0.54, 0.53, 0.7, 0.69);
}

/** New session: a grey workstation with a plus badge */
function paintNewSession(c: Canvas, body: string): void {
  paintWorkstation(c, "#607D8B");
  plusBadge(c, body);
}

/** New folder: a folder with a plus badge */
function paintNewFolder(c: Canvas, body: string): void {
  paintFolder(c, "#F2B134");
  plusBadge(c, body);
}

/** Expand all / collapse all: a tree's plus or minus box */
function paintTreeBox(c: Canvas, body: string, plus: boolean): void {
  fillBody(c, roundRect(0.12, 0.12, 0.76, 0.76, 0.12), body);
  const line: Pen = { color: GLYPH, width: 0.1, cap: "round" };
  c.line(line, 0.3, 0.5, 0.7, 0.5);
  if (plus) {
    c.line(line, 0.5, 0.3, 0.5, 0.7);
  }
}

/** Sort: two letters, one above the other, beside a downward arrow, on a disc */
function paintSort(c: Canvas, body: string, top: string, bottom: string): void {
  disc(c, body);
  centredText(c, top, 0.14, 0.16, 0.4, 0.34, 0.32, GLYPH);
  centredText(c, bottom, 0.14, 0.48, 0.4, 0.34, 0.32, GLYPH);
  arrow(c, GLYPH, [0.66, 0.22], [0.66, 0.8], 0.08, 0.16);
}

/** Run script: a page with a code bracket */
function paintScript(c: Canvas, body: string): void {
  const outline: Pt[] = [[0.16, 0.06], [0.62, 0.06], [0.84, 0.28], [0.84, 0.94], [0.16, 0.94]];
  c.fillPolygon("#FAFAFA", outline);
  c.strokePolygon({ color: "#8A8A8A", width: 0.045 }, outline);
  const code: Pen = { color: body, width: 0.09, cap: "round", join: "round" };
  c.lines(code, [[0.40, 0.42], [0.28, 0.58], [0.40, 0.74]]);
  c.lines(code, [[0.60, 0.42], [0.72, 0.58], [0.60, 0.74]]);
}

/** Select tabs: a window with a pencil */
function paintSelectTabs(c: Canvas, body: string): void {
  c.fillRect("#FAFAFA", 0.06, 0.1, 0.78, 0.7);
  c.fillRect(body, 0.06, 0.1, 0.78, 0.16);
  c.strokeRect({ color: shade(body, -0.2), width: 0.045 }, 0.06, 0.1, 0.78, 0.7);
  // the pencil is a 0.14-wide line from (0.9, 0.42) to (0.5, 0.86) with a LineCap.Triangle start, which SVG has no
  // cap for: so it's filled as the outline that stroke makes, the start's point half the width past its end
  const from: Pt = [0.9, 0.42], to: Pt = [0.5, 0.86], h = 0.14 / 2;
  const dx = to[0] - from[0], dy = to[1] - from[1], length = Math.sqrt(dx * dx + dy * dy);
  const ux = dx / length, uy = dy / length, nx = -uy * h, ny = ux * h;
  c.fillPolygon("#F29C1F", [
    [from[0] - ux * h, from[1] - uy * h],
    [from[0] + nx, from[1] + ny],
    [to[0] + nx, to[1] + ny],
    [to[0] - nx, to[1] - ny],
    [from[0] - nx, from[1] - ny],
  ]);
}

// ---------------------------------------------------------------------------------------------------------------------
// The registry

/** A painter, as the C# Painter delegate */
type Painter = (c: Canvas, body: string) => void;

/** Every template and glyph by key (lower case, as in IconTemplates: "switch", "l3switch", ..., "check", "sortaz") */
export const templates: Record<string, Template> = {};

/** Template keys (not glyphs) in picker order, as IconTemplates.Keys */
export const templateOrder: string[] = [];

/** Register a template or glyph */
function register(key: string, title: string, color: string, paint: Painter, general: boolean, glyph: boolean): void {
  templates[key] = {
    title,
    color,
    general,
    glyph,
    draw: (body, uid) => {
      const c = new Canvas(uid);
      paint(c, body);
      return c.markup();
    },
  };
  if (!glyph) templateOrder.push(key);
}

/** Register a device template */
const add = (key: string, title: string, color: string, paint: Painter) => register(key, title, color, paint, false, false);
/** Register a general icon: listed under "General" rather than "Devices" in the picker */
const addGeneral = (key: string, title: string, color: string, paint: Painter) => register(key, title, color, paint, true, false);
/** Register a toolbar glyph: drawable like a template, titled by its key, but not in `templateOrder` */
const addGlyph = (key: string, color: string, paint: Painter) => register(key, key, color, paint, false, true);

add("switch", "Switch", "#2E7D32", paintSwitch);
add("l3switch", "Layer 3 switch", "#1E63B4", paintL3Switch);
add("stackswitch", "Stack switch", "#2E7D32", paintStackSwitch);
add("coreswitch", "Core switch", "#1E63B4", paintCoreSwitch);
add("distswitch", "Distribution switch", "#2E7D32", paintDistributionSwitch);
add("router", "Router", "#1E63B4", paintRouter);
add("voicerouter", "Voice router", "#6A3D9A", paintVoiceRouter);
add("nexus", "Data centre switch", "#2E7D32", paintNexus);
add("firewall", "Firewall", "#C64B2B", paintFirewall);
add("vpn", "VPN concentrator", "#6A3D9A", paintVpn);
add("wireless", "Wireless", "#1E63B4", paintWireless);
add("accesspoint", "Access point", "#1E63B4", paintAccessPoint);
add("wlc", "Wireless controller", "#1E63B4", paintWirelessController);
add("loadbalancer", "Load balancer", "#00838F", paintLoadBalancer);
add("consoleserver", "Console server", "#455A64", paintConsoleServer);
add("ups", "UPS", "#EF6C00", paintUps);
add("server", "Server", "#607D8B", paintServer);
add("workstation", "Workstation", "#607D8B", paintWorkstation);
add("terminal", "Terminal", "#455A64", paintTerminal);
add("windows", "Windows", "#0078D4", paintWindows);
add("database", "Database", "#EF6C00", paintDatabase);
add("storage", "Storage", "#607D8B", paintStorage);
add("cloud", "Cloud", "#039BE5", paintCloud);
add("phone", "IP phone", "#607D8B", paintPhone);
add("camera", "Camera", "#607D8B", paintCamera);
add("printer", "Printer", "#607D8B", paintPrinter);
add("folder", "Folder", "#F2B134", paintFolder);
add("flag", "Flag", "#C62828", paintFlag);

// general icons, replacing the old Silk PNGs of the same meaning
addGeneral("house", "Home", "#C64B2B", paintHouse);
addGeneral("globe", "Globe", "#1E88E5", paintGlobe);
addGeneral("map", "Map", "#43A047", paintMap);
addGeneral("truck", "Truck", "#EF6C00", paintTruck);
addGeneral("book", "Book", "#1E63B4", paintBook);
addGeneral("document", "Document", "#1E63B4", paintDocument);
addGeneral("archive", "Archive", "#A1887F", paintArchive);
addGeneral("email", "Email", "#1E63B4", paintEmail);
addGeneral("chat", "Chat", "#1E88E5", paintChat);
addGeneral("rss", "Feed", "#F57C00", paintRss);
addGeneral("clock", "Clock", "#1E63B4", paintClock);
addGeneral("help", "Help", "#1E63B4", paintHelp);
addGeneral("info", "Information", "#1E63B4", paintInfo);
addGeneral("lightbulb", "Light bulb", "#F9A825", paintLightbulb);
addGeneral("bolt", "Lightning", "#F9A825", paintBolt);
addGeneral("tv", "Television", "#607D8B", paintTv);
addGeneral("mobile", "Mobile device", "#607D8B", paintMobile);
addGeneral("star", "Star", "#F9A825", paintStar);
addGeneral("lock", "Padlock", "#607D8B", paintLock);
// general icons of SuperXTerminal's own, for what sessions are often sorted by
addGeneral("building", "Building", "#607D8B", paintBuilding);
addGeneral("person", "Person", "#1E63B4", paintPerson);
addGeneral("key", "Key", "#F9A825", paintKey);
addGeneral("shield", "Shield", "#2E7D32", paintShield);
addGeneral("gear", "Settings", "#607D8B", paintGear);
addGeneral("wrench", "Wrench", "#78909C", paintWrench);
addGeneral("warning", "Warning", "#F9A825", paintWarning);
addGeneral("bell", "Bell", "#F9A825", paintBell);
addGeneral("calendar", "Calendar", "#C62828", paintCalendar);
addGeneral("flask", "Lab", "#8E24AA", paintFlask);
addGeneral("bug", "Bug", "#43A047", paintBug);
addGeneral("code", "Code", "#455A64", paintCode);
addGeneral("heart", "Heart", "#D81B60", paintHeart);
addGeneral("tag", "Tag", "#00838F", paintTag);

// toolbar glyphs: drawn the same way so they match and scale with the icons, but not offered for sessions
addGlyph("check", "#2E9E44", paintCheck);
addGlyph("clear", "#78868F", paintClear);
addGlyph("mask", "#78868F", paintMask);
addGlyph("masked", "#E68A00", paintMask);
addGlyph("unmasked", "#78868F", paintEye);
addGlyph("stop", "#D32F2F", paintStop);
addGlyph("script", "#1E63B4", paintScript);
addGlyph("selecttabs", "#1E63B4", paintSelectTabs);
addGlyph("search", "#1E63B4", paintSearch);
addGlyph("newsession", "#2E9E44", paintNewSession);
addGlyph("newfolder", "#2E9E44", paintNewFolder);
addGlyph("expandall", "#607D8B", (c, body) => paintTreeBox(c, body, true));
addGlyph("collapseall", "#607D8B", (c, body) => paintTreeBox(c, body, false));
addGlyph("sortaz", "#1E63B4", (c, body) => paintSort(c, body, "A", "Z"));
addGlyph("sortza", "#1E63B4", (c, body) => paintSort(c, body, "Z", "A"));
