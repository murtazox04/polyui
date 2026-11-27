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
            url: "https://github.com/murtazox04/polyui/releases/download/v0.1.5/PolyUICore.xcframework.zip",
            checksum: "2399ecce9ae4e60edf7977897ca3824dfb23182dc3076d29389ee5172b3cfc92"
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
