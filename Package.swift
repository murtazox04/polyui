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
            url: "https://github.com/murtazox04/polyui/releases/download/v0.1.8/PolyUICore.xcframework.zip",
            checksum: "c3a18311853c2efbfecaeab9cd8ad2b6da0a2e6024c43e061761388b92031228"
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
