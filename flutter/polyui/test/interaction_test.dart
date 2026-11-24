import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'dart:ui' show Tristate;
import 'package:flutter_test/flutter_test.dart';
import 'package:polyui/polyui.dart';

import 'support/harness.dart';

Map<String, Object?> conversation() =>
    jsonDecode(File('example/assets/conversation.json').readAsStringSync())
        as Map<String, Object?>;

void main() {
  late Harness harness;

  setUp(() => harness = Harness());
  tearDown(() => harness.dispose());

  group('Input focus', () {
    void openForm() {
      harness.open(
        'card',
        [
          component('card', 'Card', children: ['phone', 'echo']),
          component(
            'phone',
            'Input',
            props: {'label': 'Phone', 'value': '/form/phone', 'input': 'phone'},
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
    }

    EditableText editable(WidgetTester tester) =>
        tester.widget<EditableText>(find.byType(EditableText));

    testWidgets('typing keeps focus and caret through the echo', (
      tester,
    ) async {
      openForm();
      await tester.pumpWidget(harness.app());
      await tester.pumpAndSettle();
      await tester.tap(find.byType(TextField));
      await tester.pump();
      final focus = editable(tester).focusNode;
      expect(focus.hasFocus, isTrue);

      await tester.enterText(find.byType(TextField), '+99890');
      await tester.pump();
      expect(find.text('+99890'), findsNWidgets(2));
      expect(focus.hasFocus, isTrue);
      expect(editable(tester).focusNode, same(focus));
      expect(editable(tester).controller.selection.extentOffset, 6);
    });

    testWidgets('a server update of the same input keeps focus and caret', (
      tester,
    ) async {
      openForm();
      await tester.pumpWidget(harness.app());
      await tester.pumpAndSettle();
      await tester.tap(find.byType(TextField));
      await tester.enterText(find.byType(TextField), 'abc');
      await tester.pump();
      final focus = editable(tester).focusNode;
      final controller = editable(tester).controller;
      controller.selection = const TextSelection.collapsed(offset: 1);
      await tester.pump();

      harness.patch([
        component(
          'phone',
          'Input',
          props: {
            'label': 'Phone number',
            'value': '/form/phone',
            'input': 'phone',
          },
        ),
      ]);
      await tester.pump();
      expect(find.text('Phone number'), findsOneWidget);
      expect(editable(tester).focusNode, same(focus));
      expect(focus.hasFocus, isTrue);
      expect(editable(tester).controller.text, 'abc');
      expect(editable(tester).controller.selection.extentOffset, 1);

      harness.data([
        {'op': 'set', 'path': '/form/phone', 'value': 'abcdef'},
      ]);
      await tester.pump();
      expect(editable(tester).controller.text, 'abcdef');
      expect(editable(tester).controller.selection.extentOffset, 1);
      expect(focus.hasFocus, isTrue);
    });

    testWidgets('a late echo of an older value does not undo typing', (
      tester,
    ) async {
      openForm();
      await tester.pumpWidget(harness.app());
      await tester.pumpAndSettle();
      await tester.tap(find.byType(TextField));
      await tester.enterText(find.byType(TextField), 'a');
      await tester.enterText(find.byType(TextField), 'ab');
      await tester.pump();
      harness.data([
        {'op': 'set', 'path': '/form/phone', 'value': 'a'},
      ]);
      await tester.pump();
      expect(editable(tester).controller.text, 'ab');
    });

    testWidgets('the field exposes the a11y label', (tester) async {
      final semantics = tester.ensureSemantics();
      openForm();
      await tester.pumpWidget(harness.app());
      await tester.pumpAndSettle();
      expect(
        tester.getSemantics(find.byType(TextField)),
        matchesSemantics(
          label: 'Phone',
          isTextField: true,
          hasTapAction: true,
          hasFocusAction: true,
          isFocusable: true,
          hasEnabledState: true,
          isEnabled: true,
        ),
      );
      semantics.dispose();
    });
  });

  group('chart', () {
    void openBills() {
      final open = conversation()['open']! as Map<String, Object?>;
      harness.session.receiveJson(open);
      harness.seq = (open['seq']! as num).toInt();
    }

    testWidgets('tapping a bar fires on_select with the row', (tester) async {
      openBills();
      await tester.pumpWidget(harness.app(width: 380));
      await tester.pumpAndSettle();
      final node = harness.session.surface('s1').node('usage')!;
      final scene = node.chartScene!;
      final hit = scene.hits.firstWhere((hit) => hit.row == 1);
      final shape = hit.shape as ChartHitRect;
      final paint = find.descendant(
        of: find.byType(AspectRatio),
        matching: find.byType(CustomPaint),
      );
      final box = tester.getRect(paint.first);
      final scale = box.width / scene.viewBox.width;
      final target =
          box.topLeft + (shape.rect.center - scene.viewBox.topLeft) * scale;
      await tester.tapAt(target);
      await tester.pump();
      expect(harness.actions, hasLength(1));
      expect(harness.actions.single['action_id'], 'act_month');
      expect(harness.actions.single['detail'], {'row': 1});
    });

    testWidgets('each hit is a tappable semantics node', (tester) async {
      final semantics = tester.ensureSemantics();
      openBills();
      await tester.pumpWidget(harness.app(width: 380));
      await tester.pumpAndSettle();
      final scene = harness.session.surface('s1').node('usage')!.chartScene!;
      final hit = scene.hits.firstWhere((hit) => hit.row == 4);
      expect(find.semantics.byLabel(hit.label), findsOne);
      tester.semantics.tap(find.semantics.byLabel(hit.label));
      await tester.pump();
      expect(harness.actions.single['detail'], {'row': 4});
      final node = find.semantics.byLabel(hit.label).evaluate().single;
      expect(
        node.getSemanticsData().flagsCollection.isSelected,
        Tristate.isTrue,
      );
      semantics.dispose();
    });

    testWidgets('a chart without a scene says it is unavailable', (
      tester,
    ) async {
      harness.open('c', [
        component(
          'c',
          'chart:Chart',
          props: {
            'title': 'Spend',
            'kind': 'bar',
            'rows': [
              {'x': 'Jan', 'y': 1},
            ],
          },
        ),
      ]);
      await tester.pumpWidget(harness.app());
      await tester.pumpAndSettle();
      expect(find.text('Spend'), findsOneWidget);
      expect(find.text('Chart unavailable'), findsOneWidget);
    });

    testWidgets('Dashboard uses columns when wide and one when compact', (
      tester,
    ) async {
      final open = conversation()['open']! as Map<String, Object?>;
      final chart = (open['components']! as List)
          .cast<Map<String, Object?>>()
          .firstWhere((item) => item['id'] == 'usage');
      harness.open('board', [
        component(
          'board',
          'chart:Dashboard',
          props: {'title': 'This month', 'columns': 2},
          children: ['a', 'b', 'usage', 'note'],
        ),
        component('a', 'Stat', props: {'label': 'Gas', 'value': '1'}),
        component('b', 'Stat', props: {'label': 'Power', 'value': '2'}),
        chart,
        component('note', 'Text', props: {'text': 'Readings are monthly'}),
      ]);
      await tester.pumpWidget(harness.app(width: 760));
      await tester.pumpAndSettle();
      expect(find.text('This month'), findsOneWidget);
      final wideGas = tester.getTopLeft(find.text('Gas'));
      final widePower = tester.getTopLeft(find.text('Power'));
      expect(wideGas.dy, widePower.dy);
      final chartBox = tester.getRect(
        find
            .descendant(
              of: find.byType(AspectRatio),
              matching: find.byType(CustomPaint),
            )
            .first,
      );
      expect(chartBox.left, greaterThan(wideGas.dx + 100));

      await tester.pumpWidget(harness.app(width: 360));
      await tester.pumpAndSettle();
      final compactChart = tester.getRect(
        find
            .descendant(
              of: find.byType(AspectRatio),
              matching: find.byType(CustomPaint),
            )
            .first,
      );
      expect(
        compactChart.top,
        greaterThan(tester.getBottomLeft(find.text('Gas')).dy),
      );
    });
  });

  group('confirm op', () {
    void openPay() {
      harness.open('pay', [
        component(
          'pay',
          'Button',
          props: {
            'label': 'Pay',
            'tone': 'primary',
            'on_press': handle('act_pay', confirm: true),
          },
        ),
      ]);
    }

    Future<void> showSheet(WidgetTester tester) async {
      await tester.pump();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
    }

    FilledButton confirmButton(WidgetTester tester) => tester
        .widget<FilledButton>(find.byKey(const ValueKey('polyui-confirm')));

    testWidgets('opens a sheet outside the surface and sends the nonce', (
      tester,
    ) async {
      openPay();
      await tester.pumpWidget(harness.app());
      await tester.pumpAndSettle();
      await tester.tap(find.text('Pay'));
      await tester.pump();
      expect(harness.actions.single['action_id'], 'act_pay');

      harness.confirm(actionId: 'act_pay', nonce: 'n_77ad');
      await showSheet(tester);
      expect(find.text('Pay Hududgaz'), findsOneWidget);
      expect(find.text("29 322 so'm from card 4471"), findsOneWidget);
      expect(find.byType(BottomSheet), findsOneWidget);
      expect(
        find.descendant(
          of: find.byType(PolyuiSurface),
          matching: find.text('Pay Hududgaz'),
        ),
        findsNothing,
      );
      expect(confirmButton(tester).onPressed, isNull);

      await tester.pump(const Duration(milliseconds: 150));
      expect(confirmButton(tester).onPressed, isNotNull);
      await tester.tap(find.byKey(const ValueKey('polyui-confirm')));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));
      expect(find.byType(BottomSheet), findsNothing);
      expect(harness.actions, hasLength(2));
      expect(harness.actions.last['confirm'], 'n_77ad');
      expect(harness.session.surface('s1').pendingConfirm, isNull);
    });

    testWidgets('cancel sends nothing and clears the request', (tester) async {
      openPay();
      await tester.pumpWidget(harness.app());
      await tester.pumpAndSettle();
      harness.confirm(actionId: 'act_pay', nonce: 'n_1');
      await showSheet(tester);
      await tester.pump(const Duration(milliseconds: 200));
      await tester.tap(find.byKey(const ValueKey('polyui-cancel')));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));
      expect(find.byType(BottomSheet), findsNothing);
      expect(harness.actions, isEmpty);
      expect(harness.session.surface('s1').pendingConfirm, isNull);
    });

    testWidgets('an expired request cannot be confirmed', (tester) async {
      openPay();
      await tester.pumpWidget(harness.app());
      await tester.pumpAndSettle();
      harness.confirm(actionId: 'act_pay', nonce: 'n_2', expiresMs: 1000);
      await showSheet(tester);
      await tester.pump(const Duration(milliseconds: 200));
      expect(confirmButton(tester).onPressed, isNotNull);
      await tester.pump(const Duration(milliseconds: 600));
      expect(
        find.text('This request expired. Ask again to continue.'),
        findsOneWidget,
      );
      expect(confirmButton(tester).onPressed, isNull);
    });

    testWidgets('closing the surface removes the sheet', (tester) async {
      openPay();
      await tester.pumpWidget(harness.app());
      await tester.pumpAndSettle();
      harness.confirm(actionId: 'act_pay', nonce: 'n_3');
      await showSheet(tester);
      await tester.pump(const Duration(milliseconds: 200));
      expect(find.byType(BottomSheet), findsOneWidget);
      harness.seq += 1;
      harness.session.receiveJson({
        'v': 1,
        't': 'ui.close',
        'surface': 's1',
        'seq': harness.seq,
      });
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));
      expect(find.byType(BottomSheet), findsNothing);
      expect(harness.actions, isEmpty);
    });

    testWidgets('PolyuiTheme does not reach the sheet', (tester) async {
      openPay();
      const loud = Color(0xFFFF00FF);
      await tester.pumpWidget(
        PolyuiTheme(
          data: PolyuiThemeData.light.copyWith(
            accent: loud,
            raised: loud,
            background: loud,
          ),
          child: harness.app(),
        ),
      );
      await tester.pumpAndSettle();
      harness.confirm(actionId: 'act_pay', nonce: 'n_4');
      await showSheet(tester);
      await tester.pump(const Duration(milliseconds: 200));
      final materials = tester.widgetList<Material>(
        find.descendant(
          of: find.byType(BottomSheet),
          matching: find.byType(Material),
        ),
      );
      expect(materials, isNotEmpty);
      for (final material in materials) {
        expect(material.color, isNot(loud));
      }
      final texts = tester.widgetList<RichText>(
        find.descendant(
          of: find.byType(BottomSheet),
          matching: find.byType(RichText),
        ),
      );
      for (final text in texts) {
        expect(text.text.style?.color, isNot(loud));
      }
    });
  });
}
