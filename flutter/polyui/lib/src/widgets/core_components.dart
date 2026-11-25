import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';

import '../core/json.dart';
import '../core/safe_url.dart';
import '../model/component_node.dart';
import 'scope.dart';
import 'theme.dart';

class CardDepth extends InheritedWidget {
  const CardDepth({super.key, required this.depth, required super.child});

  final int depth;

  static int of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<CardDepth>()?.depth ?? 0;

  @override
  bool updateShouldNotify(CardDepth oldWidget) => oldWidget.depth != depth;
}

class RowContext extends InheritedWidget {
  const RowContext({super.key, required this.inRow, required super.child});

  final bool inRow;

  static bool of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<RowContext>()?.inRow ?? false;

  @override
  bool updateShouldNotify(RowContext oldWidget) => oldWidget.inRow != inRow;
}

class FillWidth extends StatelessWidget {
  const FillWidth({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) => RowContext.of(context)
      ? child
      : SizedBox(width: double.infinity, child: child);
}

class ContainerSemantics extends StatelessWidget {
  const ContainerSemantics({
    super.key,
    required this.node,
    required this.child,
    this.role,
  });

  final ComponentNode node;
  final SemanticsRole? role;
  final Widget child;

  @override
  Widget build(BuildContext context) => Semantics(
    container: true,
    explicitChildNodes: true,
    role: role,
    label: node.a11y.label.isEmpty ? null : node.a11y.label,
    child: child,
  );
}

class ComponentChildren extends StatelessWidget {
  const ComponentChildren({
    super.key,
    required this.ids,
    this.spacing = 0,
    this.inRow = false,
  });

  final List<String> ids;
  final double spacing;
  final bool inRow;

  @override
  Widget build(BuildContext context) => RowContext(
    inRow: inRow,
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      spacing: spacing,
      children: [
        for (final id in ids) PolyuiComponent(key: ValueKey(id), id: id),
      ],
    ),
  );
}

bool reducedMotion(BuildContext context) =>
    MediaQuery.maybeDisableAnimationsOf(context) ?? false;

Duration motion(BuildContext context, int milliseconds) =>
    reducedMotion(context)
    ? Duration.zero
    : Duration(milliseconds: milliseconds);

class TextComponent extends StatelessWidget {
  const TextComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final style = node.choice('style', const {
      'body',
      'title',
      'subtitle',
      'caption',
      'muted',
    }, 'body');
    final quiet = style == 'caption' || style == 'muted';
    final heading = style == 'title' || style == 'subtitle';
    return FillWidth(
      child: Semantics(
        header: heading,
        headingLevel: switch (style) {
          'title' => 3,
          'subtitle' => 4,
          _ => null,
        },
        child: Text(
          node.string('text'),
          style: TextStyle(
            color: quiet ? theme.muted : theme.foreground,
            height: 1.3,
            fontSize: switch (style) {
              'title' => 20,
              'caption' => 12,
              'muted' => 15,
              _ => 17,
            },
            fontWeight: heading ? FontWeight.w600 : FontWeight.w400,
          ),
        ),
      ),
    );
  }
}

class CardComponent extends StatelessWidget {
  const CardComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final depth = CardDepth.of(context);
    final nested = depth > 0;
    final tone = node.choice('tone', const {
      'plain',
      'info',
      'success',
      'warning',
      'danger',
    }, 'plain');
    final title = node.string('title');
    final subtitle = node.string('subtitle');
    final radius = BorderRadius.circular(
      nested ? theme.innerCornerRadius : theme.cornerRadius,
    );
    final toned = tone != 'plain';
    final decoration = BoxDecoration(
      color: nested
          ? theme.sunken
          : toned
          ? Color.alphaBlend(
              theme.tone(tone).withValues(alpha: 0.06),
              theme.raised,
            )
          : theme.raised,
      borderRadius: radius,
      border: nested
          ? null
          : Border.all(
              color: toned
                  ? theme.tone(tone).withValues(alpha: 0.35)
                  : theme.border,
            ),
      boxShadow: !nested && !toned && theme.cardShadow
          ? const [
              BoxShadow(
                color: Color(0x0F000000),
                blurRadius: 24,
                offset: Offset(0, 6),
              ),
            ]
          : null,
    );
    return FillWidth(
      child: ContainerSemantics(
        node: node,
        child: DecoratedBox(
          decoration: decoration,
          child: Padding(
            padding: EdgeInsets.all(nested ? theme.spacing : theme.spacing + 2),
            child: CardDepth(
              depth: depth + 1,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                spacing: theme.spacing,
                children: [
                  if (title.isNotEmpty || subtitle.isNotEmpty)
                    Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      spacing: 2,
                      children: [
                        if (title.isNotEmpty)
                          Semantics(
                            header: true,
                            child: Text(
                              title,
                              style: TextStyle(
                                fontSize: 17,
                                height: 1.25,
                                fontWeight: FontWeight.w600,
                                color: theme.foreground,
                              ),
                            ),
                          ),
                        if (subtitle.isNotEmpty)
                          Text(
                            subtitle,
                            style: TextStyle(
                              fontSize: 15,
                              height: 1.3,
                              color: theme.muted,
                            ),
                          ),
                      ],
                    ),
                  ComponentChildren(ids: node.children, spacing: theme.spacing),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

double gapFor(String gap) => switch (gap) {
  'none' => 0,
  'small' => 8,
  'large' => 24,
  _ => 12,
};

class StatStrip extends StatelessWidget {
  const StatStrip({super.key, required this.ids, required this.gap});

  final List<String> ids;
  final double gap;

  @override
  Widget build(BuildContext context) {
    final minimum = MediaQuery.textScalerOf(context).scale(120);
    return LayoutBuilder(
      builder: (context, constraints) {
        final count = ids.length;
        final each = (constraints.maxWidth - gap * (count - 1)) / count;
        if (count > 1 && each >= minimum) {
          return Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            spacing: gap,
            children: [
              for (final id in ids)
                Expanded(
                  child: PolyuiComponent(key: ValueKey(id), id: id),
                ),
            ],
          );
        }
        return ComponentChildren(ids: ids, spacing: gap < 12 ? 12 : gap);
      },
    );
  }
}

class RowComponent extends StatelessWidget {
  const RowComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  Widget build(BuildContext context) {
    final scope = PolyuiScope.of(context);
    final gap = gapFor(
      node.choice('gap', const {'none', 'small', 'medium', 'large'}, 'medium'),
    );
    final align = node.choice('align', const {
      'start',
      'center',
      'end',
      'between',
    }, 'start');
    final ids = node.children;
    final onlyStats =
        ids.isNotEmpty &&
        ids.every((id) => scope.surface.node(id)?.type == 'Stat');
    return FillWidth(
      child: ContainerSemantics(
        node: node,
        child: onlyStats
            ? StatStrip(ids: ids, gap: gap)
            : SizedBox(
                width: double.infinity,
                child: RowContext(
                  inRow: true,
                  child: Wrap(
                    spacing: gap,
                    runSpacing: gap < 8 ? 8 : gap,
                    crossAxisAlignment: WrapCrossAlignment.center,
                    alignment: switch (align) {
                      'center' => WrapAlignment.center,
                      'end' => WrapAlignment.end,
                      'between' => WrapAlignment.spaceBetween,
                      _ => WrapAlignment.start,
                    },
                    children: [
                      for (final id in ids)
                        PolyuiComponent(key: ValueKey(id), id: id),
                    ],
                  ),
                ),
              ),
      ),
    );
  }
}

class ListComponent extends StatelessWidget {
  const ListComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final title = node.string('title');
    final ordered = node.flag('ordered');
    final ids = node.children;
    return FillWidth(
      child: ContainerSemantics(
        node: node,
        role: SemanticsRole.list,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            if (title.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: Semantics(
                  header: true,
                  child: Text(
                    title,
                    style: TextStyle(
                      fontSize: 15,
                      fontWeight: FontWeight.w600,
                      color: theme.muted,
                    ),
                  ),
                ),
              ),
            for (final (index, id) in ids.indexed) ...[
              if (index > 0) Container(height: 1, color: theme.border),
              Semantics(
                key: ValueKey(id),
                container: true,
                role: SemanticsRole.listItem,
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 10),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    spacing: 10,
                    children: [
                      if (ordered)
                        ExcludeSemantics(
                          child: Text(
                            '${index + 1}.',
                            style: TextStyle(
                              fontSize: 17,
                              height: 1.3,
                              color: theme.muted,
                              fontFeatures: const [
                                FontFeature.tabularFigures(),
                              ],
                            ),
                          ),
                        ),
                      Expanded(child: PolyuiComponent(id: id)),
                    ],
                  ),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class StatComponent extends StatelessWidget {
  const StatComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final tone = node.choice('tone', const {
      'neutral',
      'positive',
      'negative',
      'warning',
    }, 'neutral');
    final label = node.string('label');
    final value = node.string('value');
    final unit = node.string('unit');
    final hint = node.string('hint');
    final spoken = node.a11y.label.isEmpty
        ? '$label: $value $unit'.trim()
        : node.a11y.label;
    return Semantics(
      container: true,
      role: SemanticsRole.status,
      label: spoken,
      value: hint.isEmpty ? null : hint,
      excludeSemantics: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        spacing: 2,
        children: [
          Text(
            label,
            style: TextStyle(
              fontSize: 15,
              fontWeight: FontWeight.w500,
              color: theme.muted,
            ),
          ),
          AnimatedSwitcher(
            duration: motion(context, 250),
            layoutBuilder: (current, previous) => Stack(
              alignment: AlignmentDirectional.topStart,
              children: [...previous, ?current],
            ),
            child: Wrap(
              key: ValueKey('$value|$unit|$tone'),
              crossAxisAlignment: WrapCrossAlignment.end,
              spacing: 6,
              children: [
                FittedBox(
                  fit: BoxFit.scaleDown,
                  alignment: AlignmentDirectional.centerStart,
                  child: Text(
                    value,
                    maxLines: 1,
                    softWrap: false,
                    style: TextStyle(
                      fontSize: 30,
                      height: 1.15,
                      fontWeight: FontWeight.w600,
                      fontFeatures: const [FontFeature.tabularFigures()],
                      color: tone == 'neutral'
                          ? theme.foreground
                          : theme.tone(tone),
                    ),
                  ),
                ),
                if (unit.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 3),
                    child: Text(
                      unit,
                      maxLines: 1,
                      style: TextStyle(
                        fontSize: 17,
                        fontWeight: FontWeight.w500,
                        color: theme.muted,
                      ),
                    ),
                  ),
              ],
            ),
          ),
          if (hint.isNotEmpty)
            Text(hint, style: TextStyle(fontSize: 13, color: theme.muted)),
        ],
      ),
    );
  }
}

class BadgeComponent extends StatelessWidget {
  const BadgeComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final color = theme.tone(
      node.choice('tone', const {
        'neutral',
        'info',
        'success',
        'warning',
        'danger',
      }, 'neutral'),
    );
    return Align(
      alignment: AlignmentDirectional.centerStart,
      widthFactor: 1,
      child: DecoratedBox(
        decoration: ShapeDecoration(
          color: color.withValues(alpha: 0.12),
          shape: const StadiumBorder(),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
          child: Text(
            node.string('text'),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            semanticsLabel: node.a11y.label.isEmpty ? null : node.a11y.label,
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: color,
            ),
          ),
        ),
      ),
    );
  }
}

class ImageComponent extends StatelessWidget {
  const ImageComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final scope = PolyuiScope.of(context);
    final aspect = node.choice('aspect', const {
      'square',
      'wide',
      'tall',
      'auto',
    }, 'auto');
    final alt = node.string('alt');
    final url = checkSafeUrl(
      jsonString(node.props['src']),
      allowHttp: scope.session.allowHttp,
    );
    Widget missing() => Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 24),
      child: Center(
        child: Text(
          alt,
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 15, color: theme.muted),
        ),
      ),
    );
    Widget picture = url == null
        ? missing()
        : Image.network(
            url.toString(),
            fit: aspect == 'auto' ? BoxFit.fitWidth : BoxFit.cover,
            width: double.infinity,
            errorBuilder: (context, error, stack) => missing(),
            frameBuilder: (context, child, frame, synchronous) =>
                synchronous || frame != null
                ? child
                : AspectRatio(
                    aspectRatio: aspect == 'auto' ? 16 / 9 : 1,
                    child: const Center(
                      child: CircularProgressIndicator.adaptive(),
                    ),
                  ),
          );
    final ratio = switch (aspect) {
      'square' => 1.0,
      'wide' => 16 / 9,
      'tall' => 3 / 4,
      _ => null,
    };
    if (ratio != null) {
      picture = AspectRatio(aspectRatio: ratio, child: picture);
    }
    return FillWidth(
      child: Semantics(
        image: true,
        label: alt,
        excludeSemantics: true,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(theme.innerCornerRadius),
          child: ColoredBox(color: theme.sunken, child: picture),
        ),
      ),
    );
  }
}

class PolyuiButton extends StatelessWidget {
  const PolyuiButton({
    super.key,
    required this.label,
    required this.tone,
    required this.onPressed,
    this.semanticsLabel,
  });

  final String label;
  final String tone;
  final VoidCallback? onPressed;
  final String? semanticsLabel;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final fill = switch (tone) {
      'primary' => theme.accent,
      'danger' => theme.negative,
      _ => theme.background,
    };
    final foreground = switch (tone) {
      'primary' => theme.accentForeground,
      'danger' => Colors.white,
      _ => theme.foreground,
    };
    return FilledButton(
      onPressed: onPressed,
      style: FilledButton.styleFrom(
        backgroundColor: fill,
        foregroundColor: foreground,
        disabledBackgroundColor: fill.withValues(alpha: 0.5),
        disabledForegroundColor: foreground.withValues(alpha: 0.5),
        minimumSize: const Size(44, 44),
        padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 10),
        elevation: 0,
        animationDuration: motion(context, 120),
        splashFactory: reducedMotion(context) ? NoSplash.splashFactory : null,
        textStyle: const TextStyle(fontSize: 17, fontWeight: FontWeight.w600),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(theme.controlCornerRadius),
          side: tone == 'secondary' || tone == 'plain'
              ? BorderSide(color: theme.borderStrong)
              : BorderSide.none,
        ),
      ),
      child: Text(
        label,
        maxLines: 2,
        textAlign: TextAlign.center,
        semanticsLabel: semanticsLabel,
      ),
    );
  }
}

class ButtonComponent extends StatelessWidget {
  const ButtonComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  Widget build(BuildContext context) {
    final scope = PolyuiScope.of(context);
    final enabled = !node.flag('disabled') && node.hasAction('on_press');
    return PolyuiButton(
      label: node.string('label'),
      tone: node.choice('tone', const {
        'primary',
        'secondary',
        'danger',
      }, 'secondary'),
      semanticsLabel: node.a11y.label.isEmpty ? null : node.a11y.label,
      onPressed: enabled
          ? () => scope.session.fire(
              surface: scope.surface.id,
              component: node.id,
              prop: 'on_press',
            )
          : null,
    );
  }
}

class DividerComponent extends StatelessWidget {
  const DividerComponent({super.key});

  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: FillWidth(
      child: Container(height: 1, color: PolyuiTheme.of(context).border),
    ),
  );
}

class ConfirmSheetComponent extends StatefulWidget {
  const ConfirmSheetComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  State<ConfirmSheetComponent> createState() => ConfirmSheetComponentState();
}

class ConfirmSheetComponentState extends State<ConfirmSheetComponent> {
  String? answeredFor;

  String get actionKey =>
      jsonString(asJsonMap(widget.node.props['on_confirm'])['action_id']) ?? '';

  void answer(String prop) {
    final key = actionKey;
    if (answeredFor == key) {
      return;
    }
    setState(() => answeredFor = key);
    if (widget.node.hasAction(prop)) {
      final scope = PolyuiScope.of(context);
      scope.session.fire(
        surface: scope.surface.id,
        component: widget.node.id,
        prop: prop,
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final node = widget.node;
    final locked = answeredFor != null && answeredFor == actionKey;
    final body = node.string('body');
    final cancel = node.string('cancel_label');
    final confirm = node.string('confirm_label');
    return FillWidth(
      child: ContainerSemantics(
        node: node,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: theme.raised,
            borderRadius: BorderRadius.circular(theme.innerCornerRadius),
            border: Border.all(color: theme.borderStrong),
          ),
          child: Padding(
            padding: EdgeInsets.all(theme.spacing),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              spacing: 12,
              children: [
                Semantics(
                  header: true,
                  child: Text(
                    node.string('title'),
                    style: TextStyle(
                      fontSize: 17,
                      fontWeight: FontWeight.w600,
                      color: theme.foreground,
                    ),
                  ),
                ),
                if (body.isNotEmpty)
                  Text(
                    body,
                    style: TextStyle(fontSize: 15, color: theme.muted),
                  ),
                SizedBox(
                  width: double.infinity,
                  child: Wrap(
                    alignment: WrapAlignment.end,
                    spacing: 10,
                    runSpacing: 8,
                    children: [
                      PolyuiButton(
                        label: cancel.isEmpty ? 'No' : cancel,
                        tone: 'secondary',
                        onPressed: locked ? null : () => answer('on_cancel'),
                      ),
                      PolyuiButton(
                        label: confirm.isEmpty ? 'Yes' : confirm,
                        tone: 'primary',
                        onPressed: locked || !node.hasAction('on_confirm')
                            ? null
                            : () => answer('on_confirm'),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class EmbedTitle extends StatelessWidget {
  const EmbedTitle({super.key, required this.node});

  final ComponentNode node;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final title = node.string('title');
    return FillWidth(
      child: Semantics(
        container: true,
        label: title.isEmpty ? node.a11y.label : title,
        excludeSemantics: true,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: theme.sunken,
            borderRadius: BorderRadius.circular(theme.innerCornerRadius),
          ),
          child: Padding(
            padding: EdgeInsets.all(theme.spacing),
            child: Row(
              spacing: 12,
              children: [
                Icon(Icons.web_asset_outlined, color: theme.muted, size: 22),
                Expanded(
                  child: Text(
                    title,
                    style: TextStyle(
                      fontSize: 17,
                      fontWeight: FontWeight.w500,
                      color: theme.foreground,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
