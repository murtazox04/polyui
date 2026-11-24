import AVFoundation
import Observation

@MainActor
@Observable
public final class PolyUISpeaker {
    public private(set) var speakingID: String?
    public private(set) var isSpeaking = false
    @ObservationIgnored private let synthesizer = AVSpeechSynthesizer()
    @ObservationIgnored private let relay = SpeechRelay()
    @ObservationIgnored private var pending: [ObjectIdentifier: (utterance: AVSpeechUtterance, id: String)] = [:]
    @ObservationIgnored private var session: PolyUISession?
    @ObservationIgnored private var surfaceID: String?

    public var rate: Float = AVSpeechUtteranceDefaultSpeechRate

    public init() {
        synthesizer.delegate = relay
        relay.started = { [weak self] identifier in self?.started(identifier) }
        relay.finished = { [weak self] identifier in self?.finished(identifier) }
    }

    public func speak(surface: String, in session: PolyUISession, language: String? = nil) {
        speak(session.say(surface: surface), surface: surface, in: session, language: language)
    }

    public func speak(_ utterances: [Utterance], surface: String, in session: PolyUISession, language: String? = nil) {
        stop()
        self.session = session
        surfaceID = surface
        let voice = language.flatMap(AVSpeechSynthesisVoice.init(language:))
        for item in utterances where !item.text.isEmpty {
            let utterance = AVSpeechUtterance(string: item.text)
            utterance.voice = voice
            utterance.rate = rate
            utterance.postUtteranceDelay = 0.15
            pending[ObjectIdentifier(utterance)] = (utterance, item.id)
            synthesizer.speak(utterance)
        }
        isSpeaking = !pending.isEmpty
    }

    public func stop() {
        synthesizer.stopSpeaking(at: .immediate)
        pending.removeAll()
        clearHighlight()
        isSpeaking = false
    }

    private func started(_ identifier: ObjectIdentifier) {
        guard let id = pending[identifier]?.id else { return }
        speakingID = id
        if let session, let surfaceID { session.highlight(surface: surfaceID, id: id) }
    }

    private func finished(_ identifier: ObjectIdentifier) {
        guard pending.removeValue(forKey: identifier) != nil else { return }
        if pending.isEmpty {
            clearHighlight()
            isSpeaking = false
        }
    }

    private func clearHighlight() {
        if speakingID != nil, let session, let surfaceID {
            session.highlight(surface: surfaceID, id: nil)
        }
        speakingID = nil
    }
}

@MainActor
private final class SpeechRelay: NSObject, AVSpeechSynthesizerDelegate {
    var started: ((ObjectIdentifier) -> Void)?
    var finished: ((ObjectIdentifier) -> Void)?

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didStart utterance: AVSpeechUtterance) {
        let identifier = ObjectIdentifier(utterance)
        Task { @MainActor in self.started?(identifier) }
    }

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        let identifier = ObjectIdentifier(utterance)
        Task { @MainActor in self.finished?(identifier) }
    }

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        let identifier = ObjectIdentifier(utterance)
        Task { @MainActor in self.finished?(identifier) }
    }
}
