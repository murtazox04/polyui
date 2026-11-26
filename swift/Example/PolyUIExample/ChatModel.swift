import Foundation
import Observation
import PolyUI

@MainActor
@Observable
final class ChatModel {
    enum Mode: Equatable {
        case replay
        case live(URL)
    }

    enum Entry: Identifiable, Equatable {
        case user(id: Int, text: String)
        case agent(id: Int, text: String)
        case surface(id: Int, surface: String)

        var id: Int {
            switch self {
            case .user(let id, _), .agent(let id, _), .surface(let id, _): id
            }
        }
    }

    private(set) var entries: [Entry] = []
    private(set) var mode: Mode = .replay
    private(set) var session: PolyUISession
    private(set) var problem: String?
    private(set) var heard: String?
    let speaker = PolyUISpeaker()
    let listener = PolyUIListener()
    let socket = PolyUIWebSocket()
    @ObservationIgnored private var scripted: ScriptedServer?
    @ObservationIgnored private var nextID = 0
    @ObservationIgnored private var openAgentEntry: Int?
    @ObservationIgnored private let hello = PolyUIHello(
        client: PolyUIHello.ClientInfo(name: "polyui-ios-example", version: "0.1.0"),
        catalogs: ["core@1", "chart@1", "embed@1"],
        locale: "en"
    )
    @ObservationIgnored let instant: Bool

    static let liveURL = URL(string: "ws://localhost:8787/ws")!

    init(instant: Bool = false) {
        self.instant = instant
        do {
            session = try PolyUISession(hello: hello)
        } catch {
            fatalError("The bundled hello is valid JSON: \(error)")
        }
        startReplay()
    }

    var question: String {
        scripted?.question ?? "What do we owe this month?"
    }

    var forecastQuestion: String {
        scripted?.forecastQuestion ?? "What is the weather in Tashkent this week?"
    }

    var latestSurface: String? {
        entries.last { if case .surface = $0 { true } else { false } }.flatMap { entry in
            if case .surface(_, let surface) = entry { surface } else { nil }
        }
    }

    var isLive: Bool {
        if case .live = mode { true } else { false }
    }

    func startReplay() {
        socket.disconnect()
        reset()
        mode = .replay
        let server = ScriptedServer(script: ScriptedServer.loadScript(), instant: instant) { [weak self] text in
            self?.incoming(text)
        }
        scripted = server
        session.send = { [weak self] text in self?.outgoing(text) }
        if let hello = try? hello.jsonText() { outgoing(hello) }
    }

    func connectLive(to url: URL = ChatModel.liveURL) {
        reset()
        scripted = nil
        mode = .live(url)
        session.send = { [weak self] text in self?.socket.send(text) }
        socket.onSessionLost = { [weak self] in self?.sessionLost() }
        guard let hello = try? hello.jsonText() else { return }
        socket.connect(to: url, hello: hello) { [weak self] text in
            self?.incoming(text)
        }
    }

    func ask(_ text: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        append(.user(id: takeID(), text: trimmed))
        let message: JSONValue = ["v": 1, "t": "user.say", "text": .string(trimmed)]
        if let text = try? message.jsonText() { session.send(text) }
    }

    func speak(surface: String) {
        if speaker.isSpeaking {
            speaker.stop()
        } else {
            speaker.speak(surface: surface, in: session, language: "en-US")
        }
    }

    func toggleListening() {
        if listener.isListening {
            listener.stop()
            return
        }
        speaker.stop()
        heard = nil
        let surface = latestSurface
        let listener = listener
        let session = session
        Task { [weak self] in
            await listener.listen(surface: surface, in: session, locale: Locale(identifier: "en-US")) { result in
                self?.handle(result)
            }
        }
    }

    func hear(_ text: String) {
        if let result = listener.hear(text, surface: latestSurface, in: session) {
            handle(result)
        }
    }

    private func handle(_ result: PolyUIHeard) {
        if result.match == nil {
            heard = nil
            ask(result.transcript)
        } else {
            heard = result.transcript
        }
    }

    private func reset() {
        speaker.stop()
        listener.cancel()
        heard = nil
        entries = []
        openAgentEntry = nil
        problem = nil
        if let fresh = try? PolyUISession(hello: hello) { session = fresh }
    }

    private func sessionLost() {
        entries.removeAll { if case .surface = $0 { true } else { false } }
        openAgentEntry = nil
        problem = "The connection was gone too long, so the earlier cards were removed."
        if let fresh = try? PolyUISession(hello: hello) {
            fresh.send = session.send
            session = fresh
        }
    }

    private func outgoing(_ text: String) {
        guard let scripted else { return }
        Task { await scripted.receive(text) }
    }

    private func incoming(_ text: String) {
        let message = try? JSONValue.parse(text)
        switch message?["t"]?.stringValue {
        case "agent.delta":
            appendAgentText(message?["text"]?.stringValue ?? "")
        case "agent.done":
            openAgentEntry = nil
        case "agent.error":
            problem = message?["message"]?.stringValue
            openAgentEntry = nil
        case "ui.open", "ui.snapshot":
            if let surface = message?["surface"]?.stringValue, !hasSurface(surface) {
                openAgentEntry = nil
                append(.surface(id: takeID(), surface: surface))
            }
            session.receive(text)
        default:
            session.receive(text)
        }
    }

    private func hasSurface(_ surface: String) -> Bool {
        entries.contains { if case .surface(_, let id) = $0 { id == surface } else { false } }
    }

    private func appendAgentText(_ chunk: String) {
        if let openAgentEntry, let index = entries.firstIndex(where: { $0.id == openAgentEntry }),
            case .agent(let id, let text) = entries[index]
        {
            entries[index] = .agent(id: id, text: text + chunk)
        } else {
            let id = takeID()
            openAgentEntry = id
            append(.agent(id: id, text: chunk))
        }
    }

    private func append(_ entry: Entry) {
        entries.append(entry)
    }

    private func takeID() -> Int {
        nextID += 1
        return nextID
    }
}
