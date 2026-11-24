import Foundation

public enum SafeURL {
    public static func check(_ raw: String?, allowHTTP: Bool = false) -> URL? {
        guard let raw, !raw.isEmpty, raw.utf8.count <= 4096 else { return nil }
        let forbidden = raw.unicodeScalars.contains { scalar in
            scalar.value <= 0x20 || (0x7F...0x9F).contains(scalar.value)
        }
        guard !forbidden, let parts = URLComponents(string: raw), let scheme = parts.scheme?.lowercased() else {
            return nil
        }
        let allowed = scheme == "https" || (allowHTTP && scheme == "http")
        guard allowed, parts.user == nil, parts.password == nil, let host = parts.host, !host.isEmpty else {
            return nil
        }
        return parts.url
    }
}
