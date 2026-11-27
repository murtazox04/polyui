import Foundation
import Testing
import UIKit
import WebKit

@testable import PolyUI

struct EmbedNonceTests {
    @Test func noncesAreLongRandomAndAlphanumeric() throws {
        let made = try (0..<200).map { _ in try #require(EmbedNonce.make()) }
        for nonce in made {
            #expect(nonce.count == 32)
            #expect(nonce.unicodeScalars.allSatisfy { $0.isASCII && CharacterSet.alphanumerics.contains($0) })
        }
        #expect(Set(made).count == made.count)
    }

    @Test func matchingNeedsTheExactNonce() {
        #expect(EmbedNonce.matches("abcDEF123", "abcDEF123"))
        #expect(!EmbedNonce.matches("abcDEF124", "abcDEF123"))
        #expect(!EmbedNonce.matches("abcDEF12", "abcDEF123"))
        #expect(!EmbedNonce.matches("", ""))
    }
}

struct EmbedEventTests {
    private let nonce = "Nonce0123456789abcdefABCDEF012345"

    private func body(_ fields: JSONValue) -> String {
        (try? fields.jsonText()) ?? ""
    }

    @Test func acceptsEveryKindFromTheMainFrameWithTheNonce() {
        let read = { (fields: JSONValue) in EmbedEvent.read(body(fields), isMainFrame: true, nonce: nonce) }
        #expect(read(["kind": "ready", "nonce": .string(nonce)]) == .ready)
        #expect(read(["kind": "height", "value": 212, "nonce": .string(nonce)]) == .height(212))
        #expect(read(["kind": "message", "text": "Hi", "nonce": .string(nonce)]) == .message("Hi"))
        #expect(read(["kind": "unavailable", "nonce": .string(nonce)]) == .unavailable)
        #expect(read(["kind": "open-link", "url": "https://example.com/a?b=1", "nonce": .string(nonce)]) == .openLink(URL(string: "https://example.com/a?b=1")!))
        #expect(
            read(["kind": "app-call", "id": "c0ffee-1", "method": "tools/call", "params": ["name": "quote"], "nonce": .string(nonce)])
                == .appCall(id: "c0ffee-1", method: "tools/call", params: ["name": "quote"]))
    }

    @Test func refusesAppCallsThatCannotBeForwarded() {
        let read = { (fields: JSONValue) in EmbedEvent.read(body(fields), isMainFrame: true, nonce: nonce) }
        #expect(read(["kind": "app-call", "id": "", "method": "tools/call", "params": [:], "nonce": .string(nonce)]) == nil)
        #expect(read(["kind": "app-call", "id": .string(String(repeating: "a", count: 65)), "method": "tools/call", "params": [:], "nonce": .string(nonce)]) == nil)
        #expect(read(["kind": "app-call", "id": "c-1", "method": "ui/update-model-context", "params": [:], "nonce": .string(nonce)]) == nil)
        #expect(read(["kind": "app-call", "id": "c-1", "method": "tools/call", "params": "x", "nonce": .string(nonce)]) == nil)
        #expect(read(["kind": "app-call", "id": "c-1", "method": "tools/call", "nonce": .string(nonce)]) == nil)
    }

    @Test func ignoresSubframesEvenWithTheRightNonce() {
        let text = body(["kind": "message", "text": "forged", "nonce": .string(nonce)])
        #expect(EmbedEvent.read(text, isMainFrame: false, nonce: nonce) == nil)
    }

    @Test(arguments: [
        #"{"kind":"message","text":"Hi"}"#,
        #"{"kind":"message","text":"Hi","nonce":"wrong"}"#,
        #"{"kind":"message","text":"Hi","nonce":"Nonce0123456789abcdefABCDEF012346"}"#,
        #"{"kind":"message","text":"","nonce":"Nonce0123456789abcdefABCDEF012345"}"#,
        #"{"kind":"height","value":"tall","nonce":"Nonce0123456789abcdefABCDEF012345"}"#,
        #"{"kind":"open-link","url":"http://example.com/","nonce":"Nonce0123456789abcdefABCDEF012345"}"#,
        #"{"kind":"open-link","url":"https://user:pw@example.com/","nonce":"Nonce0123456789abcdefABCDEF012345"}"#,
        #"{"kind":"open-link","url":"javascript:alert(1)","nonce":"Nonce0123456789abcdefABCDEF012345"}"#,
        #"{"kind":"navigate","nonce":"Nonce0123456789abcdefABCDEF012345"}"#,
        #"["ready"]"#,
        "not json",
    ])
    func refusesAnythingElse(_ text: String) {
        #expect(EmbedEvent.read(text, isMainFrame: true, nonce: nonce) == nil)
    }

    @Test func refusesBodiesThatAreNotText() {
        let dictionary: [String: String] = ["kind": "ready", "nonce": nonce]
        #expect(EmbedEvent.read(dictionary, isMainFrame: true, nonce: nonce) == nil)
        #expect(EmbedEvent.read(String(repeating: " ", count: EmbedEvent.longestBody + 1), isMainFrame: true, nonce: nonce) == nil)
    }
}

struct EmbedSizeTests {
    @Test func hintsStartAtTheirHeight() {
        #expect(EmbedSize(prop: "small").startHeight == 160)
        #expect(EmbedSize(prop: "medium").startHeight == 320)
        #expect(EmbedSize(prop: "large").startHeight == 480)
        #expect(EmbedSize(prop: "huge") == .medium)
        #expect(EmbedSize(prop: nil) == .medium)
    }

    @Test func heightsAreClampedToTheHint() {
        #expect(EmbedSize.small.clamp(10) == 80)
        #expect(EmbedSize.small.clamp(5000) == 320)
        #expect(EmbedSize.medium.clamp(5000) == 640)
        #expect(EmbedSize.large.clamp(5000) == 960)
        #expect(EmbedSize.medium.clamp(200.2) == 201)
        #expect(EmbedSize.large.clamp(.nan) == 480)
        #expect(EmbedSize.small.clamp(-.infinity) == 160)
    }
}

struct EmbedNavigationTests {
    @Test func theMainFrameLoadsOnlyWhenThePageIsExpected() {
        #expect(EmbedNavigation.allows(targetIsMainFrame: true, url: URL(string: "about:blank"), expectingPageLoad: true))
        #expect(!EmbedNavigation.allows(targetIsMainFrame: true, url: URL(string: "about:blank"), expectingPageLoad: false))
        #expect(!EmbedNavigation.allows(targetIsMainFrame: true, url: URL(string: "https://example.com/"), expectingPageLoad: false))
    }

    @Test(arguments: ["about:srcdoc", "about:blank", "https://example.com/app"])
    func subframesMayUseSrcdocBlankAndHTTPS(_ raw: String) {
        #expect(EmbedNavigation.allows(targetIsMainFrame: false, url: URL(string: raw), expectingPageLoad: false))
    }

    @Test(arguments: ["http://example.com/", "file:///etc/hosts", "javascript:alert(1)", "data:text/html,hi", "about:config", "polyui://x"])
    func subframesMayNotGoAnywhereElse(_ raw: String) {
        #expect(!EmbedNavigation.allows(targetIsMainFrame: false, url: URL(string: raw), expectingPageLoad: true))
    }

    @Test func newWindowsAreRefused() {
        #expect(!EmbedNavigation.allows(targetIsMainFrame: nil, url: URL(string: "https://example.com/"), expectingPageLoad: true))
    }
}

@MainActor
struct AppCallSessionTests {
    private func session() throws -> (PolyUISession, () -> [JSONValue]) {
        var sent: [String] = []
        let session = try PolyUISession(
            hello: PolyUIHello(catalogs: ["core@1", "embed@1"], features: ["app_calls"]),
            send: { sent.append($0) }
        )
        return (session, { sent.compactMap { try? JSONValue.parse($0) } })
    }

    @Test func theWelcomeDecidesWhetherAppCallsAreOn() throws {
        let (session, _) = try session()
        #expect(!session.appCalls)
        session.receive(#"{"v":1,"t":"welcome","session":"sess_1","catalogs":["core@1","embed@1"],"features":["resume","app_calls"]}"#)
        #expect(session.appCalls)
        session.receive(#"{"v":1,"t":"welcome","session":"sess_2","catalogs":["core@1","embed@1"]}"#)
        #expect(!session.appCalls)
    }

    @Test func callsGoToTheServerAndAnswersComeBackToTheirEmbed() throws {
        let (session, messages) = try session()
        var first: [JSONValue] = []
        var second: [JSONValue] = []
        session.callApp(surface: "s1", embed: "calc", method: "tools/call", params: ["name": "quote"]) { first.append($0) }
        session.callApp(surface: "s1", embed: "calc", method: "resources/read", params: ["uri": "ui://a/b"]) { second.append($0) }
        let calls = messages().filter { $0["t"]?.stringValue == "app.call" }
        #expect(calls.count == 2)
        #expect(calls[0]["id"] == "a1" && calls[0]["embed"] == "calc" && calls[0]["method"] == "tools/call")
        #expect(calls[0]["params"]?["name"] == "quote")
        #expect(calls[1]["id"] == "a2")

        session.receive(#"{"v":1,"t":"app.result","surface":"s1","embed":"other","id":"a1","result":{"content":[]}}"#)
        #expect(first.isEmpty)
        session.receive(#"{"v":1,"t":"app.result","surface":"s1","embed":"calc","id":"a1","result":{"content":[{"type":"text","text":"Rain"}]}}"#)
        #expect(first == [["result": ["content": [["type": "text", "text": "Rain"]]]]])
        session.receive(#"{"v":1,"t":"app.result","surface":"s1","embed":"calc","id":"a2","error":{"code":-32000,"message":"unknown resource"}}"#)
        #expect(second == [["error": ["code": -32000, "message": "unknown resource"]]])
        session.receive(#"{"v":1,"t":"app.result","surface":"s1","embed":"calc","id":"a1","result":{}}"#)
        #expect(first.count == 1)
        #expect(session.lastError == nil)
        #expect(!messages().contains { $0["t"]?.stringValue == "ui.error" })
    }

    @Test func withdrawnCallsAreNeverAnswered() throws {
        let (session, _) = try session()
        var answers: [JSONValue] = []
        session.callApp(surface: "s1", embed: "calc", method: "tools/call", params: [:]) { answers.append($0) }
        session.withdrawAppCalls(surface: "s1", embed: "calc")
        #expect(session.openAppCalls == 0)
        session.receive(#"{"v":1,"t":"app.result","surface":"s1","embed":"calc","id":"a1","result":{}}"#)
        #expect(answers.isEmpty)
    }

    @Test func aCallTheServerNeverAnswersTimesOut() async throws {
        let (session, _) = try session()
        session.appCallTimeout = .milliseconds(50)
        var answers: [JSONValue] = []
        session.callApp(surface: "s1", embed: "calc", method: "tools/call", params: [:]) { answers.append($0) }
        for _ in 0..<40 where answers.isEmpty {
            try await Task.sleep(for: .milliseconds(25))
        }
        #expect(answers == [["error": ["code": -32000, "message": "The app request timed out."]]])
        #expect(session.openAppCalls == 0)
    }
}

struct EmbedPageTests {
    @Test func theCoreBuildsAPageWithTheNonce() throws {
        let nonce = try #require(EmbedNonce.make())
        let page = try PolyUICore.embedPage(propsJSON: #"{"resource":"ui://a/b","title":"T"}"#, nonce: nonce)
        #expect(page.contains(nonce))
        #expect(page.hasPrefix("<!doctype html>"))
    }

    @Test func badInputIsAnError() {
        #expect(throws: PolyUIError.self) { try PolyUICore.embedPage(propsJSON: "{}", nonce: "short") }
        #expect(throws: PolyUIError.self) { try PolyUICore.embedPage(propsJSON: "[1]", nonce: "Nonce0123456789abcdef") }
        #expect(throws: PolyUIError.self) { try PolyUICore.embedPage(propsJSON: "{oops", nonce: "Nonce0123456789abcdef") }
    }
}

@MainActor
struct EmbedSessionTests {
    static let app = """
        <!doctype html><html><body><p>app</p><script>
        const send = (message) => window.parent.postMessage(Object.assign({ jsonrpc: "2.0" }, message), "*");
        window.addEventListener("message", (event) => {
          const data = event.data;
          if (data && data.id === 1 && data.result) {
            send({ method: "ui/notifications/initialized", params: {} });
            send({ method: "ui/notifications/size-changed", params: { height: 2000 } });
          }
          if (data && data.method === "ui/notifications/tool-result") {
            if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.polyui) {
              window.webkit.messageHandlers.polyui.postMessage(JSON.stringify({ kind: "message", text: "forged", nonce: "guess" }));
            }
            send({ id: 2, method: "ui/message", params: { role: "user", content: [{ type: "text", text: "From the app" }] } });
            send({ id: 3, method: "ui/open-link", params: { url: "https://example.com/more" } });
          }
          if (data && data.id === 3) document.title = data.result ? "accepted" : "refused";
        });
        send({ id: 1, method: "ui/initialize", params: { protocolVersion: "2026-01-26", appInfo: { name: "t", version: "1" }, appCapabilities: {} } });
        </script></body></html>
        """

    static func props(withMessage: Bool = true) -> [String: JSONValue] {
        var props: [String: JSONValue] = [
            "resource": "ui://test/app",
            "title": "Test app",
            "height": "small",
            "app": [
                "resource": ["uri": "ui://test/app", "mimeType": "text/html;profile=mcp-app", "text": .string(app)],
                "tool_input": ["city": "Tashkent"],
                "tool_result": ["content": [["type": "text", "text": "ok"]], "structuredContent": [:]],
            ],
        ]
        if withMessage { props["on_message"] = ["action_id": "act_ask", "confirm": false] }
        return props
    }

    private func waitFor(_ condition: () -> Bool) async -> Bool {
        for _ in 0..<200 {
            if condition() { return true }
            try? await Task.sleep(for: .milliseconds(50))
        }
        return condition()
    }

    @Test func theCoreKeepsTheAppForAClientThatDeclaresEmbed() throws {
        var sent: [JSONValue] = []
        let session = try PolyUISession(hello: PolyUIHello(catalogs: ["core@1", "chart@1", "embed@1"], locale: "en")) {
            sent.append(try! JSONValue.parse($0))
        }
        let open: JSONValue = [
            "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "app",
            "components": [["id": "app", "type": "embed:Embed", "props": .object(Self.props())]],
            "data": [:],
        ]
        session.receive(open)
        let node = try #require(session.surface("s1").node("app"))
        #expect(node.type == "embed:Embed")
        #expect(node.props["app"]?["resource"]?["text"]?.stringValue == Self.app)
        session.fire(surface: "s1", component: "app", prop: "on_message", detail: ["text": "From the app"])
        #expect(sent.last?["t"] == "ui.action")
        #expect(sent.last?["action_id"] == "act_ask")
        #expect(sent.last?["detail"]?["text"] == "From the app")
    }

    @Test func oneLinkQuestionAtATime() throws {
        let session = try PolyUISession(hello: PolyUIHello(locale: "en"))
        var opened: [URL] = []
        var answers: [String] = []
        session.openLink = { opened.append($0) }
        let url = try #require(URL(string: "https://example.com/more"))
        session.askToOpen(url, surface: "s1", component: "a") { answers.append("first \($0)") }
        session.askToOpen(url, surface: "s2", component: "b") { answers.append("second \($0)") }
        #expect(answers == ["second false"])
        let link = try #require(session.surface("s1").pendingLink)
        #expect(session.surface("s1").presentedSheet?.id == "link:\(link.id.uuidString)")
        session.answerLink(surface: "s1", id: link.id, accepted: true)
        session.answerLink(surface: "s1", id: link.id, accepted: false)
        #expect(opened == [url])
        #expect(answers == ["second false", "first true"])
        #expect(session.surface("s1").pendingLink == nil)
    }

    @Test func refusedWithdrawnAndUnsafeLinksNeverOpen() throws {
        let session = try PolyUISession(hello: PolyUIHello(locale: "en"))
        var opened: [URL] = []
        var answers: [Bool] = []
        session.openLink = { opened.append($0) }
        session.askToOpen(URL(string: "http://example.com/")!, surface: "s1", component: "a") { answers.append($0) }
        session.askToOpen(URL(string: "https://example.com/")!, surface: "s1", component: "a") { answers.append($0) }
        let link = try #require(session.surface("s1").pendingLink)
        session.answerLink(surface: "s1", id: link.id, accepted: false)
        session.askToOpen(URL(string: "https://example.com/")!, surface: "s1", component: "a") { answers.append($0) }
        session.withdrawLink(surface: "s1", component: "a")
        #expect(opened.isEmpty)
        #expect(answers == [false, false, false])
    }

    @Test func aServerConfirmBlocksALinkQuestion() throws {
        let session = try PolyUISession(hello: PolyUIHello(locale: "en"))
        session.receive(#"{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"pay","components":[{"id":"pay","type":"Button","props":{"label":"Pay","on_press":{"action_id":"act_pay","confirm":true}}}],"data":{}}"#)
        session.receive(#"{"v":1,"t":"ui.confirm","surface":"s1","action_id":"act_pay","nonce":"n_1","title":"Pay"}"#)
        var answers: [Bool] = []
        session.askToOpen(URL(string: "https://example.com/")!, surface: "s1", component: "a") { answers.append($0) }
        #expect(answers == [false])
        #expect(session.surface("s1").presentedSheet?.id == "confirm:act_pay:n_1")
    }

    static let themeApp = """
        <!doctype html><html><body><script>
        const send = (message) => window.parent.postMessage(Object.assign({ jsonrpc: "2.0" }, message), "*");
        let said = 0;
        const say = (theme) => send({ id: 10 + said++, method: "ui/message", params: { role: "user", content: [{ type: "text", text: "theme " + theme }] } });
        window.addEventListener("message", (event) => {
          const data = event.data;
          if (data && data.id === 1 && data.result) {
            send({ method: "ui/notifications/initialized", params: {} });
            say(data.result.hostContext.theme);
          }
          if (data && data.method === "ui/notifications/host-context-changed") say(data.params.theme);
        });
        send({ id: 1, method: "ui/initialize", params: { protocolVersion: "2026-01-26", appInfo: { name: "t", version: "1" }, appCapabilities: {} } });
        </script></body></html>
        """

    @Test func theAppHearsTheHostThemeAndItsChanges() async throws {
        let controller = EmbedPageController()
        var events: [EmbedEvent] = []
        controller.onEvent = { events.append($0) }
        let webView = controller.makeWebView()
        webView.frame = CGRect(x: 0, y: 0, width: 320, height: 320)
        let window = UIWindow(frame: webView.frame)
        window.addSubview(webView)
        window.isHidden = false
        window.overrideUserInterfaceStyle = .dark
        var props = Self.props()
        props["app"] = [
            "resource": ["uri": "ui://test/app", "mimeType": "text/html;profile=mcp-app", "text": .string(Self.themeApp)],
            "tool_input": [:],
        ]
        controller.show(try JSONValue.object(props).jsonText())
        #expect(await waitFor { events.contains(.message("theme dark")) })
        try await Task.sleep(for: .milliseconds(1100))
        window.overrideUserInterfaceStyle = .light
        #expect(await waitFor { events.contains(.message("theme light")) })
        controller.tearDown()
        window.isHidden = true
    }

    static let callingApp = """
        <!doctype html><html><body><script>
        const send = (message) => window.parent.postMessage(Object.assign({ jsonrpc: "2.0" }, message), "*");
        const say = (id, text) => send({ id, method: "ui/message", params: { role: "user", content: [{ type: "text", text }] } });
        window.addEventListener("message", (event) => {
          const data = event.data;
          if (data && data.id === 1 && data.result) {
            send({ method: "ui/notifications/initialized", params: {} });
            const tools = data.result.hostCapabilities.serverTools ? "tools on" : "tools off";
            say(2, tools);
            send({ id: 3, method: "tools/call", params: { name: "quote", arguments: { usage_gb: 240 } } });
          }
          if (data && data.id === 3) setTimeout(() => say(4, data.result ? "got " + data.result.content[0].text : "failed " + data.error.code), 1100);
        });
        send({ id: 1, method: "ui/initialize", params: { protocolVersion: "2026-01-26", appInfo: { name: "t", version: "1" }, appCapabilities: {} } });
        </script></body></html>
        """

    @Test func aRealWebViewCarriesAnAppCallAndItsAnswer() async throws {
        let controller = EmbedPageController()
        var events: [EmbedEvent] = []
        var calls: [(String, JSONValue)] = []
        controller.onEvent = { events.append($0) }
        controller.appCalls = true
        controller.onAppCall = { method, params, answer in
            calls.append((method, params))
            answer(["result": ["content": [["type": "text", "text": "Oila 100"]]]])
        }
        let webView = controller.makeWebView()
        webView.frame = CGRect(x: 0, y: 0, width: 320, height: 320)
        var props = Self.props()
        props["app"] = [
            "resource": ["uri": "ui://test/app", "mimeType": "text/html;profile=mcp-app", "text": .string(Self.callingApp)],
            "tool_input": [:],
        ]
        controller.show(try JSONValue.object(props).jsonText())
        #expect(await waitFor { events.contains(.message("tools on")) })
        #expect(await waitFor { events.contains(.message("got Oila 100")) })
        #expect(calls.count == 1)
        #expect(calls.first?.0 == "tools/call")
        #expect(calls.first?.1["name"] == "quote")
        controller.tearDown()
    }

    @Test func aRealWebViewRefusesAppCallsWhenTheServerDidNotAcceptThem() async throws {
        let controller = EmbedPageController()
        var events: [EmbedEvent] = []
        var calls = 0
        controller.onEvent = { events.append($0) }
        controller.onAppCall = { _, _, _ in calls += 1 }
        let webView = controller.makeWebView()
        webView.frame = CGRect(x: 0, y: 0, width: 320, height: 320)
        var props = Self.props()
        props["app"] = [
            "resource": ["uri": "ui://test/app", "mimeType": "text/html;profile=mcp-app", "text": .string(Self.callingApp)],
            "tool_input": [:],
        ]
        controller.show(try JSONValue.object(props).jsonText())
        #expect(await waitFor { events.contains(.message("tools off")) })
        #expect(await waitFor { events.contains(.message("failed -32601")) })
        #expect(calls == 0)
        controller.tearDown()
    }

    @Test func aRealWebViewRunsTheAppAndFiltersMessages() async throws {
        let controller = EmbedPageController()
        var events: [EmbedEvent] = []
        var links: [URL] = []
        controller.onEvent = { events.append($0) }
        controller.onLink = { url, answer in
            links.append(url)
            answer(true)
        }
        let webView = controller.makeWebView()
        webView.frame = CGRect(x: 0, y: 0, width: 320, height: 320)
        let props = try JSONValue.object(Self.props()).jsonText()
        controller.show(props)
        let nonce = controller.nonce
        #expect(nonce.count == 32)
        #expect(await waitFor { events.contains(.ready) })
        #expect(await waitFor { events.contains(.message("From the app")) })
        #expect(await waitFor { !links.isEmpty })
        #expect(events.contains(.height(320)))
        #expect(!events.contains(.message("forged")))
        #expect(links == [URL(string: "https://example.com/more")!])
        controller.tearDown()
    }
}
