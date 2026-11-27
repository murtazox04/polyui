import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';
import 'package:webview_flutter/webview_flutter.dart';
import 'package:webview_flutter_android/webview_flutter_android.dart';
import 'package:webview_flutter_wkwebview/webview_flutter_wkwebview.dart';

import '../core/polyui_core.dart';
import '../core/render_batch.dart';
import '../model/component_node.dart';
import '../model/embed_bridge.dart';
import 'confirm_request_sheet.dart';
import 'core_components.dart';
import 'scope.dart';
import 'theme.dart';

class EmbedComponent extends StatefulWidget {
  const EmbedComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  State<EmbedComponent> createState() => EmbedComponentState();
}

class EmbedComponentState extends State<EmbedComponent> {
  static EmbedComponentState? linkOwner;
  static const messageGap = Duration(seconds: 1);
  static const openAppCallLimit = 4;
  static const Map<String, Object?> tooManyAppCalls = {
    'error': {'code': -32000, 'message': 'Too many requests are still open'},
  };

  late PolyuiScope scope;
  WebViewController? controller;
  String nonce = '';
  EmbedHeight bounds = EmbedHeight.forHint('medium');
  double height = EmbedHeight.forHint('medium').start;
  bool expectingLoad = false;
  int seenRevision = 0;
  DateTime? lastMessage;
  ModalBottomSheetRoute<bool>? linkRoute;
  String themeName = 'light';
  String? sentTheme;
  bool pageReady = false;
  bool started = false;
  final Map<String, void Function()> openCalls = {};

  ComponentNode get node => widget.node;

  String get title => node.string('title');

  @override
  void initState() {
    super.initState();
    seenRevision = node.revision;
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    scope = PolyuiScope.of(context);
    if (!started) {
      started = true;
      start();
    }
    final background = PolyuiTheme.of(context).background;
    themeName =
        ThemeData.estimateBrightnessForColor(background) == Brightness.dark
        ? 'dark'
        : 'light';
    final current = controller;
    if (current != null) {
      sendTheme(current);
    }
  }

  void sendTheme(WebViewController made) {
    if (!pageReady || sentTheme == themeName) {
      return;
    }
    sentTheme = themeName;
    unawaited(
      made
          .runJavaScript('window.polyuiHost.setTheme("$themeName")')
          .catchError((Object error) {}),
    );
  }

  @override
  void didUpdateWidget(EmbedComponent oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.node != widget.node) {
      stop();
      seenRevision = node.revision;
      start();
      return;
    }
    if (node.revision == seenRevision) {
      return;
    }
    seenRevision = node.revision;
    bounds = EmbedHeight.forHint(heightHint);
    height = bounds.clamp(height);
    if (!embedHasApp(node.props)) {
      stop();
      return;
    }
    final current = controller;
    if (current == null) {
      start();
    } else {
      unawaited(pushUpdate(current, seenRevision));
    }
  }

  @override
  void dispose() {
    stop();
    super.dispose();
  }

  String get heightHint =>
      node.choice('height', const {'small', 'medium', 'large'}, 'medium');

  void start() {
    bounds = EmbedHeight.forHint(heightHint);
    height = bounds.start;
    if (!embedHasApp(node.props) || WebViewPlatform.instance == null) {
      return;
    }
    final page = buildPage();
    if (page == null) {
      return;
    }
    final made = makeController();
    controller = made;
    unawaited(configure(made, page));
  }

  String? buildPage() {
    final fresh = makeEmbedNonce();
    try {
      final page = PolyuiCore.embedPage(
        propsJson: jsonEncode(node.props),
        nonce: fresh,
        appCalls: scope.session.appCalls,
        appContext: scope.session.appContext,
      );
      nonce = fresh;
      return page;
    } on PolyuiException {
      return null;
    }
  }

  void stop() {
    closeLinkQuestion();
    dropAppCalls();
    final current = controller;
    controller = null;
    nonce = '';
    expectingLoad = false;
    pageReady = false;
    sentTheme = null;
    if (current != null) {
      unawaited(
        current
            .runJavaScript('window.polyuiHost && window.polyuiHost.teardown()')
            .catchError((Object error) {}),
      );
    }
  }

  void fallBack() {
    if (!mounted || controller == null) {
      return;
    }
    setState(stop);
  }

  WebViewController makeController() {
    final platform = WebViewPlatform.instance;
    final PlatformWebViewControllerCreationParams params =
        platform is WebKitWebViewPlatform
        ? WebKitWebViewControllerCreationParams(
            allowsInlineMediaPlayback: false,
            javaScriptCanOpenWindowsAutomatically: false,
          )
        : const PlatformWebViewControllerCreationParams();
    return WebViewController.fromPlatformCreationParams(
      params,
      onPermissionRequest: (request) => unawaited(request.deny()),
    );
  }

  Future<void> configure(WebViewController made, String page) async {
    final native = made.platform;
    if (native is AndroidWebViewController) {
      await native.setAllowFileAccess(false);
      await native.setAllowContentAccess(false);
      await native.setGeolocationEnabled(false);
      await native.setMediaPlaybackRequiresUserGesture(true);
      await native.setOnShowFileSelector((params) async => const []);
      await native.setGeolocationPermissionsPromptCallbacks(
        onShowPrompt: (request) async =>
            const GeolocationPermissionsResponse(allow: false, retain: false),
      );
    }
    if (native is WebKitWebViewController) {
      await native.setAllowsLinkPreview(false);
      await native.setAllowsBackForwardNavigationGestures(false);
    }
    await made.setJavaScriptMode(JavaScriptMode.unrestricted);
    await made.setBackgroundColor(const Color(0x00000000));
    await made.setNavigationDelegate(
      NavigationDelegate(
        onNavigationRequest: (request) => decide(made, request),
        onPageStarted: (url) {
          if (controller == made) {
            expectingLoad = false;
          }
        },
      ),
    );
    await made.addJavaScriptChannel(
      embedChannelName,
      onMessageReceived: (message) => received(made, message.message),
    );
    if (controller != made) {
      return;
    }
    expectingLoad = true;
    await made.loadHtmlString(page, baseUrl: embedBaseUrl);
  }

  NavigationDecision decide(WebViewController made, NavigationRequest request) {
    if (request.isMainFrame) {
      final first =
          controller == made && expectingLoad && request.url == embedBaseUrl;
      if (first) {
        expectingLoad = false;
        return NavigationDecision.navigate;
      }
      return NavigationDecision.prevent;
    }
    return embedAllowsSubframe(request.url)
        ? NavigationDecision.navigate
        : NavigationDecision.prevent;
  }

  void received(WebViewController made, String text) {
    if (!mounted || controller != made) {
      return;
    }
    switch (EmbedEvent.parse(text, nonce)) {
      case null:
        break;
      case EmbedReady():
        pageReady = true;
        sendTheme(made);
      case EmbedUnavailable():
        fallBack();
      case EmbedHeightEvent(:final value):
        final next = bounds.clamp(value);
        if (next != height) {
          setState(() => height = next);
        }
      case EmbedMessage(text: final said):
        sendMessage(said);
      case EmbedOpenLink(:final url):
        unawaited(askLink(made, url));
      case final EmbedAppCall call:
        callApp(made, call);
    }
  }

  void callApp(WebViewController made, EmbedAppCall call) {
    final page = nonce;
    openCalls.remove(call.id)?.call();
    if (openCalls.length >= openAppCallLimit) {
      answerApp(made, page, call.id, tooManyAppCalls);
      return;
    }
    var answered = false;
    final cancel = scope.session.callApp(
      surface: scope.surface.id,
      embed: node.id,
      method: call.method,
      params: call.params,
      done: (answer) {
        answered = true;
        openCalls.remove(call.id);
        answerApp(made, page, call.id, answer);
      },
    );
    if (!answered) {
      openCalls[call.id] = cancel;
    }
  }

  void answerApp(
    WebViewController made,
    String page,
    String id,
    Map<String, Object?> answer,
  ) {
    if (!mounted || controller != made || nonce != page) {
      return;
    }
    final script =
        'window.polyuiHost.answerApp(${jsonEncode(id)}, '
        'JSON.parse(${jsonEncode(jsonEncode(answer))}))';
    unawaited(made.runJavaScript(script).catchError((Object error) {}));
  }

  void dropAppCalls() {
    final open = openCalls.values.toList();
    openCalls.clear();
    for (final cancel in open) {
      cancel();
    }
  }

  void sendMessage(String said) {
    final now = DateTime.now();
    final previous = lastMessage;
    if (previous != null && now.difference(previous) < messageGap) {
      return;
    }
    lastMessage = now;
    scope.contextFor(node).fire('on_message', detail: {'text': said});
  }

  Future<void> askLink(WebViewController made, Uri url) async {
    if (linkOwner != null || linkRoute != null) {
      return;
    }
    final labels = scope.labels;
    final route = polyuiConfirmRoute(
      context,
      request: ConfirmRequest(
        actionId: 'polyui-open-link',
        nonce: nonce,
        title: labels.openLinkTitle,
        body: url.toString(),
        confirmLabel: labels.openLinkConfirm,
      ),
      labels: labels,
      delay: scope.confirmDelay,
    );
    linkOwner = this;
    linkRoute = route;
    final accepted =
        await Navigator.of(context, rootNavigator: true).push(route) ?? false;
    if (linkOwner == this) {
      linkOwner = null;
    }
    if (linkRoute != route) {
      return;
    }
    linkRoute = null;
    final opened = accepted && await openExternally(url);
    if (controller == made) {
      await made
          .runJavaScript('window.polyuiHost.answerLink($opened)')
          .catchError((Object error) {});
    }
  }

  Future<bool> openExternally(Uri url) async {
    try {
      return await launchUrl(url, mode: LaunchMode.externalApplication);
    } on Exception {
      return false;
    }
  }

  void closeLinkQuestion() {
    final open = linkRoute;
    linkRoute = null;
    if (linkOwner == this) {
      linkOwner = null;
    }
    if (open != null && open.isActive) {
      open.navigator?.removeRoute(open);
    }
  }

  Future<void> pushUpdate(WebViewController made, int revision) async {
    Object? kept;
    try {
      kept = await made.runJavaScriptReturningResult(
        'window.polyuiHost.update(${jsonEncode(node.props)})',
      );
    } on Exception {
      kept = false;
    }
    if (!mounted || controller != made || seenRevision != revision) {
      return;
    }
    if (kept == true || kept == 1 || kept == 'true') {
      return;
    }
    closeLinkQuestion();
    dropAppCalls();
    final page = buildPage();
    if (page == null) {
      fallBack();
      return;
    }
    setState(() => height = bounds.start);
    expectingLoad = true;
    pageReady = false;
    sentTheme = null;
    await made.loadHtmlString(page, baseUrl: embedBaseUrl);
  }

  @override
  Widget build(BuildContext context) {
    final current = controller;
    if (current == null) {
      return EmbedTitle(node: node);
    }
    final theme = PolyuiTheme.of(context);
    return FillWidth(
      child: Semantics(
        container: true,
        label: title.isEmpty ? node.a11y.label : title,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(theme.innerCornerRadius),
          child: SizedBox(
            height: height,
            child: WebViewWidget(key: ObjectKey(current), controller: current),
          ),
        ),
      ),
    );
  }
}
