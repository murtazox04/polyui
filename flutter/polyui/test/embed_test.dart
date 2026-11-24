import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:url_launcher_platform_interface/url_launcher_platform_interface.dart';
import 'package:webview_flutter_platform_interface/webview_flutter_platform_interface.dart';

import 'support/fake_platforms.dart';
import 'support/harness.dart';

const uri = 'ui://weather/forecast';

Map<String, Object?> app({String html = '<!doctype html><p>Mild days</p>'}) => {
  'resource': {
    'uri': uri,
    'mimeType': 'text/html;profile=mcp-app',
    'text': html,
  },
  'tool_input': {'city': 'Tashkent'},
  'tool_result': {
    'content': [
      {'type': 'text', 'text': 'Mild days.'},
    ],
  },
};

Map<String, Object?> embed({
  String height = 'small',
  bool withApp = true,
  bool withMessage = true,
  String html = '<!doctype html><p>Mild days</p>',
  String title = 'Five day forecast for Tashkent',
}) => component(
  'forecast',
  'embed:Embed',
  props: {
    'resource': uri,
    'title': title,
    'height': height,
    if (withMessage) 'on_message': handle('act_ask'),
    if (withApp) 'app': app(html: html),
  },
);

void main() {
  late Harness harness;
  late FakeWebViewPlatform web;
  late FakeUrlLauncher launcher;

  setUp(() {
    harness = Harness();
    web = FakeWebViewPlatform();
    WebViewPlatform.instance = web;
    launcher = FakeUrlLauncher();
    UrlLauncherPlatform.instance = launcher;
  });

  tearDown(() => harness.dispose());

  Future<void> show(WidgetTester tester, Map<String, Object?> component) async {
    harness.open('forecast', [component]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
  }

  double frameHeight(WidgetTester tester) =>
      tester.getSize(find.byKey(const ValueKey('fake-web-view'))).height;

  testWidgets('an embed without an app shows its title and no web view', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await show(tester, embed(withApp: false));
    expect(find.text('Five day forecast for Tashkent'), findsOneWidget);
    expect(find.byKey(const ValueKey('fake-web-view')), findsNothing);
    expect(web.controllers, isEmpty);
    expect(
      find.bySemanticsLabel('Five day forecast for Tashkent'),
      findsOneWidget,
    );
    semantics.dispose();
  });

  testWidgets('an embed with an app loads the native page with a fresh nonce', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await show(tester, embed());
    final page = web.last;
    expect(page.mode, JavaScriptMode.unrestricted);
    expect(page.baseUrls, ['https://embed.polyui.invalid/']);
    expect(page.channel?.name, 'polyui');
    expect(page.nonce, matches(RegExp(r'^[A-Za-z0-9]{32}$')));
    expect(page.pages.single, contains('Mild days'));
    expect(frameHeight(tester), 160);
    expect(
      find.bySemanticsLabel('Five day forecast for Tashkent'),
      findsOneWidget,
    );
    semantics.dispose();

    harness.open('forecast', [embed()]);
    await tester.pumpAndSettle();
    expect(web.controllers, hasLength(1));
  });

  testWidgets('each embed page gets its own nonce', (tester) async {
    harness.open('forecast', [embed()]);
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    final first = web.last.nonce;
    await tester.pumpWidget(const SizedBox());
    await tester.pumpWidget(harness.app());
    await tester.pumpAndSettle();
    expect(web.controllers, hasLength(2));
    expect(web.last.nonce, isNot(first));
  });

  testWidgets('events without the right nonce are ignored', (tester) async {
    await show(tester, embed());
    final page = web.last;
    final nonce = page.nonce;
    final forged = '${nonce.substring(1)}x';

    page.post({'kind': 'height', 'value': 300});
    page.post({'kind': 'height', 'value': 300, 'nonce': forged});
    page.post({'kind': 'height', 'value': 300, 'nonce': nonce.substring(1)});
    page.post({'kind': 'message', 'text': 'spoofed', 'nonce': forged});
    page.post({'kind': 'unavailable', 'nonce': forged});
    page.post({
      'kind': 'open-link',
      'url': 'https://example.com/',
      'nonce': '',
    });
    page.postText('not json');
    page.postText('["kind", "message"]');
    await tester.pumpAndSettle();

    expect(frameHeight(tester), 160);
    expect(harness.actions, isEmpty);
    expect(find.byKey(const ValueKey('fake-web-view')), findsOneWidget);
    expect(find.text('Open this link?'), findsNothing);

    page.post({'kind': 'height', 'value': 300, 'nonce': nonce});
    await tester.pumpAndSettle();
    expect(frameHeight(tester), 300);
  });

  testWidgets('height is clamped to 80 and the maximum of the hint', (
    tester,
  ) async {
    await show(tester, embed(height: 'medium'));
    final page = web.last;
    expect(frameHeight(tester), 320);

    page.post({'kind': 'height', 'value': 12, 'nonce': page.nonce});
    await tester.pumpAndSettle();
    expect(frameHeight(tester), 80);

    page.post({'kind': 'height', 'value': 5000, 'nonce': page.nonce});
    await tester.pumpAndSettle();
    expect(frameHeight(tester), 640);

    page.post({'kind': 'height', 'value': 'tall', 'nonce': page.nonce});
    await tester.pumpAndSettle();
    expect(frameHeight(tester), 640);
  });

  testWidgets('large starts at 480 and stops at 960', (tester) async {
    await show(tester, embed(height: 'large'));
    final page = web.last;
    expect(frameHeight(tester), 480);
    page.post({'kind': 'height', 'value': 99999, 'nonce': page.nonce});
    await tester.pumpAndSettle();
    expect(frameHeight(tester), 960);
  });

  testWidgets('unavailable from the page falls back to the title', (
    tester,
  ) async {
    await show(tester, embed());
    final page = web.last;
    page.post({'kind': 'unavailable', 'nonce': page.nonce});
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('fake-web-view')), findsNothing);
    expect(find.text('Five day forecast for Tashkent'), findsOneWidget);
  });

  testWidgets('a message fires on_message with the text', (tester) async {
    await show(tester, embed());
    final page = web.last;
    page.post({
      'kind': 'message',
      'text': 'Also show the forecast for Samarkand.',
      'nonce': page.nonce,
    });
    await tester.pumpAndSettle();
    final action = harness.actions.single;
    expect(action['action_id'], 'act_ask');
    expect(action['detail'], {'text': 'Also show the forecast for Samarkand.'});

    page.post({'kind': 'message', 'text': 'again', 'nonce': page.nonce});
    await tester.pumpAndSettle();
    expect(harness.actions, hasLength(1));
  });

  testWidgets('without on_message a message sends nothing', (tester) async {
    await show(tester, embed(withMessage: false));
    final page = web.last;
    page.post({'kind': 'message', 'text': 'hello', 'nonce': page.nonce});
    await tester.pumpAndSettle();
    expect(harness.actions, isEmpty);
  });

  testWidgets('only the first main frame load is allowed', (tester) async {
    await show(tester, embed());
    final navigation = web.last.delegate!;
    expect(
      await navigation.decide('https://embed.polyui.invalid/', mainFrame: true),
      NavigationDecision.navigate,
    );
    expect(
      await navigation.decide('https://embed.polyui.invalid/', mainFrame: true),
      NavigationDecision.prevent,
    );
    expect(
      await navigation.decide('https://evil.example/', mainFrame: true),
      NavigationDecision.prevent,
    );
    for (final allowed in [
      'about:srcdoc',
      'about:blank',
      'https://maps.example/tile',
    ]) {
      expect(
        await navigation.decide(allowed, mainFrame: false),
        NavigationDecision.navigate,
        reason: allowed,
      );
    }
    for (final refused in [
      'http://plain.example/',
      'javascript:alert(1)',
      'data:text/html,hi',
      'file:///etc/hosts',
      'intent://scan',
    ]) {
      expect(
        await navigation.decide(refused, mainFrame: false),
        NavigationDecision.prevent,
        reason: refused,
      );
    }
  });

  testWidgets('on Android the first load is consumed by page start', (
    tester,
  ) async {
    await show(tester, embed());
    final navigation = web.last.delegate!;
    navigation.onPageStarted!('https://embed.polyui.invalid/');
    expect(
      await navigation.decide('https://embed.polyui.invalid/', mainFrame: true),
      NavigationDecision.prevent,
    );
  });

  testWidgets('permission requests are denied', (tester) async {
    await show(tester, embed());
    final request = FakePermissionRequest();
    web.last.permissions!(request);
    expect(request.answers, ['deny']);
  });

  testWidgets('a link opens only after the user accepts the confirm sheet', (
    tester,
  ) async {
    await show(tester, embed());
    final page = web.last;
    const link = 'https://example.com/a?b=1#c';
    page.post({'kind': 'open-link', 'url': link, 'nonce': page.nonce});
    await tester.pumpAndSettle();
    expect(find.text('Open this link?'), findsOneWidget);
    expect(find.text(link), findsOneWidget);
    expect(launcher.opened, isEmpty);

    page.post({
      'kind': 'open-link',
      'url': 'https://second.example/',
      'nonce': page.nonce,
    });
    await tester.pumpAndSettle();
    expect(find.text('https://second.example/'), findsNothing);

    await tester.pump(const Duration(milliseconds: 500));
    await tester.tap(find.byKey(const ValueKey('polyui-confirm')));
    await tester.pumpAndSettle();
    expect(launcher.opened, [link]);
    expect(launcher.modes, [PreferredLaunchMode.externalApplication]);
    expect(page.scripts.last, 'window.polyuiHost.answerLink(true)');
  });

  testWidgets('a refused link is not opened and the app hears no', (
    tester,
  ) async {
    await show(tester, embed());
    final page = web.last;
    page.post({
      'kind': 'open-link',
      'url': 'https://example.com/',
      'nonce': page.nonce,
    });
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('polyui-cancel')));
    await tester.pumpAndSettle();
    expect(launcher.opened, isEmpty);
    expect(page.scripts.last, 'window.polyuiHost.answerLink(false)');
  });

  testWidgets('links that are not plain https never reach the sheet', (
    tester,
  ) async {
    await show(tester, embed());
    final page = web.last;
    for (final url in [
      'http://example.com/',
      'https://user:secret@example.com/',
      'javascript:alert(1)',
      'https://exa mple.com/',
    ]) {
      page.post({'kind': 'open-link', 'url': url, 'nonce': page.nonce});
    }
    await tester.pumpAndSettle();
    expect(find.text('Open this link?'), findsNothing);
  });

  testWidgets('new props go to the page, and a new app reloads it', (
    tester,
  ) async {
    await show(tester, embed());
    final page = web.last;
    final first = page.nonce;

    harness.patch([embed(title: 'Forecast for Tashkent')]);
    await tester.pumpAndSettle();
    expect(page.scripts.last, startsWith('window.polyuiHost.update({'));
    expect(page.scripts.last, contains('Forecast for Tashkent'));
    expect(page.pages, hasLength(1));

    page.updateResult = false;
    harness.patch([embed(html: '<!doctype html><p>New app</p>')]);
    await tester.pumpAndSettle();
    expect(page.pages, hasLength(2));
    expect(page.pages.last, contains('New app'));
    expect(page.nonce, isNot(first));

    page.post({'kind': 'height', 'value': 300, 'nonce': first});
    await tester.pumpAndSettle();
    expect(frameHeight(tester), 160);
  });

  testWidgets('removing the app tears the page down', (tester) async {
    await show(tester, embed());
    final page = web.last;
    harness.patch([embed(withApp: false)]);
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('fake-web-view')), findsNothing);
    expect(
      page.scripts.last,
      'window.polyuiHost && window.polyuiHost.teardown()',
    );
  });
}
