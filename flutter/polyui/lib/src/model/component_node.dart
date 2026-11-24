import 'package:flutter/foundation.dart';

import '../core/json.dart';
import '../core/render_batch.dart';
import 'chart_scene.dart';

class ComponentNode extends ChangeNotifier {
  ComponentNode(ComponentPayload payload, this.type)
    : id = payload.id,
      currentProps = payload.props,
      currentA11y = payload.a11y,
      currentValue = payload.value;

  final String id;
  final String type;

  @internal
  Map<String, Object?> currentProps;

  @internal
  A11y currentA11y;

  @internal
  Object? currentValue;

  @internal
  List<String> currentChildren = const [];

  @internal
  bool currentlyHighlighted = false;

  @internal
  int currentRevision = 0;

  @internal
  int? currentSelectedRow;

  @internal
  int sceneRevision = -1;

  @internal
  ChartScene? sceneCache;

  Map<String, Object?> get props => currentProps;

  A11y get a11y => currentA11y;

  Object? get value => currentValue;

  List<String> get children => currentChildren;

  bool get highlighted => currentlyHighlighted;

  int get revision => currentRevision;

  int? get selectedRow => currentSelectedRow;

  @internal
  void selectRow(int? row) {
    if (currentSelectedRow == row) {
      return;
    }
    currentSelectedRow = row;
    notifyListeners();
  }

  @internal
  void updateWith(ComponentPayload payload) {
    currentProps = payload.props;
    currentA11y = payload.a11y;
    currentValue = payload.value;
    currentRevision += 1;
    notifyListeners();
  }

  @internal
  void replaceChildren(List<String> next) {
    if (listEquals(currentChildren, next)) {
      return;
    }
    currentChildren = List.unmodifiable(next);
    notifyListeners();
  }

  @internal
  void setHighlighted(bool on) {
    if (currentlyHighlighted == on) {
      return;
    }
    currentlyHighlighted = on;
    notifyListeners();
  }

  String string(String key) => jsonString(currentProps[key]) ?? '';

  bool flag(String key) => jsonBool(currentProps[key]) ?? false;

  double? number(String key) => jsonNumber(currentProps[key]);

  String choice(String key, Set<String> allowed, String fallback) {
    final raw = jsonString(currentProps[key]);
    return raw != null && allowed.contains(raw) ? raw : fallback;
  }

  bool hasAction(String key) =>
      jsonString(asJsonMap(currentProps[key])['action_id']) != null;

  ChartScene? get chartScene {
    if (sceneRevision != currentRevision) {
      sceneRevision = currentRevision;
      sceneCache = ChartScene.tryParse(currentProps['scene']);
    }
    return sceneCache;
  }
}
