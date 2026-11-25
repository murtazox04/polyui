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
            url: "https://github.com/murtazox04/polyui/releases/download/v0.1.2/PolyUICore.xcframework.zip",
            checksum: "f5a9c591086a9e613fc223e63b8e788c146401ab99d5705bc07f9faaeb99a832"
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
