import PolyUI
import SwiftUI

struct ChatView: View {
    @Bindable var model: ChatModel
    let autoScroll: String?
    @State private var draft = ""
    @FocusState private var composing: Bool
    @Environment(\.polyUITheme) private var theme

    private let page = Color.adaptive(light: 0xF3F6F8, dark: 0x111316)

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 18) {
                        ForEach(model.entries) { entry in
                            row(entry).id(entry.id)
                        }
                        if let problem = model.problem {
                            Label(problem, systemImage: "exclamationmark.triangle")
                                .font(.subheadline)
                                .foregroundStyle(theme.negative)
                        }
                        Color.clear.frame(height: 1).id("end")
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 20)
                }
                .scrollDismissesKeyboard(.interactively)
                .onChange(of: model.entries.count) {
                    guard autoScroll == "bottom" else { return }
                    proxy.scrollTo("end", anchor: .bottom)
                }
            }
            composer
        }
        .background(page.ignoresSafeArea())
    }

    private var header: some View {
        HStack(spacing: 10) {
            Grid(horizontalSpacing: 3, verticalSpacing: 3) {
                GridRow {
                    RoundedRectangle(cornerRadius: 2.5).fill(theme.series[0])
                    RoundedRectangle(cornerRadius: 2.5).fill(theme.series[1])
                }
                GridRow {
                    RoundedRectangle(cornerRadius: 2.5).fill(theme.series[1])
                    RoundedRectangle(cornerRadius: 2.5).fill(theme.series[0])
                }
            }
            .frame(width: 20, height: 20)
            .accessibilityHidden(true)
            Text("PolyUI")
                .font(.headline)
                .foregroundStyle(theme.foreground)
            Spacer()
            Menu {
                Button("Replay the demo", systemImage: "play.circle") { model.startReplay() }
                Button("Connect to polyui serve", systemImage: "network") { model.connectLive() }
            } label: {
                HStack(spacing: 6) {
                    Circle()
                        .fill(statusColor)
                        .frame(width: 8, height: 8)
                    Text(statusText)
                        .font(.subheadline)
                        .foregroundStyle(theme.muted)
                    Image(systemName: "chevron.down")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(theme.muted)
                }
                .padding(.vertical, 6)
                .padding(.horizontal, 10)
                .background(theme.raised, in: Capsule())
                .overlay(Capsule().strokeBorder(theme.border))
            }
            .accessibilityLabel("Connection: \(statusText)")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    private var statusText: String {
        switch model.mode {
        case .replay: "Replay"
        case .live:
            switch model.socket.state {
            case .open: "Connected"
            case .connecting: "Connecting"
            case .idle, .closed: "Offline"
            }
        }
    }

    private var statusColor: Color {
        switch model.mode {
        case .replay: theme.accent
        case .live: model.socket.state == .open ? theme.positive : theme.warning
        }
    }

    @ViewBuilder
    private func row(_ entry: ChatModel.Entry) -> some View {
        switch entry {
        case .user(_, let text):
            HStack {
                Spacer(minLength: 48)
                Text(text)
                    .font(.body)
                    .foregroundStyle(theme.foreground)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(theme.accent.opacity(0.14), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            }
        case .agent(_, let text):
            Text(text)
                .font(.body)
                .foregroundStyle(theme.foreground)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
        case .surface(_, let surface):
            VStack(alignment: .trailing, spacing: 8) {
                PolyUISurfaceView(session: model.session, surface: surface)
                Button {
                    model.speak(surface: surface)
                } label: {
                    Label(model.speaker.isSpeaking ? "Stop" : "Read aloud", systemImage: model.speaker.isSpeaking ? "stop.fill" : "speaker.wave.2")
                        .font(.footnote.weight(.medium))
                }
                .buttonStyle(.borderless)
                .tint(theme.muted)
            }
        }
    }

    @ViewBuilder
    private var voiceNote: some View {
        switch model.listener.state {
        case .unavailable(let message):
            Label(message, systemImage: "mic.slash")
                .foregroundStyle(theme.warning)
        case .listening:
            Label(model.listener.transcript.isEmpty ? "Listening. Tap the microphone again when you are done." : model.listener.transcript, systemImage: "waveform")
                .foregroundStyle(theme.foreground)
        case .starting:
            Label("Starting the microphone", systemImage: "mic")
                .foregroundStyle(theme.muted)
        case .idle:
            if let heard = model.heard {
                Label("Heard \u{201C}\(heard)\u{201D}", systemImage: "checkmark.circle")
                    .foregroundStyle(theme.muted)
            }
        }
    }

    private var composer: some View {
        VStack(alignment: .leading, spacing: 6) {
            voiceNote
                .font(.footnote)
                .padding(.horizontal, 20)
                .accessibilityIdentifier("voice-note")
            composerField
        }
        .padding(.top, 8)
        .padding(.bottom, 8)
        .background(page)
    }

    private var composerField: some View {
        HStack(spacing: 6) {
            TextField("Message the assistant", text: $draft, axis: .vertical)
                .lineLimit(1...4)
                .focused($composing)
                .submitLabel(.send)
                .onSubmit(send)
                .padding(.vertical, 12)
                .padding(.leading, 16)
            Button {
                model.toggleListening()
            } label: {
                Image(systemName: model.listener.isListening ? "stop.fill" : "mic")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(model.listener.isListening ? theme.accentForeground : theme.muted)
                    .frame(width: 36, height: 36)
                    .background(model.listener.isListening ? theme.negative : Color.clear, in: Circle())
            }
            .accessibilityLabel(model.listener.isListening ? "Stop listening" : "Speak")
            Button(action: send) {
                Image(systemName: "arrow.up")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(theme.accentForeground)
                    .frame(width: 36, height: 36)
                    .background(draft.isEmpty ? theme.borderStrong : theme.accent, in: Circle())
            }
            .disabled(draft.isEmpty)
            .accessibilityLabel("Send")
            .padding(.trailing, 6)
        }
        .background(theme.raised, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 24, style: .continuous).strokeBorder(theme.border))
        .padding(.horizontal, 16)
    }

    private func send() {
        let text = draft
        draft = ""
        model.ask(text)
    }
}
