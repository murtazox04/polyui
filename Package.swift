// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "PolyUI",
    platforms: [.iOS(.v17), .macCatalyst(.v17)],
    products: [
        .library(name: "PolyUI", targets: ["PolyUI"])
    ],
    targets: [
        .binaryTarget(
            name: "PolyUICore",
            url: "https://github.com/murtazox04/polyui/releases/download/v0.1.1/PolyUICore.xcframework.zip",
            checksum: "fcd47d59e03500dd4a4a44614d11934c68e221d4fa1e925bbd0944bf34b1bc32"
        ),
        .target(
            name: "PolyUI",
            dependencies: ["PolyUICore"],
            path: "swift/PolyUI/Sources/PolyUI",
            swiftSettings: [.enableUpcomingFeature("ExistentialAny")]
        ),
    ],
    swiftLanguageModes: [.v6]
)
