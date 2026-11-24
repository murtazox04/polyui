import 'dart:convert';

import 'package:flutter/foundation.dart';

import '../core/polyui_core.dart';
import '../core/render_batch.dart';
import 'surface_model.dart';

typedef PolyuiSend = void Function(String message);

class PolyuiSession extends ChangeNotifier {
  PolyuiSession(this.core, {PolyuiSend? send}) : send = send ?? ignore;

  factory PolyuiSession.start({
    PolyuiHello hello = const PolyuiHello(),
    List<Map<String, Object?>> catalogs = const [],
    bool allowHttp = false,
    PolyuiSend? send,
  }) => PolyuiSession(
    PolyuiCore.fromHello(hello, allowHttp: allowHttp, catalogs: catalogs),
    send: send,
  );

  final PolyuiCore core;
  PolyuiSend send;

  @internal
  final Map<String, PolyuiSurfaceModel> surfaces = {};

  @internal
  final List<String> order = [];

  @internal
  final Set<String> reportedTypes = {};

  @internal
  String? currentError;

  bool get allowHttp => core.allowHttp;

  List<String> get surfaceIds => List.unmodifiable(order);

  String? get lastError => currentError;

  static void ignore(String message) {}

  PolyuiSurfaceModel surface(String id) {
    final existing = surfaces[id];
    if (existing != null) {
      return existing;
    }
    final made = PolyuiSurfaceModel(id);
    surfaces[id] = made;
    order.add(id);
    return made;
  }

  void receive(String message) {
    accept(attempt('', () => core.receive(message)));
  }

  void receiveJson(Map<String, Object?> message) =>
      receive(jsonEncode(message));

  void setValue({
    required String surface,
    required String path,
    required Object? value,
  }) {
    accept(
      attempt(
        surface,
        () => core.setValue(surface: surface, path: path, value: value),
      ),
    );
  }

  void fire({
    required String surface,
    required String component,
    required String prop,
    Map<String, Object?>? detail,
  }) {
    final row = detail?['row'];
    if (prop == 'on_select' && row is int) {
      surfaces[surface]?.node(component)?.selectRow(row);
    }
    final message = attempt(
      surface,
      () => core.fire(
        surface: surface,
        component: component,
        prop: prop,
        detail: detail,
      ),
    );
    if (message != null) {
      send(message);
    }
  }

  void confirm({required String surface, required ConfirmRequest request}) {
    dismissConfirm(surface);
    final message = attempt(
      surface,
      () => core.confirm(
        surface: surface,
        actionId: request.actionId,
        nonce: request.nonce,
      ),
    );
    if (message != null) {
      send(message);
    }
  }

  void dismissConfirm(String surface) {
    surfaces[surface]?.clearConfirm();
  }

  List<Utterance> say(String surface) =>
      attempt(surface, () => core.say(surface)) ?? const [];

  VoiceMatch? matchVoice({required String surface, required String text}) {
    final found = attempt(
      surface,
      () => core.matchVoice(surface: surface, text: text),
    );
    if (found == null) {
      return null;
    }
    accept(found.batch);
    final action = found.action;
    if (action != null) {
      send(jsonEncode(action));
    }
    return found;
  }

  void highlight({required String surface, String? id}) {
    this.surface(surface).highlight(id);
  }

  @internal
  void reportUnknown({required String surface, required String type}) {
    if (reportedTypes.add(type)) {
      reportError(surface: surface, reason: 'unknown_component', detail: type);
    }
  }

  @internal
  void accept(RenderBatch? batch) {
    if (batch == null) {
      return;
    }
    for (final message in batch.send) {
      send(message is String ? message : jsonEncode(message));
    }
    if (batch.ops.isNotEmpty ||
        (batch.fallback != null && batch.surface.isNotEmpty)) {
      surface(batch.surface).apply(batch);
    }
  }

  @internal
  T? attempt<T>(String surface, T Function() run) {
    try {
      return run();
    } on PolyuiException catch (error) {
      currentError = error.message;
      reportError(
        surface: surface,
        reason: 'bad_message',
        detail: error.message,
      );
      notifyListeners();
      return null;
    }
  }

  @internal
  void reportError({
    required String surface,
    required String reason,
    required String detail,
  }) {
    send(
      jsonEncode({
        'v': 1,
        't': 'ui.error',
        'surface': surface,
        'reason': reason,
        'detail': detail,
      }),
    );
  }

  @override
  void dispose() {
    for (final model in surfaces.values) {
      model.dispose();
    }
    core.dispose();
    super.dispose();
  }
}
