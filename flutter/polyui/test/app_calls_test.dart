import 'dart:convert';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:polyui/polyui.dart';
import 'package:polyui/src/model/embed_bridge.dart';

import 'support/harness.dart';

const nonce = 'abcdefghijklmnopqrstuvwxyzABCDEF';

String welcome(List<String> features) => jsonEncode({
  'v': 1,
  't': 'welcome',
  'session': 'sess_1',
  'catalogs': ['core@1', 'embed@1'],
  'features': features,
});

String appEvent(Map<String, Object?> fields) =>
    jsonEncode({'kind': 'app-call', 'nonce': nonce, ...fields});

Map<String, Object?> appResult({
  String surface = 's1',
  String embed = 'forecast',
  String id = 'a1',
  Object? result = const {'content': <Object?>[]},
  Map<String, Object?>? error,
}) => {
  'v': 1,
  't': 'app.result',
  'surface': surface,
  'embed': embed,
  'id': id,
  if (error == null) 'result': result else 'error': error,
};

void main() {
  group('app-call events', () {
    test('a tools/call and a resources/read are read with their params', () {
      final call = EmbedEvent.parse(
        appEvent({
          'id': 'f00d-1',
          'method': 'tools/call',
          'params': {
            'name': 'forecast',
            'arguments': {'city': 'Samarkand'},
          },
        }),
        nonce,
      );
      expect(call, isA<EmbedAppCall>());
      call as EmbedAppCall;
      expect(call.id, 'f00d-1');
      expect(call.method, 'tools/call');
      expect(call.params, {
        'name': 'forecast',
        'arguments': {'city': 'Samarkand'},
      });

      final read = EmbedEvent.parse(
        appEvent({
          'id': 'f00d-2',
          'method': 'resources/read',
          'params': {'uri': 'ui://weather/forecast'},
        }),
        nonce,
      );
      expect(read, isA<EmbedAppCall>());
      expect((read as EmbedAppCall).method, 'resources/read');
    });

    test('an id of 64 characters is kept and one of 65 is dropped', () {
      final longest = 'x' * 64;
      final kept = EmbedEvent.parse(
        appEvent({'id': longest, 'method': 'tools/call', 'params': {}}),
        nonce,
      );
      expect((kept as EmbedAppCall).id, longest);
      expect(
        EmbedEvent.parse(
          appEvent({'id': 'x' * 65, 'method': 'tools/call', 'params': {}}),
          nonce,
        ),
        isNull,
      );
    });

    test('a bad id, method or params drops the event', () {
      for (final fields in <Map<String, Object?>>[
        {'id': '', 'method': 'tools/call', 'params': {}},
        {'id': 7, 'method': 'tools/call', 'params': {}},
        {'method': 'tools/call', 'params': {}},
        {'id': 'c-1', 'method': 'tools/list', 'params': {}},
        {'id': 'c-1', 'method': 'ui/update-model-context', 'params': {}},
        {'id': 'c-1', 'method': 'Tools/Call', 'params': {}},
        {'id': 'c-1', 'params': {}},
        {'id': 'c-1', 'method': 'tools/call'},
        {'id': 'c-1', 'method': 'tools/call', 'params': null},
        {'id': 'c-1', 'method': 'tools/call', 'params': []},
        {'id': 'c-1', 'method': 'tools/call', 'params': 'name'},
      ]) {
        expect(
          EmbedEvent.parse(appEvent(fields), nonce),
          isNull,
          reason: '$fields',
        );
      }
    });

    test('an app call without the page nonce is dropped', () {
      final fields = {
        'kind': 'app-call',
        'id': 'c-1',
        'method': 'tools/call',
        'params': {},
      };
      expect(EmbedEvent.parse(jsonEncode(fields), nonce), isNull);
      expect(
        EmbedEvent.parse(
          jsonEncode({...fields, 'nonce': '${nonce.substring(1)}x'}),
          nonce,
        ),
        isNull,
      );
      expect(
        EmbedEvent.parse(
          jsonEncode({...fields, 'nonce': nonce.substring(1)}),
          nonce,
        ),
        isNull,
      );
    });
  });

  group('session app calls', () {
    late Harness harness;

    setUp(() => harness = Harness());
    tearDown(() => harness.dispose());

    List<Map<String, Object?>> calls() => harness.messages
        .where((message) => message['t'] == 'app.call')
        .toList();

    test('app calls are on only after a welcome that lists them', () {
      expect(harness.session.appCalls, isFalse);
      harness.session.receive(welcome(['stream', 'app_calls']));
      expect(harness.session.appCalls, isTrue);
      harness.session.receive(welcome(['resume']));
      expect(harness.session.appCalls, isFalse);
      harness.session.receive(welcome(['app_calls']));
      harness.session.receiveJson({
        'v': 1,
        't': 'welcome',
        'session': 'sess_2',
        'catalogs': ['core@1'],
      });
      expect(harness.session.appCalls, isFalse);
      expect(PolyuiSession.appCallsFeature, 'app_calls');
    });

    test(
      'with app calls off a call is refused at once and nothing is sent',
      () {
        harness.session.receive(welcome(['resume']));
        final answers = <Map<String, Object?>>[];
        harness.session.callApp(
          surface: 's1',
          embed: 'forecast',
          method: 'tools/call',
          params: {'name': 'forecast'},
          done: answers.add,
        );
        expect(answers, [
          {
            'error': {'code': -32601, 'message': 'Method not found'},
          },
        ]);
        expect(calls(), isEmpty);
        expect(harness.session.openAppCalls, 0);
      },
    );

    test(
      'a call is sent as app.call and its result reaches only its waiter',
      () {
        harness.session.receive(welcome(['app_calls']));
        final first = <Map<String, Object?>>[];
        final second = <Map<String, Object?>>[];
        harness.session.callApp(
          surface: 's1',
          embed: 'forecast',
          method: 'tools/call',
          params: {
            'name': 'forecast',
            'arguments': {'city': 'Samarkand'},
          },
          done: first.add,
        );
        harness.session.callApp(
          surface: 's1',
          embed: 'tariffs',
          method: 'resources/read',
          params: {'uri': 'ui://bills/tariffs'},
          done: second.add,
        );
        expect(calls(), [
          {
            'v': 1,
            't': 'app.call',
            'surface': 's1',
            'embed': 'forecast',
            'id': 'a1',
            'method': 'tools/call',
            'params': {
              'name': 'forecast',
              'arguments': {'city': 'Samarkand'},
            },
          },
          {
            'v': 1,
            't': 'app.call',
            'surface': 's1',
            'embed': 'tariffs',
            'id': 'a2',
            'method': 'resources/read',
            'params': {'uri': 'ui://bills/tariffs'},
          },
        ]);
        expect(harness.session.openAppCalls, 2);

        final sentBefore = harness.sent.length;
        harness.session.receiveJson(appResult(embed: 'tariffs', id: 'a1'));
        harness.session.receiveJson(appResult(surface: 's2', id: 'a1'));
        harness.session.receiveJson(appResult(id: 'a9'));
        harness.session.receiveJson({
          'v': 1,
          't': 'app.result',
          'surface': 's1',
          'embed': 'forecast',
          'id': 1,
        });
        expect(first, isEmpty);
        expect(second, isEmpty);

        harness.session.receiveJson(
          appResult(
            result: {
              'content': [
                {'type': 'text', 'text': 'Sunny'},
              ],
            },
          ),
        );
        expect(first, [
          {
            'result': {
              'content': [
                {'type': 'text', 'text': 'Sunny'},
              ],
            },
          },
        ]);
        expect(second, isEmpty);

        harness.session.receiveJson(appResult());
        expect(first, hasLength(1));

        harness.session.receiveJson(
          appResult(
            embed: 'tariffs',
            id: 'a2',
            error: {'code': -32000, 'message': 'Refused by the host.'},
          ),
        );
        expect(second, [
          {
            'error': {'code': -32000, 'message': 'Refused by the host.'},
          },
        ]);
        expect(harness.session.openAppCalls, 0);
        expect(harness.sent.length, sentBefore);
      },
    );

    test('a malformed error is answered with the default code and text', () {
      harness.session.receive(welcome(['app_calls']));
      final answers = <Map<String, Object?>>[];
      harness.session.callApp(
        surface: 's1',
        embed: 'forecast',
        method: 'tools/call',
        params: {'name': 'forecast'},
        done: answers.add,
      );
      harness.session.receiveJson(appResult(error: {'code': 'bad'}));
      expect(answers, [
        {
          'error': {'code': -32000, 'message': 'The app request failed.'},
        },
      ]);
    });

    test('a cancelled call never answers', () {
      harness.session.receive(welcome(['app_calls']));
      final answers = <Map<String, Object?>>[];
      final cancel = harness.session.callApp(
        surface: 's1',
        embed: 'forecast',
        method: 'tools/call',
        params: {'name': 'forecast'},
        done: answers.add,
      );
      cancel();
      expect(harness.session.openAppCalls, 0);
      harness.session.receiveJson(appResult());
      expect(answers, isEmpty);
    });
  });

  group('session app call timeout', () {
    test('an unanswered call times out after 40 seconds', () {
      fakeAsync((async) {
        final harness = Harness();
        harness.session.receive(welcome(['app_calls']));
        final answers = <Map<String, Object?>>[];
        harness.session.callApp(
          surface: 's1',
          embed: 'forecast',
          method: 'tools/call',
          params: {'name': 'forecast'},
          done: answers.add,
        );
        async.elapse(const Duration(seconds: 39));
        expect(answers, isEmpty);
        async.elapse(const Duration(seconds: 1));
        expect(answers, [
          {
            'error': {'code': -32000, 'message': 'The app request timed out.'},
          },
        ]);
        expect(harness.session.openAppCalls, 0);
        harness.session.receiveJson(appResult());
        expect(answers, hasLength(1));
        harness.dispose();
      });
    });

    test('an answered or cancelled call does not time out later', () {
      fakeAsync((async) {
        final harness = Harness();
        harness.session.receive(welcome(['app_calls']));
        final answered = <Map<String, Object?>>[];
        final cancelled = <Map<String, Object?>>[];
        harness.session.callApp(
          surface: 's1',
          embed: 'forecast',
          method: 'tools/call',
          params: {'name': 'forecast'},
          done: answered.add,
        );
        final cancel = harness.session.callApp(
          surface: 's1',
          embed: 'forecast',
          method: 'tools/call',
          params: {'name': 'forecast'},
          done: cancelled.add,
        );
        harness.session.receiveJson(appResult());
        cancel();
        async.elapse(const Duration(minutes: 2));
        expect(answered, hasLength(1));
        expect(answered.single.containsKey('result'), isTrue);
        expect(cancelled, isEmpty);
        expect(async.pendingTimers, isEmpty);
        harness.dispose();
      });
    });

    test('disposing the session stops every waiting timer', () {
      fakeAsync((async) {
        final harness = Harness();
        harness.session.receive(welcome(['app_calls']));
        final answers = <Map<String, Object?>>[];
        harness.session.callApp(
          surface: 's1',
          embed: 'forecast',
          method: 'tools/call',
          params: {'name': 'forecast'},
          done: answers.add,
        );
        harness.dispose();
        async.elapse(const Duration(minutes: 1));
        expect(answers, isEmpty);
        expect(async.pendingTimers, isEmpty);
      });
    });
  });
}
