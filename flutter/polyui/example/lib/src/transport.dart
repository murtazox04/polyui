import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:polyui/polyui.dart';
import 'package:web_socket_channel/io.dart';
import 'package:web_socket_channel/status.dart' as status;

import 'scripted_server.dart';

abstract interface class ChatTransport {
  void send(String message);

  void close();
}

final class ReplayTransport implements ChatTransport {
  ReplayTransport(this.server);

  final ScriptedServer server;

  @override
  void send(String message) => unawaited(server.receive(message));

  @override
  void close() {}
}

enum LiveState { connecting, open, closed }

final class LiveTransport implements ChatTransport {
  LiveTransport({
    required this.url,
    required this.deliver,
    required this.changed,
    this.lost,
  });

  static const connectTimeout = Duration(seconds: 8);
  static const resumeFeature = 'resume';
  static const appCallsFeature = PolyuiSession.appCallsFeature;
  static const appContextFeature = PolyuiSession.appContextFeature;
  static const dismissFeature = PolyuiSession.dismissFeature;
  static const declaredFeatures = [
    resumeFeature,
    appCallsFeature,
    appContextFeature,
    dismissFeature,
  ];
  static const retryDelays = [
    Duration(seconds: 1),
    Duration(seconds: 2),
    Duration(seconds: 4),
    Duration(seconds: 8),
    Duration(seconds: 16),
  ];

  final Uri url;
  final Deliver deliver;
  final void Function(LiveState state, String? problem) changed;
  final void Function()? lost;
  final List<String> waiting = [];
  IOWebSocketChannel? channel;
  StreamSubscription<Object?>? subscription;
  LiveState state = LiveState.connecting;
  bool closedByUs = false;
  String? hello;
  String? resumeSession;
  String? resumeToken;
  int attempt = 0;
  Timer? retry;

  static Uri defaultUrl() {
    const configured = String.fromEnvironment('POLYUI_SERVER');
    if (configured.isNotEmpty) {
      return Uri.parse(configured);
    }
    final host = Platform.isAndroid ? '10.0.2.2' : '127.0.0.1';
    return Uri.parse('ws://$host:8787/ws');
  }

  Future<void> connect() async {
    final made = IOWebSocketChannel.connect(
      url,
      connectTimeout: connectTimeout,
    );
    channel = made;
    try {
      await made.ready;
    } on Object catch (error) {
      dropped(made, 'Could not reach $url ($error).');
      return;
    }
    if (closedByUs || channel != made) {
      return;
    }
    subscription = made.stream.listen(
      (message) {
        if (message is String && channel == made) {
          noteWelcome(message);
          deliver(message);
        }
      },
      onDone: () {
        final reason = made.closeReason;
        dropped(
          made,
          'The server closed the connection'
          '${reason == null || reason.isEmpty ? '' : ': $reason'}.',
        );
      },
      onError: (Object error) =>
          dropped(made, 'The connection failed ($error).'),
    );
    final resuming = resumeToken != null && hello != null;
    if (resuming) {
      made.sink.add(helloWith(hello!, resumeSession, resumeToken));
    }
    state = LiveState.open;
    changed(state, null);
    for (final message in waiting) {
      made.sink.add(message);
    }
    waiting.clear();
  }

  void dropped(IOWebSocketChannel made, String problem) {
    if (channel != made) {
      return;
    }
    unawaited(subscription?.cancel());
    subscription = null;
    if (closedByUs) {
      finish(null);
      return;
    }
    if (resumeToken == null || attempt >= retryDelays.length) {
      finish(problem);
      return;
    }
    final delay = retryDelays[attempt];
    attempt += 1;
    state = LiveState.connecting;
    changed(state, null);
    retry = Timer(delay, () {
      if (!closedByUs) {
        unawaited(connect());
      }
    });
  }

  void noteWelcome(String text) {
    final Object? message;
    try {
      message = jsonDecode(text);
    } on FormatException {
      return;
    }
    if (message is! Map<String, Object?> || message['t'] != 'welcome') {
      return;
    }
    attempt = 0;
    final session = message['session'];
    final token = message['resume_token'];
    resumeSession = session is String && token is String ? session : null;
    resumeToken = session is String && token is String ? token : null;
    if (message['resumed'] == false) {
      lost?.call();
    }
  }

  static bool isHello(String text) {
    try {
      final parsed = jsonDecode(text);
      return parsed is Map<String, Object?> && parsed['t'] == 'hello';
    } on FormatException {
      return false;
    }
  }

  static String helloWith(String text, String? session, String? token) {
    final Object? parsed;
    try {
      parsed = jsonDecode(text);
    } on FormatException {
      return text;
    }
    if (parsed is! Map<String, Object?>) {
      return text;
    }
    final fields = Map<String, Object?>.of(parsed);
    final features = [...?(fields['features'] as List<Object?>?)];
    for (final feature in declaredFeatures) {
      if (!features.contains(feature)) {
        features.add(feature);
      }
    }
    fields['features'] = features;
    if (session != null && token != null) {
      fields['resume'] = {'session': session, 'token': token};
    } else {
      fields.remove('resume');
    }
    return jsonEncode(fields);
  }

  void finish(String? problem) {
    if (state == LiveState.closed) {
      return;
    }
    state = LiveState.closed;
    changed(state, problem);
  }

  @override
  void send(String message) {
    if (hello == null && isHello(message)) {
      hello = message;
      message = helloWith(message, null, null);
    }
    switch (state) {
      case LiveState.connecting:
        waiting.add(message);
      case LiveState.open:
        channel?.sink.add(message);
      case LiveState.closed:
        break;
    }
  }

  @override
  void close() {
    closedByUs = true;
    retry?.cancel();
    unawaited(subscription?.cancel());
    unawaited(channel?.sink.close(status.normalClosure));
    finish(null);
  }
}
