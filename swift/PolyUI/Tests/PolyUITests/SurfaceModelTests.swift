import Foundation
import Testing

@testable import PolyUI

@MainActor
struct SurfaceModelTests {
    private func create(_ id: String, _ type: String, _ props: [String: JSONValue] = [:], value: JSONValue? = nil, label: String = "") -> RenderOp {
        .create(ComponentPayload(id: id, type: type, props: props, value: value, a11y: A11y(role: "text", label: label)))
    }

    private func update(_ id: String, _ props: [String: JSONValue], value: JSONValue? = nil) -> RenderOp {
        .update(ComponentPayload(id: id, props: props, value: value))
    }

    @Test func createsChildrenAndRoot() {
        let surface = PolyUISurfaceModel(id: "s1")
        surface.apply(RenderBatch(surface: "s1", ops: [
            create("card", "Card", ["title": "Bills"]),
            create("total", "Stat", ["label": "Total", "value": "10"]),
            .children(id: "card", children: ["total", "card"]),
            .root(id: "card"),
        ], fallback: "Bills"))
        #expect(surface.rootID == "card")
        #expect(surface.root?.children == ["total"])
        #expect(surface.node("total")?.string("value") == "10")
        #expect(surface.fallback == "Bills")
    }

    @Test func updatesKeepNodeIdentity() throws {
        let surface = PolyUISurfaceModel(id: "s1")
        surface.apply(RenderBatch(surface: "s1", ops: [create("phone", "Input", ["label": "Phone"], value: .null), .root(id: "phone")]))
        let before = try #require(surface.node("phone"))
        surface.apply(RenderBatch(surface: "s1", ops: [update("phone", ["label": "Phone"], value: "+998")]))
        let after = try #require(surface.node("phone"))
        #expect(before === after)
        #expect(after.value == "+998")
        #expect(after.revision == 1)
    }

    @Test func createWithTheSameTypeIsAnUpdate() throws {
        let surface = PolyUISurfaceModel(id: "s1")
        surface.apply(RenderBatch(surface: "s1", ops: [create("a", "Text", ["text": "one"])]))
        let first = try #require(surface.node("a"))
        surface.apply(RenderBatch(surface: "s1", ops: [create("a", "Text", ["text": "two"])]))
        #expect(surface.node("a") === first)
        #expect(first.string("text") == "two")
    }

    @Test func replacedTypeGetsANewNodeAndSurvivesItsRemove() throws {
        let surface = PolyUISurfaceModel(id: "s1")
        surface.apply(RenderBatch(surface: "s1", ops: [
            create("box", "Card"), create("a", "Badge", ["text": "new"]),
            .children(id: "box", children: ["a"]), .root(id: "box"),
        ]))
        let old = try #require(surface.node("a"))
        surface.apply(RenderBatch(surface: "s1", ops: [
            .remove(id: "a"),
            create("a", "Text", ["text": "plain", "style": "muted"]),
            .children(id: "box", children: ["a"]),
        ]))
        let replaced = try #require(surface.node("a"))
        #expect(replaced !== old)
        #expect(replaced.type == "Text")
        #expect(surface.root?.children == ["a"])
    }

    @Test func removeDropsRootAndHighlight() {
        let surface = PolyUISurfaceModel(id: "s1")
        surface.apply(RenderBatch(surface: "s1", ops: [create("a", "Text"), .root(id: "a"), .highlight(id: "a")]))
        #expect(surface.highlightedID == "a")
        #expect(surface.node("a")?.isHighlighted == true)
        surface.apply(RenderBatch(surface: "s1", ops: [.remove(id: "a")]))
        #expect(surface.rootID == nil)
        #expect(surface.highlightedID == nil)
        #expect(surface.nodes.isEmpty)
    }

    @Test func highlightMovesBetweenNodes() {
        let surface = PolyUISurfaceModel(id: "s1")
        surface.apply(RenderBatch(surface: "s1", ops: [create("a", "Text"), create("b", "Text"), .highlight(id: "a")]))
        surface.apply(RenderBatch(surface: "s1", ops: [.highlight(id: "b")]))
        #expect(surface.node("a")?.isHighlighted == false)
        #expect(surface.node("b")?.isHighlighted == true)
        surface.apply(RenderBatch(surface: "s1", ops: [.highlight(id: "missing")]))
        #expect(surface.highlightedID == nil)
        #expect(surface.node("b")?.isHighlighted == false)
    }

    @Test func confirmAndCloseManageTheSheet() {
        let surface = PolyUISurfaceModel(id: "s1")
        let request = ConfirmRequest(actionID: "act", nonce: "n1", title: "Pay", body: "10 so'm", confirmLabel: "Pay", expiresMilliseconds: 60000)
        surface.apply(RenderBatch(surface: "s1", ops: [create("a", "Text"), .root(id: "a"), .confirm(request)]))
        #expect(surface.pendingConfirm == request)
        surface.apply(RenderBatch(surface: "s1", ops: [.close]))
        #expect(surface.pendingConfirm == nil)
        #expect(surface.isClosed)
        #expect(surface.rootID == nil)
        surface.apply(RenderBatch(surface: "s1", ops: [create("b", "Text"), .root(id: "b")]))
        #expect(!surface.isClosed)
    }

    @Test func structureRevisionOnlyMovesForStructure() {
        let surface = PolyUISurfaceModel(id: "s1")
        surface.apply(RenderBatch(surface: "s1", ops: [create("a", "Text"), .root(id: "a")]))
        let revision = surface.structureRevision
        surface.apply(RenderBatch(surface: "s1", ops: [update("a", ["text": "x"])]))
        #expect(surface.structureRevision == revision)
        surface.apply(RenderBatch(surface: "s1", ops: [create("b", "Text")]))
        #expect(surface.structureRevision == revision + 1)
    }

    @Test func decodingSkipsUnknownOpsWithoutFailing() throws {
        let batch = try PolyUICore.decode(RenderBatch.self, from: #"{"surface":"s1","ops":[{"op":"teleport"},{"op":"highlight","id":null},{"op":"close"}]}"#)
        #expect(batch.ops == [.unknown("teleport"), .highlight(id: nil), .close])
        let surface = PolyUISurfaceModel(id: "s1")
        surface.apply(batch)
        #expect(surface.isClosed)
    }
}

@MainActor
struct SessionTests {
    private static let hello = PolyUIHello(locale: "en")

    @Test func confirmFlowRunsThroughTheSession() throws {
        var sent: [JSONValue] = []
        let session = try PolyUISession(hello: Self.hello) { sent.append(try! JSONValue.parse($0)) }
        session.receive(#"{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"pay","components":[{"id":"pay","type":"Button","props":{"label":"Pay","on_press":{"action_id":"act_pay","confirm":true}}}],"data":{}}"#)
        let surface = session.surface("s1")
        #expect(surface.root?.type == "Button")
        session.fire(surface: "s1", component: "pay", prop: "on_press")
        #expect(sent.last?["t"] == "ui.action")
        #expect(sent.last?["action_id"] == "act_pay")
        session.receive(#"{"v":1,"t":"ui.confirm","surface":"s1","action_id":"act_pay","nonce":"n_9","title":"Pay Hududgaz","expires_ms":60000}"#)
        let request = try #require(surface.pendingConfirm)
        #expect(request.title == "Pay Hududgaz")
        session.confirm(surface: "s1", request: request)
        #expect(surface.pendingConfirm == nil)
        #expect(sent.last?["confirm"] == "n_9")
    }

    @Test func inputAndChoiceWriteThroughTheCore() throws {
        var sent: [JSONValue] = []
        let session = try PolyUISession(hello: Self.hello) { sent.append(try! JSONValue.parse($0)) }
        session.receive(#"{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"card","components":[{"id":"card","type":"Card","props":{},"children":["pick","echo","go"]},{"id":"pick","type":"ChoiceGroup","props":{"label":"Pick","value":"/pick","options":[{"id":"a","label":"A"},{"id":"b","label":"B"}]}},{"id":"echo","type":"Text","props":{"text":{"$bind":"/pick"}}},{"id":"go","type":"Button","props":{"label":"Go","on_press":{"action_id":"act_go","confirm":false,"with":["/pick"]}}}],"data":{}}"#)
        let surface = session.surface("s1")
        #expect(surface.node("pick")?.value == .null)
        session.setValue(surface: "s1", path: "/pick", value: "b")
        #expect(surface.node("pick")?.value == "b")
        #expect(surface.node("echo")?.string("text") == "b")
        session.fire(surface: "s1", component: "go", prop: "on_press")
        #expect(sent.last?["values"]?["/pick"] == "b")
    }

    @Test func voiceMatchSelectsAndSpeaksInOrder() throws {
        let session = try PolyUISession(hello: Self.hello)
        session.receive(#"{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"pick","components":[{"id":"pick","type":"ChoiceGroup","props":{"label":"Pick one","value":"/pick","options":[{"id":"gas","label":"Gas"},{"id":"power","label":"Power"}]}}],"data":{}}"#)
        let said = session.say(surface: "s1")
        #expect(said.first?.id == "pick")
        let found = try #require(session.matchVoice(surface: "s1", text: "the second one"))
        #expect(found.option == "power")
        #expect(session.surface("s1").node("pick")?.value == "power")
    }

    @Test func badMessagesAreReportedNotThrown() throws {
        var sent: [JSONValue] = []
        let session = try PolyUISession(hello: Self.hello) { sent.append(try! JSONValue.parse($0)) }
        session.receive("{oops")
        #expect(sent.last?["t"] == "ui.error")
        #expect(sent.last?["reason"] == "bad_message")
        #expect(session.surfaceIDs.isEmpty)
    }

    @Test func gapsAskForAResync() throws {
        var sent: [JSONValue] = []
        let session = try PolyUISession(hello: Self.hello) { sent.append(try! JSONValue.parse($0)) }
        session.receive(#"{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"a","components":[{"id":"a","type":"Text","props":{"text":"hi"}}],"data":{}}"#)
        session.receive(#"{"v":1,"t":"ui.patch","surface":"s1","seq":5,"upsert":[]}"#)
        #expect(sent.contains { $0["t"] == "ui.resync" })
    }
}
