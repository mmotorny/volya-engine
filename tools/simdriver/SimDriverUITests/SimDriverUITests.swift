// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Real touches in the iOS Simulator, for what in-page synthesized input
// can't reach: system sheets (file pickers), iOS gestures, the path from
// the screen to the page. `cargo xtask sim` runs this UI test with a script
// in SIMDRIVER_SCRIPT, one step per line:
//
//   open <url>                    open a URL (in Safari)
//   wait <seconds>
//   tap <x> <y>                   screen points from the top left
//   tap-text <label>              the first element with this label
//   hold <x> <y> <seconds>
//   drag <x1> <y1> <x2> <y2>
//   double-tap <x> <y>
//   pinch <scale>                 two fingers on Safari's web content
//   screenshot <path>             PNG of the screen, written on the host
//   expect-text <label>           fails unless an element has this label
//
// Elements are searched in Safari, then in any app on screen (system UI).

import XCTest

final class SimDriverUITests: XCTestCase {
    private let safari = XCUIApplication(bundleIdentifier: "com.apple.mobilesafari")
    private let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")

    func testScript() throws {
        let script = ProcessInfo.processInfo.environment["SIMDRIVER_SCRIPT"] ?? ""
        for (number, line) in script.split(separator: "\n").enumerated() {
            let step = line.trimmingCharacters(in: .whitespaces)
            if step.isEmpty { continue }
            print("simdriver: \(number + 1): \(step)")
            try run(step)
        }
    }

    private func run(_ step: String) throws {
        let (command, rest) = split(step)
        let numbers = rest.split(separator: " ").compactMap { Double($0) }
        switch command {
        case "open":
            let url = try XCTUnwrap(URL(string: rest), "bad URL: \(rest)")
            XCUIDevice.shared.system.open(url)
            _ = safari.wait(for: .runningForeground, timeout: 10)
        case "wait":
            Thread.sleep(forTimeInterval: try number(numbers, 0, step))
        case "tap":
            point(try number(numbers, 0, step), try number(numbers, 1, step)).tap()
        case "hold":
            point(try number(numbers, 0, step), try number(numbers, 1, step))
                .press(forDuration: try number(numbers, 2, step))
        case "drag":
            point(try number(numbers, 0, step), try number(numbers, 1, step))
                .press(forDuration: 0.1, thenDragTo: point(try number(numbers, 2, step), try number(numbers, 3, step)))
        case "double-tap":
            point(try number(numbers, 0, step), try number(numbers, 1, step)).doubleTap()
        case "pinch":
            // Above 1 spreads the fingers; XCUITest wants a velocity of the
            // same sign (scale factor per second).
            let scale = try number(numbers, 0, step)
            // The page on screen (Safari keeps other tabs' web views too);
            // a zoomed page's web view may be missing from the element tree,
            // then Safari's window, whose middle is the page's.
            let page = safari.webViews.allElementsBoundByIndex.first { $0.isHittable } ?? safari.windows.firstMatch
            page.pinch(withScale: scale, velocity: scale > 1 ? 2 : -2)
        case "tap-text":
            // Its centre on screen, like a finger: XCUITest's hittability
            // check often rejects web content that is visible.
            try element(rest).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        case "expect-text":
            _ = try element(rest)
        case "screenshot":
            let png = XCUIScreen.main.screenshot().pngRepresentation
            try png.write(to: URL(fileURLWithPath: rest))
        default:
            XCTFail("unknown step: \(step)")
        }
    }

    private func split(_ step: String) -> (String, String) {
        guard let space = step.firstIndex(of: " ") else { return (step, "") }
        return (String(step[..<space]), String(step[step.index(after: space)...]).trimmingCharacters(in: .whitespaces))
    }

    private func number(_ numbers: [Double], _ index: Int, _ step: String) throws -> Double {
        guard index < numbers.count else { throw Failure("missing number in: \(step)") }
        return numbers[index]
    }

    /// A screen point (portrait, points from the top left).
    private func point(_ x: Double, _ y: Double) -> XCUICoordinate {
        springboard.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: y))
    }

    /// The first element labelled `label`, in Safari or system UI, within 10 s.
    private func element(_ label: String) throws -> XCUIElement {
        let predicate = NSPredicate(format: "label == %@", label)
        let deadline = Date().addingTimeInterval(10)
        while Date() < deadline {
            for app in [safari, springboard] {
                let match = app.descendants(matching: .any).matching(predicate).firstMatch
                if match.exists { return match }
            }
            Thread.sleep(forTimeInterval: 0.25)
        }
        throw Failure("no element labelled \"\(label)\"")
    }
}

private struct Failure: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}
