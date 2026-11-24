import SwiftUI

public struct PolyUISurfaceView: View {
    private let session: PolyUISession
    private let surface: PolyUISurfaceModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.polyUILabels) private var labels

    public init(session: PolyUISession, surface id: String) {
        self.session = session
        self.surface = session.surface(id)
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let rootID = surface.rootID {
                PolyUIComponentView(id: rootID)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .animation(reduceMotion ? nil : .smooth(duration: 0.22), value: surface.structureRevision)
        .environment(session)
        .environment(surface)
        .sheet(item: sheetBinding) { sheet in
            switch sheet {
            case .confirm(let request):
                PolyUIConfirmRequestSheet(request: request) {
                    session.confirm(surface: surface.id, request: request)
                } onDismiss: {
                    session.dismissConfirm(surface: surface.id)
                }
            case .link(let link):
                PolyUIConfirmRequestSheet(request: linkQuestion(link)) {
                    session.answerLink(surface: surface.id, id: link.id, accepted: true)
                } onDismiss: {
                    session.answerLink(surface: surface.id, id: link.id, accepted: false)
                }
            }
        }
    }

    private var sheetBinding: Binding<PolyUISheet?> {
        Binding {
            surface.presentedSheet
        } set: { next in
            guard next == nil else { return }
            switch surface.presentedSheet {
            case .confirm: session.dismissConfirm(surface: surface.id)
            case .link(let link): session.answerLink(surface: surface.id, id: link.id, accepted: false)
            case nil: break
            }
        }
    }

    private func linkQuestion(_ link: PolyUILinkRequest) -> ConfirmRequest {
        ConfirmRequest(
            actionID: "open-link",
            nonce: link.id.uuidString,
            title: labels.openLinkTitle,
            body: link.url.absoluteString,
            confirmLabel: labels.openLink
        )
    }
}

public struct PolyUIComponentView: View {
    let id: String
    @Environment(PolyUISurfaceModel.self) private var surface

    public init(id: String) {
        self.id = id
    }

    public var body: some View {
        if let node = surface.node(id) {
            NodeView(node: node)
                .id(ObjectIdentifier(node))
                .transition(.opacity)
        }
    }
}

struct NodeView: View {
    let node: ComponentNode
    @Environment(PolyUISurfaceModel.self) private var surface
    @Environment(PolyUISession.self) private var session
    @Environment(\.polyUIViews) private var registry

    var body: some View {
        content
            .modifier(HighlightModifier(isOn: node.isHighlighted))
    }

    @ViewBuilder
    private var content: some View {
        if let custom = registry.builder(for: node.type) {
            custom(PolyUIComponentContext(node: node, surface: surface, session: session))
        } else {
            switch node.type {
            case "Text": TextComponent(node: node)
            case "Card": CardComponent(node: node)
            case "Row": RowComponent(node: node)
            case "List": ListComponent(node: node)
            case "Stat": StatComponent(node: node)
            case "Badge": BadgeComponent(node: node)
            case "Image": ImageComponent(node: node)
            case "Button": ButtonComponent(node: node)
            case "ChoiceGroup": ChoiceGroupComponent(node: node)
            case "Input": InputComponent(node: node)
            case "ConfirmSheet": ConfirmSheetComponent(node: node)
            case "Divider": DividerComponent()
            case "chart:Chart": ChartComponent(node: node)
            case "chart:Dashboard": DashboardComponent(node: node)
            case "embed:Embed": EmbedComponent(node: node)
            default: UnknownComponent(node: node)
            }
        }
    }
}

struct HighlightModifier: ViewModifier {
    let isOn: Bool
    @Environment(\.polyUITheme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        content
            .padding(isOn ? 6 : 0)
            .background {
                if isOn {
                    RoundedRectangle(cornerRadius: theme.controlCornerRadius, style: .continuous)
                        .fill(theme.accent.opacity(0.1))
                        .strokeBorder(theme.accent.opacity(0.55), lineWidth: 1.5)
                }
            }
            .padding(isOn ? -6 : 0)
            .animation(reduceMotion ? nil : .easeOut(duration: 0.18), value: isOn)
    }
}

struct UnknownComponent: View {
    let node: ComponentNode
    @Environment(PolyUISurfaceModel.self) private var surface
    @Environment(PolyUISession.self) private var session
    @Environment(\.polyUITheme) private var theme

    var body: some View {
        Text(node.a11y.label)
            .font(.subheadline)
            .foregroundStyle(theme.muted)
            .onAppear { session.reportUnknown(surface: surface.id, type: node.type) }
    }
}
