import Foundation
import Observation

@MainActor
@Observable
public final class ComponentNode: Identifiable {
    public let id: String
    public let type: String
    public private(set) var props: [String: JSONValue]
    public private(set) var a11y: A11y
    public private(set) var value: JSONValue?
    public internal(set) var children: [String] = []
    public internal(set) var isHighlighted = false
    public private(set) var revision = 0

    @ObservationIgnored private var sceneCache: (revision: Int, scene: ChartScene?)?
    @ObservationIgnored private var propsCache: (revision: Int, text: String?)?

    init(payload: ComponentPayload, type: String) {
        id = payload.id
        self.type = type
        props = payload.props
        a11y = payload.a11y
        value = payload.value
    }

    func update(with payload: ComponentPayload) {
        if props != payload.props { props = payload.props }
        if a11y != payload.a11y { a11y = payload.a11y }
        if value != payload.value { value = payload.value }
        revision += 1
    }

    public func string(_ key: String) -> String {
        props[key]?.stringValue ?? ""
    }

    public func flag(_ key: String) -> Bool {
        props[key]?.boolValue ?? false
    }

    public func number(_ key: String) -> Double? {
        props[key]?.doubleValue
    }

    public func choice(_ key: String, among allowed: Set<String>, default fallback: String) -> String {
        guard let raw = props[key]?.stringValue, allowed.contains(raw) else { return fallback }
        return raw
    }

    public func hasAction(_ key: String) -> Bool {
        props[key]?.objectValue?["action_id"]?.stringValue != nil
    }

    var propsJSON: String? {
        if let propsCache, propsCache.revision == revision {
            return propsCache.text
        }
        let text = try? JSONValue.object(props).jsonText()
        propsCache = (revision, text)
        return text
    }

    var chartScene: ChartScene? {
        if let sceneCache, sceneCache.revision == revision {
            return sceneCache.scene
        }
        let scene = props["scene"].flatMap(ChartScene.init(json:))
        sceneCache = (revision, scene)
        return scene
    }
}
