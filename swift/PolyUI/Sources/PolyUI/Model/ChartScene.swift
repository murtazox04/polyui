import CoreGraphics
import Foundation

public struct ChartScene: Sendable, Hashable {
    public enum PathCommand: Sendable, Hashable {
        case move(CGPoint)
        case line(CGPoint)
        case arc(radius: CGFloat, largeArc: Bool, clockwise: Bool, to: CGPoint)
        case close
    }

    public enum TextAnchor: String, Sendable, Hashable {
        case start, middle, end
    }

    public enum Mark: Sendable, Hashable {
        case rect(CGRect, fill: String)
        case line(from: CGPoint, to: CGPoint, stroke: String, width: CGFloat)
        case path([PathCommand], fill: String?, fillOpacity: CGFloat?, stroke: String?, strokeWidth: CGFloat?)
        case circle(center: CGPoint, radius: CGFloat, fill: String)
        case text(String, at: CGPoint, anchor: TextAnchor, role: String, color: String, size: CGFloat)
    }

    public enum HitShape: Sendable, Hashable {
        case rect(CGRect)
        case polygon([CGPoint])
    }

    public struct Hit: Sendable, Hashable, Identifiable {
        public var row: Int
        public var series: String?
        public var label: String
        public var shape: HitShape
        public var index: Int

        public var id: Int { index }
    }

    public struct LegendEntry: Sendable, Hashable {
        public var label: String
        public var color: String
    }

    public static let markLimit = 6000

    public var viewBox: CGRect
    public var title: String
    public var description: String
    public var marks: [Mark]
    public var legend: [LegendEntry]
    public var hits: [Hit]

    public var hasTitleMark: Bool {
        marks.contains { mark in
            if case .text(_, _, _, let role, _, _) = mark { role == "title" } else { false }
        }
    }

    public init?(json: JSONValue) {
        guard let box = json["view_box"],
            let x = box["x"]?.doubleValue, let y = box["y"]?.doubleValue,
            let width = box["width"]?.doubleValue, let height = box["height"]?.doubleValue,
            width > 0, height > 0, let rawMarks = json["marks"]?.arrayValue
        else { return nil }
        viewBox = CGRect(x: x, y: y, width: width, height: height)
        title = json["title"]?.stringValue ?? ""
        description = json["description"]?.stringValue ?? ""
        marks = rawMarks.prefix(Self.markLimit).compactMap(Self.mark)
        legend = (json["legend"]?.arrayValue ?? []).compactMap { item in
            guard let label = item["label"]?.stringValue, let color = item["color"]?.stringValue else { return nil }
            return LegendEntry(label: label, color: color)
        }
        hits = (json["hits"]?.arrayValue ?? []).prefix(Self.markLimit).enumerated().compactMap { index, item in
            Self.hit(item, index: index)
        }
    }

    private static func point(_ value: JSONValue, _ xKey: String, _ yKey: String) -> CGPoint? {
        guard let x = value[xKey]?.doubleValue, let y = value[yKey]?.doubleValue else { return nil }
        return CGPoint(x: x, y: y)
    }

    private static func mark(_ value: JSONValue) -> Mark? {
        switch value["type"]?.stringValue {
        case "rect":
            guard let origin = point(value, "x", "y"), let width = value["width"]?.doubleValue,
                let height = value["height"]?.doubleValue, let fill = value["fill"]?.stringValue
            else { return nil }
            return .rect(CGRect(origin: origin, size: CGSize(width: max(0, width), height: max(0, height))), fill: fill)
        case "line":
            guard let from = point(value, "x1", "y1"), let to = point(value, "x2", "y2"),
                let stroke = value["stroke"]?.stringValue
            else { return nil }
            return .line(from: from, to: to, stroke: stroke, width: value["stroke_width"]?.doubleValue ?? 1)
        case "path":
            guard let raw = value["commands"]?.arrayValue, !raw.isEmpty else { return nil }
            var commands: [PathCommand] = []
            for command in raw {
                guard let next = pathCommand(command) else { return nil }
                commands.append(next)
            }
            return .path(
                commands,
                fill: value["fill"]?.stringValue,
                fillOpacity: value["fill_opacity"]?.doubleValue.map { CGFloat($0) },
                stroke: value["stroke"]?.stringValue,
                strokeWidth: value["stroke_width"]?.doubleValue.map { CGFloat($0) }
            )
        case "circle":
            guard let center = point(value, "cx", "cy"), let radius = value["r"]?.doubleValue,
                let fill = value["fill"]?.stringValue
            else { return nil }
            return .circle(center: center, radius: max(0, radius), fill: fill)
        case "text":
            guard let at = point(value, "x", "y"), let size = value["size"]?.doubleValue,
                let text = value["text"]?.stringValue
            else { return nil }
            return .text(
                text,
                at: at,
                anchor: TextAnchor(rawValue: value["anchor"]?.stringValue ?? "") ?? .start,
                role: value["role"]?.stringValue ?? "note",
                color: value["color"]?.stringValue ?? "text",
                size: size
            )
        default:
            return nil
        }
    }

    private static func pathCommand(_ value: JSONValue) -> PathCommand? {
        switch value["op"]?.stringValue {
        case "M": point(value, "x", "y").map(PathCommand.move)
        case "L": point(value, "x", "y").map(PathCommand.line)
        case "Z": .close
        case "A":
            point(value, "x", "y").flatMap { target in
                value["radius"]?.doubleValue.map { radius in
                    .arc(
                        radius: abs(radius),
                        largeArc: value["large_arc"]?.boolValue ?? false,
                        clockwise: value["clockwise"]?.boolValue ?? false,
                        to: target
                    )
                }
            }
        default: nil
        }
    }

    private static func hit(_ value: JSONValue, index: Int) -> Hit? {
        guard let row = value["row"]?.doubleValue, row >= 0, row < Double(Int32.max) else { return nil }
        let shape: HitShape
        switch value["shape"]?.stringValue {
        case "rect":
            guard let origin = point(value, "x", "y"), let width = value["width"]?.doubleValue,
                let height = value["height"]?.doubleValue
            else { return nil }
            shape = .rect(CGRect(x: origin.x, y: origin.y, width: width, height: height))
        case "polygon":
            guard let raw = value["points"]?.arrayValue, raw.count >= 3 else { return nil }
            var points: [CGPoint] = []
            for pair in raw {
                guard let items = pair.arrayValue, items.count == 2,
                    let x = items[0].doubleValue, let y = items[1].doubleValue
                else { return nil }
                points.append(CGPoint(x: x, y: y))
            }
            shape = .polygon(points)
        default:
            return nil
        }
        return Hit(
            row: Int(row),
            series: value["series"]?.stringValue,
            label: value["label"]?.stringValue ?? "",
            shape: shape,
            index: index
        )
    }
}
