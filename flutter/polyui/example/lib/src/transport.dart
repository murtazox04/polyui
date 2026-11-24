import 'dart:async';
import 'dart:io';

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
  });

  static const connectTimeout = Duration(seconds: 8);

  final Uri url;
  final Deliver deliver;
  final void Function(LiveState state, String? problem) changed;
  final List<String> waiting = [];
  IOWebSocketChannel? channel;
  StreamSubscription<Object?>? subscription;
  LiveState state = LiveState.connecting;
  bool closedByUs = false;

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
      finish('Could not reach $url ($error).');
      return;
    }
    if (closedByUs) {
      return;
    }
    subscription = made.stream.listen(
      (message) {
        if (message is String) {
          deliver(message);
        }
      },
      onDone: () {
        final reason = made.closeReason;
        finish(
          closedByUs
              ? null
              : 'The server closed the connection'
                    '${reason == null || reason.isEmpty ? '' : ': $reason'}.',
        );
      },
      onError: (Object error) => finish('The connection failed ($error).'),
    );
    state = LiveState.open;
    changed(state, null);
    for (final message in waiting) {
      made.sink.add(message);
    }
    waiting.clear();
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
    unawaited(subscription?.cancel());
    unawaited(channel?.sink.close(status.normalClosure));
    finish(null);
  }
}
