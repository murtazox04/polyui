import Foundation
import Testing

@testable import PolyUI

enum Fixtures {
    static let folder = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .appendingPathComponent("../../../../tests/conformance/client")
        .standardizedFileURL

    static var clientFiles: [String] {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []
        return names.filter { $0.hasSuffix(".json") }.sorted()
    }

    static func load(_ name: String) throws -> JSONValue {
        try JSONValue.parse(String(contentsOf: folder.appendingPathComponent(name), encoding: .utf8))
    }
}

@MainActor
struct ConformanceTests {
    @Test func fixturesAreReachable() {
        #expect(Fixtures.clientFiles.count >= 10)
    }

    @Test(arguments: Fixtures.clientFiles)
    func replaysThroughTheRustCore(_ name: String) throws {
        let fixture = try Fixtures.load(name)
        let hello = try #require(fixture["hello"])
        let core = try PolyUICore(helloJSON: try hello.jsonText())
        let steps = try #require(fixture["steps"]?.arrayValue)
        for (index, step) in steps.enumerated() {
            let expected = try #require(step["expect"])
            let raw = try run(step, on: core)
            #expect(try JSONValue.parse(raw) == expected, "\(name) step \(index + 1): \(raw)")
            try checkDecoded(step, raw: raw, expected: expected, label: "\(name) step \(index + 1)")
        }
    }

    private func run(_ step: JSONValue, on core: PolyUICore) throws -> String {
        func field(_ call: JSONValue, _ key: String) -> String { call[key]?.stringValue ?? "" }
        if let message = step["receive"] {
            return try core.receiveJSON(try message.jsonText())
        }
        if let raw = step["receive_text"]?.stringValue {
            return try core.receiveJSON(raw)
        }
        if let call = step["set_value"] {
            let value = try (call["value"] ?? .null).jsonText()
            return try core.setValueJSON(surface: field(call, "surface"), path: field(call, "path"), valueJSON: value)
        }
        if let call = step["fire"] {
            return try core.fireJSON(
                surface: field(call, "surface"),
                component: field(call, "component"),
                prop: field(call, "prop"),
                detailJSON: try call["detail"]?.jsonText()
            )
        }
        if let call = step["confirm"] {
            return try core.confirmJSON(surface: field(call, "surface"), actionID: field(call, "action_id"), nonce: field(call, "nonce"))
        }
        if let surface = step["say"]?.stringValue {
            return try core.sayJSON(surface: surface)
        }
        if let call = step["match_voice"] {
            return try core.matchVoiceJSON(surface: field(call, "surface"), text: field(call, "text"))
        }
        Issue.record("unknown step \(step)")
        return "null"
    }

    private func checkDecoded(_ step: JSONValue, raw: String, expected: JSONValue, label: String) throws {
        let expectedText = try expected.jsonText()
        if step["receive"] != nil || step["receive_text"] != nil || step["set_value"] != nil {
            let actual = try PolyUICore.decode(RenderBatch.self, from: raw)
            let wanted = try PolyUICore.decode(RenderBatch.self, from: expectedText)
            #expect(actual == wanted, "\(label)")
            #expect(!actual.ops.contains { if case .unknown = $0 { true } else { false } }, "\(label) has an unknown op")
        } else if step["say"] != nil {
            #expect(try PolyUICore.decode([Utterance].self, from: raw) == PolyUICore.decode([Utterance].self, from: expectedText), "\(label)")
        } else if step["match_voice"] != nil, expected != .null {
            let actual = try PolyUICore.decode(VoiceMatch.self, from: raw)
            #expect(actual == (try PolyUICore.decode(VoiceMatch.self, from: expectedText)), "\(label)")
        }
    }

    @Test func decodedOpsKeepEveryField() throws {
        let core = try PolyUICore(helloJSON: try Fixtures.load("confirm_flow.json")["hello"]!.jsonText())
        let fixture = try Fixtures.load("confirm_flow.json")
        let steps = try #require(fixture["steps"]?.arrayValue)
        var confirms: [ConfirmRequest] = []
        for step in steps {
            if let message = step["receive"] {
                let batch = try core.receive(try message.jsonText())
                for op in batch.ops {
                    if case .confirm(let request) = op { confirms.append(request) }
                }
            }
        }
        let request = try #require(confirms.first)
        #expect(!request.actionID.isEmpty)
        #expect(!request.nonce.isEmpty)
        #expect(!request.title.isEmpty)
    }

    @Test func invalidHelloThrows() {
        #expect(throws: PolyUIError.self) { try PolyUICore(helloJSON: "{\"v\":1}") }
    }

    @Test func invalidValueJSONThrowsAndKeepsTheClient() throws {
        let core = try PolyUICore()
        #expect(throws: PolyUIError.self) { try core.setValueJSON(surface: "s1", path: "/a", valueJSON: "{bad") }
        #expect(try core.say(surface: "s1") == [])
    }

    @Test func nullResultsBecomeNil() throws {
        let core = try PolyUICore()
        #expect(try core.fire(surface: "s1", component: "x", prop: "on_press") == nil)
        #expect(try core.confirm(surface: "s1", actionID: "a", nonce: "n") == nil)
        #expect(try core.matchVoice(surface: "s1", text: "first") == nil)
    }
}
