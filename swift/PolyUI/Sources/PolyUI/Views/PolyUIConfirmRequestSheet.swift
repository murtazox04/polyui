import SwiftUI

public struct PolyUILabels: Sendable {
    public var cancel: String
    public var confirmFallback: String
    public var expired: String
    public var chartUnavailable: String
    public var openLinkTitle: String
    public var openLink: String

    public init(
        cancel: String = "Cancel",
        confirmFallback: String = "OK",
        expired: String = "This request expired. Ask again to continue.",
        chartUnavailable: String = "Chart unavailable",
        openLinkTitle: String = "Open this link outside the app?",
        openLink: String = "Open link"
    ) {
        self.cancel = cancel
        self.confirmFallback = confirmFallback
        self.expired = expired
        self.chartUnavailable = chartUnavailable
        self.openLinkTitle = openLinkTitle
        self.openLink = openLink
    }
}

extension EnvironmentValues {
    @Entry public var polyUILabels = PolyUILabels()
    @Entry public var polyUIConfirmDelay: Duration = .milliseconds(400)
}

struct PolyUIConfirmRequestSheet: View {
    let request: ConfirmRequest
    let onConfirm: () -> Void
    let onDismiss: () -> Void
    @Environment(\.polyUILabels) private var labels
    @Environment(\.polyUIConfirmDelay) private var delay
    @Environment(\.dismiss) private var dismiss
    @State private var armed = false
    @State private var expired = false
    @State private var opened = Date()
    @State private var answered = false
    @State private var contentHeight: CGFloat = 320

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(request.title)
                .font(.title2.weight(.semibold))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityAddTraits(.isHeader)
            if let body = request.body, !body.isEmpty {
                Text(body)
                    .font(.body)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let expires = request.expiresMilliseconds, expires > 0 {
                if expired {
                    Label(labels.expired, systemImage: "clock.badge.exclamationmark")
                        .font(.subheadline)
                        .foregroundStyle(.red)
                        .accessibilityAddTraits(.updatesFrequently)
                } else {
                    ProgressView(timerInterval: opened...opened.addingTimeInterval(Double(expires) / 1000), countsDown: true) {
                        EmptyView()
                    } currentValueLabel: {
                        EmptyView()
                    }
                    .progressViewStyle(.linear)
                    .tint(.secondary)
                    .accessibilityHidden(true)
                }
            }
            VStack(spacing: 10) {
                Button {
                    guard armed, !expired, !answered else { return }
                    answered = true
                    onConfirm()
                } label: {
                    Text(request.confirmLabel.flatMap { $0.isEmpty ? nil : $0 } ?? labels.confirmFallback)
                        .font(.body.weight(.semibold))
                        .frame(maxWidth: .infinity, minHeight: 34)
                }
                .buttonStyle(.borderedProminent)
                .disabled(!armed || expired)
                Button {
                    dismiss()
                } label: {
                    Text(labels.cancel)
                        .font(.body.weight(.semibold))
                        .frame(maxWidth: .infinity, minHeight: 34)
                }
                .buttonStyle(.bordered)
            }
            .controlSize(.large)
            .padding(.top, 8)
        }
        .padding(.horizontal, 24)
        .padding(.top, 28)
        .padding(.bottom, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .fixedSize(horizontal: false, vertical: true)
        .onGeometryChange(for: CGFloat.self) { proxy in
            proxy.size.height
        } action: { height in
            contentHeight = height
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .presentationDetents([.height(contentHeight)])
        .presentationBackground(Color(uiColor: .systemBackground))
        .presentationDragIndicator(.visible)
        .task(id: request.id) {
            opened = Date()
            armed = false
            expired = false
            try? await Task.sleep(for: delay)
            armed = true
            if let expires = request.expiresMilliseconds, expires > 0 {
                try? await Task.sleep(for: .milliseconds(expires) - delay)
                if !Task.isCancelled { expired = true }
            }
        }
        .onDisappear {
            if !answered { onDismiss() }
        }
    }
}
