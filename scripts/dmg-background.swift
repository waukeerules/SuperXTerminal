// Draws the .dmg window's background (src-tauri/dmg/background.tiff): the app on the left, Applications on the right
// (their icons are placed there by tauri.conf.json's bundle.macOS.dmg), an arrow between them and what to do. Light,
// as Finder writes the icons' names in dark text. Drawn at 1x and 2x, joined into one TIFF so Retina screens get the
// sharp one. Run from the project folder:
//   swift scripts/dmg-background.swift && tiffutil -cathidpicheck src-tauri/dmg/background.png src-tauri/dmg/background@2x.png -out src-tauri/dmg/background.tiff && rm src-tauri/dmg/background*.png
import AppKit

let size = CGSize(width: 660, height: 400)
// where tauri.conf.json puts the icons (points, from the top left). Everything sits in the top 300 points: Finder's
// tab and path bars, when the user has them on, cover the window's bottom
let app = CGPoint(x: 180, y: 165)
let applications = CGPoint(x: 480, y: 165)

func draw(scale: CGFloat, to path: String) {
    let width = Int(size.width * scale), height = Int(size.height * scale)
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    context.scaleBy(x: scale, y: scale)
    // top-left origin, as Finder's positions
    context.translateBy(x: 0, y: size.height)
    context.scaleBy(x: 1, y: -1)

    // a soft light gradient
    let gradient = CGGradient(colorsSpace: space, colors: [
        CGColor(srgbRed: 0.97, green: 0.97, blue: 0.98, alpha: 1),
        CGColor(srgbRed: 0.90, green: 0.91, blue: 0.93, alpha: 1),
    ] as CFArray, locations: [0, 1])!
    context.drawLinearGradient(gradient, start: .zero, end: CGPoint(x: 0, y: size.height), options: [])

    // the arrow from the app to Applications, in the app's accent blue
    let accent = CGColor(srgbRed: 0.0, green: 0.48, blue: 0.80, alpha: 0.85)
    let from = app.x + 70, to = applications.x - 70, y = app.y
    context.setStrokeColor(accent)
    context.setLineWidth(5)
    context.setLineCap(.round)
    context.move(to: CGPoint(x: from, y: y))
    context.addLine(to: CGPoint(x: to - 6, y: y))
    context.strokePath()
    context.setFillColor(accent)
    context.move(to: CGPoint(x: to + 6, y: y))
    context.addLine(to: CGPoint(x: to - 14, y: y - 13))
    context.addLine(to: CGPoint(x: to - 14, y: y + 13))
    context.closePath()
    context.fillPath()

    // the words, drawn the right way up
    let graphics = NSGraphicsContext(cgContext: context, flipped: true)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = graphics
    func text(_ string: String, size: CGFloat, weight: NSFont.Weight, colour: NSColor, y: CGFloat) {
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: size, weight: weight), .foregroundColor: colour, .paragraphStyle: paragraph,
        ]
        NSAttributedString(string: string, attributes: attributes).draw(in: CGRect(x: 0, y: y, width: 660, height: size * 1.5))
    }
    text("SuperXTerminal", size: 22, weight: .semibold, colour: NSColor(srgbRed: 0.12, green: 0.12, blue: 0.14, alpha: 1), y: 28)
    text("Drag SuperXTerminal onto Applications to install it", size: 13, weight: .regular,
         colour: NSColor(srgbRed: 0.35, green: 0.36, blue: 0.40, alpha: 1), y: 262)
    NSGraphicsContext.restoreGraphicsState()

    let image = context.makeImage()!
    let destination = CGImageDestinationCreateWithURL(URL(fileURLWithPath: path) as CFURL, "public.png" as CFString, 1, nil)!
    CGImageDestinationAddImage(destination, image, [kCGImagePropertyDPIWidth: 72 * scale, kCGImagePropertyDPIHeight: 72 * scale] as CFDictionary)
    CGImageDestinationFinalize(destination)
}

try? FileManager.default.createDirectory(atPath: "src-tauri/dmg", withIntermediateDirectories: true)
draw(scale: 1, to: "src-tauri/dmg/background.png")
draw(scale: 2, to: "src-tauri/dmg/background@2x.png")
print("Drew src-tauri/dmg/background.png and background@2x.png")
