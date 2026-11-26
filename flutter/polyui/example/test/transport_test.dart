import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:polyui_example/src/transport.dart';

void main() {
  const hello =
      '{"v":1,"t":"hello","catalogs":["core@1"],"modalities":["screen"],"features":["stream"]}';

  test('the first hello declares resume without a token', () {
    final sent =
        jsonDecode(LiveTransport.helloWith(hello, null, null))
            as Map<String, Object?>;
    expect(sent['features'], ['stream', 'resume']);
    expect(sent.containsKey('resume'), isFalse);
  });

  test('a reconnect carries the session and token', () {
    final sent =
        jsonDecode(LiveTransport.helloWith(hello, 'sess_1', 'rt_2'))
            as Map<String, Object?>;
    expect(sent['resume'], {'session': 'sess_1', 'token': 'rt_2'});
    expect(sent['features'], ['stream', 'resume']);
  });

  test('only hello messages are recognized', () {
    expect(LiveTransport.isHello(hello), isTrue);
    expect(LiveTransport.isHello('{"t":"user.say","text":"hello"}'), isFalse);
    expect(LiveTransport.isHello('not json'), isFalse);
    expect(LiveTransport.helloWith('[1]', null, null), '[1]');
  });
}
