import Foundation
import Testing

@testable import PolyUI

@MainActor
struct WebSocketTests {
    private static let hello = #"{"v":1,"t":"hello","catalogs":["core@1"],"modalities":["screen"],"features":["stream"]}"#

    @Test func theFirstHelloDeclaresResumeWithoutAToken() throws {
        let sent = try JSONValue.parse(PolyUIWebSocket.hello(Self.hello, resume: nil))
        #expect(sent["features"]?.arrayValue == ["stream", "resume"])
        #expect(sent["resume"] == nil)
        #expect(sent["catalogs"]?.arrayValue == ["core@1"])
    }

    @Test func aReconnectCarriesTheSessionAndToken() throws {
        let resume = PolyUIWebSocket.Resume(session: "sess_1", token: "rt_2")
        let sent = try JSONValue.parse(PolyUIWebSocket.hello(Self.hello, resume: resume))
        #expect(sent["resume"]?["session"]?.stringValue == "sess_1")
        #expect(sent["resume"]?["token"]?.stringValue == "rt_2")
        let twice = try JSONValue.parse(PolyUIWebSocket.hello(PolyUIWebSocket.hello(Self.hello, resume: resume), resume: nil))
        #expect(twice["features"]?.arrayValue == ["stream", "resume"])
        #expect(twice["resume"] == nil)
    }

    @Test func textThatIsNotAnObjectIsLeftAlone() {
        #expect(PolyUIWebSocket.hello("[1]", resume: nil) == "[1]")
    }
}
