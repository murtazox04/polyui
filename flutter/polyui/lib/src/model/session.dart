import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';

import '../core/polyui_core.dart';
import '../core/render_batch.dart';
import 'embed_bridge.dart';
import 'surface_model.dart';

typedef PolyuiSend = void Function(String message);

typedef PolyuiAppAnswer = void Function(Map<String, Object?> answer);

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

  static const appCallsFeature = 'app_calls';
  static const appContextFeature = 'app_context';
  static const dismissFeature = 'dismiss';
  static const appContextMethod = embedAppContextMethod;
  static const appCallTimeout = Duration(seconds: 90);
  static const Map<String, Object?> appCallTimedOut = {
    'error': {'code': -32000, 'message': 'The app request timed out.'},
  };
  static const Map<String, Object?> appCallsOff = {
    'error': {'code': -32601, 'message': 'Method not found'},
  };

  final PolyuiCore core;
  PolyuiSend send;

  @internal
  bool acceptedAppCalls = false;

  @internal
  bool acceptedAppContext = false;

  @internal
  bool acceptedDismiss = false;

  @internal
  final Map<String, PolyuiAppWaiter> appWaiters = {};

  @internal
  int appCounter = 0;

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

  bool get appCalls => acceptedAppCalls;

  bool get appContext => acceptedAppCalls && acceptedAppContext;

  bool get sendsDismiss => acceptedDismiss;

  int get openAppCalls => appWaiters.length;

  static void ignore(String message) {}

  static void ignoreCancel() {}

  @internal
  static String appKey(String surface, String embed, String id) =>
      jsonEncode([surface, embed, id]);

  @internal
  static Map<String, Object?>? peek(String text) {
    if (!text.contains('"welcome"') && !text.contains('"app.result"')) {
      return null;
    }
    final Object? decoded;
    try {
      decoded = jsonDecode(text);
    } on FormatException {
      return null;
    }
    if (decoded is! Map<String, Object?>) {
      return null;
    }
    final type = decoded['t'];
    return type == 'welcome' || type == 'app.result' ? decoded : null;
  }

  @internal
  static Map<String, Object?> answerOf(Map<String, Object?> message) {
    final error = message['error'];
    if (error is Map<String, Object?>) {
      final code = error['code'];
      final text = error['message'];
      return {
        'error': {
          'code': code is num ? code : -32000,
          'message': text is String ? text : 'The app request failed.',
        },
      };
    }
    return {'result': message['result']};
  }

  @internal
  void appResult(Map<String, Object?> message) {
    final surface = message['surface'];
    final embed = message['embed'];
    final id = message['id'];
    if (surface is! String || embed is! String || id is! String) {
      return;
    }
    final waiter = appWaiters.remove(appKey(surface, embed, id));
    if (waiter == null) {
      return;
    }
    waiter.cancel();
    waiter.done(answerOf(message));
  }

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
    final peeked = peek(message);
    if (peeked?['t'] == 'app.result') {
      appResult(peeked!);
      return;
    }
    if (peeked != null) {
      final features = peeked['features'];
      final listed = features is List ? features : const <Object?>[];
      acceptedAppCalls = listed.contains(appCallsFeature);
      acceptedAppContext = listed.contains(appContextFeature);
      acceptedDismiss = listed.contains(dismissFeature);
    }
    accept(attempt('', () => core.receive(message)));
  }

  void Function() callApp({
    required String surface,
    required String embed,
    required String method,
    required Map<String, Object?> params,
    required PolyuiAppAnswer done,
  }) {
    if (!acceptedAppCalls || (method == appContextMethod && !appContext)) {
      done(appCallsOff);
      return ignoreCancel;
    }
    appCounter += 1;
    final id = 'a$appCounter';
    final key = appKey(surface, embed, id);
    final waiter = PolyuiAppWaiter(done);
    appWaiters[key] = waiter;
    waiter.timer = Timer(appCallTimeout, () {
      if (appWaiters.remove(key) == waiter) {
        waiter.done(appCallTimedOut);
      }
    });
    send(
      jsonEncode({
        'v': 1,
        't': 'app.call',
        'surface': surface,
        'embed': embed,
        'id': id,
        'method': method,
        'params': params,
      }),
    );
    return () {
      if (appWaiters[key] == waiter) {
        appWaiters.remove(key);
      }
      waiter.cancel();
    };
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

  void cancelConfirm({
    required String surface,
    required ConfirmRequest request,
  }) {
    if (surfaces[surface]?.pendingConfirm?.key != request.key) {
      return;
    }
    dismissConfirm(surface);
    if (!acceptedDismiss) {
      return;
    }
    send(
      jsonEncode({
        'v': 1,
        't': 'ui.dismiss',
        'surface': surface,
        'action_id': request.actionId,
        'nonce': request.nonce,
      }),
    );
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
    final answer = found.option;
    final model = surfaces[surface];
    if (model?.pendingConfirm != null && (answer == 'yes' || answer == 'no')) {
      model?.clearConfirm();
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
    for (final waiter in appWaiters.values) {
      waiter.cancel();
    }
    appWaiters.clear();
    for (final model in surfaces.values) {
      model.dispose();
    }
    core.dispose();
    super.dispose();
  }
}

@internal
final class PolyuiAppWaiter {
  PolyuiAppWaiter(this.done);

  final PolyuiAppAnswer done;
  Timer? timer;

  void cancel() {
    timer?.cancel();
    timer = null;
  }
}
