import SwiftUI

@MainActor
public struct PolyUIComponentContext {
    public let node: ComponentNode
    public let surface: PolyUISurfaceModel
    public let session: PolyUISession

    public func fire(_ prop: String, detail: JSONValue? = nil) {
        session.fire(surface: surface.id, component: node.id, prop: prop, detail: detail)
    }

    public func setValue(path: String, value: JSONValue) {
        session.setValue(surface: surface.id, path: path, value: value)
    }

    public func child(_ id: String) -> some View {
        PolyUIComponentView(id: id)
    }

    public var children: some View {
        ForEach(node.children, id: \.self) { PolyUIComponentView(id: $0) }
    }
}

public struct PolyUIViewRegistry: Sendable {
    public typealias Builder = @MainActor @Sendable (PolyUIComponentContext) -> AnyView

    private var builders: [String: Builder] = [:]

    public init() {}

    public mutating func register<Content: View>(
        _ type: String,
        @ViewBuilder _ build: @escaping @MainActor @Sendable (PolyUIComponentContext) -> Content
    ) {
        builders[type] = { AnyView(build($0)) }
    }

    func builder(for type: String) -> Builder? {
        builders[type]
    }
}

extension EnvironmentValues {
    @Entry public var polyUIViews = PolyUIViewRegistry()
}

extension View {
    public func polyUIView<Content: View>(
        for type: String,
        @ViewBuilder _ build: @escaping @MainActor @Sendable (PolyUIComponentContext) -> Content
    ) -> some View {
        transformEnvironment(\.polyUIViews) { registry in
            registry.register(type, build)
        }
    }
}
