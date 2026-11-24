import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:polyui/polyui.dart';

import 'scripted_server.dart';

sealed class ChatEntry {
  const ChatEntry(this.id);

  final int id;
}

final class UserEntry extends ChatEntry {
  const UserEntry(super.id, this.text);

  final String text;
}

final class AgentEntry extends ChatEntry {
  const AgentEntry(super.id, this.text);

  final String text;
}

final class SurfaceEntry extends ChatEntry {
  const SurfaceEntry(super.id, this.surface);

  final String surface;
}

class ChatModel extends ChangeNotifier {
  ChatModel({required this.script, this.instant = false}) {
    start();
  }

  static const hello = PolyuiHello(
    clientName: 'polyui-flutter-example',
    locale: 'en',
  );

  final Map<String, Object?> script;
  final bool instant;
  final List<ChatEntry> entries = [];
  late PolyuiSession session;
  ScriptedServer? server;
  String? problem;
  int nextId = 0;
  int? openAgentEntry;

  String get question => server?.question ?? 'What do we owe this month?';

  void start() {
    entries.clear();
    openAgentEntry = null;
    problem = null;
    session = PolyuiSession.start(hello: hello, send: outgoing);
    server = ScriptedServer(
      script: script,
      instant: instant,
      deliver: incoming,
    );
    outgoing(hello.jsonText());
    notifyListeners();
  }

  void replay() {
    final previous = session;
    start();
    previous.dispose();
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

  void outgoing(String text) {
    final target = server;
    if (target != null) {
      unawaited(target.receive(text));
    }
  }

  void incoming(String text) {
    final message = jsonDecode(text) as Map<String, Object?>;
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
    session.dispose();
    super.dispose();
  }
}
