import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:polyui/polyui.dart';

import 'support/harness.dart';

const acme = <String, Object?>{
  'id': 'acme',
  'version': 1,
  'types': {
    'Badge2': {
      'description': 'A status badge.',
      'role': 'status',
      'props': {
        'text': {'kind': 'string', 'required': true},
      },
      'alt': '{text}',
    },
  },
};

const tags = ['core@1', 'acme@1'];

void openBadge(Harness harness) => harness.open('card', [
  component('card', 'Card', props: {'title': 'Bill'}, children: ['badge']),
  component('badge', 'acme:Badge2', props: {'text': 'Paid'}),
]);

Widget badge(BuildContext context, PolyuiComponentContext component) =>
    Chip(label: Text('acme ${component.node.string('text')}'));

void main() {
  testWidgets('without its catalog an extension type falls back to Text', (
    tester,
  ) async {
    final harness = Harness(catalogs: tags);
    addTearDown(harness.dispose);
    openBadge(harness);
    expect(harness.session.surface('s1').node('badge')?.type, 'Text');
    await tester.pumpWidget(harness.app(builders: {'acme:Badge2': badge}));
    await tester.pumpAndSettle();
    expect(find.text('acme Paid'), findsNothing);
  });

  testWidgets('an extension type renders through a host builder', (
    tester,
  ) async {
    final harness = Harness(catalogs: tags, extensions: [acme]);
    addTearDown(harness.dispose);
    openBadge(harness);
    expect(harness.session.surface('s1').node('badge')?.type, 'acme:Badge2');
    await tester.pumpWidget(harness.app(builders: {'acme:Badge2': badge}));
    await tester.pumpAndSettle();
    expect(find.widgetWithText(Chip, 'acme Paid'), findsOneWidget);
    expect(
      harness.messages.where(
        (message) => message['reason'] == 'unknown_component',
      ),
      isEmpty,
    );
  });

  test('a rejected extension catalog is a clear error', () {
    expect(
      () => PolyuiCore.fromHello(
        const PolyuiHello(catalogs: tags, locale: 'en'),
        catalogs: const [
          {'id': 'core', 'version': 1, 'types': <String, Object?>{}},
        ],
      ),
      throwsA(
        isA<PolyuiException>().having(
          (error) => error.message,
          'message',
          contains('an extension catalog was rejected: [0].id'),
        ),
      ),
    );
  });
}
