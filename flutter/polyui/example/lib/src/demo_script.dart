import 'chat_model.dart';

class DemoScript {
  const DemoScript({
    this.step = const String.fromEnvironment('POLYUI_DEMO'),
    this.scrollToBottom =
        const String.fromEnvironment('POLYUI_SCROLL') == 'bottom',
    this.instant = const bool.fromEnvironment('POLYUI_INSTANT'),
    this.log = const bool.fromEnvironment('POLYUI_LOG'),
    this.live = const String.fromEnvironment('POLYUI_MODE') == 'live',
    this.liveQuestion = const String.fromEnvironment('POLYUI_ASK'),
    this.voiceStep = const String.fromEnvironment('POLYUI_VOICE'),
  });

  final String step;
  final bool scrollToBottom;
  final bool instant;
  final bool log;
  final bool live;
  final String liveQuestion;
  final String voiceStep;

  bool get pressesInsideApp => step == 'embed-press';

  bool reaches(String target) {
    const order = ['ask', 'select', 'pay', 'paid'];
    final current = order.indexOf(step);
    return current >= 0 && current >= order.indexOf(target);
  }

  Future<void> run(ChatModel model) async {
    if (live) {
      model.goLive(model.liveUrl);
      if (liveQuestion.isEmpty) {
        return;
      }
      final connected = await waitFor(
        () => model.connection == Connection.live,
      );
      if (connected) {
        model.ask(liveQuestion);
      }
      return;
    }
    if (step == 'embed' || pressesInsideApp) {
      model.ask(model.weatherQuestion);
      return;
    }
    if (!reaches('ask')) {
      return;
    }
    model.ask(model.question);
    final opened = await waitFor(
      () => model.session.surface('s1').rootId != null,
    );
    if (!opened) {
      return;
    }
    final session = model.session;
    if (reaches('select')) {
      session.fire(
        surface: 's1',
        component: 'usage',
        prop: 'on_select',
        detail: {'row': 1},
      );
    }
    if (reaches('pay')) {
      session.setValue(surface: 's1', path: '/payment/card', value: 'visa');
      session.setValue(
        surface: 's1',
        path: '/payment/phone',
        value: '+998901112233',
      );
      await Future<void>.delayed(const Duration(milliseconds: 300));
      session.fire(surface: 's1', component: 'pay', prop: 'on_press');
    }
    if (reaches('paid')) {
      final asked = await waitFor(
        () => session.surface('s1').pendingConfirm != null,
      );
      if (!asked) {
        return;
      }
      await Future<void>.delayed(const Duration(milliseconds: 700));
      final request = session.surface('s1').pendingConfirm;
      if (request != null) {
        session.confirm(surface: 's1', request: request);
      }
    }
  }

  Future<bool> waitFor(bool Function() condition) async {
    for (var attempt = 0; attempt < 100; attempt += 1) {
      if (condition()) {
        return true;
      }
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    return condition();
  }
}
