import 'dart:async';
import 'dart:convert';

typedef Deliver = void Function(String message);

class ScriptedServer {
  ScriptedServer({
    required this.script,
    required this.deliver,
    bool instant = false,
  }) : gap = instant ? Duration.zero : const Duration(milliseconds: 60);

  final Map<String, Object?> script;
  final Deliver deliver;
  final Duration gap;
  bool opened = false;
  int nonceCounter = 0;

  String get question =>
      script['question'] as String? ?? 'What do we owe this month?';

  Future<void> receive(String text) async {
    final Object? message;
    try {
      message = jsonDecode(text);
    } on FormatException {
      return;
    }
    if (message is! Map<String, Object?>) {
      return;
    }
    switch (message['t']) {
      case 'hello':
        await send(script['welcome']);
      case 'user.say' when !opened:
        opened = true;
        for (final chunk in script['answer'] as List? ?? const []) {
          await send({'v': 1, 't': 'agent.delta', 'text': chunk});
        }
        await send(script['open']);
        await send({'v': 1, 't': 'agent.done'});
      case 'user.say':
        await say(
          'This demo only knows one conversation. '
          'Replay it from the menu to start again.',
        );
      case 'ui.action':
        await action(message);
    }
  }

  Future<void> action(Map<String, Object?> message) async {
    switch (message['action_id']) {
      case 'act_pay' when message['confirm'] == null:
        nonceCounter += 1;
        final values = message['values'] as Map<String, Object?>? ?? const {};
        final card = values['/payment/card'] as String? ?? 'humo';
        final cards = script['cards'] as Map<String, Object?>? ?? const {};
        final cardName = cards[card] as String? ?? 'your card';
        await send({
          'v': 1,
          't': 'ui.confirm',
          'surface': 's1',
          'action_id': 'act_pay',
          'nonce': 'n_flutter$nonceCounter',
          'title': 'Pay Hududiy elektr tarmoqlari',
          'body': "184 500 so'm from $cardName",
          'confirm_label': "Pay 184 500 so'm",
          'expires_ms': 60000,
        });
      case 'act_pay':
        for (final step in script['paid'] as List? ?? const []) {
          await send(step);
        }
        await say("Paid. 184 500 so'm went to Hududiy elektr tarmoqlari.");
      case 'act_month':
        final detail = message['detail'] as Map<String, Object?>?;
        final row = (detail?['row'] as num?)?.toInt();
        final months = (script['months'] as List? ?? const [])
            .cast<Map<String, Object?>>();
        if (row == null || row < 0 || row >= months.length) {
          return;
        }
        final month = months[row]['month'] as String;
        final kwh = (months[row]['kwh'] as num).toDouble();
        if (row == 0) {
          await say(
            '$month: ${kwh.round()} kWh, the first month on the chart.',
          );
          return;
        }
        final previousName = months[row - 1]['month'] as String;
        final previous = (months[row - 1]['kwh'] as num).toDouble();
        final change = ((kwh - previous) / previous * 100).round();
        final direction = change >= 0 ? 'up' : 'down';
        await say(
          '$month: ${kwh.round()} kWh, $direction ${change.abs()}% on $previousName.',
        );
    }
  }

  Future<void> say(String text) async {
    await send({'v': 1, 't': 'agent.delta', 'text': text});
    await send({'v': 1, 't': 'agent.done'});
  }

  Future<void> send(Object? message) async {
    if (message == null) {
      return;
    }
    if (gap > Duration.zero) {
      await Future<void>.delayed(gap);
    }
    deliver(jsonEncode(message));
  }
}
