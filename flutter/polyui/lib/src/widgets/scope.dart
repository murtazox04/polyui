import 'package:flutter/widgets.dart';

import '../model/component_node.dart';
import '../model/session.dart';
import '../model/surface_model.dart';
import 'components.dart';
import 'theme.dart';

typedef PolyuiComponentBuilder =
    Widget Function(BuildContext context, PolyuiComponentContext component);

class PolyuiComponentContext {
  const PolyuiComponentContext({
    required this.node,
    required this.surface,
    required this.session,
  });

  final ComponentNode node;
  final PolyuiSurfaceModel surface;
  final PolyuiSession session;

  void fire(String prop, {Map<String, Object?>? detail}) => session.fire(
    surface: surface.id,
    component: node.id,
    prop: prop,
    detail: detail,
  );

  void setValue(String path, Object? value) =>
      session.setValue(surface: surface.id, path: path, value: value);

  Widget child(String id) => PolyuiComponent(key: ValueKey(id), id: id);

  List<Widget> get children => [for (final id in node.children) child(id)];
}

class PolyuiScope extends InheritedWidget {
  const PolyuiScope({
    super.key,
    required this.session,
    required this.surface,
    required this.builders,
    required this.labels,
    this.confirmDelay = const Duration(milliseconds: 400),
    required super.child,
  });

  final PolyuiSession session;
  final PolyuiSurfaceModel surface;
  final Map<String, PolyuiComponentBuilder> builders;
  final PolyuiLabels labels;
  final Duration confirmDelay;

  static PolyuiScope of(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<PolyuiScope>();
    assert(scope != null, 'PolyuiComponent must be inside a PolyuiSurface');
    return scope!;
  }

  PolyuiComponentContext contextFor(ComponentNode node) =>
      PolyuiComponentContext(node: node, surface: surface, session: session);

  @override
  bool updateShouldNotify(PolyuiScope oldWidget) =>
      oldWidget.session != session ||
      oldWidget.surface != surface ||
      oldWidget.builders != builders ||
      oldWidget.labels != labels ||
      oldWidget.confirmDelay != confirmDelay;
}

class PolyuiComponent extends StatelessWidget {
  const PolyuiComponent({super.key, required this.id});

  final String id;

  @override
  Widget build(BuildContext context) {
    final scope = PolyuiScope.of(context);
    return ValueListenableBuilder<ComponentNode?>(
      valueListenable: scope.surface.slot(id),
      builder: (context, node, child) => node == null
          ? const SizedBox.shrink()
          : PolyuiNodeView(key: ObjectKey(node), node: node),
    );
  }
}

class PolyuiNodeView extends StatefulWidget {
  const PolyuiNodeView({super.key, required this.node});

  final ComponentNode node;

  @override
  State<PolyuiNodeView> createState() => PolyuiNodeViewState();
}

class PolyuiNodeViewState extends State<PolyuiNodeView>
    with SingleTickerProviderStateMixin {
  late final AnimationController appearance = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 200),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (appearance.isDismissed && !appearance.isAnimating) {
      if (MediaQuery.maybeDisableAnimationsOf(context) ?? false) {
        appearance.value = 1;
      } else {
        appearance.forward();
      }
    }
  }

  @override
  void dispose() {
    appearance.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scope = PolyuiScope.of(context);
    return FadeTransition(
      opacity: appearance,
      alwaysIncludeSemantics: true,
      child: ListenableBuilder(
        listenable: widget.node,
        builder: (context, child) => PolyuiHighlight(
          on: widget.node.highlighted,
          child: buildComponent(context, scope, widget.node),
        ),
      ),
    );
  }
}

class PolyuiHighlight extends StatelessWidget {
  const PolyuiHighlight({super.key, required this.on, required this.child});

  final bool on;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final reduce = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    return Stack(
      clipBehavior: Clip.none,
      children: [
        child,
        Positioned(
          left: -6,
          top: -6,
          right: -6,
          bottom: -6,
          child: IgnorePointer(
            child: AnimatedOpacity(
              opacity: on ? 1 : 0,
              duration: reduce
                  ? Duration.zero
                  : const Duration(milliseconds: 180),
              curve: Curves.easeOut,
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: theme.accent.withValues(alpha: 0.1),
                  borderRadius: BorderRadius.circular(
                    theme.controlCornerRadius,
                  ),
                  border: Border.all(
                    color: theme.accent.withValues(alpha: 0.55),
                    width: 1.5,
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class UnknownComponent extends StatefulWidget {
  const UnknownComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  State<UnknownComponent> createState() => UnknownComponentState();
}

class UnknownComponentState extends State<UnknownComponent> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) {
        return;
      }
      final scope = PolyuiScope.of(context);
      scope.session.reportUnknown(
        surface: scope.surface.id,
        type: widget.node.type,
      );
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    return Text(
      widget.node.a11y.label,
      style: TextStyle(fontSize: 15, color: theme.muted),
    );
  }
}
