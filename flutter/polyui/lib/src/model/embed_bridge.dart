import 'dart:convert';
import 'dart:math';

import 'package:flutter/foundation.dart';

import '../core/json.dart';
import '../core/safe_url.dart';

const embedBaseUrl = 'https://embed.polyui.invalid/';
const embedChannelName = 'polyui';
const embedNonceLength = 32;
const embedMinHeight = 80.0;

const nonceAlphabet =
    'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';

String makeEmbedNonce([Random? random]) {
  final source = random ?? Random.secure();
  return String.fromCharCodes([
    for (var index = 0; index < embedNonceLength; index += 1)
      nonceAlphabet.codeUnitAt(source.nextInt(nonceAlphabet.length)),
  ]);
}

@immutable
final class EmbedHeight {
  const EmbedHeight(this.start, this.max);

  factory EmbedHeight.forHint(String hint) => switch (hint) {
    'small' => const EmbedHeight(160, 320),
    'large' => const EmbedHeight(480, 960),
    _ => const EmbedHeight(320, 640),
  };

  final double start;
  final double max;

  double clamp(num value) => value.toDouble().clamp(embedMinHeight, max);
}

@immutable
sealed class EmbedEvent {
  const EmbedEvent();

  static EmbedEvent? parse(String text, String nonce) {
    final Object? decoded;
    try {
      decoded = jsonDecode(text);
    } on FormatException {
      return null;
    }
    if (decoded is! Map<String, Object?>) {
      return null;
    }
    final sent = decoded['nonce'];
    if (sent is! String || !sameNonce(sent, nonce)) {
      return null;
    }
    switch (decoded['kind']) {
      case 'ready':
        return const EmbedReady();
      case 'unavailable':
        return const EmbedUnavailable();
      case 'height':
        final value = jsonNumber(decoded['value']);
        return value == null || !value.isFinite
            ? null
            : EmbedHeightEvent(value);
      case 'message':
        final said = jsonString(decoded['text']);
        return said == null || said.isEmpty ? null : EmbedMessage(said);
      case 'open-link':
        final link = checkSafeUrl(jsonString(decoded['url']));
        return link == null ? null : EmbedOpenLink(link);
      default:
        return null;
    }
  }

  static bool sameNonce(String sent, String expected) {
    if (sent.length != expected.length) {
      return false;
    }
    var difference = 0;
    for (var index = 0; index < sent.length; index += 1) {
      difference |= sent.codeUnitAt(index) ^ expected.codeUnitAt(index);
    }
    return difference == 0;
  }
}

final class EmbedReady extends EmbedEvent {
  const EmbedReady();
}

final class EmbedUnavailable extends EmbedEvent {
  const EmbedUnavailable();
}

final class EmbedHeightEvent extends EmbedEvent {
  const EmbedHeightEvent(this.value);

  final double value;
}

final class EmbedMessage extends EmbedEvent {
  const EmbedMessage(this.text);

  final String text;
}

final class EmbedOpenLink extends EmbedEvent {
  const EmbedOpenLink(this.url);

  final Uri url;
}

bool embedHasApp(Map<String, Object?> props) => props['app'] is Map;

bool embedAllowsSubframe(String url) {
  if (url == 'about:srcdoc' || url == 'about:blank') {
    return true;
  }
  return Uri.tryParse(url)?.scheme.toLowerCase() == 'https';
}
