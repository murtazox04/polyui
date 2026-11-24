import Foundation
import Observation
import UIKit

@MainActor
@Observable
public final class PolyUISession {
    @ObservationIgnored public let core: PolyUICore
    @ObservationIgnored public var send: (String) -> Void
    @ObservationIgnored public let allowHTTP: Bool
    @ObservationIgnored public var openLink: @MainActor (URL) -> Void = { url in
        UIApplication.shared.open(url)
    }
    public private(set) var surfaceIDs: [String] = []
    public private(set) var lastError: String?
    @ObservationIgnored private var surfaces: [String: PolyUISurfaceModel] = [:]
    @ObservationIgnored private var reportedTypes: Set<String> = []

    public init(core: PolyUICore, allowHTTP: Bool = false, send: @escaping (String) -> Void = { _ in }) {
        self.core = core
        self.allowHTTP = allowHTTP
        self.send = send
    }

    public convenience init(
        hello: PolyUIHello = PolyUIHello(),
        catalogs: [JSONValue] = [],
        allowHTTP: Bool = false,
        send: @escaping (String) -> Void = { _ in }
    ) throws {
        self.init(core: try PolyUICore(hello: hello, catalogs: catalogs, allowHTTP: allowHTTP), allowHTTP: allowHTTP, send: send)
    }

    public func surface(_ id: String) -> PolyUISurfaceModel {
        if let existing = surfaces[id] { return existing }
        let made = PolyUISurfaceModel(id: id)
        surfaces[id] = made
        surfaceIDs.append(id)
        return made
    }

    public func receive(_ message: String) {
        accept(attempt(surface: "") { try core.receive(message) })
    }

    public func receive(_ message: JSONValue) {
        guard let text = try? message.jsonText() else { return }
        receive(text)
    }

    public func setValue(surface: String, path: String, value: JSONValue) {
        accept(attempt(surface: surface) { try core.setValue(surface: surface, path: path, value: value) })
    }

    public func fire(surface: String, component: String, prop: String, detail: JSONValue? = nil) {
        if let message = attempt(surface: surface, { try core.fire(surface: surface, component: component, prop: prop, detail: detail) }) ?? nil {
            send(message)
        }
    }

    public func confirm(surface: String, request: ConfirmRequest) {
        dismissConfirm(surface: surface)
        if let message = attempt(surface: surface, { try core.confirm(surface: surface, actionID: request.actionID, nonce: request.nonce) }) ?? nil {
            send(message)
        }
    }

    public func dismissConfirm(surface: String) {
        surfaces[surface]?.pendingConfirm = nil
    }

    public func askToOpen(_ url: URL, surface: String, component: String, answer: @escaping @MainActor (Bool) -> Void) {
        let model = self.surface(surface)
        let busy = surfaces.values.contains { $0.pendingLink != nil || $0.pendingConfirm != nil }
        guard !busy, !model.isClosed, SafeURL.check(url.absoluteString) != nil else {
            answer(false)
            return
        }
        model.pendingLink = PolyUILinkRequest(url: url, component: component, answer: answer)
    }

    public func answerLink(surface: String, id: UUID, accepted: Bool) {
        guard let link = surfaces[surface]?.takeLink(id) else { return }
        if accepted { openLink(link.url) }
        link.answer(accepted)
    }

    public func withdrawLink(surface: String, component: String) {
        surfaces[surface]?.withdrawLink(from: component)
    }

    public func say(surface: String) -> [Utterance] {
        attempt(surface: surface) { try core.say(surface: surface) } ?? []
    }

    @discardableResult
    public func matchVoice(surface: String, text: String) -> VoiceMatch? {
        guard let found = attempt(surface: surface, { try core.matchVoice(surface: surface, text: text) }) ?? nil else {
            return nil
        }
        if let batch = found.batch { accept(batch) }
        if let action = found.action {
            if let nonce = action["confirm"]?.stringValue, surfaces[surface]?.pendingConfirm?.nonce == nonce {
                dismissConfirm(surface: surface)
            }
            transmit(action)
        }
        return found
    }

    public func highlight(surface: String, id: String?) {
        self.surface(surface).apply(RenderBatch(surface: surface, ops: [.highlight(id: id)]))
    }

    func reportUnknown(surface: String, type: String) {
        guard reportedTypes.insert(type).inserted else { return }
        reportError(surface: surface, reason: "unknown_component", detail: type)
    }

    private func accept(_ batch: RenderBatch?) {
        guard let batch else { return }
        for message in batch.send { transmit(message) }
        if !batch.ops.isEmpty || (batch.fallback != nil && !batch.surface.isEmpty) {
            surface(batch.surface).apply(batch)
        }
    }

    private func transmit(_ message: JSONValue) {
        if case .string(let text) = message {
            send(text)
        } else if let text = try? message.jsonText() {
            send(text)
        }
    }

    private func attempt<Value>(surface: String, _ run: () throws -> Value) -> Value? {
        do {
            return try run()
        } catch {
            let detail = String(describing: error)
            lastError = detail
            reportError(surface: surface, reason: "bad_message", detail: detail)
            return nil
        }
    }

    private func reportError(surface: String, reason: String, detail: String) {
        let message: JSONValue = ["v": 1, "t": "ui.error", "surface": .string(surface), "reason": .string(reason), "detail": .string(detail)]
        transmit(message)
    }
}
