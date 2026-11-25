import PolyUI
import SwiftUI

@main
struct PolyUIExampleApp: App {
    private let demo = DemoScript(defaults: .standard)
    @State private var model = ChatModel(instant: DemoScript(defaults: .standard).instant)

    var body: some Scene {
        WindowGroup {
            ChatView(model: model, autoScroll: demo.scroll)
                .tint(PolyUITheme.standard.accent)
                .task { await demo.run(on: model) }
        }
    }
}

struct DemoScript {
    let step: String?
    let scroll: String?
    let instant: Bool
    let voice: String?

    init(defaults: UserDefaults) {
        step = defaults.string(forKey: "PolyUIDemo")
        scroll = defaults.string(forKey: "PolyUIScroll")
        instant = defaults.bool(forKey: "PolyUIInstant")
        voice = defaults.string(forKey: "PolyUIVoice")
    }

    @MainActor
    func run(on model: ChatModel) async {
        guard let step else { return }
        if step == "forecast" {
            model.ask(model.forecastQuestion)
            return
        }
        model.ask(model.question)
        guard await waitFor({ model.session.surface("s1").rootID != nil }) else { return }
        let session = model.session
        if let voice {
            try? await Task.sleep(for: .milliseconds(300))
            model.hear(voice)
        }
        if step == "select" || step == "pay" || step == "paid" {
            session.fire(surface: "s1", component: "usage", prop: "on_select", detail: ["row": 1])
        }
        if step == "pay" || step == "paid" {
            session.setValue(surface: "s1", path: "/payment/phone", value: "+998 90 123 45 67")
            try? await Task.sleep(for: .milliseconds(300))
            session.fire(surface: "s1", component: "pay", prop: "on_press")
        }
        if step == "paid" {
            guard await waitFor({ session.surface("s1").pendingConfirm != nil }) else { return }
            try? await Task.sleep(for: .milliseconds(600))
            if let request = session.surface("s1").pendingConfirm {
                session.confirm(surface: "s1", request: request)
            }
        }
    }

    @MainActor
    private func waitFor(_ condition: () -> Bool) async -> Bool {
        for _ in 0..<100 {
            if condition() { return true }
            try? await Task.sleep(for: .milliseconds(50))
        }
        return condition()
    }
}
