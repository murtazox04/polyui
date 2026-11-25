@DefaultAsset('package:polyui/polyui_ffi')
library;

import 'dart:ffi';

import 'package:ffi/ffi.dart';

final class PolyuiClientHandle extends Opaque {}

typedef ErrorSlot = Pointer<Pointer<Utf8>>;

@Native<Pointer<PolyuiClientHandle> Function(Pointer<Utf8>, Bool, ErrorSlot)>(
  symbol: 'polyui_client_new',
)
external Pointer<PolyuiClientHandle> polyuiClientNew(
  Pointer<Utf8> hello,
  bool allowHttp,
  ErrorSlot error,
);

@Native<
  Pointer<PolyuiClientHandle> Function(
    Pointer<Utf8>,
    Pointer<Utf8>,
    Bool,
    ErrorSlot,
  )
>(symbol: 'polyui_client_new_with_catalogs')
external Pointer<PolyuiClientHandle> polyuiClientNewWithCatalogs(
  Pointer<Utf8> hello,
  Pointer<Utf8> catalogs,
  bool allowHttp,
  ErrorSlot error,
);

@Native<
  Pointer<Utf8> Function(Pointer<PolyuiClientHandle>, Pointer<Utf8>, ErrorSlot)
>(symbol: 'polyui_client_receive')
external Pointer<Utf8> polyuiClientReceive(
  Pointer<PolyuiClientHandle> client,
  Pointer<Utf8> message,
  ErrorSlot error,
);

@Native<
  Pointer<Utf8> Function(
    Pointer<PolyuiClientHandle>,
    Pointer<Utf8>,
    Pointer<Utf8>,
    Pointer<Utf8>,
    ErrorSlot,
  )
>(symbol: 'polyui_client_set_value')
external Pointer<Utf8> polyuiClientSetValue(
  Pointer<PolyuiClientHandle> client,
  Pointer<Utf8> surface,
  Pointer<Utf8> path,
  Pointer<Utf8> value,
  ErrorSlot error,
);

@Native<
  Pointer<Utf8> Function(
    Pointer<PolyuiClientHandle>,
    Pointer<Utf8>,
    Pointer<Utf8>,
    Pointer<Utf8>,
    Pointer<Utf8>,
    ErrorSlot,
  )
>(symbol: 'polyui_client_fire')
external Pointer<Utf8> polyuiClientFire(
  Pointer<PolyuiClientHandle> client,
  Pointer<Utf8> surface,
  Pointer<Utf8> component,
  Pointer<Utf8> prop,
  Pointer<Utf8> detail,
  ErrorSlot error,
);

@Native<
  Pointer<Utf8> Function(
    Pointer<PolyuiClientHandle>,
    Pointer<Utf8>,
    Pointer<Utf8>,
    Pointer<Utf8>,
    ErrorSlot,
  )
>(symbol: 'polyui_client_confirm')
external Pointer<Utf8> polyuiClientConfirm(
  Pointer<PolyuiClientHandle> client,
  Pointer<Utf8> surface,
  Pointer<Utf8> actionId,
  Pointer<Utf8> nonce,
  ErrorSlot error,
);

@Native<
  Pointer<Utf8> Function(Pointer<PolyuiClientHandle>, Pointer<Utf8>, ErrorSlot)
>(symbol: 'polyui_client_say')
external Pointer<Utf8> polyuiClientSay(
  Pointer<PolyuiClientHandle> client,
  Pointer<Utf8> surface,
  ErrorSlot error,
);

@Native<
  Pointer<Utf8> Function(
    Pointer<PolyuiClientHandle>,
    Pointer<Utf8>,
    Pointer<Utf8>,
    ErrorSlot,
  )
>(symbol: 'polyui_client_match_voice')
external Pointer<Utf8> polyuiClientMatchVoice(
  Pointer<PolyuiClientHandle> client,
  Pointer<Utf8> surface,
  Pointer<Utf8> text,
  ErrorSlot error,
);

@Native<Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, ErrorSlot)>(
  symbol: 'polyui_embed_page',
)
external Pointer<Utf8> polyuiEmbedPage(
  Pointer<Utf8> props,
  Pointer<Utf8> nonce,
  ErrorSlot error,
);

@Native<Void Function(Pointer<Utf8>)>(symbol: 'polyui_string_free')
external void polyuiStringFree(Pointer<Utf8> text);

@Native<Void Function(Pointer<PolyuiClientHandle>)>(
  symbol: 'polyui_client_free',
)
external void polyuiClientFree(Pointer<PolyuiClientHandle> client);
