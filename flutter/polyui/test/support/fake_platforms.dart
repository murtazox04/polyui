import 'dart:convert';

import 'package:flutter/widgets.dart';
import 'package:url_launcher_platform_interface/link.dart';
import 'package:url_launcher_platform_interface/url_launcher_platform_interface.dart';
import 'package:webview_flutter_platform_interface/webview_flutter_platform_interface.dart';

class FakeWebViewPlatform extends WebViewPlatform {
  final List<FakeWebViewController> controllers = [];

  FakeWebViewController get last => controllers.last;

  @override
  PlatformWebViewController createPlatformWebViewController(
    PlatformWebViewControllerCreationParams params,
  ) {
    final made = FakeWebViewController(params);
    controllers.add(made);
    return made;
  }

  @override
  PlatformNavigationDelegate createPlatformNavigationDelegate(
    PlatformNavigationDelegateCreationParams params,
  ) => FakeNavigationDelegate(params);

  @override
  PlatformWebViewWidget createPlatformWebViewWidget(
    PlatformWebViewWidgetCreationParams params,
  ) => FakeWebViewWidget(params);
}

class FakeWebViewController extends PlatformWebViewController {
  FakeWebViewController(super.params) : super.implementation();

  final List<String> pages = [];
  final List<String?> baseUrls = [];
  final List<String> scripts = [];
  JavaScriptMode? mode;
  JavaScriptChannelParams? channel;
  FakeNavigationDelegate? delegate;
  void Function(PlatformWebViewPermissionRequest request)? permissions;
  Object updateResult = true;

  String get nonce {
    final match = RegExp(
      r'const nonce = "([A-Za-z0-9]+)"',
    ).firstMatch(pages.last);
    return match!.group(1)!;
  }

  void post(Map<String, Object?> event) =>
      channel!.onMessageReceived(JavaScriptMessage(message: jsonEncode(event)));

  void postText(String text) =>
      channel!.onMessageReceived(JavaScriptMessage(message: text));

  @override
  Future<void> loadHtmlString(String html, {String? baseUrl}) async {
    pages.add(html);
    baseUrls.add(baseUrl);
  }

  @override
  Future<void> setJavaScriptMode(JavaScriptMode javaScriptMode) async {
    mode = javaScriptMode;
  }

  @override
  Future<void> setBackgroundColor(Color color) async {}

  @override
  Future<void> setPlatformNavigationDelegate(
    PlatformNavigationDelegate handler,
  ) async {
    delegate = handler as FakeNavigationDelegate;
  }

  @override
  Future<void> addJavaScriptChannel(
    JavaScriptChannelParams javaScriptChannelParams,
  ) async {
    channel = javaScriptChannelParams;
  }

  @override
  Future<void> setOnPlatformPermissionRequest(
    void Function(PlatformWebViewPermissionRequest request) onPermissionRequest,
  ) async {
    permissions = onPermissionRequest;
  }

  @override
  Future<void> runJavaScript(String javaScript) async {
    scripts.add(javaScript);
  }

  @override
  Future<Object> runJavaScriptReturningResult(String javaScript) async {
    scripts.add(javaScript);
    return updateResult;
  }
}

class FakeNavigationDelegate extends PlatformNavigationDelegate {
  FakeNavigationDelegate(super.params) : super.implementation();

  NavigationRequestCallback? onNavigationRequest;
  PageEventCallback? onPageStarted;

  Future<NavigationDecision> decide(String url, {required bool mainFrame}) =>
      Future.value(
        onNavigationRequest!(
          NavigationRequest(url: url, isMainFrame: mainFrame),
        ),
      );

  @override
  Future<void> setOnNavigationRequest(
    NavigationRequestCallback onNavigationRequest,
  ) async {
    this.onNavigationRequest = onNavigationRequest;
  }

  @override
  Future<void> setOnPageStarted(PageEventCallback onPageStarted) async {
    this.onPageStarted = onPageStarted;
  }
}

class FakeWebViewWidget extends PlatformWebViewWidget {
  FakeWebViewWidget(super.params) : super.implementation();

  @override
  Widget build(BuildContext context) =>
      const SizedBox.expand(key: ValueKey('fake-web-view'));
}

class FakePermissionRequest extends PlatformWebViewPermissionRequest {
  FakePermissionRequest()
    : super(types: {WebViewPermissionResourceType.microphone});

  final List<String> answers = [];

  @override
  Future<void> grant() async {
    answers.add('grant');
  }

  @override
  Future<void> deny() async {
    answers.add('deny');
  }
}

class FakeUrlLauncher extends UrlLauncherPlatform {
  final List<String> opened = [];
  final List<PreferredLaunchMode> modes = [];

  @override
  LinkDelegate? get linkDelegate => null;

  @override
  Future<bool> launchUrl(String url, LaunchOptions options) async {
    opened.add(url);
    modes.add(options.mode);
    return true;
  }
}
