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

    public private(set) var state: State = .idle
    @ObservationIgnored private var task: URLSessionWebSocketTask?
    @ObservationIgnored private var reader: Task<Void, Never>?
    @ObservationIgnored private let urlSession: URLSession

    public init(urlSession: URLSession = .shared) {
        self.urlSession = urlSession
    }

    public func connect(to url: URL, hello: String, onMessage: @escaping @MainActor (String) -> Void) {
        disconnect()
        let socket = urlSession.webSocketTask(with: url)
        task = socket
        state = .connecting
        socket.resume()
        send(hello)
        reader = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    let text: String? =
                        switch try await socket.receive() {
                        case .string(let text): text
                        case .data(let data): String(data: data, encoding: .utf8)
                        @unknown default: nil
                        }
                    guard let self else { return }
                    if self.state != .open { self.state = .open }
                    if let text { onMessage(text) }
                } catch {
                    self?.state = .closed(error.localizedDescription)
                    return
                }
            }
        }
    }

    public func send(_ text: String) {
        guard let task else { return }
        task.send(.string(text)) { [weak self] error in
            guard let error else { return }
            let message = error.localizedDescription
            Task { @MainActor in self?.state = .closed(message) }
        }
    }

    public func disconnect() {
        reader?.cancel()
        reader = nil
        task?.cancel(with: .normalClosure, reason: nil)
        task = nil
        if state != .idle { state = .closed(nil) }
    }
}
