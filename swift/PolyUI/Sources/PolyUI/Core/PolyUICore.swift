import CPolyUI
import Foundation

public enum PolyUIError: Error, Sendable, Hashable, CustomStringConvertible {
    case core(String)
    case undecodable(String)

    public var description: String {
        switch self {
        case .core(let message): message
        case .undecodable(let message): "PolyUI could not read the core output: \(message)"
        }
    }
}

public struct PolyUIHello: Sendable, Hashable, Encodable {
    public struct ClientInfo: Sendable, Hashable, Encodable {
        public var name: String
        public var version: String

        public init(name: String, version: String) {
            self.name = name
            self.version = version
        }
    }

    public var version = 1
    public var kind = "hello"
    public var client: ClientInfo
    public var catalogs: [String]
    public var modalities: [String]
    public var features: [String]
    public var locale: String

    public init(
        client: ClientInfo = ClientInfo(name: "polyui-swiftui", version: "0.1.0"),
        catalogs: [String] = ["core@1", "chart@1"],
        modalities: [String] = ["screen", "voice"],
        features: [String] = ["sync"],
        locale: String = Locale.current.identifier(.bcp47)
    ) {
        self.client = client
        self.catalogs = catalogs
        self.modalities = modalities
        self.features = features
        self.locale = locale
    }

    enum CodingKeys: String, CodingKey {
        case version = "v"
        case kind = "t"
        case client, catalogs, modalities, features, locale
    }

    public func jsonText() throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return String(decoding: try encoder.encode(self), as: UTF8.self)
    }
}

final class NativeClient {
    let handle: OpaquePointer

    init(handle: OpaquePointer) {
        self.handle = handle
    }

    deinit {
        polyui_client_free(handle)
    }
}

typealias ErrorSlot = UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>

@MainActor
public final class PolyUICore {
    private let native: NativeClient
    public let hello: String

    public init(helloJSON: String, catalogsJSON: String? = nil, allowHTTP: Bool = false) throws {
        var error: UnsafeMutablePointer<CChar>?
        let handle = helloJSON.withCString { hello in
            if let catalogsJSON {
                catalogsJSON.withCString { polyui_client_new_with_catalogs(hello, $0, allowHTTP, &error) }
            } else {
                polyui_client_new(hello, allowHTTP, &error)
            }
        }
        guard let handle else {
            throw PolyUIError.core(Self.consume(error) ?? "PolyUI could not start a client")
        }
        native = NativeClient(handle: handle)
        hello = helloJSON
    }

    public convenience init(hello: PolyUIHello = PolyUIHello(), catalogs: [JSONValue] = [], allowHTTP: Bool = false) throws {
        try self.init(
            helloJSON: try hello.jsonText(),
            catalogsJSON: catalogs.isEmpty ? nil : try JSONValue.array(catalogs).jsonText(),
            allowHTTP: allowHTTP
        )
    }

    public func receiveJSON(_ message: String) throws -> String {
        try call { client, error in
            message.withCString { polyui_client_receive(client, $0, error) }
        }
    }

    public func setValueJSON(surface: String, path: String, valueJSON: String) throws -> String {
        try call { client, error in
            surface.withCString { surface in
                path.withCString { path in
                    valueJSON.withCString { polyui_client_set_value(client, surface, path, $0, error) }
                }
            }
        }
    }

    public func fireJSON(surface: String, component: String, prop: String, detailJSON: String? = nil) throws -> String {
        try call { client, error in
            surface.withCString { surface in
                component.withCString { component in
                    prop.withCString { prop in
                        if let detailJSON {
                            detailJSON.withCString { polyui_client_fire(client, surface, component, prop, $0, error) }
                        } else {
                            polyui_client_fire(client, surface, component, prop, nil, error)
                        }
                    }
                }
            }
        }
    }

    public func confirmJSON(surface: String, actionID: String, nonce: String) throws -> String {
        try call { client, error in
            surface.withCString { surface in
                actionID.withCString { actionID in
                    nonce.withCString { polyui_client_confirm(client, surface, actionID, $0, error) }
                }
            }
        }
    }

    public func sayJSON(surface: String) throws -> String {
        try call { client, error in
            surface.withCString { polyui_client_say(client, $0, error) }
        }
    }

    public func matchVoiceJSON(surface: String, text: String) throws -> String {
        try call { client, error in
            surface.withCString { surface in
                text.withCString { polyui_client_match_voice(client, surface, $0, error) }
            }
        }
    }

    public func receive(_ message: String) throws -> RenderBatch {
        try Self.decode(RenderBatch.self, from: try receiveJSON(message))
    }

    public func setValue(surface: String, path: String, value: JSONValue) throws -> RenderBatch {
        let text = try setValueJSON(surface: surface, path: path, valueJSON: try value.jsonText())
        return try Self.decode(RenderBatch.self, from: text)
    }

    public func fire(surface: String, component: String, prop: String, detail: JSONValue? = nil) throws -> String? {
        let text = try fireJSON(surface: surface, component: component, prop: prop, detailJSON: try detail?.jsonText())
        return text == "null" ? nil : text
    }

    public func confirm(surface: String, actionID: String, nonce: String) throws -> String? {
        let text = try confirmJSON(surface: surface, actionID: actionID, nonce: nonce)
        return text == "null" ? nil : text
    }

    public func say(surface: String) throws -> [Utterance] {
        try Self.decode([Utterance].self, from: try sayJSON(surface: surface))
    }

    public func matchVoice(surface: String, text: String) throws -> VoiceMatch? {
        let raw = try matchVoiceJSON(surface: surface, text: text)
        return raw == "null" ? nil : try Self.decode(VoiceMatch.self, from: raw)
    }

    public nonisolated static func embedPage(
        propsJSON: String,
        nonce: String,
        appCalls: Bool = false,
        appContext: Bool = false
    ) throws -> String {
        var error: UnsafeMutablePointer<CChar>?
        let host = #"{"app_calls":\#(appCalls),"app_context":\#(appContext)}"#
        let page = propsJSON.withCString { props in
            nonce.withCString { nonceText in
                host.withCString { polyui_embed_page_with(props, nonceText, $0, &error) }
            }
        }
        guard let page else {
            throw PolyUIError.core(consume(error) ?? "PolyUI could not build the embed page")
        }
        defer { polyui_string_free(page) }
        return String(cString: page)
    }

    private func call(_ body: (OpaquePointer, ErrorSlot) -> UnsafeMutablePointer<CChar>?) throws -> String {
        var error: UnsafeMutablePointer<CChar>?
        let result = withUnsafeMutablePointer(to: &error) { body(native.handle, $0) }
        guard let result else {
            throw PolyUIError.core(Self.consume(error) ?? "PolyUI call failed without a message")
        }
        defer { polyui_string_free(result) }
        return String(cString: result)
    }

    private nonisolated static func consume(_ pointer: UnsafeMutablePointer<CChar>?) -> String? {
        guard let pointer else { return nil }
        defer { polyui_string_free(pointer) }
        return String(cString: pointer)
    }

    static func decode<Value: Decodable>(_ type: Value.Type, from text: String) throws -> Value {
        do {
            return try JSONDecoder().decode(type, from: Data(text.utf8))
        } catch {
            throw PolyUIError.undecodable(String(describing: error))
        }
    }
}
