import SwiftUI
import UIKit

public struct PolyUITheme: Sendable {
    public var background: Color
    public var sunken: Color
    public var raised: Color
    public var foreground: Color
    public var muted: Color
    public var border: Color
    public var borderStrong: Color
    public var accent: Color
    public var accentForeground: Color
    public var positive: Color
    public var negative: Color
    public var warning: Color
    public var info: Color
    public var series: [Color]
    public var chartGrid: Color
    public var chartAxis: Color
    public var cornerRadius: CGFloat
    public var innerCornerRadius: CGFloat
    public var controlCornerRadius: CGFloat
    public var spacing: CGFloat
    public var cardShadow: Bool

    public init(
        background: Color, sunken: Color, raised: Color, foreground: Color, muted: Color,
        border: Color, borderStrong: Color, accent: Color, accentForeground: Color,
        positive: Color, negative: Color, warning: Color, info: Color, series: [Color],
        chartGrid: Color, chartAxis: Color, cornerRadius: CGFloat = 16, innerCornerRadius: CGFloat = 12,
        controlCornerRadius: CGFloat = 10, spacing: CGFloat = 16, cardShadow: Bool = true
    ) {
        self.background = background
        self.sunken = sunken
        self.raised = raised
        self.foreground = foreground
        self.muted = muted
        self.border = border
        self.borderStrong = borderStrong
        self.accent = accent
        self.accentForeground = accentForeground
        self.positive = positive
        self.negative = negative
        self.warning = warning
        self.info = info
        self.series = series.isEmpty ? [accent] : series
        self.chartGrid = chartGrid
        self.chartAxis = chartAxis
        self.cornerRadius = cornerRadius
        self.innerCornerRadius = innerCornerRadius
        self.controlCornerRadius = controlCornerRadius
        self.spacing = spacing
        self.cardShadow = cardShadow
    }

    public static let standard = PolyUITheme(
        background: .adaptive(light: 0xFEFDFC, dark: 0x191B1E),
        sunken: .adaptive(light: 0xF5F5F2, dark: 0x121316),
        raised: .adaptive(light: 0xFFFEFD, dark: 0x222428),
        foreground: .adaptive(light: 0x1A1D23, dark: 0xF1F0ED),
        muted: .adaptive(light: 0x575B63, dark: 0xA1A5AB),
        border: .adaptive(light: 0xDFE1E5, dark: 0x2D2F33),
        borderStrong: .adaptive(light: 0xC1C4C9, dark: 0x484A50),
        accent: .adaptive(light: 0x00686A, dark: 0x5ED0C7),
        accentForeground: .adaptive(light: 0xF9FDFD, dark: 0x011D1F),
        positive: .adaptive(light: 0x0A6D37, dark: 0x6AD18A),
        negative: .adaptive(light: 0xB02B27, dark: 0xF87E77),
        warning: .adaptive(light: 0x995600, dark: 0xF2B95A),
        info: .adaptive(light: 0x1B62A7, dark: 0x7FBEF3),
        series: [
            .adaptive(light: 0x007D7F, dark: 0x49C7C0),
            .adaptive(light: 0xE49839, dark: 0xEEB154),
            .adaptive(light: 0x476DB4, dark: 0x7FA4EA),
            .adaptive(light: 0xD45D66, dark: 0xEB8186),
            .adaptive(light: 0x4E9A5B, dark: 0x73C881),
            .adaptive(light: 0x885CB5, dark: 0xBC97E8),
            .adaptive(light: 0x0087AB, dark: 0x6DBDDC),
            .adaptive(light: 0x697284, dark: 0x9BA5B8),
        ],
        chartGrid: .adaptive(light: 0xE5E6E9, dark: 0x2A2B2E),
        chartAxis: .adaptive(light: 0xC1C4C9, dark: 0x484A50)
    )

    public func chartColor(_ token: String?) -> Color? {
        guard let token else { return nil }
        switch token {
        case "axis": return chartAxis
        case "grid": return chartGrid
        case "text": return foreground
        case "muted": return muted
        default:
            guard token.hasPrefix("series-"), let index = Int(token.dropFirst(7)), index >= 1 else { return nil }
            return series[(index - 1) % series.count]
        }
    }

    func tone(_ name: String) -> Color {
        switch name {
        case "info": info
        case "success", "positive": positive
        case "warning": warning
        case "danger", "negative": negative
        default: muted
        }
    }
}

extension Color {
    public static func adaptive(light: UInt32, dark: UInt32) -> Color {
        Color(uiColor: UIColor { traits in
            UIColor(rgb: traits.userInterfaceStyle == .dark ? dark : light)
        })
    }
}

extension UIColor {
    convenience init(rgb: UInt32) {
        self.init(
            red: CGFloat((rgb >> 16) & 0xFF) / 255,
            green: CGFloat((rgb >> 8) & 0xFF) / 255,
            blue: CGFloat(rgb & 0xFF) / 255,
            alpha: 1
        )
    }
}

extension EnvironmentValues {
    @Entry public var polyUITheme: PolyUITheme = .standard
}

extension View {
    public func polyUITheme(_ theme: PolyUITheme) -> some View {
        environment(\.polyUITheme, theme)
    }
}
