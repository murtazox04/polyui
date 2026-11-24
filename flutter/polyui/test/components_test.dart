import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:polyui/polyui.dart';

import 'support/harness.dart';

void main() {
  late Harness harness;

  setUp(() => harness = Harness());
  tearDown(() => harness.dispose());

  testWidgets('Text draws each style and marks titles as headings', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    harness.open('card', [
      component('card', 'Card', children: ['title', 'body', 'caption']),
      component(
        'title',
        'Text',
        props: {'text': 'Your bills', 'style': 'title'},
      ),
      component('body', 'Text', props: {'text': 'Two bills are due.'}),
      component(
        'caption',
        'Text',
        props: {'text': 'Updated now', 'style': 'caption'},
      ),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();

    expect(find.text('Your bills'), findsOneWidget);
    expect(find.text('Two bills are due.'), findsOneWidget);
    expect(
      tester.getSemantics(find.text('Your bills')),
      matchesSemantics(label: 'Your bills', isHeader: true),
    );
    expect(
      tester
          .getSemantics(find.text('Your bills'))
          .getSemanticsData()
          .headingLevel,
      3,
    );
    final caption = tester.widget<Text>(find.text('Updated now'));
    expect(caption.style?.fontSize, 12);
    semantics.dispose();
  });

  testWidgets('Card exposes its a11y label and nests with the sunken style', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    harness.open('outer', [
      component(
        'outer',
        'Card',
        props: {'title': 'Electricity', 'subtitle': 'Due 25 November'},
        children: ['inner'],
      ),
      component(
        'inner',
        'Card',
        props: {'title': 'Details', 'tone': 'warning'},
        children: ['note'],
      ),
      component('note', 'Text', props: {'text': 'Meter read on the 20th'}),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();

    expect(find.text('Electricity'), findsOneWidget);
    expect(find.text('Due 25 November'), findsOneWidget);
    expect(find.bySemanticsLabel(RegExp('^Electricity')), findsWidgets);
    final boxes = tester
        .widgetList<DecoratedBox>(
          find.ancestor(
            of: find.text('Details'),
            matching: find.byType(DecoratedBox),
          ),
        )
        .map((box) => (box.decoration as BoxDecoration).color)
        .toList();
    expect(boxes.first, PolyuiThemeData.light.sunken);
    semantics.dispose();
  });

  testWidgets('Row puts stats side by side and stacks them when narrow', (
    tester,
  ) async {
    harness.open('row', [
      component('row', 'Row', props: {'gap': 'large'}, children: ['a', 'b']),
      component(
        'a',
        'Stat',
        props: {'label': 'Due now', 'value': '184 500', 'unit': "so'm"},
      ),
      component('b', 'Stat', props: {'label': 'Used', 'value': '205'}),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    final wide = [
      tester.getTopLeft(find.text('Due now')),
      tester.getTopLeft(find.text('Used')),
    ];
    expect(wide[0].dy, wide[1].dy);
    expect(wide[1].dx, greaterThan(wide[0].dx));

    await tester.pumpWidget(harness.app(width: 200));
    await tester.pumpAndSettle();
    final narrow = [
      tester.getTopLeft(find.text('Due now')),
      tester.getTopLeft(find.text('Used')),
    ];
    expect(narrow[1].dy, greaterThan(narrow[0].dy));
  });

  testWidgets('Row wraps other children and large text stacks stats', (
    tester,
  ) async {
    harness.open('root', [
      component('root', 'Card', children: ['badges', 'stats']),
      component('badges', 'Row', children: ['one', 'two']),
      component('one', 'Badge', props: {'text': 'Paid', 'tone': 'success'}),
      component('two', 'Badge', props: {'text': 'Late', 'tone': 'danger'}),
      component('stats', 'Row', children: ['a', 'b']),
      component('a', 'Stat', props: {'label': 'Gas', 'value': '1'}),
      component('b', 'Stat', props: {'label': 'Power', 'value': '2'}),
    ]);
    await tester.pumpWidget(harness.app(textScale: 2.5));
    await tester.pumpAndSettle();
    expect(
      tester.getTopLeft(find.text('Late')).dy,
      tester.getTopLeft(find.text('Paid')).dy,
    );
    expect(
      tester.getTopLeft(find.text('Power')).dy,
      greaterThan(tester.getTopLeft(find.text('Gas')).dy),
    );
  });

  testWidgets('List numbers ordered items and has list semantics', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    harness.open('list', [
      component(
        'list',
        'List',
        props: {'title': 'Steps', 'ordered': true},
        children: ['one', 'two'],
      ),
      component('one', 'Text', props: {'text': 'Open the app'}),
      component('two', 'Text', props: {'text': 'Tap pay'}),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    expect(find.text('1.'), findsOneWidget);
    expect(find.text('2.'), findsOneWidget);
    expect(find.text('Steps'), findsOneWidget);
    final list = find.byWidgetPredicate(
      (widget) =>
          widget is Semantics && widget.properties.role == SemanticsRole.list,
    );
    expect(list, findsOneWidget);
    semantics.dispose();
  });

  testWidgets('Stat shows value, unit, hint and a status label', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    harness.open('due', [
      component(
        'due',
        'Stat',
        props: {
          'label': 'Due now',
          'value': '184 500',
          'unit': "so'm",
          'tone': 'negative',
          'hint': "900 so'm per kWh",
        },
      ),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    expect(find.text('184 500'), findsOneWidget);
    expect(find.text("so'm"), findsOneWidget);
    expect(find.text("900 so'm per kWh"), findsOneWidget);
    final value = tester.widget<Text>(find.text('184 500'));
    expect(value.style?.color, PolyuiThemeData.light.negative);
    expect(find.bySemanticsLabel("Due now: 184 500 so'm"), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('Badge uses its tone color', (tester) async {
    harness.open('badge', [
      component('badge', 'Badge', props: {'text': 'Paid', 'tone': 'success'}),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    final text = tester.widget<Text>(find.text('Paid'));
    expect(text.style?.color, PolyuiThemeData.light.positive);
  });

  testWidgets('Image loads https URLs and shows the alt text otherwise', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    harness.open('card', [
      component('card', 'Card', children: ['good', 'plain']),
      component(
        'good',
        'Image',
        props: {
          'src': 'https://example.com/meter.png',
          'alt': 'Meter photo',
          'aspect': 'wide',
        },
      ),
      component(
        'plain',
        'Image',
        props: {'src': 'https://example.com/b.png', 'alt': 'Bill scan'},
      ),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pump();
    final images = tester.widgetList<Image>(find.byType(Image)).toList();
    expect(images, hasLength(2));
    expect(
      (images.first.image as NetworkImage).url,
      'https://example.com/meter.png',
    );
    expect(find.bySemanticsLabel('Meter photo'), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('Image never loads a URL that fails the URL check', (
    tester,
  ) async {
    final open = Harness(allowHttp: true);
    addTearDown(open.dispose);
    open.open('card', [
      component('card', 'Card', children: ['userinfo']),
      component(
        'userinfo',
        'Image',
        props: {
          'src': 'https://user:secret@example.com/a.png',
          'alt': 'Hidden',
        },
      ),
    ]);
    await tester.pumpWidget(open.app());
    await tester.pump();
    expect(find.byType(Image), findsNothing);
    expect(find.text('[image: Hidden]'), findsOneWidget);

    expect(checkSafeUrl('https://example.com/a.png'), isNotNull);
    expect(checkSafeUrl('http://example.com/a.png'), isNull);
    expect(
      checkSafeUrl('http://example.com/a.png', allowHttp: true),
      isNotNull,
    );
    expect(checkSafeUrl('javascript:alert(1)'), isNull);
    expect(checkSafeUrl('data:image/png;base64,AAAA'), isNull);
    expect(checkSafeUrl('https://example.com/a b.png'), isNull);
    expect(checkSafeUrl('https://example.com/${'a' * 4100}'), isNull);
  });

  testWidgets('Button fires its handle and respects disabled', (tester) async {
    harness.open('row', [
      component('row', 'Row', children: ['pay', 'later']),
      component(
        'pay',
        'Button',
        props: {
          'label': 'Pay',
          'tone': 'primary',
          'on_press': handle('act_pay'),
        },
      ),
      component(
        'later',
        'Button',
        props: {
          'label': 'Later',
          'disabled': true,
          'on_press': handle('act_later'),
        },
      ),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    await tester.tap(find.text('Pay'));
    await tester.tap(find.text('Later'), warnIfMissed: false);
    await tester.pump();
    expect(harness.actions, hasLength(1));
    expect(harness.actions.single['action_id'], 'act_pay');
    expect(harness.actions.single['source'], 'tap');
  });

  testWidgets('ChoiceGroup writes the data model and fires on_change', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    harness.open(
      'card',
      [
        component('card', 'Card', children: ['source', 'echo']),
        component(
          'source',
          'ChoiceGroup',
          props: {
            'label': 'Pay from',
            'value': '/payment/card',
            'options': [
              {'id': 'humo', 'label': 'Humo card', 'detail': 'Family card'},
              {'id': 'visa', 'label': 'Visa card'},
            ],
            'on_change': handle('act_card', collect: ['/payment/card']),
          },
        ),
        component(
          'echo',
          'Text',
          props: {
            'text': {r'$bind': '/payment/card'},
          },
        ),
      ],
      data: {
        'payment': {'card': 'humo'},
      },
    );
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    expect(find.text('humo'), findsOneWidget);
    expect(
      tester.getSemantics(find.text('Humo card')),
      matchesSemantics(
        label: 'Humo card, Family card',
        hasCheckedState: true,
        isChecked: true,
        isInMutuallyExclusiveGroup: true,
        hasTapAction: true,
      ),
    );

    await tester.tap(find.text('Visa card'));
    await tester.pumpAndSettle();
    expect(find.text('visa'), findsOneWidget);
    expect(harness.actions.single['action_id'], 'act_card');
    expect(harness.actions.single['values'], {'/payment/card': 'visa'});
    semantics.dispose();
  });

  testWidgets('ChoiceGroup with multiple keeps option order', (tester) async {
    harness.open('pick', [
      component(
        'pick',
        'ChoiceGroup',
        props: {
          'value': '/tags',
          'multiple': true,
          'options': [
            {'id': 'a', 'label': 'Alpha'},
            {'id': 'b', 'label': 'Beta'},
          ],
        },
      ),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    await tester.tap(find.text('Beta'));
    await tester.pump();
    await tester.tap(find.text('Alpha'));
    await tester.pump();
    expect(harness.session.surface('s1').node('pick')!.value, ['a', 'b']);
    expect(find.byIcon(Icons.check_box), findsNWidgets(2));
  });

  testWidgets('Input writes through set_value and caps max_length', (
    tester,
  ) async {
    harness.open(
      'card',
      [
        component('card', 'Card', children: ['phone', 'echo']),
        component(
          'phone',
          'Input',
          props: {
            'label': 'Phone',
            'value': '/form/phone',
            'input': 'phone',
            'max_length': 6,
          },
        ),
        component(
          'echo',
          'Text',
          props: {
            'text': {r'$bind': '/form/phone'},
          },
        ),
      ],
      data: {
        'form': {'phone': ''},
      },
    );
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), '+99890123');
    await tester.pump();
    expect(find.text('+99890'), findsNWidgets(2));
    final field = tester.widget<TextField>(find.byType(TextField));
    expect(field.keyboardType, TextInputType.phone);
  });

  testWidgets('ConfirmSheet component answers once', (tester) async {
    harness.open('ask', [
      component(
        'ask',
        'ConfirmSheet',
        props: {
          'title': 'Remove the card?',
          'body': 'You can add it again later.',
          'confirm_label': 'Remove',
          'on_confirm': handle('act_yes'),
          'on_cancel': handle('act_no'),
        },
      ),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    expect(find.text('Remove the card?'), findsOneWidget);
    expect(find.text('No'), findsOneWidget);
    await tester.tap(find.text('Remove'));
    await tester.pump();
    await tester.tap(find.text('No'), warnIfMissed: false);
    await tester.pump();
    expect(harness.actions.map((action) => action['action_id']), ['act_yes']);
  });

  testWidgets('Divider draws a line without semantics', (tester) async {
    final semantics = tester.ensureSemantics();
    harness.open('card', [
      component('card', 'Card', children: ['a', 'line', 'b']),
      component('a', 'Text', props: {'text': 'Above'}),
      component('line', 'Divider'),
      component('b', 'Text', props: {'text': 'Below'}),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    expect(find.byType(ExcludeSemantics), findsWidgets);
    expect(
      tester.getTopLeft(find.text('Below')).dy -
          tester.getBottomLeft(find.text('Above')).dy,
      greaterThan(1),
    );
    semantics.dispose();
  });

  testWidgets('Embed shows its title only', (tester) async {
    harness.open('app', [
      component(
        'app',
        'embed:Embed',
        props: {'resource': 'ui://maps/route', 'title': 'Route to the office'},
      ),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    expect(find.text('Route to the office'), findsOneWidget);
    expect(find.byIcon(Icons.web_asset_outlined), findsOneWidget);
  });

  testWidgets('a missing required prop arrives as muted text', (tester) async {
    harness.open('card', [
      component('card', 'Card', children: ['broken']),
      {
        'id': 'broken',
        'type': 'Stat',
        'props': {
          'label': 'Balance',
          'value': {r'$bind': '/missing'},
        },
        'alt': 'Balance unavailable',
      },
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    final text = tester.widget<Text>(find.text('Balance unavailable'));
    expect(text.style?.color, PolyuiThemeData.light.muted);
  });

  testWidgets('custom builders replace a type and can fire', (tester) async {
    harness.open('badge', [
      component(
        'badge',
        'Button',
        props: {'label': 'Open map', 'on_press': handle('act_map')},
      ),
    ]);
    await tester.pumpWidget(
      harness.app(
        builders: {
          'Button': (context, component) => TextButton(
            onPressed: () => component.fire('on_press', detail: {'pin': 3}),
            child: Text('custom ${component.node.string('label')}'),
          ),
        },
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('custom Open map'));
    await tester.pump();
    expect(harness.actions.single['detail'], {'pin': 3});
  });

  testWidgets('PolyuiTheme and the theme extension restyle components', (
    tester,
  ) async {
    harness.open('badge', [
      component('badge', 'Badge', props: {'text': 'Paid', 'tone': 'success'}),
    ]);
    const custom = Color(0xFF3355FF);
    await tester.pumpWidget(
      harness.app(
        theme: ThemeData(
          extensions: [PolyuiThemeData.light.copyWith(positive: custom)],
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(tester.widget<Text>(find.text('Paid')).style?.color, custom);

    await tester.pumpWidget(harness.app(theme: ThemeData.dark()));
    await tester.pumpAndSettle();
    expect(
      tester.widget<Text>(find.text('Paid')).style?.color,
      PolyuiThemeData.dark.positive,
    );
  });

  testWidgets('highlight ops outline one component at a time', (tester) async {
    harness.open('card', [
      component('card', 'Card', children: ['a', 'b']),
      component('a', 'Text', props: {'text': 'First'}),
      component('b', 'Text', props: {'text': 'Second'}),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    harness.session.highlight(surface: 's1', id: 'a');
    await tester.pumpAndSettle();
    expect(harness.session.surface('s1').node('a')!.highlighted, isTrue);
    harness.session.receiveJson({
      'v': 1,
      't': 'ui.sync',
      'surface': 's1',
      'highlight': 'b',
      'at_ms': 10,
    });
    await tester.pumpAndSettle();
    final surface = harness.session.surface('s1');
    expect(surface.node('a')!.highlighted, isFalse);
    expect(surface.node('b')!.highlighted, isTrue);
    expect(surface.highlightedId, 'b');
  });

  testWidgets('reduced motion skips the appear animation', (tester) async {
    harness.open('t', [
      component('t', 'Text', props: {'text': 'Instant'}),
    ]);
    await tester.pumpWidget(harness.app(disableAnimations: true));
    await tester.pump();
    final fade = tester.widget<FadeTransition>(
      find
          .ancestor(
            of: find.text('Instant'),
            matching: find.byType(FadeTransition),
          )
          .first,
    );
    expect(fade.opacity.value, 1);
    expect(tester.binding.hasScheduledFrame, isFalse);
  });

  testWidgets('patches replace whole components and close clears', (
    tester,
  ) async {
    harness.open('card', [
      component('card', 'Card', children: ['pay']),
      component(
        'pay',
        'Button',
        props: {'label': 'Pay', 'on_press': handle('act_pay')},
      ),
    ]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    harness.patch([
      component('pay', 'Badge', props: {'text': 'Paid', 'tone': 'success'}),
    ]);
    await tester.pumpAndSettle();
    expect(find.text('Pay'), findsNothing);
    expect(find.text('Paid'), findsOneWidget);

    harness.seq += 1;
    harness.session.receiveJson({
      'v': 1,
      't': 'ui.close',
      'surface': 's1',
      'seq': harness.seq,
    });
    await tester.pumpAndSettle();
    expect(find.text('Paid'), findsNothing);
    expect(harness.session.surface('s1').isClosed, isTrue);
  });
}
