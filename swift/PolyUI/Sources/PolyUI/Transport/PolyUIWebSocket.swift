import Foundation
import Observation

@MainActor
@Observable
public final class PolyUIWebSocket {
    public enum State: Sendable, Hashable {
        case idle
        case connecting
        case open
        case closed(String?)
    }

    public static let resumeFeature = "resume"

    public private(set) var state: State = .idle
    public private(set) var resumed: Bool?
    @ObservationIgnored public var onSessionLost: (@MainActor () -> Void)?
    @ObservationIgnored public var retryDelays: [Duration] = [.seconds(1), .seconds(2), .seconds(4), .seconds(8), .seconds(16)]
    @ObservationIgnored private var task: URLSessionWebSocketTask?
    @ObservationIgnored private var reader: Task<Void, Never>?
    @ObservationIgnored private var retry: Task<Void, Never>?
    @ObservationIgnored private var target: Target?
    @ObservationIgnored private var resume: Resume?
    @ObservationIgnored private var attempt = 0
    @ObservationIgnored private let urlSession: URLSession

    private struct Target {
        let url: URL
        let hello: String
        let onMessage: @MainActor (String) -> Void
    }

    struct Resume {
        let session: String
        let token: String
    }

    public init(urlSession: URLSession = .shared) {
        self.urlSession = urlSession
    }

    public func connect(to url: URL, hello: String, onMessage: @escaping @MainActor (String) -> Void) {
        disconnect()
        target = Target(url: url, hello: hello, onMessage: onMessage)
        resume = nil
        resumed = nil
        attempt = 0
        open()
    }

    public func send(_ text: String) {
        guard let task else { return }
        task.send(.string(text)) { error in
            guard error != nil else { return }
            Task { @MainActor in task.cancel(with: .abnormalClosure, reason: nil) }
        }
    }

    public func disconnect() {
        retry?.cancel()
        retry = nil
        reader?.cancel()
        reader = nil
        task?.cancel(with: .normalClosure, reason: nil)
        task = nil
        target = nil
        resume = nil
        if state != .idle { state = .closed(nil) }
    }

    private func open() {
        guard let target else { return }
        let socket = urlSession.webSocketTask(with: target.url)
        task = socket
        state = .connecting
        socket.resume()
        send(Self.hello(target.hello, resume: resume))
        reader = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    let text: String? =
                        switch try await socket.receive() {
                        case .string(let text): text
                        case .data(let data): String(data: data, encoding: .utf8)
                        @unknown default: nil
                        }
                    guard let self, self.task === socket else { return }
                    if self.state != .open { self.state = .open }
                    if let text {
                        self.noteWelcome(text)
                        target.onMessage(text)
                    }
                } catch {
                    guard let self, self.task === socket else { return }
                    self.dropped(error.localizedDescription)
                    return
                }
            }
        }
    }

    private func dropped(_ reason: String) {
        task = nil
        reader = nil
        guard resume != nil, attempt < retryDelays.count else {
            state = .closed(reason)
            return
        }
        let delay = retryDelays[attempt]
        attempt += 1
        state = .connecting
        retry = Task { [weak self] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled, let self else { return }
            self.open()
        }
    }

    private func noteWelcome(_ text: String) {
        guard let message = try? JSONValue.parse(text), message["t"]?.stringValue == "welcome" else { return }
        attempt = 0
        if let session = message["session"]?.stringValue, let token = message["resume_token"]?.stringValue {
            resume = Resume(session: session, token: token)
        } else {
            resume = nil
        }
        resumed = message["resumed"]?.boolValue
        if resumed == false { onSessionLost?() }
    }

    static func hello(_ text: String, resume: Resume?) -> String {
        guard case .object(var fields)? = try? JSONValue.parse(text) else { return text }
        var features = fields["features"]?.arrayValue ?? []
        if !features.contains(.string(resumeFeature)) { features.append(.string(resumeFeature)) }
        fields["features"] = .array(features)
        if let resume {
            fields["resume"] = .object(["session": .string(resume.session), "token": .string(resume.token)])
        } else {
            fields.removeValue(forKey: "resume")
        }
        return (try? JSONValue.object(fields).jsonText()) ?? text
    }
}
