Map<String, Object?> asJsonMap(Object? value) {
  if (value is Map<String, Object?>) {
    return value;
  }
  if (value is Map) {
    return value.map((key, item) => MapEntry('$key', item));
  }
  return const {};
}

Map<String, Object?>? asNullableJsonMap(Object? value) =>
    value is Map ? asJsonMap(value) : null;

bool jsonEquals(Object? left, Object? right) {
  if (left is Map && right is Map) {
    if (left.length != right.length) {
      return false;
    }
    for (final entry in left.entries) {
      if (!right.containsKey(entry.key) ||
          !jsonEquals(entry.value, right[entry.key])) {
        return false;
      }
    }
    return true;
  }
  if (left is List && right is List) {
    if (left.length != right.length) {
      return false;
    }
    for (var index = 0; index < left.length; index += 1) {
      if (!jsonEquals(left[index], right[index])) {
        return false;
      }
    }
    return true;
  }
  return left == right;
}

String? jsonString(Object? value) => value is String ? value : null;

double? jsonNumber(Object? value) => value is num ? value.toDouble() : null;

bool? jsonBool(Object? value) => value is bool ? value : null;

List<Object?> jsonList(Object? value) =>
    value is List ? value.cast<Object?>() : const [];
