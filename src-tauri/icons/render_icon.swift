import Foundation
import CoreGraphics
import ImageIO

// 使い方: swift render_icon.swift <output.png> [mode]
//   mode:
//     (なし)          … アプリアイコン(緑角丸・バッジなし)
//     badge           … アプリアイコン(緑角丸・赤バッジ付き)
//     menubar         … メニューバー用テンプレート(黒インボックスのみ)
//     menubar-unread  … メニューバー用テンプレート(黒インボックス+ドット)
let out = CommandLine.arguments[1]
let mode = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : ""
let isMenubar = mode.hasPrefix("menubar")

let size = 1024
let cs = CGColorSpace(name: CGColorSpace.sRGB)!
let ctx = CGContext(
    data: nil, width: size, height: size,
    bitsPerComponent: 8, bytesPerRow: 0, space: cs,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
)!

// SVG風の座標系(左上原点・y下向き)に変換
ctx.translateBy(x: 0, y: CGFloat(size))
ctx.scaleBy(x: 1, y: -1)

func rgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> CGColor {
    CGColor(colorSpace: cs, components: [r / 255, g / 255, b / 255, 1])!
}
let greenTop = rgb(0x45, 0xC0, 0x8E)
let greenBottom = rgb(0x2B, 0x8F, 0x68)
let white = rgb(0xFF, 0xFF, 0xFF)
let red = rgb(0xE5, 0x48, 0x4D)
let black = rgb(0, 0, 0)

if isMenubar {
    // テンプレートアイコン: 背景なし。インボックス(内容中心 512,550)を
    // キャンバス中央へ寄せて少し拡大する
    ctx.translateBy(x: 512, y: 512)
    ctx.scaleBy(x: 1.2, y: 1.2)
    ctx.translateBy(x: -512, y: -550)
} else {
    // 背景: フルブリードの角丸スクエア + 縦グラデーション
    let bgRect = CGRect(x: 0, y: 0, width: 1024, height: 1024)
    let bgPath = CGPath(roundedRect: bgRect, cornerWidth: 224, cornerHeight: 224, transform: nil)
    ctx.saveGState()
    ctx.addPath(bgPath)
    ctx.clip()
    let gradient = CGGradient(
        colorsSpace: cs,
        colors: [greenTop, greenBottom] as CFArray,
        locations: [0, 1]
    )!
    ctx.drawLinearGradient(gradient, start: CGPoint(x: 512, y: 0), end: CGPoint(x: 512, y: 1024), options: [])
    ctx.restoreGState()
}

let fg = isMenubar ? black : white

// インボックス(受信トレイ)
ctx.setStrokeColor(fg)
ctx.setLineJoin(.round)
ctx.setLineCap(.round)

// 外枠(トレイの箱)
ctx.setLineWidth(80)
let box = CGPath(
    roundedRect: CGRect(x: 208, y: 300, width: 608, height: 500),
    cornerWidth: 92, cornerHeight: 92, transform: nil
)
ctx.addPath(box)
ctx.strokePath()

// トレイの受け口(中央がへこんだ横ライン = ここに届く)
ctx.beginPath()
ctx.move(to: CGPoint(x: 248, y: 572))
ctx.addLine(to: CGPoint(x: 412, y: 572))
ctx.addLine(to: CGPoint(x: 472, y: 644))
ctx.addLine(to: CGPoint(x: 552, y: 644))
ctx.addLine(to: CGPoint(x: 612, y: 572))
ctx.addLine(to: CGPoint(x: 776, y: 572))
ctx.strokePath()

let dotCenter = CGPoint(x: 796, y: 300)
if mode == "badge" {
    // 未読バッジ(白フチ + 赤丸)
    ctx.setFillColor(white)
    ctx.fillEllipse(in: CGRect(x: dotCenter.x - 132, y: dotCenter.y - 132, width: 264, height: 264))
    ctx.setFillColor(red)
    ctx.fillEllipse(in: CGRect(x: dotCenter.x - 104, y: dotCenter.y - 104, width: 208, height: 208))
} else if mode == "menubar-unread" {
    // トレイとの分離用に周囲をくり抜いてから単色ドットを描く
    ctx.setBlendMode(.clear)
    ctx.fillEllipse(in: CGRect(x: dotCenter.x - 150, y: dotCenter.y - 150, width: 300, height: 300))
    ctx.setBlendMode(.normal)
    ctx.setFillColor(black)
    ctx.fillEllipse(in: CGRect(x: dotCenter.x - 108, y: dotCenter.y - 108, width: 216, height: 216))
}

let img = ctx.makeImage()!
let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: out) as CFURL, "public.png" as CFString, 1, nil)!
CGImageDestinationAddImage(dest, img, nil)
guard CGImageDestinationFinalize(dest) else {
    fatalError("failed to write \(out)")
}
print("wrote \(out)")
