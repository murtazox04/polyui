import 'package:flutter/services.dart';
import 'package:flutter_tts/flutter_tts.dart';
import 'package:polyui/polyui.dart';
import 'package:speech_to_text/speech_recognition_result.dart';
import 'package:speech_to_text/speech_to_text.dart';

final class SpeechFailure implements Exception {
  const SpeechFailure(this.reason);

  final String reason;

  @override
  String toString() => 'SpeechFailure: $reason';
}

class TtsOutput implements PolyuiSpeechOutput {
  final FlutterTts tts = FlutterTts();
  bool prepared = false;
  String? language;

  @override
  Future<void> speak(String text, {required String locale}) async {
    try {
      if (!prepared) {
        await tts.awaitSpeakCompletion(true);
        prepared = true;
      }
      if (language != locale) {
        language = locale;
        final available = await tts.isLanguageAvailable(locale);
        if (available == true) {
          await tts.setLanguage(locale);
        }
      }
      final result = await tts.speak(text);
      if (result != 1) {
        throw SpeechFailure('the speech engine answered $result');
      }
    } on PlatformException catch (error) {
      throw SpeechFailure(error.message ?? error.code);
    } on MissingPluginException catch (error) {
      throw SpeechFailure(error.message ?? 'no speech plugin');
    }
  }

  @override
  Future<void> stop() async {
    try {
      await tts.stop();
    } on PlatformException {
      return;
    } on MissingPluginException {
      return;
    }
  }
}

class RecognizerInput implements PolyuiSpeechInput {
  final SpeechToText speech = SpeechToText();
  bool initialized = false;
  void Function()? finished;
  void Function(String transcript)? heard;

  Future<bool> prepare() async {
    if (initialized) {
      return true;
    }
    initialized = await speech.initialize(
      onStatus: (status) {
        if (status == SpeechToText.doneStatus) {
          end();
        }
      },
      onError: (error) {
        if (error.permanent) {
          end();
        }
      },
    );
    return initialized;
  }

  void end() {
    final done = finished;
    finished = null;
    heard = null;
    done?.call();
  }

  @override
  Future<PolyuiListenStart> listen({
    required String locale,
    required void Function(String transcript) onFinal,
    required void Function() onDone,
  }) async {
    final bool ready;
    try {
      ready = await prepare();
    } on PlatformException {
      return PolyuiListenStart.unavailable;
    } on MissingPluginException {
      return PolyuiListenStart.unavailable;
    }
    if (!ready) {
      final allowed = await speech.hasPermission.catchError(
        (Object error) => false,
      );
      return allowed ? PolyuiListenStart.unavailable : PolyuiListenStart.denied;
    }
    heard = onFinal;
    finished = onDone;
    await speech.listen(
      onResult: (SpeechRecognitionResult result) {
        if (result.finalResult) {
          heard?.call(result.recognizedWords);
        }
      },
      listenOptions: SpeechListenOptions(
        partialResults: false,
        cancelOnError: true,
        listenMode: ListenMode.confirmation,
      ),
    );
    return PolyuiListenStart.started;
  }

  @override
  Future<void> stop() async {
    finished = null;
    heard = null;
    if (initialized) {
      await speech.stop();
    }
  }
}
