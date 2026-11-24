import 'package:flutter/foundation.dart';

import '../core/render_batch.dart';
import 'component_node.dart';

class PolyuiSurfaceModel extends ChangeNotifier {
  PolyuiSurfaceModel(this.id);

  final String id;

  @internal
  final Map<String, ComponentNode> nodes = {};

  @internal
  final Map<String, ValueNotifier<ComponentNode?>> slots = {};

  @internal
  String? currentRoot;

  @internal
  String? currentHighlight;

  @internal
  ConfirmRequest? currentConfirm;

  @internal
  bool currentlyClosed = false;

  @internal
  String? currentFallback;

  @internal
  int currentStructure = 0;

  String? get rootId => currentRoot;

  ComponentNode? get root => currentRoot == null ? null : nodes[currentRoot];

  String? get highlightedId => currentHighlight;

  ConfirmRequest? get pendingConfirm => currentConfirm;

  bool get isClosed => currentlyClosed;

  String? get fallback => currentFallback;

  int get structureRevision => currentStructure;

  ComponentNode? node(String id) => nodes[id];

  ValueListenable<ComponentNode?> slot(String id) =>
      slots.putIfAbsent(id, () => ValueNotifier(nodes[id]));

  void apply(RenderBatch batch) {
    var changed = false;
    if (batch.fallback != null && batch.fallback != currentFallback) {
      currentFallback = batch.fallback;
      changed = true;
    }
    final recreated = {
      for (final op in batch.ops)
        if (op is CreateOp) op.payload.id,
    };
    var structural = false;
    for (final op in batch.ops) {
      switch (op) {
        case CreateOp(:final payload):
          create(payload);
          structural = true;
        case UpdateOp(:final payload):
          nodes[payload.id]?.updateWith(payload);
        case ChildrenOp(:final id, :final children):
          nodes[id]?.replaceChildren(
            children.where((child) => child != id).toList(growable: false),
          );
        case RemoveOp(:final id):
          if (!recreated.contains(id)) {
            remove(id);
            structural = true;
          }
        case RootOp(:final id):
          if (nodes.containsKey(id) && currentRoot != id) {
            currentRoot = id;
            structural = true;
          }
        case ConfirmOp(:final request):
          currentConfirm = request;
          changed = true;
        case HighlightOp(:final id):
          highlight(id);
        case CloseOp():
          close();
          structural = true;
        case UnknownOp():
          break;
      }
    }
    if (structural) {
      currentStructure += 1;
    }
    if (structural || changed) {
      notifyListeners();
    }
  }

  void highlight(String? id) {
    if (currentHighlight == id) {
      return;
    }
    final previous = currentHighlight;
    if (previous != null) {
      nodes[previous]?.setHighlighted(false);
    }
    final target = id == null ? null : nodes[id];
    target?.setHighlighted(true);
    currentHighlight = target?.id;
  }

  @internal
  void clearConfirm() {
    if (currentConfirm == null) {
      return;
    }
    currentConfirm = null;
    notifyListeners();
  }

  @internal
  void create(ComponentPayload payload) {
    currentlyClosed = false;
    final type = payload.type ?? 'Text';
    final existing = nodes[payload.id];
    if (existing != null && existing.type == type) {
      existing.updateWith(payload);
      return;
    }
    if (currentHighlight == payload.id) {
      currentHighlight = null;
    }
    final made = ComponentNode(payload, type);
    nodes[payload.id] = made;
    slots[payload.id]?.value = made;
  }

  @internal
  void remove(String id) {
    if (nodes.remove(id) == null) {
      return;
    }
    slots[id]?.value = null;
    if (currentRoot == id) {
      currentRoot = null;
    }
    if (currentHighlight == id) {
      currentHighlight = null;
    }
  }

  @internal
  void close() {
    nodes.clear();
    for (final slot in slots.values) {
      slot.value = null;
    }
    currentRoot = null;
    currentHighlight = null;
    currentConfirm = null;
    currentlyClosed = true;
  }

  @override
  void dispose() {
    for (final slot in slots.values) {
      slot.dispose();
    }
    super.dispose();
  }
}
