import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:polyui/polyui.dart';

final Directory fixtures = Directory('../../tests/conformance/client');

List<File> fixtureFiles() =>
    fixtures
        .listSync()
        .whereType<File>()
        .where((file) => file.path.endsWith('.json'))
        .toList()
      ..sort((left, right) => left.path.compareTo(right.path));

String fieldOf(Map<String, Object?> call, String key) =>
    call[key] as String? ?? '';

String runStep(PolyuiCore core, Map<String, Object?> step) {
  if (step.containsKey('receive')) {
    return core.receiveJson(jsonEncode(step['receive']));
  }
  if (step['receive_text'] case final String raw) {
    return core.receiveJson(raw);
  }
  if (step['set_value'] case final Map<String, Object?> call) {
    return core.setValueJson(
      surface: fieldOf(call, 'surface'),
      path: fieldOf(call, 'path'),
      valueJson: jsonEncode(call['value']),
    );
  }
  if (step['fire'] case final Map<String, Object?> call) {
    return core.fireJson(
      surface: fieldOf(call, 'surface'),
      component: fieldOf(call, 'component'),
      prop: fieldOf(call, 'prop'),
      detailJson: call.containsKey('detail')
          ? jsonEncode(call['detail'])
          : null,
    );
  }
  if (step['confirm'] case final Map<String, Object?> call) {
    return core.confirmJson(
      surface: fieldOf(call, 'surface'),
      actionId: fieldOf(call, 'action_id'),
      nonce: fieldOf(call, 'nonce'),
    );
  }
  if (step['say'] case final String surface) {
    return core.sayJson(surface);
  }
  if (step['match_voice'] case final Map<String, Object?> call) {
    return core.matchVoiceJson(
      surface: fieldOf(call, 'surface'),
      text: fieldOf(call, 'text'),
    );
  }
  throw StateError('unknown step: ${step.keys}');
}

void checkDecoded(Map<String, Object?> step, String raw, Object? expected) {
  final rendered =
      step.containsKey('receive') ||
      step.containsKey('receive_text') ||
      step.containsKey('set_value');
  if (rendered) {
    final actual = RenderBatch.fromJson(jsonDecode(raw));
    expect(actual, RenderBatch.fromJson(expected));
    expect(actual.ops.whereType<UnknownOp>(), isEmpty);
  } else if (step.containsKey('say')) {
    final actual = (jsonDecode(raw) as List).map(Utterance.fromJson).toList();
    expect(actual, (expected as List).map(Utterance.fromJson).toList());
  } else if (step.containsKey('match_voice') && expected != null) {
    expect(VoiceMatch.fromJson(jsonDecode(raw)), VoiceMatch.fromJson(expected));
  }
}

void main() {
  test('the fixtures are reachable', () {
    expect(fixtureFiles().length, greaterThanOrEqualTo(10));
  });

  for (final file in fixtureFiles()) {
    final name = file.uri.pathSegments.last;
    test('$name replays through the Rust core', () {
      final fixture =
          jsonDecode(file.readAsStringSync()) as Map<String, Object?>;
      final core = PolyuiCore(jsonEncode(fixture['hello']));
      addTearDown(core.dispose);
      final steps = (fixture['steps'] as List).cast<Map<String, Object?>>();
      for (final (index, step) in steps.indexed) {
        final expected = step['expect'];
        final raw = runStep(core, step);
        expect(
          jsonDecode(raw),
          expected,
          reason: '$name step ${index + 1}: $raw',
        );
        checkDecoded(step, raw, expected);
      }
    });
  }
}
