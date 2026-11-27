import 'package:flutter/material.dart';

import '../core/render_batch.dart';
import '../model/session.dart';
import '../model/surface_model.dart';
import 'confirm_request_sheet.dart';
import 'scope.dart';
import 'theme.dart';

class PolyuiSurface extends StatefulWidget {
  const PolyuiSurface({
    super.key,
    required this.session,
    required this.surface,
    this.builders = const {},
    this.labels = const PolyuiLabels(),
    this.confirmDelay = const Duration(milliseconds: 400),
  });

  final PolyuiSession session;
  final String surface;
  final Map<String, PolyuiComponentBuilder> builders;
  final PolyuiLabels labels;
  final Duration confirmDelay;

  @override
  State<PolyuiSurface> createState() => PolyuiSurfaceState();
}

class PolyuiSurfaceState extends State<PolyuiSurface> {
  late PolyuiSurfaceModel model;
  ModalBottomSheetRoute<bool>? sheet;
  String? sheetKey;

  @override
  void initState() {
    super.initState();
    model = widget.session.surface(widget.surface);
    model.addListener(changed);
    WidgetsBinding.instance.addPostFrameCallback((_) => syncConfirm());
  }

  @override
  void didUpdateWidget(PolyuiSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.session != widget.session ||
        oldWidget.surface != widget.surface) {
      model.removeListener(changed);
      dismissSheet();
      model = widget.session.surface(widget.surface);
      model.addListener(changed);
      WidgetsBinding.instance.addPostFrameCallback((_) => syncConfirm());
    }
  }

  @override
  void dispose() {
    model.removeListener(changed);
    dismissSheet();
    super.dispose();
  }

  void changed() {
    if (!mounted) {
      return;
    }
    setState(() {});
    WidgetsBinding.instance.addPostFrameCallback((_) => syncConfirm());
  }

  void dismissSheet() {
    final open = sheet;
    sheet = null;
    sheetKey = null;
    if (open != null && open.isActive) {
      open.navigator?.removeRoute(open);
    }
  }

  void syncConfirm() {
    if (!mounted) {
      return;
    }
    final pending = model.pendingConfirm;
    if (pending?.key == sheetKey) {
      return;
    }
    dismissSheet();
    if (pending != null) {
      present(pending);
    }
  }

  Future<void> present(ConfirmRequest request) async {
    final navigator = Navigator.of(context, rootNavigator: true);
    final route = polyuiConfirmRoute(
      context,
      request: request,
      labels: widget.labels,
      delay: widget.confirmDelay,
    );
    sheet = route;
    sheetKey = request.key;
    final confirmed = await navigator.push(route);
    if (sheetKey != request.key) {
      return;
    }
    sheet = null;
    sheetKey = null;
    if (confirmed ?? false) {
      widget.session.confirm(surface: model.id, request: request);
    } else {
      widget.session.cancelConfirm(surface: model.id, request: request);
    }
  }

  @override
  Widget build(BuildContext context) {
    final rootId = model.rootId;
    final reduce = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    return Material(
      type: MaterialType.transparency,
      child: DefaultTextStyle.merge(
        style: const TextStyle(letterSpacing: 0),
        child: PolyuiScope(
          session: widget.session,
          surface: model,
          builders: widget.builders,
          labels: widget.labels,
          confirmDelay: widget.confirmDelay,
          child: AnimatedSize(
            duration: reduce
                ? Duration.zero
                : const Duration(milliseconds: 220),
            curve: Curves.easeOut,
            alignment: Alignment.topCenter,
            clipBehavior: Clip.none,
            child: SizedBox(
              width: double.infinity,
              child: rootId == null
                  ? const SizedBox.shrink()
                  : PolyuiComponent(key: ValueKey(rootId), id: rootId),
            ),
          ),
        ),
      ),
    );
  }
}
