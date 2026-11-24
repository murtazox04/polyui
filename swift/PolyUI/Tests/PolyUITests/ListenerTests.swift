import Foundation
import Testing

@testable import PolyUI

@MainActor
struct ListenerTests {
    private static let cards = #"{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"card","components":[{"id":"card","type":"Card","props":{"title":"Bill"},"children":["source","pay"]},{"id":"source","type":"ChoiceGroup","props":{"label":"Pay from","value":"/payment/card","options":[{"id":"humo","label":"Humo card ending 4471"},{"id":"visa","label":"Visa card ending 0918"}]}},{"id":"pay","type":"Button","props":{"label":"Pay","on_press":{"action_id":"act_pay","confirm":true,"with":["/payment/card"]}}}],"data":{"payment":{"card":"humo"}}}"#

    @Test func aFinalTranscriptPicksAnOptionThroughTheCore() throws {
        let session = try PolyUISession(hello: PolyUIHello(locale: "en"))
        session.receive(Self.cards)
        let listener = PolyUIListener()
        let heard = try #require(listener.hear("  the visa card ", surface: "s1", in: session))
        #expect(heard.transcript == "the visa card")
        #expect(heard.match?.component == "source")
        #expect(heard.match?.option == "visa")
        #expect(session.surface("s1").node("source")?.value == "visa")
    }

    @Test func aSpokenYesConfirmsThePendingRequestAndClosesTheSheet() throws {
        var sent: [JSONValue] = []
        let session = try PolyUISession(hello: PolyUIHello(locale: "en")) { sent.append(try! JSONValue.parse($0)) }
        session.receive(Self.cards)
        session.fire(surface: "s1", component: "pay", prop: "on_press")
        session.receive(#"{"v":1,"t":"ui.confirm","surface":"s1","action_id":"act_pay","nonce":"n_7","title":"Pay"}"#)
        #expect(session.surface("s1").pendingConfirm != nil)
        let heard = try #require(PolyUIListener().hear("yes", surface: "s1", in: session))
        #expect(heard.match?.action != nil)
        #expect(sent.last?["t"] == "ui.action")
        #expect(sent.last?["confirm"] == "n_7")
        #expect(session.surface("s1").pendingConfirm == nil)
    }

    @Test func withoutASurfaceOrWordsNothingIsMatched() throws {
        var sent: [String] = []
        let session = try PolyUISession(hello: PolyUIHello(locale: "en")) { sent.append($0) }
        let listener = PolyUIListener()
        #expect(listener.hear("   ", surface: "s1", in: session) == nil)
        let heard = try #require(listener.hear("what is the weather", surface: nil, in: session))
        #expect(heard.match == nil)
        #expect(sent.isEmpty)
        #expect(listener.state == .idle)
    }

    @Test func listenerLabelsDescribeTheFallback() {
        let labels = PolyUIListenerLabels()
        #expect(labels.denied.contains("type instead"))
        #expect(labels.unavailable.contains("Type instead"))
    }
}
