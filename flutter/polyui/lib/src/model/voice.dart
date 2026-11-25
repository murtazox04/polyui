import 'dart:async';

import 'package:flutter/foundation.dart';

import '../core/render_batch.dart';
import 'session.dart';

enum PolyuiListenStart { started, denied, unavailable }

enum PolyuiVoiceState { idle, speaking, listening }

enum PolyuiVoiceProblem {
  microphoneDenied,
  recognitionUnavailable,
  speechFailed,
}

abstract interface class PolyuiSpeechOutput {
  Future<void> speak(String text, {required String locale});

  Future<void> stop();
}

abstract interface class PolyuiSpeechInput {
  Future<PolyuiListenStart> listen({
    required String locale,
    required void Function(String transcript) onFinal,
    required void Function() onDone,
  });

  Future<void> stop();
}

typedef PolyuiUnmatchedSpeech = void Function(String transcript);

typedef PolyuiMatchedSpeech =
    void Function(String transcript, VoiceMatch match);

class PolyuiVoice extends ChangeNotifier {
  PolyuiVoice({
    required this.session,
    required this.output,
    required this.input,
    required this.locale,
    this.onUnmatched,
    this.onMatched,
  });

  final PolyuiSession session;
  final PolyuiSpeechOutput output;
  final PolyuiSpeechInput input;
  final String locale;
  final PolyuiUnmatchedSpeech? onUnmatched;
  final PolyuiMatchedSpeech? onMatched;

  @internal
  PolyuiVoiceState currentState = PolyuiVoiceState.idle;

  @internal
  PolyuiVoiceProblem? currentProblem;

  @internal
  int speechRun = 0;

  @internal
  String? listeningSurface;

  @internal
  bool disposed = false;

  PolyuiVoiceState get state => currentState;

  PolyuiVoiceProblem? get problem => currentProblem;

  Future<void> speak(String surface) async {
    await stopListening();
    final utterances = session.say(surface);
    await speakAll(surface, utterances);
  }

  Future<void> speakText(String text) async {
    await stopListening();
    if (text.trim().isEmpty) {
      return;
    }
    await speakAll(null, [Utterance(id: '', text: text)]);
  }

  @internal
  Future<void> speakAll(String? surface, List<Utterance> utterances) async {
    final run = speechRun += 1;
    if (utterances.isEmpty) {
      return;
    }
    setState(PolyuiVoiceState.speaking, problem: null);
    try {
      for (final utterance in utterances) {
        if (run != speechRun || disposed) {
          return;
        }
        if (surface != null && utterance.id.isNotEmpty) {
          session.highlight(surface: surface, id: utterance.id);
        }
        await output.speak(utterance.text, locale: locale);
      }
    } on Exception {
      if (run == speechRun) {
        currentProblem = PolyuiVoiceProblem.speechFailed;
      }
    } finally {
      if (surface != null && !disposed) {
        session.highlight(surface: surface, id: null);
      }
      if (run == speechRun && !disposed) {
        setState(PolyuiVoiceState.idle, problem: currentProblem);
      }
    }
  }

  Future<void> stopSpeaking() async {
    if (currentState != PolyuiVoiceState.speaking) {
      return;
    }
    speechRun += 1;
    await output.stop();
    setState(PolyuiVoiceState.idle, problem: currentProblem);
  }

  Future<PolyuiListenStart> listen(String surface) async {
    await stopSpeaking();
    await stopListening();
    listeningSurface = surface;
    final PolyuiListenStart started;
    try {
      started = await input.listen(
        locale: locale,
        onFinal: (transcript) {
          final target = listeningSurface;
          if (target != null) {
            heard(target, transcript);
          }
        },
        onDone: () {
          listeningSurface = null;
          if (currentState == PolyuiVoiceState.listening) {
            setState(PolyuiVoiceState.idle, problem: currentProblem);
          }
        },
      );
    } on Exception {
      listeningSurface = null;
      setState(
        PolyuiVoiceState.idle,
        problem: PolyuiVoiceProblem.recognitionUnavailable,
      );
      return PolyuiListenStart.unavailable;
    }
    switch (started) {
      case PolyuiListenStart.started:
        setState(PolyuiVoiceState.listening, problem: null);
      case PolyuiListenStart.denied:
        listeningSurface = null;
        setState(
          PolyuiVoiceState.idle,
          problem: PolyuiVoiceProblem.microphoneDenied,
        );
      case PolyuiListenStart.unavailable:
        listeningSurface = null;
        setState(
          PolyuiVoiceState.idle,
          problem: PolyuiVoiceProblem.recognitionUnavailable,
        );
    }
    return started;
  }

  Future<void> stopListening() async {
    if (currentState != PolyuiVoiceState.listening) {
      return;
    }
    listeningSurface = null;
    await input.stop();
    setState(PolyuiVoiceState.idle, problem: currentProblem);
  }

  VoiceMatch? heard(String surface, String transcript) {
    final spoken = transcript.trim();
    if (spoken.isEmpty) {
      return null;
    }
    final found = session.matchVoice(surface: surface, text: spoken);
    if (found == null) {
      onUnmatched?.call(spoken);
    } else {
      onMatched?.call(spoken, found);
    }
    return found;
  }

  void clearProblem() {
    if (currentProblem != null) {
      setState(currentState, problem: null);
    }
  }

  @internal
  void setState(PolyuiVoiceState next, {required PolyuiVoiceProblem? problem}) {
    if (disposed) {
      return;
    }
    if (next == currentState && problem == currentProblem) {
      return;
    }
    currentState = next;
    currentProblem = problem;
    notifyListeners();
  }

  @override
  void dispose() {
    disposed = true;
    speechRun += 1;
    unawaited(output.stop().catchError((Object error) {}));
    unawaited(input.stop().catchError((Object error) {}));
    super.dispose();
  }
}
