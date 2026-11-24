import Foundation
import Observation

@MainActor
@Observable
public final class PolyUISurfaceModel: Identifiable {
    public let id: String
    public private(set) var rootID: String?
    public private(set) var nodes: [String: ComponentNode] = [:]
    public private(set) var highlightedID: String?
    public internal(set) var pendingConfirm: ConfirmRequest?
    public internal(set) var pendingLink: PolyUILinkRequest?
    public private(set) var isClosed = false
    public private(set) var fallback: String?
    public private(set) var structureRevision = 0

    public init(id: String) {
        self.id = id
    }

    public var root: ComponentNode? {
        rootID.flatMap { nodes[$0] }
    }

    public func node(_ id: String) -> ComponentNode? {
        nodes[id]
    }

    public var presentedSheet: PolyUISheet? {
        if let pendingConfirm { return .confirm(pendingConfirm) }
        if let pendingLink { return .link(pendingLink) }
        return nil
    }

    public func apply(_ batch: RenderBatch) {
        if let fallback = batch.fallback { self.fallback = fallback }
        let recreated = Set(batch.ops.compactMap { op -> String? in
            if case .create(let payload) = op { payload.id } else { nil }
        })
        var structural = false
        for op in batch.ops {
            switch op {
            case .create(let payload):
                create(payload)
                structural = true
            case .update(let payload):
                nodes[payload.id]?.update(with: payload)
            case .children(let id, let children):
                guard let parent = nodes[id] else { continue }
                let kept = children.filter { $0 != id }
                if parent.children != kept {
                    parent.children = kept
                    structural = true
                }
            case .remove(let id):
                guard !recreated.contains(id) else { continue }
                remove(id)
                structural = true
            case .root(let id):
                guard nodes[id] != nil, rootID != id else { continue }
                rootID = id
                structural = true
            case .confirm(let request):
                pendingConfirm = request
            case .highlight(let id):
                highlight(id)
            case .close:
                close()
                structural = true
            case .unknown:
                continue
            }
        }
        if structural { structureRevision += 1 }
    }

    func highlight(_ id: String?) {
        guard highlightedID != id else { return }
        if let highlightedID { nodes[highlightedID]?.isHighlighted = false }
        let target = id.flatMap { nodes[$0] }
        target?.isHighlighted = true
        highlightedID = target?.id
    }

    private func create(_ payload: ComponentPayload) {
        isClosed = false
        let type = payload.type ?? "Text"
        if let existing = nodes[payload.id], existing.type == type {
            existing.update(with: payload)
            return
        }
        if highlightedID == payload.id { highlightedID = nil }
        nodes[payload.id] = ComponentNode(payload: payload, type: type)
    }

    private func remove(_ id: String) {
        guard nodes.removeValue(forKey: id) != nil else { return }
        if rootID == id { rootID = nil }
        if highlightedID == id { highlightedID = nil }
    }

    func takeLink(_ id: UUID) -> PolyUILinkRequest? {
        guard let link = pendingLink, link.id == id else { return nil }
        pendingLink = nil
        return link
    }

    func withdrawLink(from component: String) {
        guard let link = pendingLink, link.component == component else { return }
        pendingLink = nil
        link.answer(false)
    }

    private func close() {
        nodes.removeAll()
        rootID = nil
        highlightedID = nil
        pendingConfirm = nil
        if let link = pendingLink {
            pendingLink = nil
            link.answer(false)
        }
        isClosed = true
    }
}

public struct PolyUILinkRequest: Identifiable {
    public let id: UUID
    public let url: URL
    public let component: String
    let answer: @MainActor (Bool) -> Void

    init(url: URL, component: String, answer: @escaping @MainActor (Bool) -> Void) {
        id = UUID()
        self.url = url
        self.component = component
        self.answer = answer
    }
}

public enum PolyUISheet: Identifiable {
    case confirm(ConfirmRequest)
    case link(PolyUILinkRequest)

    public var id: String {
        switch self {
        case .confirm(let request): "confirm:\(request.id)"
        case .link(let link): "link:\(link.id.uuidString)"
        }
    }
}
