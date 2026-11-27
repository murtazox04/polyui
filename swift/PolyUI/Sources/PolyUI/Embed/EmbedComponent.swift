import SwiftUI
import WebKit

struct EmbedComponent: View {
    let node: ComponentNode
    @Environment(PolyUISurfaceModel.self) private var surface
    @Environment(PolyUISession.self) private var session
    @Environment(\.polyUITheme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var measured: Double?
    @State private var unavailableProps: String?

    var body: some View {
        let size = EmbedSize(prop: node.props["height"]?.stringValue)
        let title = node.string("title").isEmpty ? node.a11y.label : node.string("title")
        let shape = RoundedRectangle(cornerRadius: theme.innerCornerRadius, style: .continuous)
        if node.props["app"]?.objectValue != nil, let propsJSON = node.propsJSON, propsJSON != unavailableProps {
            EmbedWebView(
                propsJSON: propsJSON,
                title: title,
                onEvent: { event in handle(event, size: size, propsJSON: propsJSON) },
                onLink: { url, answer in
                    session.askToOpen(url, surface: surface.id, component: node.id, answer: answer)
                },
                appCalls: session.appCalls,
                onAppCall: { method, params, answer in
                    session.callApp(surface: surface.id, embed: node.id, method: method, params: params, answer: answer)
                }
            )
            .frame(height: measured ?? size.startHeight)
            .frame(maxWidth: .infinity)
            .clipShape(shape)
            .overlay(shape.strokeBorder(theme.border))
            .animation(reduceMotion ? nil : .smooth(duration: 0.2), value: measured)
            .accessibilityElement(children: .contain)
            .accessibilityLabel(title)
            .onDisappear {
                session.withdrawLink(surface: surface.id, component: node.id)
                session.withdrawAppCalls(surface: surface.id, embed: node.id)
            }
        } else {
            EmbedTitleView(title: title)
        }
    }

    private func handle(_ event: EmbedEvent, size: EmbedSize, propsJSON: String) {
        switch event {
        case .ready, .openLink, .appCall:
            break
        case .height(let value):
            let clamped = size.clamp(value)
            if measured != clamped { measured = clamped }
        case .message(let text):
            guard node.hasAction("on_message") else { return }
            session.fire(surface: surface.id, component: node.id, prop: "on_message", detail: ["text": .string(text)])
        case .unavailable:
            unavailableProps = propsJSON
        }
    }
}

struct EmbedTitleView: View {
    let title: String
    @Environment(\.polyUITheme) private var theme

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "square.on.square.dashed")
                .font(.title3)
                .foregroundStyle(theme.muted)
                .accessibilityHidden(true)
            Text(title)
                .font(.body.weight(.medium))
                .foregroundStyle(theme.foreground)
            Spacer(minLength: 0)
        }
        .padding(theme.spacing)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.sunken, in: RoundedRectangle(cornerRadius: theme.innerCornerRadius, style: .continuous))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
    }
}

struct EmbedWebView: UIViewRepresentable {
    let propsJSON: String
    let title: String
    let onEvent: @MainActor (EmbedEvent) -> Void
    let onLink: @MainActor (URL, @escaping @MainActor (Bool) -> Void) -> Void
    let appCalls: Bool
    let onAppCall: @MainActor (String, JSONValue, @escaping @MainActor (JSONValue) -> Void) -> Void

    func makeCoordinator() -> EmbedPageController {
        EmbedPageController()
    }

    func makeUIView(context: Context) -> WKWebView {
        let controller = context.coordinator
        let webView = controller.makeWebView()
        wire(controller, webView)
        controller.show(propsJSON)
        return webView
    }

    func updateUIView(_ webView: WKWebView, context: Context) {
        wire(context.coordinator, webView)
        context.coordinator.show(propsJSON)
    }

    static func dismantleUIView(_ webView: WKWebView, coordinator: EmbedPageController) {
        coordinator.tearDown()
    }

    private func wire(_ controller: EmbedPageController, _ webView: WKWebView) {
        controller.onEvent = onEvent
        controller.onLink = onLink
        controller.appCalls = appCalls
        controller.onAppCall = onAppCall
        webView.accessibilityLabel = title
    }
}
