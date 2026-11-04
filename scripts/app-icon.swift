// Draws SuperXTerminal's app icon (src-tauri/icons/app-icon.png, 1024 x 1024): SuperXPuTTY's terminal screen with its
// ">_" prompt, as a Mac icon (a rounded square on Apple's icon grid, with its shadow), and a blue "X" raised like a
// superscript for the X in the name. Run from the project folder, then make every size the app needs, and Windows'
// icon from the tile alone (scripts/windows-icon.py):
//   swift scripts/app-icon.swift && npx tauri icon src-tauri/icons/app-icon.png && python3 scripts/windows-icon.py
import AppKit

/// The icon's size in pixels, square
let canvas: CGFloat = 1024
/// sRGB, the colour space every colour here is given in
let space = CGColorSpace(name: CGColorSpace.sRGB)!
/// What the icon is drawn on
let context = CGContext(data: nil, width: Int(canvas), height: Int(canvas), bitsPerComponent: 8, bytesPerRow: 0, space: space,
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
/// A colour from its hex value (0xRRGGBB), and how opaque
func colour(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: CGFloat((hex >> 16) & 0xff) / 255, green: CGFloat((hex >> 8) & 0xff) / 255, blue: CGFloat(hex & 0xff) / 255, alpha: alpha)
}

// Apple's grid: an 824-point tile in the middle of 1024, corners of about 185
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
let tilePath = CGPath(roundedRect: tile, cornerWidth: 185, cornerHeight: 185, transform: nil)

// its shadow, below it
context.saveGState()
context.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: colour(0x000000, 0.35))
context.addPath(tilePath)
context.setFillColor(colour(0x2a2c31))
context.fillPath()
context.restoreGState()

// the tile: graphite, lighter at the top
context.saveGState()
context.addPath(tilePath)
context.clip()
context.drawLinearGradient(CGGradient(colorsSpace: space, colors: [colour(0x4a4e57), colour(0x24262b)] as CFArray, locations: [0, 1])!,
                           start: CGPoint(x: 0, y: tile.maxY), end: CGPoint(x: 0, y: tile.minY), options: [])
// a soft sheen along the top edge
context.setStrokeColor(colour(0xffffff, 0.18))
context.setLineWidth(6)
context.addPath(CGPath(roundedRect: tile.insetBy(dx: 3, dy: 3), cornerWidth: 182, cornerHeight: 182, transform: nil))
context.strokePath()
context.restoreGState()

// the screen, set into the tile
let screen = CGRect(x: 172, y: 236, width: 680, height: 560)
let screenPath = CGPath(roundedRect: screen, cornerWidth: 64, cornerHeight: 64, transform: nil)
context.saveGState()
context.addPath(screenPath)
context.clip()
context.drawLinearGradient(CGGradient(colorsSpace: space, colors: [colour(0x1b1d22), colour(0x0e0f12)] as CFArray, locations: [0, 1])!,
                           start: CGPoint(x: 0, y: screen.maxY), end: CGPoint(x: 0, y: screen.minY), options: [])
// the glass's reflection, across its top
context.setFillColor(colour(0xffffff, 0.05))
context.move(to: CGPoint(x: screen.minX, y: screen.maxY))
context.addLine(to: CGPoint(x: screen.maxX, y: screen.maxY))
context.addLine(to: CGPoint(x: screen.maxX, y: screen.maxY - 150))
context.addLine(to: CGPoint(x: screen.minX, y: screen.maxY - 260))
context.closePath()
context.fillPath()
context.restoreGState()
context.addPath(screenPath)
context.setStrokeColor(colour(0x000000, 0.6))
context.setLineWidth(5)
context.strokePath()

// the words: ">_" in white, and the blue X
let graphics = NSGraphicsContext(cgContext: context, flipped: false)
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = graphics
/// Draw text at a point, in the system font or its monospaced one
func draw(_ text: String, size: CGFloat, weight: NSFont.Weight, colour: NSColor, at point: CGPoint, mono: Bool) {
    let font = mono ? NSFont.monospacedSystemFont(ofSize: size, weight: weight) : NSFont.systemFont(ofSize: size, weight: weight)
    NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: colour]).draw(at: point)
}
draw(">_", size: 300, weight: .bold, colour: NSColor(srgbRed: 0.93, green: 0.94, blue: 0.96, alpha: 1), at: CGPoint(x: 222, y: 300), mono: true)
// the X, raised to the top right as a superscript, with a little glow
context.saveGState()
context.setShadow(offset: .zero, blur: 24, color: colour(0x3399ff, 0.55))
draw("X", size: 250, weight: .heavy, colour: NSColor(srgbRed: 0.20, green: 0.60, blue: 1.0, alpha: 1), at: CGPoint(x: 600, y: 470), mono: false)
context.restoreGState()
NSGraphicsContext.restoreGraphicsState()

/// The finished icon, written to src-tauri/icons/app-icon.png
let image = context.makeImage()!
let url = URL(fileURLWithPath: "src-tauri/icons/app-icon.png")
let destination = CGImageDestinationCreateWithURL(url as CFURL, "public.png" as CFString, 1, nil)!
CGImageDestinationAddImage(destination, image, nil)
CGImageDestinationFinalize(destination)
print("Drew src-tauri/icons/app-icon.png")
