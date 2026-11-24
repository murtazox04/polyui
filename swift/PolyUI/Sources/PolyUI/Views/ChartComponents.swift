import SwiftUI

struct ChartGeometry {
    let viewBox: CGRect
    let scale: CGFloat

    init(viewBox: CGRect, width: CGFloat) {
        self.viewBox = viewBox
        scale = width / viewBox.width
    }

    func point(_ point: CGPoint) -> CGPoint {
        CGPoint(x: (point.x - viewBox.minX) * scale, y: (point.y - viewBox.minY) * scale)
    }

    func rect(_ rect: CGRect) -> CGRect {
        CGRect(origin: point(rect.origin), size: CGSize(width: rect.width * scale, height: rect.height * scale))
    }

    func path(_ commands: [ChartScene.PathCommand]) -> Path {
        var path = Path()
        var current = CGPoint.zero
        for command in commands {
            switch command {
            case .move(let target):
                current = point(target)
                path.move(to: current)
            case .line(let target):
                current = point(target)
                path.addLine(to: current)
            case .arc(let radius, let largeArc, let clockwise, let target):
                let end = point(target)
                Self.appendArc(to: &path, from: current, to: end, radius: radius * scale, largeArc: largeArc, sweep: clockwise)
                current = end
            case .close:
                path.closeSubpath()
            }
        }
        return path
    }

    func hitPath(_ shape: ChartScene.HitShape) -> Path {
        switch shape {
        case .rect(let rect):
            Path(self.rect(rect))
        case .polygon(let points):
            Path { path in
                path.addLines(points.map(point))
                path.closeSubpath()
            }
        }
    }

    static func appendArc(to path: inout Path, from start: CGPoint, to end: CGPoint, radius: CGFloat, largeArc: Bool, sweep: Bool) {
        guard radius > 0, start != end else {
            path.addLine(to: end)
            return
        }
        let halfX = (start.x - end.x) / 2
        let halfY = (start.y - end.y) / 2
        var radius = radius
        let lambda = (halfX * halfX + halfY * halfY) / (radius * radius)
        if lambda > 1 { radius *= lambda.squareRoot() }
        let squared = radius * radius
        let numerator = max(0, squared * squared - squared * halfY * halfY - squared * halfX * halfX)
        let denominator = squared * halfY * halfY + squared * halfX * halfX
        let coefficient = (largeArc != sweep ? 1 : -1) * (denominator == 0 ? 0 : (numerator / denominator).squareRoot())
        let center = CGPoint(
            x: coefficient * halfY + (start.x + end.x) / 2,
            y: coefficient * -halfX + (start.y + end.y) / 2
        )
        let startAngle = atan2(start.y - center.y, start.x - center.x)
        var delta = atan2(end.y - center.y, end.x - center.x) - startAngle
        if sweep && delta < 0 { delta += 2 * .pi }
        if !sweep && delta > 0 { delta -= 2 * .pi }
        let steps = max(2, Int(abs(delta) / (.pi / 90)))
        for step in 1...steps {
            let angle = startAngle + delta * CGFloat(step) / CGFloat(steps)
            path.addLine(to: CGPoint(x: center.x + radius * cos(angle), y: center.y + radius * sin(angle)))
        }
    }
}

struct ChartCanvas: View {
    let scene: ChartScene
    let selectedRow: Int?
    @Environment(\.polyUITheme) private var theme

    var body: some View {
        Canvas { context, size in
            let geometry = ChartGeometry(viewBox: scene.viewBox, width: size.width)
            if let selectedRow {
                for hit in scene.hits where hit.row == selectedRow {
                    let outline = geometry.hitPath(hit.shape)
                    context.fill(outline, with: .color(theme.accent.opacity(0.08)))
                    context.stroke(outline, with: .color(theme.accent), lineWidth: 1.5)
                }
            }
            for mark in scene.marks {
                draw(mark, in: &context, geometry: geometry)
            }
        }
    }

    private func draw(_ mark: ChartScene.Mark, in context: inout GraphicsContext, geometry: ChartGeometry) {
        switch mark {
        case .rect(let rect, let fill):
            guard let color = theme.chartColor(fill) else { return }
            let frame = geometry.rect(rect)
            let radius = min(4 * geometry.scale, frame.width / 2, frame.height / 2)
            context.fill(Path(roundedRect: frame, cornerRadius: max(0, radius), style: .continuous), with: .color(color))
        case .line(let from, let to, let stroke, let width):
            guard let color = theme.chartColor(stroke) else { return }
            var line = Path()
            line.move(to: geometry.point(from))
            line.addLine(to: geometry.point(to))
            context.stroke(line, with: .color(color), lineWidth: max(0.5, width * geometry.scale))
        case .path(let commands, let fill, let opacity, let stroke, let strokeWidth):
            let shape = geometry.path(commands)
            if let color = theme.chartColor(fill) {
                context.fill(shape, with: .color(color.opacity(opacity ?? 1)))
            }
            if let color = theme.chartColor(stroke) {
                let width = max(0.5, (strokeWidth ?? 1) * geometry.scale)
                context.stroke(shape, with: .color(color), style: StrokeStyle(lineWidth: width, lineCap: .round, lineJoin: .round))
            }
        case .circle(let center, let radius, let fill):
            guard let color = theme.chartColor(fill) else { return }
            let point = geometry.point(center)
            let scaled = radius * geometry.scale
            context.fill(Path(ellipseIn: CGRect(x: point.x - scaled, y: point.y - scaled, width: scaled * 2, height: scaled * 2)), with: .color(color))
        case .text(let text, let at, let anchor, let role, let color, let size):
            let fontSize = max(size * geometry.scale, size * 0.85)
            let weight: Font.Weight = role == "title" ? .semibold : .regular
            let tint = theme.chartColor(color) ?? theme.foreground
            let label = Text(text).font(.system(size: fontSize, weight: weight)).foregroundStyle(tint)
            let point = geometry.point(at)
            let resolved = context.resolve(label)
            let unit: UnitPoint =
                switch anchor {
                case .start: .bottomLeading
                case .middle: .bottom
                case .end: .bottomTrailing
                }
            context.draw(resolved, at: CGPoint(x: point.x, y: point.y + fontSize * 0.24), anchor: unit)
        }
    }
}

struct ChartComponent: View {
    let node: ComponentNode
    @Environment(PolyUISurfaceModel.self) private var surface
    @Environment(PolyUISession.self) private var session
    @Environment(\.polyUITheme) private var theme
    @Environment(\.polyUILabels) private var labels
    @State private var selectedRow: Int?

    var body: some View {
        let title = node.string("title")
        VStack(alignment: .leading, spacing: 10) {
            if let scene = node.chartScene {
                if !scene.hasTitleMark && !title.isEmpty {
                    Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(theme.foreground)
                }
                stage(scene)
                legend(scene)
            } else {
                Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(theme.foreground)
                Text(labels.chartUnavailable)
                    .font(.subheadline)
                    .foregroundStyle(theme.muted)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(node.a11y.label.isEmpty ? title : node.a11y.label)
    }

    private func stage(_ scene: ChartScene) -> some View {
        let selectable = node.hasAction("on_select")
        return Color.clear
            .aspectRatio(scene.viewBox.width / scene.viewBox.height, contentMode: .fit)
            .frame(maxWidth: .infinity)
            .overlay {
                ChartCanvas(scene: scene, selectedRow: selectedRow)
                    .accessibilityLabel(scene.description.isEmpty ? scene.title : scene.description)
                    .accessibilityAddTraits(.isImage)
            }
            .overlay {
                GeometryReader { proxy in
                    let geometry = ChartGeometry(viewBox: scene.viewBox, width: proxy.size.width)
                    ForEach(scene.hits) { hit in
                        let outline = geometry.hitPath(hit.shape)
                        outline
                            .fill(Color.clear)
                            .contentShape(outline)
                            .onTapGesture { select(hit.row, enabled: selectable) }
                            .accessibilityElement()
                            .accessibilityLabel(hit.label)
                            .accessibilityAddTraits(selectable ? (selectedRow == hit.row ? [.isButton, .isSelected] : .isButton) : [])
                            .accessibilityAction { select(hit.row, enabled: selectable) }
                    }
                }
            }
    }

    @ViewBuilder
    private func legend(_ scene: ChartScene) -> some View {
        if !scene.legend.isEmpty {
            FlowLayout(spacing: 12, lineSpacing: 6) {
                ForEach(Array(scene.legend.enumerated()), id: \.offset) { _, entry in
                    HStack(spacing: 6) {
                        RoundedRectangle(cornerRadius: 3)
                            .fill(theme.chartColor(entry.color) ?? theme.accent)
                            .frame(width: 10, height: 10)
                        Text(entry.label)
                            .font(.caption)
                            .foregroundStyle(theme.muted)
                    }
                    .accessibilityElement(children: .combine)
                }
            }
        }
    }

    private func select(_ row: Int, enabled: Bool) {
        guard enabled else { return }
        selectedRow = row
        session.fire(surface: surface.id, component: node.id, prop: "on_select", detail: ["row": .number(Double(row))])
    }
}

struct FlowLayout: Layout {
    var spacing: CGFloat
    var lineSpacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout Void) -> CGSize {
        let rows = arrange(width: proposal.width ?? .infinity, subviews: subviews)
        let width = rows.map(\.width).max() ?? 0
        let height = rows.map(\.height).reduce(0, +) + lineSpacing * CGFloat(max(0, rows.count - 1))
        return CGSize(width: proposal.width ?? width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Void) {
        var y = bounds.minY
        for row in arrange(width: bounds.width, subviews: subviews) {
            var x = bounds.minX
            for index in row.indices {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index].place(at: CGPoint(x: x, y: y + (row.height - size.height) / 2), proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
            y += row.height + lineSpacing
        }
    }

    private struct Line {
        var indices: [Int] = []
        var width: CGFloat = 0
        var height: CGFloat = 0
    }

    private func arrange(width: CGFloat, subviews: Subviews) -> [Line] {
        var lines: [Line] = []
        var line = Line()
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            let needed = line.indices.isEmpty ? size.width : line.width + spacing + size.width
            if needed > width && !line.indices.isEmpty {
                lines.append(line)
                line = Line()
            }
            line.width = line.indices.isEmpty ? size.width : line.width + spacing + size.width
            line.height = max(line.height, size.height)
            line.indices.append(index)
        }
        if !line.indices.isEmpty { lines.append(line) }
        return lines
    }
}

struct DashboardComponent: View {
    let node: ComponentNode
    @Environment(PolyUISurfaceModel.self) private var surface
    @Environment(\.polyUITheme) private var theme
    @Environment(\.horizontalSizeClass) private var sizeClass

    private enum Cell: Hashable {
        case strip([String])
        case single(String)
    }

    var body: some View {
        let title = node.string("title")
        let requested = Int((node.number("columns") ?? 2).rounded())
        let columns = min(4, max(1, sizeClass == .compact ? min(requested, 1) : requested))
        VStack(alignment: .leading, spacing: theme.spacing) {
            if !title.isEmpty {
                Text(title)
                    .font(.headline)
                    .foregroundStyle(theme.foreground)
                    .accessibilityAddTraits(.isHeader)
            }
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: theme.spacing, alignment: .top), count: columns), alignment: .leading, spacing: theme.spacing) {
                ForEach(cells, id: \.self) { cell in
                    switch cell {
                    case .strip(let ids):
                        ViewThatFits(in: .horizontal) {
                            HStack(alignment: .top, spacing: theme.spacing) {
                                ForEach(ids, id: \.self) { PolyUIComponentView(id: $0).frame(maxWidth: .infinity, alignment: .leading) }
                            }
                            VStack(alignment: .leading, spacing: 12) {
                                ForEach(ids, id: \.self) { PolyUIComponentView(id: $0) }
                            }
                        }
                        .padding(theme.spacing)
                        .background(theme.sunken, in: RoundedRectangle(cornerRadius: theme.innerCornerRadius, style: .continuous))
                    case .single(let id):
                        PolyUIComponentView(id: id)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .polyUIContainer(node.a11y)
    }

    private var cells: [Cell] {
        var cells: [Cell] = []
        var run: [String] = []
        for id in node.children {
            if surface.node(id)?.type == "Stat" {
                run.append(id)
            } else {
                if !run.isEmpty { cells.append(.strip(run)) }
                run = []
                cells.append(.single(id))
            }
        }
        if !run.isEmpty { cells.append(.strip(run)) }
        return cells
    }
}
