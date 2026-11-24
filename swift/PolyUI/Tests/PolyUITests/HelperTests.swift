import CoreGraphics
import Foundation
import SwiftUI
import Testing

@testable import PolyUI

struct SafeURLTests {
    @Test(arguments: [
        "https://example.com/a.png",
        "https://cdn.example.com:8443/img?x=1",
    ])
    func acceptsHTTPS(_ raw: String) {
        #expect(SafeURL.check(raw) != nil)
    }

    @Test(arguments: [
        "http://example.com/a.png",
        "javascript:alert(1)",
        "data:image/png;base64,AAAA",
        "file:///etc/passwd",
        "https://user:secret@example.com/a.png",
        "https:///nohost",
        "https://exa mple.com",
        "https://example.com/\u{0007}",
        "",
    ])
    func rejectsEverythingElse(_ raw: String) {
        #expect(SafeURL.check(raw) == nil)
    }

    @Test func httpNeedsTheHostToAllowIt() {
        #expect(SafeURL.check("http://example.com/a.png", allowHTTP: true) != nil)
        #expect(SafeURL.check(String(repeating: "a", count: 5000)) == nil)
    }
}

struct ChartSceneTests {
    private let scene: JSONValue = [
        "view_box": ["x": 0, "y": 0, "width": 200, "height": 100],
        "title": "Use",
        "description": "Use. Highest is Jul.",
        "marks": [
            ["type": "rect", "x": 10, "y": 10, "width": 20, "height": 80, "fill": "series-1"],
            ["type": "line", "x1": 0, "y1": 90, "x2": 200, "y2": 90, "stroke": "grid", "stroke_width": 1],
            ["type": "text", "x": 5, "y": 12, "text": "Use", "anchor": "start", "role": "title", "color": "text", "size": 15],
            ["type": "path", "commands": [["op": "M", "x": 0, "y": 0], ["op": "A", "radius": 10, "large_arc": false, "clockwise": true, "x": 10, "y": 10], ["op": "Z"]], "fill": "series-2"],
            ["type": "hologram"],
            ["type": "circle", "cx": 1, "cy": 1, "r": "big", "fill": "series-1"],
        ],
        "legend": [["label": "kWh", "color": "series-1"]],
        "hits": [
            ["row": 0, "shape": "rect", "x": 10, "y": 10, "width": 20, "height": 80, "label": "Jun: 182"],
            ["row": 1, "shape": "polygon", "points": [[0, 0], [10, 0], [10, 10]], "label": "Jul: 241"],
            ["row": -1, "shape": "rect", "x": 0, "y": 0, "width": 1, "height": 1, "label": "bad"],
        ],
    ]

    @Test func decodesMarksAndSkipsBrokenOnes() throws {
        let decoded = try #require(ChartScene(json: scene))
        #expect(decoded.viewBox == CGRect(x: 0, y: 0, width: 200, height: 100))
        #expect(decoded.marks.count == 4)
        #expect(decoded.hits.map(\.row) == [0, 1])
        #expect(decoded.legend.first?.label == "kWh")
        #expect(decoded.hasTitleMark)
    }

    @Test func rejectsAMissingViewBox() {
        #expect(ChartScene(json: ["marks": []]) == nil)
        #expect(ChartScene(json: ["view_box": ["x": 0, "y": 0, "width": 0, "height": 10], "marks": []]) == nil)
    }

    @Test func geometryScalesToTheViewWidth() {
        let geometry = ChartGeometry(viewBox: CGRect(x: 0, y: 0, width: 200, height: 100), width: 100)
        #expect(geometry.rect(CGRect(x: 10, y: 10, width: 20, height: 80)) == CGRect(x: 5, y: 5, width: 10, height: 40))
    }

    @Test func arcsEndWhereTheSceneSays() {
        var path = Path()
        path.move(to: CGPoint(x: 0, y: -10))
        ChartGeometry.appendArc(to: &path, from: CGPoint(x: 0, y: -10), to: CGPoint(x: 10, y: 0), radius: 10, largeArc: false, sweep: true)
        let end = path.currentPoint ?? .zero
        #expect(abs(end.x - 10) < 0.001 && abs(end.y) < 0.001)
        let bounds = path.boundingRect
        #expect(bounds.maxX <= 10.001 && bounds.minY >= -10.001)
    }

    @Test @MainActor func themeMapsEveryToken() {
        let theme = PolyUITheme.standard
        for token in ["series-1", "series-8", "series-9", "axis", "grid", "text", "muted"] {
            #expect(theme.chartColor(token) != nil, "\(token)")
        }
        #expect(theme.chartColor("series-0") == nil)
        #expect(theme.chartColor("url(#x)") == nil)
    }
}

@MainActor
struct JSONValueTests {
    @Test func roundTripsThroughText() throws {
        let value: JSONValue = ["a": [1, 2.5, "x", true, nil], "b": ["c": "d"]]
        #expect(try JSONValue.parse(try value.jsonText()) == value)
        #expect(try JSONValue.parse("\"\\/\"") == "/")
    }

    @Test func inputDisplaysNumbersWithoutTrailingZeros() {
        #expect(InputComponent.display(.number(12)) == "12")
        #expect(InputComponent.display(.number(1.5)) == "1.5")
        #expect(InputComponent.display(.null) == "")
    }
}
