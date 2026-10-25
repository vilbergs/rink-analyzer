// Draws the app icon (a top-down rink on a dark tile) as a 1024 px PNG.
// Usage: swift scripts/make-icon.swift out.png
import AppKit

let size: CGFloat = 1024
let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()
let ctx = NSGraphicsContext.current!.cgContext

func rgb(_ hex: UInt32) -> CGColor {
    CGColor(red: CGFloat((hex >> 16) & 0xff) / 255, green: CGFloat((hex >> 8) & 0xff) / 255,
            blue: CGFloat(hex & 0xff) / 255, alpha: 1)
}

// Tile, inset like macOS icons.
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
ctx.addPath(CGPath(roundedRect: tile, cornerWidth: 185, cornerHeight: 185, transform: nil))
ctx.setFillColor(rgb(0x16191f))
ctx.fillPath()

// The rink: 60 x 30 m with 8.5 m corners.
let rink = CGRect(x: 172, y: 332, width: 680, height: 360)
let corner: CGFloat = 96
ctx.addPath(CGPath(roundedRect: rink, cornerWidth: corner, cornerHeight: corner, transform: nil))
ctx.setFillColor(rgb(0xf3f6f9))
ctx.fillPath()

// Everything on the ice stays inside the boards.
ctx.saveGState()
ctx.addPath(CGPath(roundedRect: rink, cornerWidth: corner, cornerHeight: corner, transform: nil))
ctx.clip()

func vline(_ x: CGFloat, _ w: CGFloat, _ color: UInt32) {
    ctx.setFillColor(rgb(color))
    ctx.fill(CGRect(x: x - w / 2, y: rink.minY, width: w, height: rink.height))
}
vline(rink.midX, 14, 0xc8322d)
vline(rink.minX + 0.38 * rink.width, 12, 0x1f4fbf)
vline(rink.maxX - 0.38 * rink.width, 12, 0x1f4fbf)
vline(rink.minX + 0.067 * rink.width, 4, 0xc8322d)
vline(rink.maxX - 0.067 * rink.width, 4, 0xc8322d)

// Center circle.
ctx.setStrokeColor(rgb(0x1f4fbf))
ctx.setLineWidth(5)
ctx.strokeEllipse(in: CGRect(x: rink.midX - 60, y: rink.midY - 60, width: 120, height: 120))

// You (orange ring) and a ghost route to where you should have been.
let me = CGPoint(x: 330, y: 600)
let ghost = CGPoint(x: 470, y: 470)
ctx.setStrokeColor(rgb(0x35c47c))
ctx.setLineWidth(10)
ctx.setLineDash(phase: 0, lengths: [22, 16])
ctx.move(to: me)
ctx.addQuadCurve(to: ghost, control: CGPoint(x: 430, y: 610))
ctx.strokePath()
ctx.setLineDash(phase: 0, lengths: [])
ctx.setFillColor(rgb(0x35c47c).copy(alpha: 0.35)!)
ctx.fillEllipse(in: CGRect(x: ghost.x - 44, y: ghost.y - 44, width: 88, height: 88))
ctx.setFillColor(rgb(0x1f5fbf))
ctx.fillEllipse(in: CGRect(x: me.x - 40, y: me.y - 40, width: 80, height: 80))
ctx.setStrokeColor(rgb(0xf2a93b))
ctx.setLineWidth(12)
ctx.strokeEllipse(in: CGRect(x: me.x - 52, y: me.y - 52, width: 104, height: 104))

ctx.restoreGState()

image.unlockFocus()
let rep = NSBitmapImageRep(data: image.tiffRepresentation!)!
let png = rep.representation(using: .png, properties: [:])!
try! png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
