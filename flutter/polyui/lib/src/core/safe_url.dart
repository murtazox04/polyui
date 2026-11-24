Uri? checkSafeUrl(String? raw, {bool allowHttp = false}) {
  if (raw == null || raw.isEmpty || raw.length > 4096) {
    return null;
  }
  for (final unit in raw.runes) {
    if (unit <= 0x20 || (unit >= 0x7F && unit <= 0x9F)) {
      return null;
    }
  }
  final Uri parsed;
  try {
    parsed = Uri.parse(raw);
  } on FormatException {
    return null;
  }
  final scheme = parsed.scheme.toLowerCase();
  final allowed = scheme == 'https' || (allowHttp && scheme == 'http');
  if (!allowed || parsed.userInfo.isNotEmpty || parsed.host.isEmpty) {
    return null;
  }
  return parsed;
}
