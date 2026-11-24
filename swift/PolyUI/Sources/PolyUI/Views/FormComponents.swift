import SwiftUI

struct ChoiceOption: Hashable, Identifiable {
    let id: String
    let label: String
    let detail: String

    static func read(_ value: JSONValue?) -> [ChoiceOption] {
        var seen: Set<String> = []
        return (value?.arrayValue ?? []).compactMap { item in
            guard let id = item["id"]?.stringValue, seen.insert(id).inserted else { return nil }
            return ChoiceOption(id: id, label: item["label"]?.stringValue ?? "", detail: item["detail"]?.stringValue ?? "")
        }
    }

    static func selected(_ value: JSONValue?) -> Set<String> {
        switch value {
        case .string(let id): [id]
        case .array(let items): Set(items.compactMap(\.stringValue))
        default: []
        }
    }
}

struct ChoiceGroupComponent: View {
    let node: ComponentNode
    @Environment(PolyUISurfaceModel.self) private var surface
    @Environment(PolyUISession.self) private var session
    @Environment(\.polyUITheme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let options = ChoiceOption.read(node.props["options"])
        let selected = ChoiceOption.selected(node.value)
        let multiple = node.flag("multiple")
        let label = node.string("label")
        let shape = RoundedRectangle(cornerRadius: theme.innerCornerRadius, style: .continuous)
        VStack(alignment: .leading, spacing: 8) {
            if !label.isEmpty {
                Text(label)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(theme.muted)
                    .accessibilityHidden(true)
            }
            VStack(spacing: 0) {
                ForEach(Array(options.enumerated()), id: \.element.id) { index, option in
                    if index > 0 { Rectangle().fill(theme.border).frame(height: 1) }
                    row(option, isOn: selected.contains(option.id), multiple: multiple, options: options, selected: selected)
                }
            }
            .background(theme.background, in: shape)
            .clipShape(shape)
            .overlay(shape.strokeBorder(theme.border, lineWidth: 1))
        }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.15), value: selected)
        .frame(maxWidth: .infinity, alignment: .leading)
        .polyUIContainer(node.a11y)
    }

    private func row(_ option: ChoiceOption, isOn: Bool, multiple: Bool, options: [ChoiceOption], selected: Set<String>) -> some View {
        Button {
            toggle(option, multiple: multiple, options: options, selected: selected)
        } label: {
            HStack(alignment: .center, spacing: 12) {
                Image(systemName: symbol(isOn: isOn, multiple: multiple))
                    .font(.title3)
                    .foregroundStyle(isOn ? theme.accent : theme.borderStrong)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 2) {
                    Text(option.label)
                        .font(.body)
                        .foregroundStyle(theme.foreground)
                    if !option.detail.isEmpty {
                        Text(option.detail)
                            .font(.footnote)
                            .foregroundStyle(theme.muted)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 11)
            .frame(minHeight: 44)
            .background(isOn ? theme.accent.opacity(0.08) : .clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(option.detail.isEmpty ? option.label : "\(option.label), \(option.detail)")
        .accessibilityAddTraits(isOn ? [.isButton, .isSelected] : .isButton)
    }

    private func symbol(isOn: Bool, multiple: Bool) -> String {
        if multiple {
            isOn ? "checkmark.square.fill" : "square"
        } else {
            isOn ? "largecircle.fill.circle" : "circle"
        }
    }

    private func toggle(_ option: ChoiceOption, multiple: Bool, options: [ChoiceOption], selected: Set<String>) {
        guard let path = node.props["value"]?.stringValue else { return }
        let next: JSONValue
        if multiple {
            var chosen = selected
            if chosen.contains(option.id) { chosen.remove(option.id) } else { chosen.insert(option.id) }
            next = .array(options.filter { chosen.contains($0.id) }.map { .string($0.id) })
        } else {
            next = .string(option.id)
        }
        session.setValue(surface: surface.id, path: path, value: next)
        if node.hasAction("on_change") {
            session.fire(surface: surface.id, component: node.id, prop: "on_change")
        }
    }
}

struct InputComponent: View {
    let node: ComponentNode
    @Environment(PolyUISurfaceModel.self) private var surface
    @Environment(PolyUISession.self) private var session
    @Environment(\.polyUITheme) private var theme
    @State private var text = ""
    @State private var recent: [String] = []
    @FocusState private var focused: Bool

    private static let recentLimit = 8

    var body: some View {
        let kind = node.choice("input", among: ["text", "number", "phone", "email", "multiline"], default: "text")
        let shape = RoundedRectangle(cornerRadius: theme.controlCornerRadius, style: .continuous)
        VStack(alignment: .leading, spacing: 6) {
            Text(node.string("label"))
                .font(.subheadline.weight(.medium))
                .foregroundStyle(theme.foreground)
                .accessibilityHidden(true)
            field(kind)
                .focused($focused)
                .font(.body)
                .foregroundStyle(theme.foreground)
                .tint(theme.accent)
                .padding(.horizontal, 12)
                .padding(.vertical, 11)
                .frame(minHeight: 44)
                .background(theme.background, in: shape)
                .overlay(shape.strokeBorder(focused ? theme.accent : theme.borderStrong, lineWidth: focused ? 1.5 : 1))
                .accessibilityLabel(node.a11y.label.isEmpty ? node.string("label") : node.a11y.label)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onAppear { text = Self.display(node.value) }
        .onChange(of: node.value) { _, value in sync(value) }
        .onChange(of: text) { _, typed in edited(typed) }
        .onChange(of: focused) { _, isFocused in
            if !isFocused { recent.removeAll() }
        }
    }

    @ViewBuilder
    private func field(_ kind: String) -> some View {
        let placeholder = node.string("placeholder")
        switch kind {
        case "multiline":
            TextField(placeholder, text: $text, axis: .vertical)
                .lineLimit(3...8)
        case "number":
            TextField(placeholder, text: $text)
                .keyboardType(.decimalPad)
                .autocorrectionDisabled()
        case "phone":
            TextField(placeholder, text: $text)
                .keyboardType(.phonePad)
                .textContentType(.telephoneNumber)
                .autocorrectionDisabled()
        case "email":
            TextField(placeholder, text: $text)
                .keyboardType(.emailAddress)
                .textContentType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
        default:
            TextField(placeholder, text: $text)
        }
    }

    private var maxLength: Int {
        node.number("max_length").map { max(1, Int($0)) } ?? 200
    }

    private func edited(_ typed: String) {
        let limited = typed.count > maxLength ? String(typed.prefix(maxLength)) : typed
        if limited != typed {
            text = limited
            return
        }
        guard limited != Self.display(node.value), let path = node.props["value"]?.stringValue else { return }
        recent.append(limited)
        if recent.count > Self.recentLimit { recent.removeFirst() }
        session.setValue(surface: surface.id, path: path, value: .string(limited))
    }

    private func sync(_ value: JSONValue?) {
        let desired = Self.display(value)
        guard desired != text else { return }
        if focused && recent.contains(desired) { return }
        text = desired
    }

    static func display(_ value: JSONValue?) -> String {
        switch value {
        case .string(let text): text
        case .number(let number) where number.isFinite:
            number == number.rounded() && abs(number) < 1e15 ? String(Int64(number)) : String(number)
        default: ""
        }
    }
}
