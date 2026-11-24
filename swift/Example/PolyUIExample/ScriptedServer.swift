import Foundation
import PolyUI

@MainActor
final class ScriptedServer {
    private let script: JSONValue
    private let deliver: (String) -> Void
    private let gap: Duration
    private var opened = false
    private var nonceCounter = 0

    init(script: JSONValue, instant: Bool, deliver: @escaping (String) -> Void) {
        self.script = script
        self.deliver = deliver
        gap = instant ? .zero : .milliseconds(60)
    }

    static func loadScript() -> JSONValue {
        guard let url = Bundle.main.url(forResource: "conversation", withExtension: "json"),
            let text = try? String(contentsOf: url, encoding: .utf8),
            let script = try? JSONValue.parse(text)
        else { return .null }
        return script
    }

    var question: String {
        script["question"]?.stringValue ?? "What do we owe this month?"
    }

    func receive(_ text: String) async {
        guard let message = try? JSONValue.parse(text) else { return }
        switch message["t"]?.stringValue {
        case "hello":
            await send(script["welcome"])
        case "user.say" where !opened:
            opened = true
            for chunk in script["answer"]?.arrayValue ?? [] {
                await send(["v": 1, "t": "agent.delta", "text": chunk])
            }
            await send(script["open"])
            await send(["v": 1, "t": "agent.done"])
        case "user.say":
            await say("This demo only knows one conversation. Reset it from the menu to replay it.")
        case "ui.action":
            await action(message)
        default:
            break
        }
    }

    private func action(_ message: JSONValue) async {
        switch message["action_id"]?.stringValue {
        case "act_pay" where message["confirm"] == nil:
            nonceCounter += 1
            let card = message["values"]?["/payment/card"]?.stringValue ?? "humo"
            let cardName = script["cards"]?[card]?.stringValue ?? "your card"
            await send([
                "v": 1, "t": "ui.confirm", "surface": "s1", "action_id": "act_pay",
                "nonce": .string("n_ios\(nonceCounter)"),
                "title": "Pay Hududiy elektr tarmoqlari",
                "body": .string("184 500 so'm from \(cardName)"),
                "confirm_label": "Pay 184 500 so'm",
                "expires_ms": 60000,
            ])
        case "act_pay":
            for step in script["paid"]?.arrayValue ?? [] {
                await send(step)
            }
            await say("Paid. 184 500 so'm went to Hududiy elektr tarmoqlari.")
        case "act_month":
            guard let row = message["detail"]?["row"]?.doubleValue.map({ Int($0) }) else { return }
            let months = script["months"]?.arrayValue ?? []
            guard months.indices.contains(row), let month = months[row]["month"]?.stringValue,
                let kwh = months[row]["kwh"]?.doubleValue
            else { return }
            if row > 0, let previous = months[row - 1]["kwh"]?.doubleValue, let previousName = months[row - 1]["month"]?.stringValue {
                let change = Int(((kwh - previous) / previous * 100).rounded())
                await say("\(month): \(Int(kwh)) kWh, \(change >= 0 ? "up" : "down") \(abs(change))% on \(previousName).")
            } else {
                await say("\(month): \(Int(kwh)) kWh, the first month on the chart.")
            }
        default:
            break
        }
    }

    private func say(_ text: String) async {
        await send(["v": 1, "t": "agent.delta", "text": .string(text)])
        await send(["v": 1, "t": "agent.done"])
    }

    private func send(_ message: JSONValue?) async {
        guard let message, let text = try? message.jsonText() else { return }
        if gap > .zero { try? await Task.sleep(for: gap) }
        deliver(text)
    }
}
