import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';

import '../model/chart_scene.dart';
import '../model/component_node.dart';
import 'core_components.dart';
import 'scope.dart';
import 'theme.dart';

@immutable
class ChartGeometry {
  const ChartGeometry(this.viewBox, this.scale);

  factory ChartGeometry.fit(Rect viewBox, double width) =>
      ChartGeometry(viewBox, width / viewBox.width);

  final Rect viewBox;
  final double scale;

  Offset point(Offset point) => Offset(
    (point.dx - viewBox.left) * scale,
    (point.dy - viewBox.top) * scale,
  );

  Rect rect(Rect rect) =>
      Rect.fromPoints(point(rect.topLeft), point(rect.bottomRight));

  Path path(List<ChartPathCommand> commands) {
    final path = Path();
    for (final command in commands) {
      switch (command) {
        case ChartMoveTo(:final point):
          final target = this.point(point);
          path.moveTo(target.dx, target.dy);
        case ChartLineTo(:final point):
          final target = this.point(point);
          path.lineTo(target.dx, target.dy);
        case ChartArcTo(
          :final point,
          :final radius,
          :final largeArc,
          :final clockwise,
        ):
          final target = this.point(point);
          if (radius <= 0) {
            path.lineTo(target.dx, target.dy);
          } else {
            path.arcToPoint(
              target,
              radius: Radius.circular(radius * scale),
              largeArc: largeArc,
              clockwise: clockwise,
            );
          }
        case ChartClose():
          path.close();
      }
    }
    return path;
  }

  Path hitPath(ChartHitShape shape) => switch (shape) {
    ChartHitRect(:final rect) => Path()..addRect(this.rect(rect)),
    ChartHitPolygon(:final points) =>
      Path()..addPolygon([for (final item in points) point(item)], true),
  };
}

class ChartPainter extends CustomPainter {
  ChartPainter({
    required this.scene,
    required this.theme,
    required this.selectedRow,
    required this.textStyle,
    required this.textDirection,
    this.onSelect,
  });

  final ChartScene scene;
  final PolyuiThemeData theme;
  final int? selectedRow;
  final TextStyle textStyle;
  final TextDirection textDirection;
  final ValueChanged<int>? onSelect;

  @override
  void paint(Canvas canvas, Size size) {
    final geometry = ChartGeometry.fit(scene.viewBox, size.width);
    final selected = selectedRow;
    if (selected != null) {
      for (final hit in scene.hits.where((hit) => hit.row == selected)) {
        final outline = geometry.hitPath(hit.shape);
        canvas.drawPath(
          outline,
          Paint()..color = theme.accent.withValues(alpha: 0.08),
        );
        canvas.drawPath(
          outline,
          Paint()
            ..color = theme.accent
            ..style = PaintingStyle.stroke
            ..strokeWidth = 1.5,
        );
      }
    }
    for (final mark in scene.marks) {
      draw(canvas, mark, geometry);
    }
  }

  void draw(Canvas canvas, ChartMark mark, ChartGeometry geometry) {
    switch (mark) {
      case ChartRectMark(:final rect, :final fill):
        final color = theme.chartColor(fill);
        if (color == null) {
          return;
        }
        final frame = geometry.rect(rect);
        final radius = math.max(
          0.0,
          math.min(4 * geometry.scale, math.min(frame.width, frame.height) / 2),
        );
        canvas.drawRRect(
          RRect.fromRectAndRadius(frame, Radius.circular(radius)),
          Paint()..color = color,
        );
      case ChartLineMark(:final from, :final to, :final stroke, :final width):
        final color = theme.chartColor(stroke);
        if (color == null) {
          return;
        }
        canvas.drawLine(
          geometry.point(from),
          geometry.point(to),
          Paint()
            ..color = color
            ..strokeWidth = math.max(0.5, width * geometry.scale),
        );
      case ChartPathMark(
        :final commands,
        :final fill,
        :final fillOpacity,
        :final stroke,
        :final strokeWidth,
      ):
        final shape = geometry.path(commands);
        final fillColor = theme.chartColor(fill);
        if (fillColor != null) {
          canvas.drawPath(
            shape,
            Paint()
              ..color = fillColor.withValues(
                alpha: fillColor.a * (fillOpacity ?? 1).clamp(0, 1),
              ),
          );
        }
        final strokeColor = theme.chartColor(stroke);
        if (strokeColor != null) {
          canvas.drawPath(
            shape,
            Paint()
              ..color = strokeColor
              ..style = PaintingStyle.stroke
              ..strokeCap = StrokeCap.round
              ..strokeJoin = StrokeJoin.round
              ..strokeWidth = math.max(
                0.5,
                (strokeWidth ?? 1) * geometry.scale,
              ),
          );
        }
      case ChartCircleMark(:final center, :final radius, :final fill):
        final color = theme.chartColor(fill);
        if (color == null) {
          return;
        }
        canvas.drawCircle(
          geometry.point(center),
          radius * geometry.scale,
          Paint()..color = color,
        );
      case ChartTextMark(
        :final text,
        :final at,
        :final anchor,
        :final role,
        :final color,
        :final size,
      ):
        final fontSize = math.max(size * geometry.scale, size * 0.85);
        final painter = TextPainter(
          text: TextSpan(
            text: text,
            style: textStyle.copyWith(
              fontSize: fontSize,
              fontWeight: role == 'title' ? FontWeight.w600 : FontWeight.w400,
              color: theme.chartColor(color) ?? theme.foreground,
            ),
          ),
          textDirection: textDirection,
          textScaler: TextScaler.noScaling,
          maxLines: 1,
        )..layout();
        final point = geometry.point(at);
        final dx = switch (anchor) {
          ChartTextAnchor.start => point.dx,
          ChartTextAnchor.middle => point.dx - painter.width / 2,
          ChartTextAnchor.end => point.dx - painter.width,
        };
        final baseline = painter.computeDistanceToActualBaseline(
          TextBaseline.alphabetic,
        );
        painter.paint(canvas, Offset(dx, point.dy - baseline));
        painter.dispose();
    }
  }

  ChartHit? hitAt(Offset position, Size size) {
    final geometry = ChartGeometry.fit(scene.viewBox, size.width);
    for (final hit in scene.hits.reversed) {
      if (geometry.hitPath(hit.shape).contains(position)) {
        return hit;
      }
    }
    return null;
  }

  @override
  SemanticsBuilderCallback get semanticsBuilder => (size) {
    final geometry = ChartGeometry.fit(scene.viewBox, size.width);
    final select = onSelect;
    return [
      CustomPainterSemantics(
        key: const ValueKey('picture'),
        rect: Offset.zero & size,
        properties: SemanticsProperties(
          image: true,
          label: scene.description.isEmpty ? scene.title : scene.description,
          textDirection: textDirection,
        ),
      ),
      for (final hit in scene.hits)
        CustomPainterSemantics(
          key: ValueKey(hit.index),
          rect: geometry.hitPath(hit.shape).getBounds(),
          properties: SemanticsProperties(
            label: hit.label,
            textDirection: textDirection,
            button: select != null,
            selected: select == null ? null : hit.row == selectedRow,
            onTap: select == null ? null : () => select(hit.row),
          ),
        ),
    ];
  };

  @override
  bool shouldRepaint(ChartPainter oldDelegate) =>
      oldDelegate.scene != scene ||
      oldDelegate.theme != theme ||
      oldDelegate.selectedRow != selectedRow ||
      oldDelegate.textStyle != textStyle;

  @override
  bool shouldRebuildSemantics(ChartPainter oldDelegate) =>
      shouldRepaint(oldDelegate) ||
      (oldDelegate.onSelect == null) != (onSelect == null);
}

class ChartComponent extends StatefulWidget {
  const ChartComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  State<ChartComponent> createState() => ChartComponentState();
}

class ChartComponentState extends State<ChartComponent> {
  void select(int row) {
    final scope = PolyuiScope.of(context);
    scope.session.fire(
      surface: scope.surface.id,
      component: widget.node.id,
      prop: 'on_select',
      detail: {'row': row},
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final scope = PolyuiScope.of(context);
    final node = widget.node;
    final title = node.string('title');
    final scene = node.chartScene;
    final heading = Text(
      title,
      style: TextStyle(
        fontSize: 15,
        fontWeight: FontWeight.w600,
        color: theme.foreground,
      ),
    );
    final List<Widget> content;
    if (scene == null) {
      content = [
        heading,
        Text(
          scope.labels.chartUnavailable,
          style: TextStyle(fontSize: 15, color: theme.muted),
        ),
      ];
    } else {
      final selectable = node.hasAction('on_select');
      final painter = ChartPainter(
        scene: scene,
        theme: theme,
        selectedRow: node.selectedRow,
        textStyle: DefaultTextStyle.of(context).style,
        textDirection: Directionality.of(context),
        onSelect: selectable ? select : null,
      );
      content = [
        if (!scene.hasTitleMark && title.isNotEmpty) heading,
        AspectRatio(
          aspectRatio: scene.viewBox.width / scene.viewBox.height,
          child: LayoutBuilder(
            builder: (context, constraints) => GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTapUp: selectable
                  ? (details) {
                      final hit = painter.hitAt(
                        details.localPosition,
                        constraints.biggest,
                      );
                      if (hit != null) {
                        select(hit.row);
                      }
                    }
                  : null,
              child: CustomPaint(painter: painter, size: constraints.biggest),
            ),
          ),
        ),
        if (scene.legend.isNotEmpty) ChartLegend(entries: scene.legend),
      ];
    }
    return FillWidth(
      child: ContainerSemantics(
        node: node,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          spacing: 10,
          children: content,
        ),
      ),
    );
  }
}

class ChartLegend extends StatelessWidget {
  const ChartLegend({super.key, required this.entries});

  final List<ChartLegendEntry> entries;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    return Wrap(
      spacing: 12,
      runSpacing: 6,
      children: [
        for (final entry in entries)
          MergeSemantics(
            child: Row(
              mainAxisSize: MainAxisSize.min,
              spacing: 6,
              children: [
                Container(
                  width: 10,
                  height: 10,
                  decoration: BoxDecoration(
                    color: theme.chartColor(entry.color) ?? theme.accent,
                    borderRadius: BorderRadius.circular(3),
                  ),
                ),
                Text(
                  entry.label,
                  style: TextStyle(fontSize: 12, color: theme.muted),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

sealed class DashboardCell {
  const DashboardCell();
}

final class DashboardStrip extends DashboardCell {
  const DashboardStrip(this.ids);

  final List<String> ids;
}

final class DashboardSingle extends DashboardCell {
  const DashboardSingle(this.id);

  final String id;
}

class DashboardComponent extends StatelessWidget {
  const DashboardComponent({super.key, required this.node});

  static const compactWidth = 600.0;

  final ComponentNode node;

  List<DashboardCell> cells(PolyuiScope scope) {
    final cells = <DashboardCell>[];
    var run = <String>[];
    for (final id in node.children) {
      if (scope.surface.node(id)?.type == 'Stat') {
        run.add(id);
      } else {
        if (run.isNotEmpty) {
          cells.add(DashboardStrip(run));
        }
        run = [];
        cells.add(DashboardSingle(id));
      }
    }
    if (run.isNotEmpty) {
      cells.add(DashboardStrip(run));
    }
    return cells;
  }

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final scope = PolyuiScope.of(context);
    final title = node.string('title');
    final requested = (node.number('columns') ?? 2).round().clamp(1, 4);
    final all = cells(scope);
    Widget cell(DashboardCell cell) => switch (cell) {
      DashboardStrip(:final ids) => DecoratedBox(
        decoration: BoxDecoration(
          color: theme.sunken,
          borderRadius: BorderRadius.circular(theme.innerCornerRadius),
        ),
        child: Padding(
          padding: EdgeInsets.all(theme.spacing),
          child: StatStrip(ids: ids, gap: theme.spacing),
        ),
      ),
      DashboardSingle(:final id) => PolyuiComponent(key: ValueKey(id), id: id),
    };
    return FillWidth(
      child: ContainerSemantics(
        node: node,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          spacing: theme.spacing,
          children: [
            if (title.isNotEmpty)
              Semantics(
                header: true,
                child: Text(
                  title,
                  style: TextStyle(
                    fontSize: 17,
                    fontWeight: FontWeight.w600,
                    color: theme.foreground,
                  ),
                ),
              ),
            LayoutBuilder(
              builder: (context, constraints) {
                final columns = constraints.maxWidth < compactWidth
                    ? 1
                    : requested;
                return RowContext(
                  inRow: false,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    mainAxisSize: MainAxisSize.min,
                    spacing: theme.spacing,
                    children: [
                      for (var start = 0; start < all.length; start += columns)
                        Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          spacing: theme.spacing,
                          children: [
                            for (
                              var index = start;
                              index < start + columns;
                              index += 1
                            )
                              Expanded(
                                child: index < all.length
                                    ? cell(all[index])
                                    : const SizedBox.shrink(),
                              ),
                          ],
                        ),
                    ],
                  ),
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}
