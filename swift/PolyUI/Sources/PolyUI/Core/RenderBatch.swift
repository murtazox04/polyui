import Foundation

public struct A11y: Sendable, Hashable, Codable {
    public var role: String
    public var label: String

    public init(role: String = "", label: String = "") {
        self.role = role
        self.label = label
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        role = try container.decodeIfPresent(String.self, forKey: .role) ?? ""
        label = try container.decodeIfPresent(String.self, forKey: .label) ?? ""
    }
}

public struct ComponentPayload: Sendable, Hashable {
    public var id: String
    public var type: String?
    public var props: [String: JSONValue]
    public var value: JSONValue?
    public var a11y: A11y

    public init(id: String, type: String? = nil, props: [String: JSONValue] = [:], value: JSONValue? = nil, a11y: A11y = A11y()) {
        self.id = id
        self.type = type
        self.props = props
        self.value = value
        self.a11y = a11y
    }
}

public struct ConfirmRequest: Sendable, Hashable, Identifiable, Codable {
    public var actionID: String
    public var nonce: String
    public var title: String
    public var body: String?
    public var confirmLabel: String?
    public var expiresMilliseconds: Int?

    public var id: String { "\(actionID):\(nonce)" }

    public init(actionID: String, nonce: String, title: String, body: String? = nil, confirmLabel: String? = nil, expiresMilliseconds: Int? = nil) {
        self.actionID = actionID
        self.nonce = nonce
        self.title = title
        self.body = body
        self.confirmLabel = confirmLabel
        self.expiresMilliseconds = expiresMilliseconds
    }

    enum CodingKeys: String, CodingKey {
        case actionID = "action_id"
        case nonce
        case title
        case body
        case confirmLabel = "confirm_label"
        case expiresMilliseconds = "expires_ms"
    }
}

public enum RenderOp: Sendable, Hashable {
    case create(ComponentPayload)
    case update(ComponentPayload)
    case children(id: String, children: [String])
    case remove(id: String)
    case root(id: String)
    case confirm(ConfirmRequest)
    case highlight(id: String?)
    case close
    case unknown(String)
}

extension RenderOp: Decodable {
    enum CodingKeys: String, CodingKey {
        case op, id, type, props, value, a11y, children
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let name = try container.decode(String.self, forKey: .op)
        switch name {
        case "create", "update":
            let payload = ComponentPayload(
                id: try container.decode(String.self, forKey: .id),
                type: try container.decodeIfPresent(String.self, forKey: .type),
                props: try container.decodeIfPresent([String: JSONValue].self, forKey: .props) ?? [:],
                value: container.contains(.value) ? try container.decode(JSONValue.self, forKey: .value) : nil,
                a11y: try container.decodeIfPresent(A11y.self, forKey: .a11y) ?? A11y()
            )
            self = name == "create" ? .create(payload) : .update(payload)
        case "children":
            self = .children(
                id: try container.decode(String.self, forKey: .id),
                children: try container.decodeIfPresent([String].self, forKey: .children) ?? []
            )
        case "remove":
            self = .remove(id: try container.decode(String.self, forKey: .id))
        case "root":
            self = .root(id: try container.decode(String.self, forKey: .id))
        case "confirm":
            self = .confirm(try ConfirmRequest(from: decoder))
        case "highlight":
            self = .highlight(id: try container.decodeIfPresent(String.self, forKey: .id))
        case "close":
            self = .close
        default:
            self = .unknown(name)
        }
    }
}

public struct RenderBatch: Sendable, Hashable, Decodable {
    public var surface: String
    public var ops: [RenderOp]
    public var fallback: String?
    public var send: [JSONValue]

    public init(surface: String, ops: [RenderOp] = [], fallback: String? = nil, send: [JSONValue] = []) {
        self.surface = surface
        self.ops = ops
        self.fallback = fallback
        self.send = send
    }

    enum CodingKeys: String, CodingKey {
        case surface, ops, fallback, send
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        surface = try container.decodeIfPresent(String.self, forKey: .surface) ?? ""
        ops = try container.decodeIfPresent([RenderOp].self, forKey: .ops) ?? []
        fallback = try container.decodeIfPresent(String.self, forKey: .fallback)
        send = try container.decodeIfPresent([JSONValue].self, forKey: .send) ?? []
    }
}

public struct Utterance: Sendable, Hashable, Codable {
    public var id: String
    public var text: String

    public init(id: String, text: String) {
        self.id = id
        self.text = text
    }
}

public struct VoiceMatch: Sendable, Hashable, Decodable {
    public var component: String
    public var option: String?
    public var batch: RenderBatch?
    public var action: JSONValue?

    enum CodingKeys: String, CodingKey {
        case component, option, batch, action
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        component = try container.decode(String.self, forKey: .component)
        option = try container.decodeIfPresent(String.self, forKey: .option)
        batch = try container.decodeIfPresent(RenderBatch.self, forKey: .batch)
        let action = try container.decodeIfPresent(JSONValue.self, forKey: .action)
        self.action = action == .null ? nil : action
    }
}
