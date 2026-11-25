import AVFoundation
import Observation
import Speech

public struct PolyUIHeard: Sendable, Hashable {
    public var transcript: String
    public var match: VoiceMatch?
}

public struct PolyUIListenerLabels: Sendable, Hashable {
    public var denied: String
    public var unavailable: String
    public var noMicrophone: String
    public var failed: String

    public init(
        denied: String = "Speech recognition is off for this app. Turn it on in Settings, or type instead.",
        unavailable: String = "Speech recognition is not available right now. Type instead.",
        noMicrophone: String = "No microphone is available. Type instead.",
        failed: String = "Could not hear that. Try again or type instead."
    ) {
        self.denied = denied
        self.unavailable = unavailable
        self.noMicrophone = noMicrophone
        self.failed = failed
    }
}

@MainActor
@Observable
public final class PolyUIListener {
    public enum State: Sendable, Hashable {
        case idle
        case starting
        case listening
        case unavailable(String)
    }

    public private(set) var state: State = .idle
    public private(set) var transcript = ""
    @ObservationIgnored public var labels = PolyUIListenerLabels()
    @ObservationIgnored private let engine = AVAudioEngine()
    @ObservationIgnored private var request: SFSpeechAudioBufferRecognitionRequest?
    @ObservationIgnored private var task: SFSpeechRecognitionTask?
    @ObservationIgnored private var target: (surface: String?, session: PolyUISession)?
    @ObservationIgnored private var onHeard: (@MainActor (PolyUIHeard) -> Void)?
    @ObservationIgnored private var audioWork: Task<Bool, Never>?
    @ObservationIgnored private var audioActive = false
    @ObservationIgnored private var tapInstalled = false

    public init() {}

    public var isListening: Bool {
        state == .listening || state == .starting
    }

    public func listen(
        surface: String?,
        in session: PolyUISession,
        locale: Locale = .current,
        onHeard: @escaping @MainActor (PolyUIHeard) -> Void
    ) async {
        guard !isListening else { return }
        state = .starting
        transcript = ""
        guard await Self.speechPermission() == .authorized, await AVAudioApplication.requestRecordPermission() else {
            state = .unavailable(labels.denied)
            return
        }
        guard state == .starting else { return }
        guard let recognizer = SFSpeechRecognizer(locale: locale), recognizer.isAvailable else {
            state = .unavailable(labels.unavailable)
            return
        }
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.requiresOnDeviceRecognition = recognizer.supportsOnDeviceRecognition
        let activated = await switchAudio(on: true)
        guard state == .starting else {
            if activated { await switchAudio(on: false) }
            return
        }
        guard activated else {
            state = .unavailable(labels.unavailable)
            return
        }
        audioActive = true
        guard Self.feed(engine.inputNode, into: request) else {
            stopAudio()
            state = .unavailable(labels.noMicrophone)
            return
        }
        tapInstalled = true
        do {
            engine.prepare()
            try engine.start()
        } catch {
            stopAudio()
            state = .unavailable(labels.unavailable)
            return
        }
        self.request = request
        target = (surface, session)
        self.onHeard = onHeard
        task = Self.recognize(request, with: recognizer, reporting: self)
        state = .listening
    }

    public func stop() {
        if state == .starting {
            cancel()
            return
        }
        guard state == .listening else { return }
        request?.endAudio()
        stopAudio()
    }

    public func cancel() {
        guard isListening else { return }
        task?.cancel()
        finishListening()
        transcript = ""
        state = .idle
    }

    @discardableResult
    public func hear(_ spoken: String, surface: String?, in session: PolyUISession) -> PolyUIHeard? {
        let text = spoken.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return nil }
        let match = surface.flatMap { session.matchVoice(surface: $0, text: text) }
        return PolyUIHeard(transcript: text, match: match)
    }

    func partial(_ text: String?, isFinal: Bool, failed: Bool) {
        guard request != nil else { return }
        if let text { transcript = text }
        guard isFinal || failed else { return }
        let heard = transcript
        let target = self.target
        let onHeard = self.onHeard
        finishListening()
        if let target, let result = hear(heard, surface: target.surface, in: target.session) {
            state = .idle
            onHeard?(result)
        } else {
            state = failed ? .unavailable(labels.failed) : .idle
        }
    }

    private func finishListening() {
        stopAudio()
        request = nil
        task = nil
        target = nil
        onHeard = nil
    }

    private func stopAudio() {
        if engine.isRunning { engine.stop() }
        if tapInstalled {
            engine.inputNode.removeTap(onBus: 0)
            tapInstalled = false
        }
        if audioActive {
            audioActive = false
            Task { await switchAudio(on: false) }
        }
    }

    @discardableResult
    private func switchAudio(on: Bool) async -> Bool {
        let previous = audioWork
        let next = Task {
            _ = await previous?.value
            return await Self.setAudioSession(active: on)
        }
        audioWork = next
        return await next.value
    }

    @concurrent
    private nonisolated static func setAudioSession(active: Bool) async -> Bool {
        let audio = AVAudioSession.sharedInstance()
        do {
            if active {
                try audio.setCategory(.playAndRecord, mode: .measurement, options: [.duckOthers, .defaultToSpeaker])
                try audio.setActive(true)
            } else {
                try audio.setActive(false, options: .notifyOthersOnDeactivation)
            }
            return true
        } catch {
            return false
        }
    }

    private nonisolated static func speechPermission() async -> SFSpeechRecognizerAuthorizationStatus {
        let current = SFSpeechRecognizer.authorizationStatus()
        guard current == .notDetermined else { return current }
        return await withCheckedContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { continuation.resume(returning: $0) }
        }
    }

    private nonisolated static func feed(_ input: AVAudioInputNode, into request: SFSpeechAudioBufferRecognitionRequest) -> Bool {
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { return false }
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
            request.append(buffer)
        }
        return true
    }

    private nonisolated static func recognize(
        _ request: SFSpeechAudioBufferRecognitionRequest,
        with recognizer: SFSpeechRecognizer,
        reporting listener: PolyUIListener
    ) -> SFSpeechRecognitionTask {
        recognizer.recognitionTask(with: request) { [weak listener] result, error in
            let text = result?.bestTranscription.formattedString
            let isFinal = result?.isFinal ?? false
            let failed = error != nil
            Task { @MainActor [listener] in
                listener?.partial(text, isFinal: isFinal, failed: failed)
            }
        }
    }
}
