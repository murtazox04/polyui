import Foundation
import SwiftUI
import Testing

@testable import PolyUI

@MainActor
final class BuiltBadges {
    private(set) var texts: [String] = []

    func record(_ text: String) {
        texts.append(text)
    }
}

@MainActor
struct ExtensionCatalogTests {
    private static let hello = PolyUIHello(catalogs: ["core@1", "acme@1"], modalities: ["screen"], locale: "en")

    private static let acme: JSONValue = [
        "id": "acme",
        "version": 1,
        "types": [
            "Badge2": [
                "description": "A status badge.",
                "role": "status",
                "props": ["text": ["kind": "string", "required": true]],
                "alt": "{text}",
            ]
        ],
    ]

    private static let open = #"{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"card","components":[{"id":"card","type":"Card","props":{"title":"Bill"},"children":["badge"]},{"id":"badge","type":"acme:Badge2","props":{"text":"Paid"}}],"data":{}}"#

    @Test func withoutItsCatalogAnExtensionTypeIsText() throws {
        let session = try PolyUISession(hello: Self.hello)
        session.receive(Self.open)
        #expect(session.surface("s1").node("badge")?.type == "Text")
    }

    @Test func extensionTypesRenderThroughAHostView() throws {
        var sent: [JSONValue] = []
        let session = try PolyUISession(hello: Self.hello, catalogs: [Self.acme]) { sent.append(try! JSONValue.parse($0)) }
        session.receive(Self.open)
        let badge = try #require(session.surface("s1").node("badge"))
        #expect(badge.type == "acme:Badge2")
        #expect(badge.string("text") == "Paid")
        #expect(!sent.contains { $0["reason"] == "unknown_component" })

        let built = BuiltBadges()
        let surface = PolyUISurfaceView(session: session, surface: "s1")
            .polyUIView(for: "acme:Badge2") { context in
                let text = context.node.string("text") ?? ""
                let _ = built.record(text)
                Text(text)
            }
            .frame(width: 320, height: 240)
        let renderer = ImageRenderer(content: surface)
        #expect(renderer.uiImage != nil)
        #expect(built.texts.contains("Paid"))
    }

    @Test func aRejectedCatalogIsAClearError() {
        let reserved: JSONValue = ["id": "core", "version": 1, "types": [:]]
        #expect {
            try PolyUICore(hello: Self.hello, catalogs: [reserved])
        } throws: { error in
            String(describing: error).contains("an extension catalog was rejected: [0].id")
        }
    }
}
