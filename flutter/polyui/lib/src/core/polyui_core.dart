import 'dart:convert';
import 'dart:ffi';
import 'dart:ui' show PlatformDispatcher;

import 'package:ffi/ffi.dart';
import 'package:meta/meta.dart';

import '../native/bindings.dart';
import 'render_batch.dart';

final class PolyuiException implements Exception {
  const PolyuiException(this.message);

  final String message;

  @override
  String toString() => 'PolyuiException: $message';
}

@immutable
final class PolyuiHello {
  const PolyuiHello({
    this.clientName = 'polyui-flutter',
    this.clientVersion = '0.1.0',
    this.catalogs = const ['core@1', 'chart@1'],
    this.modalities = const ['screen', 'voice'],
    this.features = const ['sync'],
    this.locale,
  });

  final String clientName;
  final String clientVersion;
  final List<String> catalogs;
  final List<String> modalities;
  final List<String> features;
  final String? locale;

  Map<String, Object?> toJson() => {
    'v': 1,
    't': 'hello',
    'client': {'name': clientName, 'version': clientVersion},
    'catalogs': catalogs,
    'modalities': modalities,
    'features': features,
    'locale': locale ?? PlatformDispatcher.instance.locale.toLanguageTag(),
  };

  String jsonText() => jsonEncode(toJson());
}

typedef NativeCall =
    Pointer<Utf8> Function(
      Pointer<PolyuiClientHandle> client,
      Allocator arena,
      ErrorSlot error,
    );

final class PolyuiCore implements Finalizable {
  PolyuiCore(this.hello, {this.allowHttp = false, this.catalogs = const []}) {
    final created = using((arena) {
      final error = arena<Pointer<Utf8>>()..value = nullptr;
      final helloText = nativeText(hello, arena, 'the hello');
      final client = catalogs.isEmpty
          ? polyuiClientNew(helloText, allowHttp, error)
          : polyuiClientNewWithCatalogs(
              helloText,
              nativeText(jsonEncode(catalogs), arena, 'the catalog list'),
              allowHttp,
              error,
            );
      if (client == nullptr) {
        throw PolyuiException(
          takeText(error.value) ?? 'PolyUI could not start a client',
        );
      }
      releaseText(error.value);
      return client;
    });
    handle = created;
    finalizer.attach(this, created.cast(), detach: this);
  }

  PolyuiCore.fromHello(
    PolyuiHello hello, {
    bool allowHttp = false,
    List<Map<String, Object?>> catalogs = const [],
  }) : this(hello.jsonText(), allowHttp: allowHttp, catalogs: catalogs);

  static final NativeFinalizer finalizer = NativeFinalizer(
    Native.addressOf<
          NativeFunction<Void Function(Pointer<PolyuiClientHandle>)>
        >(polyuiClientFree)
        .cast(),
  );

  final String hello;
  final bool allowHttp;
  final List<Map<String, Object?>> catalogs;

  @internal
  late Pointer<PolyuiClientHandle> handle;

  bool get isDisposed => handle == nullptr;

  void dispose() {
    if (handle == nullptr) {
      return;
    }
    finalizer.detach(this);
    polyuiClientFree(handle);
    handle = nullptr;
  }

  String receiveJson(String message) => call(
    (client, arena, error) => polyuiClientReceive(
      client,
      nativeText(message, arena, 'the message'),
      error,
    ),
  );

  String setValueJson({
    required String surface,
    required String path,
    required String valueJson,
  }) => call(
    (client, arena, error) => polyuiClientSetValue(
      client,
      nativeText(surface, arena, 'the surface'),
      nativeText(path, arena, 'the path'),
      nativeText(valueJson, arena, 'the value'),
      error,
    ),
  );

  String fireJson({
    required String surface,
    required String component,
    required String prop,
    String? detailJson,
  }) => call(
    (client, arena, error) => polyuiClientFire(
      client,
      nativeText(surface, arena, 'the surface'),
      nativeText(component, arena, 'the component'),
      nativeText(prop, arena, 'the prop'),
      detailJson == null
          ? nullptr
          : nativeText(detailJson, arena, 'the detail'),
      error,
    ),
  );

  String confirmJson({
    required String surface,
    required String actionId,
    required String nonce,
  }) => call(
    (client, arena, error) => polyuiClientConfirm(
      client,
      nativeText(surface, arena, 'the surface'),
      nativeText(actionId, arena, 'the action id'),
      nativeText(nonce, arena, 'the nonce'),
      error,
    ),
  );

  String sayJson(String surface) => call(
    (client, arena, error) => polyuiClientSay(
      client,
      nativeText(surface, arena, 'the surface'),
      error,
    ),
  );

  String matchVoiceJson({required String surface, required String text}) =>
      call(
        (client, arena, error) => polyuiClientMatchVoice(
          client,
          nativeText(surface, arena, 'the surface'),
          nativeText(text, arena, 'the text'),
          error,
        ),
      );

  static String embedPage({required String propsJson, required String nonce}) =>
      using((arena) {
        final error = arena<Pointer<Utf8>>()..value = nullptr;
        final page = polyuiEmbedPage(
          nativeText(propsJson, arena, 'the embed props'),
          nativeText(nonce, arena, 'the embed nonce'),
          error,
        );
        if (page == nullptr) {
          throw PolyuiException(
            takeText(error.value) ?? 'PolyUI could not build the embed page',
          );
        }
        releaseText(error.value);
        return takeText(page)!;
      });

  RenderBatch receive(String message) =>
      RenderBatch.fromJson(decode(receiveJson(message)));

  RenderBatch setValue({
    required String surface,
    required String path,
    required Object? value,
  }) => RenderBatch.fromJson(
    decode(
      setValueJson(surface: surface, path: path, valueJson: jsonEncode(value)),
    ),
  );

  String? fire({
    required String surface,
    required String component,
    required String prop,
    Map<String, Object?>? detail,
  }) => nullable(
    fireJson(
      surface: surface,
      component: component,
      prop: prop,
      detailJson: detail == null ? null : jsonEncode(detail),
    ),
  );

  String? confirm({
    required String surface,
    required String actionId,
    required String nonce,
  }) =>
      nullable(confirmJson(surface: surface, actionId: actionId, nonce: nonce));

  List<Utterance> say(String surface) => (decode(sayJson(surface)) as List)
      .map(Utterance.fromJson)
      .toList(growable: false);

  VoiceMatch? matchVoice({required String surface, required String text}) {
    final raw = matchVoiceJson(surface: surface, text: text);
    return raw == 'null' ? null : VoiceMatch.fromJson(decode(raw));
  }

  @internal
  String call(NativeCall body) {
    final client = handle;
    if (client == nullptr) {
      throw const PolyuiException('PolyUI client was disposed');
    }
    return using((arena) {
      final error = arena<Pointer<Utf8>>()..value = nullptr;
      final result = body(client, arena, error);
      if (result == nullptr) {
        throw PolyuiException(
          takeText(error.value) ?? 'PolyUI call failed without a message',
        );
      }
      releaseText(error.value);
      return takeText(result)!;
    });
  }

  @internal
  static Pointer<Utf8> nativeText(String text, Allocator arena, String what) {
    if (text.contains('\u0000')) {
      throw PolyuiException('PolyUI: $what contains a NUL character');
    }
    return text.toNativeUtf8(allocator: arena);
  }

  @internal
  static String? takeText(Pointer<Utf8> pointer) {
    if (pointer == nullptr) {
      return null;
    }
    try {
      return pointer.toDartString();
    } finally {
      polyuiStringFree(pointer);
    }
  }

  @internal
  static void releaseText(Pointer<Utf8> pointer) {
    if (pointer != nullptr) {
      polyuiStringFree(pointer);
    }
  }

  @internal
  static String? nullable(String text) => text == 'null' ? null : text;

  @internal
  static Object? decode(String text) {
    try {
      return jsonDecode(text);
    } on FormatException catch (error) {
      throw PolyuiException(
        'PolyUI could not read the core output: ${error.message}',
      );
    }
  }
}
