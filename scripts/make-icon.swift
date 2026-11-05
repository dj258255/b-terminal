// b-terminal 앱 아이콘을 코드로 그린다(재현할 수 있게). 사용: swift scripts/make-icon.swift assets/icon
// 어두운 둥근 사각형 위에 초록 프롬프트(>)와 밑줄 커서, 왼쪽 위에 작은 b
import AppKit

let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "assets/icon"
let iconset = "\(outDir)/b-terminal.iconset"
try? FileManager.default.createDirectory(atPath: iconset, withIntermediateDirectories: true)

func draw(_ px: Int) -> Data {
    let s = CGFloat(px)
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    // macOS 아이콘 격자: 가장자리 여백 약 10%
    let inset = s * 0.1
    let rect = NSRect(x: inset, y: inset, width: s - inset * 2, height: s - inset * 2)
    let bg = NSBezierPath(roundedRect: rect, xRadius: rect.width * 0.225, yRadius: rect.width * 0.225)
    NSGradient(starting: NSColor(red: 0.20, green: 0.22, blue: 0.27, alpha: 1),
               ending: NSColor(red: 0.11, green: 0.12, blue: 0.15, alpha: 1))!.draw(in: bg, angle: -90)
    NSColor(white: 1, alpha: 0.08).setStroke()
    bg.lineWidth = max(1, s * 0.006)
    bg.stroke()
    // 프롬프트 >
    let green = NSColor(red: 0.60, green: 0.85, blue: 0.48, alpha: 1)
    let p = NSBezierPath()
    p.lineWidth = s * 0.07
    p.lineCapStyle = .round
    p.lineJoinStyle = .round
    p.move(to: NSPoint(x: s * 0.30, y: s * 0.62))
    p.line(to: NSPoint(x: s * 0.46, y: s * 0.48))
    p.line(to: NSPoint(x: s * 0.30, y: s * 0.34))
    green.setStroke()
    p.stroke()
    // 커서 _
    NSColor(red: 0.85, green: 0.86, blue: 0.89, alpha: 1).setFill()
    NSBezierPath(roundedRect: NSRect(x: s * 0.52, y: s * 0.30, width: s * 0.20, height: s * 0.07),
                 xRadius: s * 0.02, yRadius: s * 0.02).fill()
    // 작은 b
    let font = NSFont.monospacedSystemFont(ofSize: s * 0.16, weight: .bold)
    let attrs: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: NSColor(red: 0.48, green: 0.68, blue: 0.95, alpha: 1)]
    ("b" as NSString).draw(at: NSPoint(x: s * 0.24, y: s * 0.66), withAttributes: attrs)
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

for (name, px) in [("16x16", 16), ("16x16@2x", 32), ("32x32", 32), ("32x32@2x", 64), ("128x128", 128),
                   ("128x128@2x", 256), ("256x256", 256), ("256x256@2x", 512), ("512x512", 512), ("512x512@2x", 1024)] {
    try! draw(px).write(to: URL(fileURLWithPath: "\(iconset)/icon_\(name).png"))
}
try! draw(512).write(to: URL(fileURLWithPath: "\(outDir)/b-terminal-512.png"))
print("ok: \(iconset)")
