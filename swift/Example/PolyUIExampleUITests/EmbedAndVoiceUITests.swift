import XCTest

final class EmbedAndVoiceUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    private func launch(_ arguments: [String]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = arguments + ["-PolyUIInstant", "YES", "-PolyUIScroll", "bottom"]
        app.launch()
        return app
    }

    @MainActor
    private func text(_ app: XCUIApplication, containing fragment: String) -> XCUIElement {
        app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", fragment)).firstMatch
    }

    @MainActor
    func testTheForecastAppRendersItsToolResultAndSendsAMessage() {
        let app = launch(["-PolyUIDemo", "forecast"])
        let web = app.webViews.firstMatch
        XCTAssertTrue(web.waitForExistence(timeout: 30))
        XCTAssertTrue(web.staticTexts["Forecast for Tashkent"].waitForExistence(timeout: 20), "tool input reached the app")
        XCTAssertTrue(web.staticTexts["Mild days, cooler at the weekend."].waitForExistence(timeout: 10), "tool result reached the app")
        XCTAssertTrue(web.staticTexts["14°"].exists, "the five days are drawn")
        XCTAssertTrue(web.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "connect-src")).firstMatch.waitForExistence(timeout: 10), "the app's fetch is blocked by its CSP")
        XCTAssertTrue(app.otherElements["Five day forecast for Tashkent"].exists, "the web view is labelled with the title")

        web.buttons["Also show Samarkand"].tap()
        XCTAssertTrue(web.staticTexts["Sent to the assistant."].waitForExistence(timeout: 10))
        let reply = text(app, containing: "The forecast app asked")
        XCTAssertTrue(reply.waitForExistence(timeout: 10), "the message became a ui.action and the server answered it")
        XCTAssertTrue(reply.label.contains("Also show the forecast for Samarkand."), "the action carried the app's text as detail")
    }

    @MainActor
    func testALinkGoesThroughTheConfirmSheet() {
        let app = launch(["-PolyUIDemo", "forecast"])
        let web = app.webViews.firstMatch
        let more = web.buttons["Full forecast"]
        XCTAssertTrue(more.waitForExistence(timeout: 30))
        XCTAssertTrue(web.staticTexts["Mild days, cooler at the weekend."].waitForExistence(timeout: 20))

        more.tap()
        XCTAssertTrue(app.staticTexts["Open this link outside the app?"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["https://example.com/forecast/tashkent"].exists, "the sheet shows the full URL")
        app.buttons["Cancel"].tap()
        XCTAssertTrue(web.staticTexts["Link opening denied by user"].waitForExistence(timeout: 10), "a refused link is an error for the app")

        more.tap()
        let open = app.buttons["Open link"]
        XCTAssertTrue(open.waitForExistence(timeout: 10))
        expectation(for: NSPredicate(format: "isEnabled == true"), evaluatedWith: open)
        waitForExpectations(timeout: 5)
        open.tap()
        let safari = XCUIApplication(bundleIdentifier: "com.apple.mobilesafari")
        XCTAssertTrue(safari.wait(for: .runningForeground, timeout: 15), "the link opened outside the app")
        app.activate()
        XCTAssertTrue(app.webViews.firstMatch.staticTexts["Link opened."].waitForExistence(timeout: 10), "the app heard that the link was accepted")
    }

    @MainActor
    func testASpokenAnswerGoesThroughMatchVoice() {
        let app = launch(["-PolyUIDemo", "ask", "-PolyUIVoice", "the visa card"])
        XCTAssertTrue(text(app, containing: "Heard").waitForExistence(timeout: 30))
        let visa = app.buttons.containing(NSPredicate(format: "label CONTAINS %@", "Visa card ending 0918")).firstMatch
        XCTAssertTrue(visa.waitForExistence(timeout: 10))
        XCTAssertTrue(visa.isSelected, "match_voice picked the Visa card")
    }

    @MainActor
    func testRefusedSpeechPermissionFallsBackToTyping() {
        let app = launch(["-PolyUIDemo", "ask"])
        addUIInterruptionMonitor(withDescription: "Speech permission") { alert in
            let refuse = alert.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Don")).firstMatch
            guard refuse.exists else { return false }
            refuse.tap()
            return true
        }
        let speak = app.buttons["Speak"]
        XCTAssertTrue(speak.waitForExistence(timeout: 30))
        speak.tap()
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let refuse = springboard.alerts.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Don")).firstMatch
        if refuse.waitForExistence(timeout: 5) { refuse.tap() }
        XCTAssertTrue(text(app, containing: "Speech recognition is off for this app").waitForExistence(timeout: 10))
        XCTAssertTrue(app.textFields["Message the assistant"].exists || app.textViews["Message the assistant"].exists)
    }
}
