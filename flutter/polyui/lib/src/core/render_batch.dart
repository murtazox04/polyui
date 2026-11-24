import 'package:flutter/foundation.dart';

import 'json.dart';

@immutable
final class A11y {
  const A11y({this.role = '', this.label = ''});

  factory A11y.fromJson(Object? json) {
    final map = asJsonMap(json);
    return A11y(
      role: map['role'] as String? ?? '',
      label: map['label'] as String? ?? '',
    );
  }

  final String role;
  final String label;

  @override
  bool operator ==(Object other) =>
      other is A11y && other.role == role && other.label == label;

  @override
  int get hashCode => Object.hash(role, label);
}

@immutable
final class ComponentPayload {
  const ComponentPayload({
    required this.id,
    this.type,
    this.props = const {},
    this.value,
    this.a11y = const A11y(),
  });

  final String id;
  final String? type;
  final Map<String, Object?> props;
  final Object? value;
  final A11y a11y;

  @override
  bool operator ==(Object other) =>
      other is ComponentPayload &&
      other.id == id &&
      other.type == type &&
      jsonEquals(other.props, props) &&
      jsonEquals(other.value, value) &&
      other.a11y == a11y;

  @override
  int get hashCode => Object.hash(id, type, a11y);
}

@immutable
final class ConfirmRequest {
  const ConfirmRequest({
    required this.actionId,
    required this.nonce,
    required this.title,
    this.body,
    this.confirmLabel,
    this.expiresMs,
  });

  factory ConfirmRequest.fromJson(Map<String, Object?> json) => ConfirmRequest(
    actionId: json['action_id'] as String? ?? '',
    nonce: json['nonce'] as String? ?? '',
    title: json['title'] as String? ?? '',
    body: json['body'] as String?,
    confirmLabel: json['confirm_label'] as String?,
    expiresMs: (json['expires_ms'] as num?)?.toInt(),
  );

  final String actionId;
  final String nonce;
  final String title;
  final String? body;
  final String? confirmLabel;
  final int? expiresMs;

  String get key => '$actionId:$nonce';

  @override
  bool operator ==(Object other) =>
      other is ConfirmRequest &&
      other.actionId == actionId &&
      other.nonce == nonce &&
      other.title == title &&
      other.body == body &&
      other.confirmLabel == confirmLabel &&
      other.expiresMs == expiresMs;

  @override
  int get hashCode =>
      Object.hash(actionId, nonce, title, body, confirmLabel, expiresMs);
}

@immutable
sealed class RenderOp {
  const RenderOp();

  factory RenderOp.fromJson(Object? json) {
    final map = asJsonMap(json);
    final name = map['op'];
    switch (name) {
      case 'create' || 'update':
        final payload = ComponentPayload(
          id: map['id'] as String? ?? '',
          type: map['type'] as String?,
          props: asJsonMap(map['props']),
          value: map['value'],
          a11y: A11y.fromJson(map['a11y']),
        );
        return name == 'create' ? CreateOp(payload) : UpdateOp(payload);
      case 'children':
        return ChildrenOp(
          map['id'] as String? ?? '',
          (map['children'] as List<Object?>? ?? const [])
              .whereType<String>()
              .toList(growable: false),
        );
      case 'remove':
        return RemoveOp(map['id'] as String? ?? '');
      case 'root':
        return RootOp(map['id'] as String? ?? '');
      case 'confirm':
        return ConfirmOp(ConfirmRequest.fromJson(map));
      case 'highlight':
        return HighlightOp(map['id'] as String?);
      case 'close':
        return const CloseOp();
      default:
        return UnknownOp('$name');
    }
  }
}

final class CreateOp extends RenderOp {
  const CreateOp(this.payload);

  final ComponentPayload payload;

  @override
  bool operator ==(Object other) =>
      other is CreateOp && other.payload == payload;

  @override
  int get hashCode => payload.hashCode;
}

final class UpdateOp extends RenderOp {
  const UpdateOp(this.payload);

  final ComponentPayload payload;

  @override
  bool operator ==(Object other) =>
      other is UpdateOp && other.payload == payload;

  @override
  int get hashCode => payload.hashCode;
}

final class ChildrenOp extends RenderOp {
  const ChildrenOp(this.id, this.children);

  final String id;
  final List<String> children;

  @override
  bool operator ==(Object other) =>
      other is ChildrenOp &&
      other.id == id &&
      listEquals(other.children, children);

  @override
  int get hashCode => Object.hash(id, Object.hashAll(children));
}

final class RemoveOp extends RenderOp {
  const RemoveOp(this.id);

  final String id;

  @override
  bool operator ==(Object other) => other is RemoveOp && other.id == id;

  @override
  int get hashCode => id.hashCode;
}

final class RootOp extends RenderOp {
  const RootOp(this.id);

  final String id;

  @override
  bool operator ==(Object other) => other is RootOp && other.id == id;

  @override
  int get hashCode => id.hashCode;
}

final class ConfirmOp extends RenderOp {
  const ConfirmOp(this.request);

  final ConfirmRequest request;

  @override
  bool operator ==(Object other) =>
      other is ConfirmOp && other.request == request;

  @override
  int get hashCode => request.hashCode;
}

final class HighlightOp extends RenderOp {
  const HighlightOp(this.id);

  final String? id;

  @override
  bool operator ==(Object other) => other is HighlightOp && other.id == id;

  @override
  int get hashCode => id.hashCode;
}

final class CloseOp extends RenderOp {
  const CloseOp();

  @override
  bool operator ==(Object other) => other is CloseOp;

  @override
  int get hashCode => 0;
}

final class UnknownOp extends RenderOp {
  const UnknownOp(this.name);

  final String name;

  @override
  bool operator ==(Object other) => other is UnknownOp && other.name == name;

  @override
  int get hashCode => name.hashCode;
}

@immutable
final class RenderBatch {
  const RenderBatch({
    required this.surface,
    this.ops = const [],
    this.fallback,
    this.send = const [],
  });

  factory RenderBatch.fromJson(Object? json) {
    final map = asJsonMap(json);
    return RenderBatch(
      surface: map['surface'] as String? ?? '',
      ops: (map['ops'] as List<Object?>? ?? const [])
          .map(RenderOp.fromJson)
          .toList(growable: false),
      fallback: map['fallback'] as String?,
      send: List<Object?>.unmodifiable(
        map['send'] as List<Object?>? ?? const [],
      ),
    );
  }

  final String surface;
  final List<RenderOp> ops;
  final String? fallback;
  final List<Object?> send;

  @override
  bool operator ==(Object other) =>
      other is RenderBatch &&
      other.surface == surface &&
      listEquals(other.ops, ops) &&
      other.fallback == fallback &&
      jsonEquals(other.send, send);

  @override
  int get hashCode => Object.hash(surface, Object.hashAll(ops), fallback);
}

@immutable
final class Utterance {
  const Utterance({required this.id, required this.text});

  factory Utterance.fromJson(Object? json) {
    final map = asJsonMap(json);
    return Utterance(
      id: map['id'] as String? ?? '',
      text: map['text'] as String? ?? '',
    );
  }

  final String id;
  final String text;

  @override
  bool operator ==(Object other) =>
      other is Utterance && other.id == id && other.text == text;

  @override
  int get hashCode => Object.hash(id, text);
}

@immutable
final class VoiceMatch {
  const VoiceMatch({
    required this.component,
    this.option,
    this.batch,
    this.action,
  });

  factory VoiceMatch.fromJson(Object? json) {
    final map = asJsonMap(json);
    final batch = map['batch'];
    return VoiceMatch(
      component: map['component'] as String? ?? '',
      option: map['option'] as String?,
      batch: batch == null ? null : RenderBatch.fromJson(batch),
      action: asNullableJsonMap(map['action']),
    );
  }

  final String component;
  final String? option;
  final RenderBatch? batch;
  final Map<String, Object?>? action;

  @override
  bool operator ==(Object other) =>
      other is VoiceMatch &&
      other.component == component &&
      other.option == option &&
      other.batch == batch &&
      jsonEquals(other.action, action);

  @override
  int get hashCode => Object.hash(component, option, batch);
}
