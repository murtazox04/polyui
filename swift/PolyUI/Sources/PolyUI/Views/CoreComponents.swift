import SwiftUI

extension EnvironmentValues {
    @Entry var polyUICardDepth = 0
}

extension View {
    func polyUIContainer(_ a11y: A11y) -> some View {
        accessibilityElement(children: .contain)
            .accessibilityLabel(a11y.label)
    }

    @ViewBuilder
    func polyUILabel(_ label: String) -> some View {
        if label.isEmpty { self } else { accessibilityLabel(label) }
    }
}

struct TextComponent: View {
    let node: ComponentNode
    @Environment(\.polyUITheme) private var theme

    var body: some View {
        let style = node.choice("style", among: ["body", "title", "subtitle", "caption", "muted"], default: "body")
        Text(node.string("text"))
            .font(font(style))
            .foregroundStyle(style == "caption" || style == "muted" ? theme.muted : theme.foreground)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityAddTraits(style == "title" || style == "subtitle" ? .isHeader : [])
            .accessibilityHeading(style == "title" ? .h3 : style == "subtitle" ? .h4 : .unspecified)
    }

    private func font(_ style: String) -> Font {
        switch style {
        case "title": .title3.weight(.semibold)
        case "subtitle": .headline
        case "caption": .caption
        case "muted": .subheadline
        default: .body
        }
    }
}

struct CardComponent: View {
    let node: ComponentNode
    @Environment(\.polyUITheme) private var theme
    @Environment(\.polyUICardDepth) private var depth

    var body: some View {
        let tone = node.choice("tone", among: ["plain", "info", "success", "warning", "danger"], default: "plain")
        let title = node.string("title")
        let subtitle = node.string("subtitle")
        let nested = depth > 0
        let shape = RoundedRectangle(cornerRadius: nested ? theme.innerCornerRadius : theme.cornerRadius, style: .continuous)
        VStack(alignment: .leading, spacing: theme.spacing) {
            if !title.isEmpty || !subtitle.isEmpty {
                VStack(alignment: .leading, spacing: 2) {
                    if !title.isEmpty {
                        Text(title)
                            .font(.headline)
                            .foregroundStyle(theme.foreground)
                            .accessibilityAddTraits(.isHeader)
                    }
                    if !subtitle.isEmpty {
                        Text(subtitle)
                            .font(.subheadline)
                            .foregroundStyle(theme.muted)
                    }
                }
                .fixedSize(horizontal: false, vertical: true)
            }
            ForEach(node.children, id: \.self) { PolyUIComponentView(id: $0) }
        }
        .padding(nested ? theme.spacing : theme.spacing + 2)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            if nested {
                shape.fill(theme.sunken)
            } else if tone == "plain" {
                shape.fill(theme.raised)
                    .shadow(color: .black.opacity(theme.cardShadow ? 0.06 : 0), radius: 14, y: 6)
            } else {
                shape.fill(theme.raised).overlay(shape.fill(theme.tone(tone).opacity(0.06)))
            }
        }
        .overlay {
            if !nested {
                shape.strokeBorder(tone == "plain" ? theme.border : theme.tone(tone).opacity(0.35), lineWidth: 1)
            }
        }
        .environment(\.polyUICardDepth, depth + 1)
        .polyUIContainer(node.a11y)
    }
}

struct RowComponent: View {
    let node: ComponentNode
    @Environment(PolyUISurfaceModel.self) private var surface

    var body: some View {
        let gap = spacing(node.choice("gap", among: ["none", "small", "medium", "large"], default: "medium"))
        let align = node.choice("align", among: ["start", "center", "end", "between"], default: "start")
        let onlyStats = !node.children.isEmpty && node.children.allSatisfy { surface.node($0)?.type == "Stat" }
        ViewThatFits(in: .horizontal) {
            HStack(alignment: onlyStats ? .top : .center, spacing: gap) {
                if align == "center" || align == "end" { Spacer(minLength: 0) }
                ForEach(Array(node.children.enumerated()), id: \.element) { index, id in
                    if align == "between" && index > 0 { Spacer(minLength: 0) }
                    cell(id)
                }
                if align == "center" || align == "start" { Spacer(minLength: 0) }
            }
            VStack(alignment: .leading, spacing: gap) {
                ForEach(node.children, id: \.self) { PolyUIComponentView(id: $0) }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .polyUIContainer(node.a11y)
    }

    @ViewBuilder
    private func cell(_ id: String) -> some View {
        if surface.node(id)?.type == "Stat" {
            PolyUIComponentView(id: id).frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
        } else {
            PolyUIComponentView(id: id)
        }
    }

    private func spacing(_ gap: String) -> CGFloat {
        switch gap {
        case "none": 0
        case "small": 8
        case "large": 24
        default: 12
        }
    }
}

struct ListComponent: View {
    let node: ComponentNode
    @Environment(\.polyUITheme) private var theme

    var body: some View {
        let title = node.string("title")
        let ordered = node.flag("ordered")
        VStack(alignment: .leading, spacing: 8) {
            if !title.isEmpty {
                Text(title)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(theme.muted)
                    .accessibilityAddTraits(.isHeader)
            }
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(node.children.enumerated()), id: \.element) { index, id in
                    if index > 0 { Divider().overlay(theme.border) }
                    HStack(alignment: .firstTextBaseline, spacing: 10) {
                        if ordered {
                            Text("\(index + 1).")
                                .font(.body.monospacedDigit())
                                .foregroundStyle(theme.muted)
                                .accessibilityHidden(true)
                        }
                        PolyUIComponentView(id: id)
                    }
                    .padding(.vertical, 10)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .polyUIContainer(node.a11y)
    }
}

struct StatComponent: View {
    let node: ComponentNode
    @Environment(\.polyUITheme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let tone = node.choice("tone", among: ["neutral", "positive", "negative", "warning"], default: "neutral")
        let value = node.string("value")
        let unit = node.string("unit")
        let hint = node.string("hint")
        VStack(alignment: .leading, spacing: 2) {
            Text(node.string("label"))
                .font(.subheadline.weight(.medium))
                .foregroundStyle(theme.muted)
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    figure(value, tone: tone)
                    unitText(unit)
                }
                VStack(alignment: .leading, spacing: 0) {
                    figure(value, tone: tone)
                    unitText(unit)
                }
            }
            if !hint.isEmpty {
                Text(hint)
                    .font(.caption)
                    .foregroundStyle(theme.muted)
            }
        }
        .fixedSize(horizontal: false, vertical: true)
        .animation(reduceMotion ? nil : .snappy(duration: 0.3), value: value)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(node.a11y.label.isEmpty ? "\(node.string("label")): \(value) \(unit)" : node.a11y.label)
        .accessibilityValue(hint)
        .accessibilityAddTraits(.updatesFrequently)
    }
}

extension StatComponent {
    fileprivate func figure(_ value: String, tone: String) -> some View {
        Text(value)
            .font(.system(.title, weight: .semibold))
            .monospacedDigit()
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            .foregroundStyle(tone == "neutral" ? theme.foreground : theme.tone(tone))
            .contentTransition(reduceMotion ? .identity : .numericText())
    }

    @ViewBuilder
    fileprivate func unitText(_ unit: String) -> some View {
        if !unit.isEmpty {
            Text(unit)
                .font(.body.weight(.medium))
                .foregroundStyle(theme.muted)
                .lineLimit(1)
        }
    }
}

struct BadgeComponent: View {
    let node: ComponentNode
    @Environment(\.polyUITheme) private var theme

    var body: some View {
        let tone = node.choice("tone", among: ["neutral", "info", "success", "warning", "danger"], default: "neutral")
        let color = theme.tone(tone)
        Text(node.string("text"))
            .font(.caption.weight(.semibold))
            .lineLimit(1)
            .foregroundStyle(color)
            .padding(.horizontal, 10)
            .padding(.vertical, 3)
            .background(color.opacity(0.12), in: Capsule())
            .accessibilityLabel(node.a11y.label)
    }
}

struct ImageComponent: View {
    let node: ComponentNode
    @Environment(PolyUISession.self) private var session
    @Environment(\.polyUITheme) private var theme

    var body: some View {
        let aspect = node.choice("aspect", among: ["square", "wide", "tall", "auto"], default: "auto")
        let alt = node.string("alt")
        let shape = RoundedRectangle(cornerRadius: theme.innerCornerRadius, style: .continuous)
        Group {
            if let url = SafeURL.check(node.props["src"]?.stringValue, allowHTTP: session.allowHTTP) {
                AsyncImage(url: url, transaction: Transaction(animation: .easeOut(duration: 0.2))) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().aspectRatio(contentMode: aspect == "auto" ? .fit : .fill)
                    case .failure:
                        missing(alt)
                    default:
                        ZStack { theme.sunken; ProgressView() }
                    }
                }
            } else {
                missing(alt)
            }
        }
        .frame(maxWidth: .infinity)
        .modifier(AspectModifier(aspect: aspect))
        .background(theme.sunken)
        .clipShape(shape)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(alt)
        .accessibilityAddTraits(.isImage)
    }

    private func missing(_ alt: String) -> some View {
        Text(alt)
            .font(.subheadline)
            .foregroundStyle(theme.muted)
            .multilineTextAlignment(.center)
            .padding(.vertical, 24)
            .padding(.horizontal, 16)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct AspectModifier: ViewModifier {
    let aspect: String

    func body(content: Content) -> some View {
        switch aspect {
        case "square": content.aspectRatio(1, contentMode: .fit)
        case "wide": content.aspectRatio(16.0 / 9.0, contentMode: .fit)
        case "tall": content.aspectRatio(3.0 / 4.0, contentMode: .fit)
        default: content
        }
    }
}

struct PolyUIButtonStyle: ButtonStyle {
    let tone: String
    let theme: PolyUITheme
    @Environment(\.isEnabled) private var isEnabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func makeBody(configuration: Configuration) -> some View {
        let shape = RoundedRectangle(cornerRadius: theme.controlCornerRadius, style: .continuous)
        configuration.label
            .font(.body.weight(.semibold))
            .foregroundStyle(foreground)
            .padding(.horizontal, 18)
            .frame(minHeight: 44)
            .background(shape.fill(fill))
            .overlay(shape.strokeBorder(tone == "secondary" ? theme.borderStrong : .clear, lineWidth: 1))
            .opacity(isEnabled ? (configuration.isPressed ? 0.82 : 1) : 0.5)
            .scaleEffect(configuration.isPressed && !reduceMotion ? 0.98 : 1)
            .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: configuration.isPressed)
            .contentShape(shape)
    }

    private var fill: Color {
        switch tone {
        case "primary": theme.accent
        case "danger": theme.negative
        default: theme.background
        }
    }

    private var foreground: Color {
        switch tone {
        case "primary": theme.accentForeground
        case "danger": .white
        default: theme.foreground
        }
    }
}

struct ButtonComponent: View {
    let node: ComponentNode
    @Environment(PolyUISurfaceModel.self) private var surface
    @Environment(PolyUISession.self) private var session
    @Environment(\.polyUITheme) private var theme

    var body: some View {
        let tone = node.choice("tone", among: ["primary", "secondary", "danger"], default: "secondary")
        Button {
            session.fire(surface: surface.id, component: node.id, prop: "on_press")
        } label: {
            Text(node.string("label")).lineLimit(2).multilineTextAlignment(.center)
        }
        .buttonStyle(PolyUIButtonStyle(tone: tone, theme: theme))
        .disabled(node.flag("disabled") || !node.hasAction("on_press"))
        .polyUILabel(node.a11y.label)
    }
}

struct DividerComponent: View {
    @Environment(\.polyUITheme) private var theme

    var body: some View {
        Rectangle()
            .fill(theme.border)
            .frame(height: 1)
            .frame(maxWidth: .infinity)
            .accessibilityHidden(true)
    }
}

struct ConfirmSheetComponent: View {
    let node: ComponentNode
    @Environment(PolyUISurfaceModel.self) private var surface
    @Environment(PolyUISession.self) private var session
    @Environment(\.polyUITheme) private var theme
    @State private var answeredFor: String?

    var body: some View {
        let actionKey = node.props["on_confirm"]?["action_id"]?.stringValue ?? ""
        let locked = answeredFor != nil && answeredFor == actionKey
        let body = node.string("body")
        VStack(alignment: .leading, spacing: 12) {
            Text(node.string("title"))
                .font(.headline)
                .foregroundStyle(theme.foreground)
                .accessibilityAddTraits(.isHeader)
            if !body.isEmpty {
                Text(body)
                    .font(.subheadline)
                    .foregroundStyle(theme.muted)
            }
            HStack(spacing: 10) {
                Spacer(minLength: 0)
                Button(node.string("cancel_label").isEmpty ? "No" : node.string("cancel_label")) {
                    answer("on_cancel", key: actionKey)
                }
                .buttonStyle(PolyUIButtonStyle(tone: "secondary", theme: theme))
                .disabled(locked)
                Button(node.string("confirm_label").isEmpty ? "Yes" : node.string("confirm_label")) {
                    answer("on_confirm", key: actionKey)
                }
                .buttonStyle(PolyUIButtonStyle(tone: "primary", theme: theme))
                .disabled(locked || !node.hasAction("on_confirm"))
            }
        }
        .padding(theme.spacing)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.raised, in: RoundedRectangle(cornerRadius: theme.innerCornerRadius, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: theme.innerCornerRadius, style: .continuous).strokeBorder(theme.borderStrong))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(node.a11y.label)
    }

    private func answer(_ prop: String, key: String) {
        guard answeredFor != key else { return }
        answeredFor = key
        if node.hasAction(prop) {
            session.fire(surface: surface.id, component: node.id, prop: prop)
        }
    }
}
