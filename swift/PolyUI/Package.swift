// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "PolyUI",
    platforms: [.iOS(.v17), .macCatalyst(.v17)],
    products: [
        .library(name: "PolyUI", targets: ["PolyUI"])
    ],
    targets: [
        .binaryTarget(name: "PolyUICore", path: "PolyUICore.xcframework"),
        .target(
            name: "PolyUI",
            dependencies: ["PolyUICore"],
            swiftSettings: [.enableUpcomingFeature("ExistentialAny")]
        ),
        .testTarget(
            name: "PolyUITests",
            dependencies: ["PolyUI"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
