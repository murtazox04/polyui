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
            url: "https://github.com/murtazox04/polyui/releases/download/v0.1.6/PolyUICore.xcframework.zip",
            checksum: "1d48c46fe5e077550971f4ca9e847a55dc3d3e5d1ba190aae11563280ea462b9"
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
