import Foundation
import Security

public enum EmbedSize: String, Sendable, CaseIterable {
    case small
    case medium
    case large

    public static let minimumHeight: Double = 80

    public init(prop: String?) {
        self = prop.flatMap(EmbedSize.init(rawValue:)) ?? .medium
    }

    public var startHeight: Double {
        switch self {
        case .small: 160
        case .medium: 320
        case .large: 480
        }
    }

    public var maximumHeight: Double {
        switch self {
        case .small: 320
        case .medium: 640
        case .large: 960
        }
    }

    public func clamp(_ height: Double) -> Double {
        guard height.isFinite else { return startHeight }
        return min(maximumHeight, max(Self.minimumHeight, height.rounded(.up)))
    }
}

enum EmbedNonce {
    static let length = 32
    private static let alphabet = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".utf8)
    private static let fairLimit = UInt8(alphabet.count * (256 / alphabet.count))

    static func make() -> String? {
        var picked: [UInt8] = []
        picked.reserveCapacity(length)
        while picked.count < length {
            var bytes = [UInt8](repeating: 0, count: 64)
            guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else { return nil }
            for byte in bytes where byte < fairLimit && picked.count < length {
                picked.append(alphabet[Int(byte) % alphabet.count])
            }
        }
        return String(decoding: picked, as: UTF8.self)
    }

    static func matches(_ sent: String, _ expected: String) -> Bool {
        let left = Array(sent.utf8)
        let right = Array(expected.utf8)
        guard !right.isEmpty, left.count == right.count else { return false }
        var difference: UInt8 = 0
        for index in left.indices {
            difference |= left[index] ^ right[index]
        }
        return difference == 0
    }
}

enum EmbedEvent: Equatable, Sendable {
    case ready
    case height(Double)
    case message(String)
    case openLink(URL)
    case appCall(id: String, method: String, params: JSONValue)
    case unavailable

    static let longestBody = 98_304
    static let longestCallID = 64
    static let appMethods: Set<String> = ["tools/call", "resources/read", "ui/update-model-context"]

    static func read(_ body: Any, isMainFrame: Bool, nonce: String) -> EmbedEvent? {
        guard isMainFrame, let text = body as? String, text.utf8.count <= longestBody,
            let fields = (try? JSONValue.parse(text))?.objectValue,
            let sent = fields["nonce"]?.stringValue, EmbedNonce.matches(sent, nonce)
        else { return nil }
        switch fields["kind"]?.stringValue {
        case "ready":
            return .ready
        case "height":
            return fields["value"]?.doubleValue.map(EmbedEvent.height)
        case "message":
            guard let said = fields["text"]?.stringValue, !said.isEmpty else { return nil }
            return .message(said)
        case "open-link":
            return SafeURL.check(fields["url"]?.stringValue).map(EmbedEvent.openLink)
        case "app-call":
            guard let id = fields["id"]?.stringValue, !id.isEmpty, id.count <= longestCallID,
                let method = fields["method"]?.stringValue, appMethods.contains(method),
                let params = fields["params"], params.objectValue != nil
            else { return nil }
            return .appCall(id: id, method: method, params: params)
        case "unavailable":
            return .unavailable
        default:
            return nil
        }
    }
}

enum EmbedNavigation {
    static func allows(targetIsMainFrame: Bool?, url: URL?, expectingPageLoad: Bool) -> Bool {
        guard let targetIsMainFrame else { return false }
        if targetIsMainFrame { return expectingPageLoad }
        guard let url, let scheme = url.scheme?.lowercased() else { return false }
        if scheme == "about" {
            let place = url.absoluteString.lowercased()
            return place == "about:srcdoc" || place == "about:blank"
        }
        return scheme == "https"
    }
}
