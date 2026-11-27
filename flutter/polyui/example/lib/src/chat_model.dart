import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:polyui/polyui.dart';

import 'scripted_server.dart';
import 'transport.dart';

sealed class ChatEntry {
  const ChatEntry(this.id);

  final int id;
}

final class UserEntry extends ChatEntry {
  const UserEntry(super.id, this.text, {this.spoken = false});

  final String text;
  final bool spoken;
}

final class AgentEntry extends ChatEntry {
  const AgentEntry(super.id, this.text);

  final String text;
}

final class SurfaceEntry extends ChatEntry {
  const SurfaceEntry(super.id, this.surface);

  final String surface;
}

enum ChatMode { replay, live }

enum Connection { replay, connecting, live, offline }

class ChatModel extends ChangeNotifier {
  ChatModel({
    required this.script,
    required this.weather,
    this.instant = false,
    this.log = false,
  }) {
    start(ChatMode.replay);
  }

  static const hello = PolyuiHello(
    clientName: 'polyui-flutter-example',
    catalogs: ['core@1', 'chart@1', 'embed@1'],
    locale: 'en',
  );

  final Map<String, Object?> script;
  final Map<String, Object?> weather;
  final bool instant;
  final bool log;
  final List<ChatEntry> entries = [];
  late PolyuiSession session;
  ChatTransport? transport;
  ScriptedServer? server;
  ChatMode mode = ChatMode.replay;
  Connection connection = Connection.replay;
  Uri liveUrl = LiveTransport.defaultUrl();
  String? problem;
  int nextId = 0;
  int run = 0;
  int? openAgentEntry;

  String get question => server?.question ?? 'What do we owe this month?';

  String get weatherQuestion =>
      server?.weatherQuestion ?? 'What is the weather this week?';

  String? get latestSurface {
    for (final entry in entries.reversed) {
      if (entry is SurfaceEntry) {
        return entry.surface;
      }
    }
    return null;
  }

  void start(ChatMode next, {Uri? url}) {
    transport?.close();
    transport = null;
    server = null;
    entries.clear();
    openAgentEntry = null;
    problem = null;
    mode = next;
    final current = run += 1;
    void deliver(String message) {
      if (current == run) {
        incoming(message);
      }
    }

    session = PolyuiSession.start(hello: hello, send: outgoing);
    switch (next) {
      case ChatMode.replay:
        final scripted = ScriptedServer(
          script: script,
          weather: weather,
          instant: instant,
          deliver: deliver,
        );
        server = scripted;
        transport = ReplayTransport(scripted);
        connection = Connection.replay;
      case ChatMode.live:
        liveUrl = url ?? liveUrl;
        final live = LiveTransport(
          url: liveUrl,
          deliver: deliver,
          changed: (state, reason) {
            if (current == run) {
              liveChanged(state, reason);
            }
          },
          lost: () {
            if (current == run) {
              sessionLost();
            }
          },
        );
        transport = live;
        connection = Connection.connecting;
        unawaited(live.connect());
    }
    outgoing(hello.jsonText());
    notifyListeners();
  }

  void restart(ChatMode next, {Uri? url}) {
    final previous = session;
    start(next, url: url);
    previous.dispose();
  }

  void replay() => restart(ChatMode.replay);

  void goLive(Uri url) => restart(ChatMode.live, url: url);

  void liveChanged(LiveState state, String? reason) {
    connection = switch (state) {
      LiveState.connecting => Connection.connecting,
      LiveState.open => Connection.live,
      LiveState.closed => Connection.offline,
    };
    if (reason != null) {
      problem = reason;
    }
    notifyListeners();
  }

  void sessionLost() {
    entries.removeWhere((entry) => entry is SurfaceEntry);
    openAgentEntry = null;
    problem =
        'The connection was gone too long, so the earlier cards were removed.';
    notifyListeners();
  }

  void ask(String text) {
    final trimmed = text.trim();
    if (trimmed.isEmpty) {
      return;
    }
    entries.add(UserEntry(takeId(), trimmed));
    notifyListeners();
    outgoing(jsonEncode({'v': 1, 't': 'user.say', 'text': trimmed}));
  }

  void spokenMatch(String text) {
    entries.add(UserEntry(takeId(), text, spoken: true));
    notifyListeners();
  }

  void outgoing(String text) {
    if (log) {
      debugPrint('polyui-example out: $text');
    }
    transport?.send(text);
  }

  void incoming(String text) {
    if (log) {
      debugPrint(
        'polyui-example in: '
        '${text.length > 300 ? '${text.substring(0, 300)}...' : text}',
      );
    }
    final Object? decoded;
    try {
      decoded = jsonDecode(text);
    } on FormatException {
      session.receive(text);
      return;
    }
    final message = decoded is Map<String, Object?> ? decoded : const {};
    switch (message['t']) {
      case 'agent.delta':
        appendAgentText(message['text'] as String? ?? '');
      case 'agent.done':
        openAgentEntry = null;
      case 'agent.error':
        problem = message['message'] as String?;
        openAgentEntry = null;
        notifyListeners();
      case 'ui.open' || 'ui.snapshot':
        final surface = message['surface'] as String?;
        if (surface != null && !hasSurface(surface)) {
          openAgentEntry = null;
          entries.add(SurfaceEntry(takeId(), surface));
          notifyListeners();
        }
        session.receive(text);
      default:
        session.receive(text);
    }
  }

  bool hasSurface(String surface) =>
      entries.any((entry) => entry is SurfaceEntry && entry.surface == surface);

  void appendAgentText(String chunk) {
    final open = openAgentEntry;
    final index = open == null
        ? -1
        : entries.indexWhere((entry) => entry.id == open);
    final current = index >= 0 ? entries[index] : null;
    if (current is AgentEntry) {
      entries[index] = AgentEntry(current.id, current.text + chunk);
    } else {
      final id = takeId();
      openAgentEntry = id;
      entries.add(AgentEntry(id, chunk));
    }
    notifyListeners();
  }

  int takeId() => nextId += 1;

  @override
  void dispose() {
    transport?.close();
    session.dispose();
    super.dispose();
  }
}
