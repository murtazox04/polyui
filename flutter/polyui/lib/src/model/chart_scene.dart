import 'dart:ui';

import 'package:flutter/foundation.dart';

import '../core/json.dart';

enum ChartTextAnchor { start, middle, end }

@immutable
sealed class ChartPathCommand {
  const ChartPathCommand();

  static ChartPathCommand? tryParse(Object? json) {
    final map = asJsonMap(json);
    final point = chartPoint(map, 'x', 'y');
    switch (map['op']) {
      case 'M':
        return point == null ? null : ChartMoveTo(point);
      case 'L':
        return point == null ? null : ChartLineTo(point);
      case 'Z':
        return const ChartClose();
      case 'A':
        final radius = jsonNumber(map['radius']);
        if (point == null || radius == null) {
          return null;
        }
        return ChartArcTo(
          point,
          radius: radius.abs(),
          largeArc: jsonBool(map['large_arc']) ?? false,
          clockwise: jsonBool(map['clockwise']) ?? false,
        );
      default:
        return null;
    }
  }
}

final class ChartMoveTo extends ChartPathCommand {
  const ChartMoveTo(this.point);

  final Offset point;
}

final class ChartLineTo extends ChartPathCommand {
  const ChartLineTo(this.point);

  final Offset point;
}

final class ChartArcTo extends ChartPathCommand {
  const ChartArcTo(
    this.point, {
    required this.radius,
    required this.largeArc,
    required this.clockwise,
  });

  final Offset point;
  final double radius;
  final bool largeArc;
  final bool clockwise;
}

final class ChartClose extends ChartPathCommand {
  const ChartClose();
}

@immutable
sealed class ChartMark {
  const ChartMark();

  static ChartMark? tryParse(Object? json) {
    final map = asJsonMap(json);
    switch (map['type']) {
      case 'rect':
        final origin = chartPoint(map, 'x', 'y');
        final width = jsonNumber(map['width']);
        final height = jsonNumber(map['height']);
        final fill = jsonString(map['fill']);
        if (origin == null || width == null || height == null || fill == null) {
          return null;
        }
        return ChartRectMark(
          Rect.fromLTWH(
            origin.dx,
            origin.dy,
            width < 0 ? 0 : width,
            height < 0 ? 0 : height,
          ),
          fill,
        );
      case 'line':
        final from = chartPoint(map, 'x1', 'y1');
        final to = chartPoint(map, 'x2', 'y2');
        final stroke = jsonString(map['stroke']);
        if (from == null || to == null || stroke == null) {
          return null;
        }
        return ChartLineMark(
          from,
          to,
          stroke,
          jsonNumber(map['stroke_width']) ?? 1,
        );
      case 'path':
        final raw = jsonList(map['commands']);
        if (raw.isEmpty) {
          return null;
        }
        final commands = <ChartPathCommand>[];
        for (final item in raw) {
          final command = ChartPathCommand.tryParse(item);
          if (command == null) {
            return null;
          }
          commands.add(command);
        }
        return ChartPathMark(
          commands,
          fill: jsonString(map['fill']),
          fillOpacity: jsonNumber(map['fill_opacity']),
          stroke: jsonString(map['stroke']),
          strokeWidth: jsonNumber(map['stroke_width']),
        );
      case 'circle':
        final center = chartPoint(map, 'cx', 'cy');
        final radius = jsonNumber(map['r']);
        final fill = jsonString(map['fill']);
        if (center == null || radius == null || fill == null) {
          return null;
        }
        return ChartCircleMark(center, radius < 0 ? 0 : radius, fill);
      case 'text':
        final at = chartPoint(map, 'x', 'y');
        final size = jsonNumber(map['size']);
        final text = jsonString(map['text']);
        if (at == null || size == null || text == null) {
          return null;
        }
        return ChartTextMark(
          text,
          at,
          anchor: switch (map['anchor']) {
            'middle' => ChartTextAnchor.middle,
            'end' => ChartTextAnchor.end,
            _ => ChartTextAnchor.start,
          },
          role: jsonString(map['role']) ?? 'note',
          color: jsonString(map['color']) ?? 'text',
          size: size,
        );
      default:
        return null;
    }
  }
}

final class ChartRectMark extends ChartMark {
  const ChartRectMark(this.rect, this.fill);

  final Rect rect;
  final String fill;
}

final class ChartLineMark extends ChartMark {
  const ChartLineMark(this.from, this.to, this.stroke, this.width);

  final Offset from;
  final Offset to;
  final String stroke;
  final double width;
}

final class ChartPathMark extends ChartMark {
  const ChartPathMark(
    this.commands, {
    this.fill,
    this.fillOpacity,
    this.stroke,
    this.strokeWidth,
  });

  final List<ChartPathCommand> commands;
  final String? fill;
  final double? fillOpacity;
  final String? stroke;
  final double? strokeWidth;
}

final class ChartCircleMark extends ChartMark {
  const ChartCircleMark(this.center, this.radius, this.fill);

  final Offset center;
  final double radius;
  final String fill;
}

final class ChartTextMark extends ChartMark {
  const ChartTextMark(
    this.text,
    this.at, {
    required this.anchor,
    required this.role,
    required this.color,
    required this.size,
  });

  final String text;
  final Offset at;
  final ChartTextAnchor anchor;
  final String role;
  final String color;
  final double size;
}

@immutable
sealed class ChartHitShape {
  const ChartHitShape();
}

final class ChartHitRect extends ChartHitShape {
  const ChartHitRect(this.rect);

  final Rect rect;
}

final class ChartHitPolygon extends ChartHitShape {
  const ChartHitPolygon(this.points);

  final List<Offset> points;
}

@immutable
final class ChartHit {
  const ChartHit({
    required this.row,
    required this.label,
    required this.shape,
    required this.index,
    this.series,
  });

  static ChartHit? tryParse(Object? json, int index) {
    final map = asJsonMap(json);
    final row = jsonNumber(map['row']);
    if (row == null || row < 0 || row >= 2147483647) {
      return null;
    }
    final ChartHitShape shape;
    switch (map['shape']) {
      case 'rect':
        final origin = chartPoint(map, 'x', 'y');
        final width = jsonNumber(map['width']);
        final height = jsonNumber(map['height']);
        if (origin == null || width == null || height == null) {
          return null;
        }
        shape = ChartHitRect(
          Rect.fromLTWH(origin.dx, origin.dy, width, height),
        );
      case 'polygon':
        final raw = jsonList(map['points']);
        if (raw.length < 3) {
          return null;
        }
        final points = <Offset>[];
        for (final pair in raw) {
          final items = jsonList(pair);
          final x = items.length == 2 ? jsonNumber(items[0]) : null;
          final y = items.length == 2 ? jsonNumber(items[1]) : null;
          if (x == null || y == null) {
            return null;
          }
          points.add(Offset(x, y));
        }
        shape = ChartHitPolygon(points);
      default:
        return null;
    }
    return ChartHit(
      row: row.toInt(),
      series: jsonString(map['series']),
      label: jsonString(map['label']) ?? '',
      shape: shape,
      index: index,
    );
  }

  final int row;
  final String? series;
  final String label;
  final ChartHitShape shape;
  final int index;
}

@immutable
final class ChartLegendEntry {
  const ChartLegendEntry(this.label, this.color);

  final String label;
  final String color;
}

@immutable
final class ChartScene {
  const ChartScene({
    required this.viewBox,
    required this.title,
    required this.description,
    required this.marks,
    required this.legend,
    required this.hits,
  });

  static const markLimit = 6000;

  static ChartScene? tryParse(Object? json) {
    if (json is! Map) {
      return null;
    }
    final map = asJsonMap(json);
    final box = asJsonMap(map['view_box']);
    final x = jsonNumber(box['x']);
    final y = jsonNumber(box['y']);
    final width = jsonNumber(box['width']);
    final height = jsonNumber(box['height']);
    final marks = map['marks'];
    if (x == null ||
        y == null ||
        width == null ||
        height == null ||
        width <= 0 ||
        height <= 0 ||
        marks is! List) {
      return null;
    }
    final hits = jsonList(map['hits']).take(markLimit).toList();
    return ChartScene(
      viewBox: Rect.fromLTWH(x, y, width, height),
      title: jsonString(map['title']) ?? '',
      description: jsonString(map['description']) ?? '',
      marks: marks
          .take(markLimit)
          .map(ChartMark.tryParse)
          .nonNulls
          .toList(growable: false),
      legend: [
        for (final item in jsonList(map['legend']))
          if (jsonString(asJsonMap(item)['label']) case final label?)
            if (jsonString(asJsonMap(item)['color']) case final color?)
              ChartLegendEntry(label, color),
      ],
      hits: [
        for (var index = 0; index < hits.length; index += 1)
          ?ChartHit.tryParse(hits[index], index),
      ],
    );
  }

  final Rect viewBox;
  final String title;
  final String description;
  final List<ChartMark> marks;
  final List<ChartLegendEntry> legend;
  final List<ChartHit> hits;

  bool get hasTitleMark =>
      marks.any((mark) => mark is ChartTextMark && mark.role == 'title');
}

Offset? chartPoint(Map<String, Object?> map, String xKey, String yKey) {
  final x = jsonNumber(map[xKey]);
  final y = jsonNumber(map[yKey]);
  return x == null || y == null ? null : Offset(x, y);
}
