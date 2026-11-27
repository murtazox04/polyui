import Foundation
import WebKit

@MainActor
final class EmbedPageController: NSObject {
    static let handlerName = "polyui"

    var onEvent: (EmbedEvent) -> Void = { _ in }
    var onLink: (URL, @escaping @MainActor (Bool) -> Void) -> Void = { _, answer in answer(false) }
    var onAppCall: (String, JSONValue, @escaping @MainActor (JSONValue) -> Void) -> Void = { _, _, answer in
        answer(["error": ["code": -32601, "message": "Method not found"]])
    }
    var appCalls = false
    var appContext = false

    private(set) var webView: WKWebView?
    private(set) var nonce = ""
    private var shownProps: String?
    private var expectingPageLoad = false
    private var linkPending = false

    func makeWebView() -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        configuration.preferences.isElementFullscreenEnabled = false
        configuration.dataDetectorTypes = []
        configuration.mediaTypesRequiringUserActionForPlayback = .all
        configuration.userContentController.add(EmbedMessageRelay(target: self), contentWorld: .page, name: Self.handlerName)
        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.allowsBackForwardNavigationGestures = false
        webView.allowsLinkPreview = false
        webView.isOpaque = false
        webView.backgroundColor = .clear
        webView.scrollView.backgroundColor = .clear
        webView.scrollView.isScrollEnabled = false
        webView.scrollView.bounces = false
        webView.scrollView.contentInsetAdjustmentBehavior = .never
        self.webView = webView
        return webView
    }

    func show(_ propsJSON: String) {
        guard let webView, propsJSON != shownProps else { return }
        let first = shownProps == nil
        shownProps = propsJSON
        if first {
            load(propsJSON)
            return
        }
        guard let literal = Self.stringLiteral(propsJSON) else {
            load(propsJSON)
            return
        }
        webView.evaluateJavaScript("window.polyuiHost.update(JSON.parse(\(literal)))") { [weak self] result, _ in
            guard let self, self.shownProps == propsJSON else { return }
            if (result as? Bool) != true { self.load(propsJSON) }
        }
    }

    func tearDown() {
        guard let webView else { return }
        webView.evaluateJavaScript("window.polyuiHost && window.polyuiHost.teardown()")
        webView.configuration.userContentController.removeScriptMessageHandler(forName: Self.handlerName, contentWorld: .page)
        webView.navigationDelegate = nil
        webView.uiDelegate = nil
        linkPending = false
        onEvent = { _ in }
        onLink = { _, answer in answer(false) }
        onAppCall = { _, _, _ in }
        self.webView = nil
    }

    func receive(_ message: WKScriptMessage) {
        guard let webView, message.webView === webView,
            let event = EmbedEvent.read(message.body, isMainFrame: message.frameInfo.isMainFrame, nonce: nonce)
        else { return }
        if case .appCall(let id, let method, let params) = event {
            let pageNonce = nonce
            onAppCall(method, params) { [weak self] answer in
                self?.answerApp(id, answer: answer, pageNonce: pageNonce)
            }
            return
        }
        guard case .openLink(let url) = event else {
            onEvent(event)
            return
        }
        guard !linkPending else { return }
        linkPending = true
        let pageNonce = nonce
        onLink(url) { [weak self] accepted in
            self?.answerLink(accepted, pageNonce: pageNonce)
        }
    }

    private func answerLink(_ accepted: Bool, pageNonce: String) {
        guard linkPending, pageNonce == nonce, let webView else { return }
        linkPending = false
        webView.evaluateJavaScript("window.polyuiHost.answerLink(\(accepted ? "true" : "false"))")
    }

    private func answerApp(_ id: String, answer: JSONValue, pageNonce: String) {
        guard pageNonce == nonce, let webView, let idLiteral = Self.stringLiteral(id),
            let answerText = try? answer.jsonText(), let answerLiteral = Self.stringLiteral(answerText)
        else { return }
        webView.evaluateJavaScript("window.polyuiHost.answerApp(\(idLiteral), JSON.parse(\(answerLiteral)))")
    }

    private func load(_ propsJSON: String) {
        guard let webView, let fresh = EmbedNonce.make(),
            let page = try? PolyUICore.embedPage(propsJSON: propsJSON, nonce: fresh, appCalls: appCalls, appContext: appContext)
        else {
            onEvent(.unavailable)
            return
        }
        nonce = fresh
        linkPending = false
        expectingPageLoad = true
        webView.loadHTMLString(page, baseURL: nil)
    }

    private static func stringLiteral(_ text: String) -> String? {
        guard let data = try? JSONEncoder().encode(text) else { return nil }
        return String(decoding: data, as: UTF8.self)
    }
}

extension EmbedPageController: WKNavigationDelegate {
    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction) async -> WKNavigationActionPolicy {
        let target = navigationAction.targetFrame?.isMainFrame
        let allowed = EmbedNavigation.allows(targetIsMainFrame: target, url: navigationAction.request.url, expectingPageLoad: expectingPageLoad)
        if target == true { expectingPageLoad = false }
        return allowed ? .allow : .cancel
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        guard let props = shownProps else { return }
        load(props)
    }
}

extension EmbedPageController: WKUIDelegate {
    func webView(
        _ webView: WKWebView,
        createWebViewWith configuration: WKWebViewConfiguration,
        for navigationAction: WKNavigationAction,
        windowFeatures: WKWindowFeatures
    ) -> WKWebView? {
        nil
    }

    func webView(
        _ webView: WKWebView,
        decideMediaCapturePermissionsFor origin: WKSecurityOrigin,
        initiatedBy frame: WKFrameInfo,
        type: WKMediaCaptureType
    ) async -> WKPermissionDecision {
        .deny
    }

    func webView(_ webView: WKWebView, contextMenuConfigurationFor elementInfo: WKContextMenuElementInfo) async -> UIContextMenuConfiguration? {
        nil
    }
}

@MainActor
final class EmbedMessageRelay: NSObject, WKScriptMessageHandler {
    private weak var target: EmbedPageController?

    init(target: EmbedPageController) {
        self.target = target
    }

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        target?.receive(message)
    }
}
