import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:polyui/polyui.dart';

import 'support/harness.dart';

class FakeOutput implements PolyuiSpeechOutput {
  final List<String> spoken = [];
  final List<String?> highlighted = [];
  PolyuiSession? watched;
  Completer<void>? hold;
  int stops = 0;
  bool broken = false;

  @override
  Future<void> speak(String text, {required String locale}) async {
    if (broken) {
      throw Exception('no voice');
    }
    spoken.add(text);
    highlighted.add(watched?.surface('s1').highlightedId);
    await hold?.future;
  }

  @override
  Future<void> stop() async {
    stops += 1;
    final pending = hold;
    hold = null;
    pending?.complete();
  }
}

class FakeInput implements PolyuiSpeechInput {
  FakeInput(this.answer);

  PolyuiListenStart answer;
  void Function(String transcript)? onFinal;
  void Function()? onDone;
  int stops = 0;

  @override
  Future<PolyuiListenStart> listen({
    required String locale,
    required void Function(String transcript) onFinal,
    required void Function() onDone,
  }) async {
    this.onFinal = onFinal;
    this.onDone = onDone;
    return answer;
  }

  @override
  Future<void> stop() async {
    stops += 1;
  }
}

void main() {
  late Harness harness;
  late FakeOutput output;
  late FakeInput input;
  late PolyuiVoice voice;
  late List<String> unmatched;
  late List<String> matched;

  setUp(() {
    harness = Harness();
    output = FakeOutput()..watched = harness.session;
    input = FakeInput(PolyuiListenStart.started);
    unmatched = [];
    matched = [];
    voice = PolyuiVoice(
      session: harness.session,
      output: output,
      input: input,
      locale: 'en',
      onUnmatched: unmatched.add,
      onMatched: (transcript, match) =>
          matched.add('$transcript -> ${match.component}'),
    );
    harness.open('card', [
      component(
        'card',
        'Card',
        props: {'title': 'Your bills'},
        children: ['choices', 'pay'],
      ),
      component(
        'choices',
        'ChoiceGroup',
        props: {
          'label': 'Pick one',
          'value': '/pick',
          'options': [
            {'id': 'gas', 'label': 'Gas'},
            {'id': 'power', 'label': 'Power'},
          ],
          'on_change': handle('act_pick'),
        },
      ),
      component(
        'pay',
        'Button',
        props: {
          'label': 'Pay now',
          'on_press': handle('act_pay', confirm: true, collect: ['/pick']),
        },
      ),
    ]);
  });

  tearDown(() {
    voice.dispose();
    harness.dispose();
  });

  test(
    'a final transcript goes through match_voice and its action is sent',
    () {
      final found = voice.heard('s1', '  the second one ');
      expect(found?.component, 'choices');
      expect(found?.option, 'power');
      final action = harness.actions.single;
      expect(action['action_id'], 'act_pick');
      expect(action['source'], 'voice');
      expect(action['values'], {'/pick': 'power'});
      expect(harness.session.surface('s1').node('choices')?.value, 'power');
      expect(unmatched, isEmpty);
      expect(matched, ['the second one -> choices']);
    },
  );

  test('words that match nothing go to the agent instead', () {
    expect(voice.heard('s1', 'what is the weather in Samarkand'), isNull);
    expect(unmatched, ['what is the weather in Samarkand']);
    expect(matched, isEmpty);
    expect(harness.actions, isEmpty);
    expect(voice.heard('s1', '   '), isNull);
    expect(unmatched, hasLength(1));
  });

  testWidgets('a spoken yes confirms the open sheet and closes it', (
    tester,
  ) async {
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    await tester.tap(find.text('Gas'));
    await tester.tap(find.text('Pay now'));
    await tester.pump();
    harness.confirm(actionId: 'act_pay', nonce: 'n_voice');
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('polyui-confirm')), findsOneWidget);

    final found = voice.heard('s1', 'yes please');
    await tester.pumpAndSettle();
    expect(found?.option, 'yes');
    final confirmed = harness.actions.last;
    expect(confirmed['action_id'], 'act_pay');
    expect(confirmed['confirm'], 'n_voice');
    expect(confirmed['source'], 'voice');
    expect(harness.session.surface('s1').pendingConfirm, isNull);
    expect(find.byKey(const ValueKey('polyui-confirm')), findsNothing);
  });

  testWidgets('a spoken no closes the sheet and sends nothing', (tester) async {
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    harness.confirm(actionId: 'act_pay', nonce: 'n_voice');
    await tester.pumpAndSettle();
    final before = harness.actions.length;
    expect(voice.heard('s1', 'no')?.option, 'no');
    await tester.pumpAndSettle();
    expect(harness.actions, hasLength(before));
    expect(find.byKey(const ValueKey('polyui-confirm')), findsNothing);
  });

  test('listening hands the final transcript to match_voice', () async {
    expect(await voice.listen('s1'), PolyuiListenStart.started);
    expect(voice.state, PolyuiVoiceState.listening);
    input.onFinal!('first');
    expect(harness.actions.single['values'], {'/pick': 'gas'});
    input.onDone!();
    expect(voice.state, PolyuiVoiceState.idle);
  });

  test('a denied microphone leaves touch working and says why', () async {
    input.answer = PolyuiListenStart.denied;
    expect(await voice.listen('s1'), PolyuiListenStart.denied);
    expect(voice.state, PolyuiVoiceState.idle);
    expect(voice.problem, PolyuiVoiceProblem.microphoneDenied);

    input.answer = PolyuiListenStart.unavailable;
    await voice.listen('s1');
    expect(voice.problem, PolyuiVoiceProblem.recognitionUnavailable);
    voice.clearProblem();
    expect(voice.problem, isNull);
  });

  test('say reads the surface in order and highlights each part', () async {
    await voice.speak('s1');
    expect(output.spoken, [
      'Your bills.',
      'Pick one. First, Gas; second, Power.',
      'Pay now.',
    ]);
    expect(output.highlighted, ['card', 'choices', 'pay']);
    expect(harness.session.surface('s1').highlightedId, isNull);
    expect(voice.state, PolyuiVoiceState.idle);
  });

  test('listening while speaking stops the speech first', () async {
    output.hold = Completer<void>();
    final speaking = voice.speak('s1');
    await Future<void>.delayed(Duration.zero);
    expect(voice.state, PolyuiVoiceState.speaking);
    await voice.listen('s1');
    await speaking;
    expect(output.stops, 1);
    expect(output.spoken, ['Your bills.']);
    expect(voice.state, PolyuiVoiceState.listening);
    expect(harness.session.surface('s1').highlightedId, isNull);
  });

  test('a broken speech engine is reported and the screen stays', () async {
    output.broken = true;
    await voice.speakText('Paid.');
    expect(voice.problem, PolyuiVoiceProblem.speechFailed);
    expect(voice.state, PolyuiVoiceState.idle);
  });
}
