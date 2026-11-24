import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:polyui/polyui.dart';

class Harness {
  Harness({
    List<String> catalogs = const ['core@1', 'chart@1', 'embed@1'],
    bool allowHttp = false,
  }) {
    session = PolyuiSession.start(
      hello: PolyuiHello(catalogs: catalogs, locale: 'en'),
      allowHttp: allowHttp,
      send: sent.add,
    );
  }

  final List<String> sent = [];
  late final PolyuiSession session;
  var seq = 0;

  List<Map<String, Object?>> get messages => [
    for (final text in sent) jsonDecode(text) as Map<String, Object?>,
  ];

  List<Map<String, Object?>> get actions =>
      messages.where((message) => message['t'] == 'ui.action').toList();

  void open(
    String root,
    List<Map<String, Object?>> components, {
    Map<String, Object?> data = const {},
    String surface = 's1',
  }) {
    seq += 1;
    session.receiveJson({
      'v': 1,
      't': 'ui.open',
      'surface': surface,
      'seq': seq,
      'root': root,
      'components': components,
      'data': data,
    });
  }

  void patch(
    List<Map<String, Object?>> upsert, {
    List<String> remove = const [],
    String surface = 's1',
  }) {
    seq += 1;
    session.receiveJson({
      'v': 1,
      't': 'ui.patch',
      'surface': surface,
      'seq': seq,
      'upsert': upsert,
      'remove': remove,
    });
  }

  void data(List<Map<String, Object?>> ops, {String surface = 's1'}) {
    seq += 1;
    session.receiveJson({
      'v': 1,
      't': 'ui.data',
      'surface': surface,
      'seq': seq,
      'ops': ops,
    });
  }

  void confirm({
    required String actionId,
    required String nonce,
    String title = 'Pay Hududgaz',
    String body = "29 322 so'm from card 4471",
    String confirmLabel = 'Pay',
    int expiresMs = 60000,
    String surface = 's1',
  }) {
    session.receiveJson({
      'v': 1,
      't': 'ui.confirm',
      'surface': surface,
      'action_id': actionId,
      'nonce': nonce,
      'title': title,
      'body': body,
      'confirm_label': confirmLabel,
      'expires_ms': expiresMs,
    });
  }

  Widget app({
    String surface = 's1',
    Map<String, PolyuiComponentBuilder> builders = const {},
    ThemeData? theme,
    bool disableAnimations = false,
    double textScale = 1,
    double width = 400,
  }) => MaterialApp(
    theme: theme,
    home: Builder(
      builder: (context) => MediaQuery(
        data: MediaQuery.of(context).copyWith(
          disableAnimations: disableAnimations,
          textScaler: TextScaler.linear(textScale),
        ),
        child: Scaffold(
          body: SingleChildScrollView(
            padding: const EdgeInsets.all(16),
            child: Center(
              child: SizedBox(
                width: width,
                child: PolyuiSurface(
                  session: session,
                  surface: surface,
                  builders: builders,
                ),
              ),
            ),
          ),
        ),
      ),
    ),
  );

  void dispose() => session.dispose();
}

Map<String, Object?> component(
  String id,
  String type, {
  Map<String, Object?> props = const {},
  List<String>? children,
}) => {'id': id, 'type': type, 'props': props, 'children': ?children};

Map<String, Object?> handle(
  String id, {
  bool confirm = false,
  List<String>? collect,
}) => {'action_id': id, 'confirm': confirm, 'with': ?collect};
